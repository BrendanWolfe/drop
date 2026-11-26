import { constants } from "node:fs";
import {
  copyFile,
  lstat,
  mkdir,
  readdir,
  realpath,
  rm,
  chmod,
} from "node:fs/promises";
import path from "node:path";

export function libraryPathParts(relative: string): string[] {
  const parts = relative.split("/");
  if (
    parts.some(
      (part) =>
        !part || part === "." || part === ".." || /[\\:\p{Cc}]/u.test(part),
    )
  )
    throw new Error("Choose a valid path inside the game library.");
  return parts;
}

export async function libraryEntry(base: string, relative: string) {
  let resolved = await realpath(base);
  for (const part of relative ? libraryPathParts(relative) : []) {
    resolved = path.join(resolved, part);
    if ((await lstat(resolved)).isSymbolicLink())
      throw new Error("Symbolic links cannot be used in installer packages.");
  }
  return { resolved, stat: await lstat(resolved) };
}

export async function listInstallerEntries(base: string, directory: string) {
  const { resolved, stat } = await libraryEntry(base, directory);
  if (!stat.isDirectory()) throw new Error("Choose a library folder.");
  return (await readdir(resolved, { withFileTypes: true }))
    .filter(
      (entry) =>
        !entry.name.startsWith(".") && (entry.isDirectory() || entry.isFile()),
    )
    .map((entry) => ({
      name: entry.name,
      directory: entry.isDirectory(),
      ...(entry.isFile() && /\.zip$/iu.test(entry.name) ? { zip: true } : {}),
    }))
    .sort(
      (a, b) =>
        Number(b.directory) - Number(a.directory) ||
        a.name.localeCompare(b.name),
    );
}

export function installerFileNames(paths: string[]) {
  const seen = new Set<string>();
  return paths.map((relative) => {
    const name = libraryPathParts(relative).at(-1)!;
    const key = name.toLowerCase();
    if (
      name.startsWith(".") ||
      /[<>"|?*]|[. ]$/u.test(name) ||
      /^(con|prn|aux|nul|com[0-9¹²³]|lpt[0-9¹²³])(?:\.|$)/iu.test(name)
    )
      throw new Error(
        "Choose files with names that can be installed on any supported platform.",
      );
    if (seen.has(key))
      throw new Error(
        `Two installer files are named ${name}. Choose files with different names.`,
      );
    seen.add(key);
    return name;
  });
}

export async function resolveInstallerFiles(base: string, paths: string[]) {
  const names = installerFileNames(paths);
  return Promise.all(
    paths.map(async (relative, index) => {
      const { resolved, stat } = await libraryEntry(base, relative);
      if (!stat.isFile())
        throw new Error("Choose regular files for the installer package.");
      return { resolved, name: names[index]!, mode: stat.mode & 0o777 };
    }),
  );
}

export async function snapshotInstallerFiles(
  directory: string,
  files: Awaited<ReturnType<typeof resolveInstallerFiles>>,
) {
  await mkdir(path.dirname(directory), { recursive: true });
  await mkdir(directory);
  try {
    for (const file of files) {
      const destination = path.join(directory, file.name);
      await copyFile(
        file.resolved,
        destination,
        constants.COPYFILE_EXCL | constants.COPYFILE_FICLONE,
      );
      await chmod(destination, file.mode);
    }
  } catch (cause) {
    await rm(directory, { force: true, recursive: true });
    throw cause;
  }
}
