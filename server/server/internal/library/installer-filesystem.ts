import { createHash } from "node:crypto";
import { lstat, readdir } from "node:fs/promises";
import path from "node:path";
import { installerFileNames } from "./installer-files.ts";
import { validateAssetTargets } from "./installer-assets.ts";
import { validLibraryInputPath } from "./installer-input-paths.ts";

export function companionName(name: string) {
  if (!name || name.includes("/") || name.trim() !== name)
    throw new Error("Enter one file or folder name for the companion.");
  return installerFileNames([name])[0]!;
}

function within(root: string, candidate: string) {
  const relative = path.relative(root, candidate);
  return (
    relative === "" ||
    (!path.isAbsolute(relative) &&
      relative !== ".." &&
      !relative.startsWith(`..${path.sep}`))
  );
}

async function statWithoutLinks(value: string) {
  if (
    !path.isAbsolute(value) ||
    path.resolve(value) !== value ||
    /[\\\p{Cc}]/u.test(value)
  )
    throw new Error("Use a normalized absolute server path.");
  const { root } = path.parse(value);
  let current = root;
  let info = await lstat(root, { bigint: true });
  for (const part of value.slice(root.length).split(path.sep).filter(Boolean)) {
    current = path.join(current, part);
    info = await lstat(current, { bigint: true });
    if (info.isSymbolicLink() || (!info.isFile() && !info.isDirectory()))
      throw new Error(
        "Companion paths cannot contain symbolic links or special files.",
      );
  }
  return info;
}

export function configuredCompanionRoots() {
  return (process.env.DROP_INSTALLER_COMPANION_ROOTS ?? "")
    .split(path.delimiter)
    .filter(Boolean);
}

export async function filesystemCompanionSource(
  source: string,
  kind: string,
  roots: string[],
  libraryRoots: string[],
  archiveEntry?: string | null,
) {
  if (!path.isAbsolute(source) || path.resolve(source) !== source)
    throw new Error("Use a normalized absolute server path.");
  if (
    libraryRoots.some(
      (root) =>
        within(root, source) || (kind === "folder" && within(source, root)),
    )
  )
    throw new Error(
      "Choose a library companion for content inside a game library.",
    );
  if (
    !roots.some(
      (root) =>
        path.isAbsolute(root) &&
        path.resolve(root) === root &&
        within(root, source),
    )
  )
    throw new Error("Choose a path under DROP_INSTALLER_COMPANION_ROOTS.");
  const info = await statWithoutLinks(source);
  const zip =
    (archiveEntry || kind === "folder") &&
    info.isFile() &&
    /\.zip$/iu.test(source);
  if (
    (archiveEntry && (!zip || !validLibraryInputPath(archiveEntry))) ||
    (kind === "file"
      ? !info.isFile()
      : kind !== "folder" || (!zip && !info.isDirectory()))
  )
    throw new Error("Choose a regular file, directory, or ZIP folder.");
  return {
    info,
    zip,
    backend: !zip && kind === "file" ? path.dirname(source) : source,
  };
}

export async function resolveFilesystemCompanion(
  selection: {
    path: string;
    kind: "file" | "folder";
    name: string;
    archiveEntry?: string;
  },
  roots: string[],
  libraryRoots: string[],
  listZip: (source: string) => Promise<string[]>,
) {
  const { path: source, kind, archiveEntry } = selection;
  const name = companionName(selection.name);
  const { zip, backend, info } = await filesystemCompanionSource(
    source,
    kind,
    roots,
    libraryRoots,
    archiveEntry,
  );
  const hash = createHash("sha256");
  function stamp(relative: string, stat: typeof info) {
    hash.update(
      JSON.stringify([
        relative,
        `${stat.dev}`,
        `${stat.ino}`,
        `${stat.size}`,
        `${stat.mtimeNs}`,
        `${stat.ctimeNs}`,
      ]),
    );
  }
  hash.update(JSON.stringify([source, kind, archiveEntry || null]));
  stamp("", info);
  const targets: string[] = [];
  if (zip) {
    for (const entry of await listZip(source)) {
      const name = entry.replace(/^\.\//u, "");
      if (
        !archiveEntry ||
        (kind === "file"
          ? name === archiveEntry
          : name.startsWith(`${archiveEntry}/`))
      )
        targets.push(entry);
    }
  } else if (kind === "file") targets.push(path.basename(source));
  else {
    async function walk(directory: string) {
      const entries = await readdir(path.join(source, directory));
      for (const entry of entries.sort()) {
        const relative = directory ? `${directory}/${entry}` : entry;
        const stat = await lstat(path.join(source, relative), { bigint: true });
        if (stat.isDirectory()) await walk(relative);
        else if (stat.isFile()) {
          stamp(relative, stat);
          targets.push(relative);
        } else
          throw new Error(
            "Companion folders cannot contain symbolic links or special files.",
          );
        if (targets.length > 20_000)
          throw new Error("Too many companion files.");
      }
    }
    await walk("");
  }
  validateAssetTargets(targets);
  return {
    source,
    backend,
    kind,
    name,
    targets,
    archiveEntry: archiveEntry || undefined,
    key: hash.digest("hex"),
  };
}
