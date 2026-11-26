import { readdir } from "node:fs/promises";
import path from "node:path";
import { installerFileNames, libraryEntry } from "./installer-files.ts";
import {
  listInputDirectory,
  validLibraryInputPath,
} from "./installer-input-paths.ts";
import { castManifest, type DropletManifest } from "./manifest/utils.ts";
import type { DownloadManifestDetails } from "./manifest";

export type InstallerAssetSelection = {
  source: string;
  kind: "file" | "folder";
  archiveEntry?: string;
};
export const installerAssetPrefix = "installer-asset-";
const maxFiles = 20_000;

function safePath(name: string) {
  if (!validLibraryInputPath(name)) throw new Error("Invalid companion path.");
  for (const part of name.split("/")) installerFileNames([part]);
  return name;
}

function zipPath(name: string) {
  return name.replace(/^\.\//u, "");
}

export async function browseInstallerZip(
  base: string,
  archive: string,
  directory: string,
  list: (source: string) => Promise<string[]>,
) {
  const { resolved, stat } = await libraryEntry(base, archive);
  if (!stat.isFile() || !/\.zip$/iu.test(archive))
    throw new Error("Choose a ZIP file.");
  if (directory) safePath(directory);
  const names = (await list(resolved)).map(zipPath).filter((name) => {
    try {
      safePath(name);
      return true;
    } catch {
      return false;
    }
  });
  return listInputDirectory(new Set(names), directory).map(
    ({ name, kind }) => ({ name, directory: kind === "folder", zip: false }),
  );
}

export async function resolveInstallerAsset(
  base: string,
  selection: InstallerAssetSelection,
  list: (source: string) => Promise<string[]>,
) {
  const { resolved, stat } = await libraryEntry(base, selection.source);
  const isFolder = selection.kind === "folder";
  let sourcePath: string;
  let source: string;
  let targets: string[];
  let targetPath: string;
  if (selection.archiveEntry !== undefined) {
    if (!stat.isFile() || !/\.zip$/iu.test(selection.source))
      throw new Error("Choose a ZIP file.");
    targetPath = safePath(selection.archiveEntry);
    source = resolved;
    sourcePath = selection.source;
    targets = (await list(source)).filter((raw) => {
      const name = zipPath(raw);
      return isFolder ? name.startsWith(`${targetPath}/`) : name === targetPath;
    });
  } else {
    if (isFolder ? !stat.isDirectory() : !stat.isFile())
      throw new Error("Choose a regular companion file or folder.");
    targetPath = safePath(path.posix.basename(selection.source));
    source = path.dirname(resolved);
    sourcePath = path.posix.dirname(selection.source);
    if (sourcePath === ".") sourcePath = "";
    targets = [];
    async function walk(directory: string) {
      for (const file of await readdir(path.join(source, directory), {
        withFileTypes: true,
      })) {
        if (file.name.startsWith(".")) continue;
        const relative = `${directory}/${file.name}`;
        safePath(relative);
        if (file.isDirectory()) await walk(relative);
        else if (file.isFile()) targets.push(relative);
        else
          throw new Error(
            "Companion folders cannot contain symbolic links or special files.",
          );
        if (targets.length > maxFiles)
          throw new Error("Too many companion files.");
      }
    }
    if (isFolder) await walk(targetPath);
    else targets.push(targetPath);
  }
  validateAssetTargets(targets);
  return {
    source,
    sourcePath,
    targetPath,
    targets,
    name: path.posix.basename(targetPath),
  };
}

export function validateAssetTargets(targets: string[]) {
  if (!targets.length || targets.length > maxFiles)
    throw new Error("The companion must contain between 1 and 20,000 files.");
  const seen = new Set<string>();
  for (const raw of targets) {
    const name = safePath(zipPath(raw)).toLowerCase();
    if (seen.has(name))
      throw new Error("The companion contains duplicate file names.");
    seen.add(name);
  }
  for (const name of seen) {
    const parts = name.split("/");
    while (parts.length > 1) {
      parts.pop();
      if (seen.has(parts.join("/")))
        throw new Error("A companion file and folder share a name.");
    }
  }
}

export function validateAssetManifest(
  raw: Parameters<typeof castManifest>[0],
  targets: string[],
) {
  const manifest = castManifest(raw);
  const indexed = new Set(
    Object.values(manifest.chunks).flatMap((chunk) =>
      chunk.files.map((file) => file.filename),
    ),
  );
  if (
    manifest.version !== "2" ||
    indexed.size !== targets.length ||
    targets.some((name) => !indexed.has(name))
  )
    throw new Error(
      "The depot did not index exactly the selected files. Update Torrential and retry.",
    );
  return manifest;
}

export function installerAssetDownload(
  id: string,
  manifest: DropletManifest,
  destination?: { kind: string; target: string; archiveEntry?: string | null },
): DownloadManifestDetails {
  const files: Array<[string, string]> = [];
  const chunks = Object.fromEntries(
    Object.entries(manifest.chunks).map(([idKey, chunk]) => [
      idKey,
      {
        ...chunk,
        files: chunk.files.map((file) => {
          const raw = zipPath(file.filename);
          const filename = destination
            ? destination.kind === "file"
              ? destination.target
              : `${destination.target}/${destination.archiveEntry ? raw.slice(destination.archiveEntry.length + 1) : raw}`
            : raw;
          files.push([filename, id]);
          return { ...file, filename };
        }),
      },
    ]),
  );
  return {
    manifests: { [id]: { ...manifest, chunks } },
    fileList: Object.fromEntries(files),
    installSize: manifest.size,
    downloadSize: manifest.size,
  };
}
