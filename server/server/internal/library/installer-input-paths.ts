export type LibraryInput = {
  gameId: string;
  versionId: string;
  kind: "file" | "folder";
  path: string;
};

export function normalizeInstallerLaunchPath(
  value: string,
): string | undefined {
  const path = value.trim().replaceAll("\\", "/").replace(/^\.\//u, "");
  if (!path) return undefined;
  if (
    path.length > 1024 ||
    path
      .split("/")
      .some(
        (part) =>
          !part ||
          part === "." ||
          part === ".." ||
          /[:<>"|?*\p{Cc}]|[. ]$/u.test(part),
      )
  )
    throw new Error(
      "Enter a launch file path relative to the installed game folder.",
    );
  return path;
}

export function validLibraryInputPath(path: string): boolean {
  return (
    path.length <= 1024 &&
    path
      .split("/")
      .every(
        (part) =>
          !!part &&
          !part.startsWith(".") &&
          !/[\\:<>"|?*\p{Cc}]|[. ]$/u.test(part),
      )
  );
}

export function installedFilePaths(files: string[]): Set<string> {
  return new Set(
    files
      .map((file) => file.replace(/^\.\//u, ""))
      .filter(validLibraryInputPath),
  );
}

export function hasLibraryInput(
  files: Set<string>,
  input: Pick<LibraryInput, "kind" | "path">,
): boolean {
  if (input.kind === "folder" && input.path === "") return true;
  if (!validLibraryInputPath(input.path)) return false;
  if (input.kind === "file") return files.has(input.path);
  for (const file of files) if (file.startsWith(`${input.path}/`)) return true;
  return false;
}

export function listInputDirectory(
  files: Set<string>,
  directory: string,
  search = "",
) {
  const entries = new Map<string, "file" | "folder">();
  const prefix = directory ? `${directory}/` : "";
  const filter = search.toLowerCase();
  for (const file of files) {
    if (!file.startsWith(prefix)) continue;
    const rest = file.slice(prefix.length);
    const slash = rest.indexOf("/");
    const name = slash < 0 ? rest : rest.slice(0, slash);
    if (!name.toLowerCase().includes(filter)) continue;
    entries.set(name, slash < 0 ? "file" : "folder");
  }
  return [...entries.entries()]
    .sort(
      ([a, kindA], [b, kindB]) =>
        kindB.localeCompare(kindA) || a.localeCompare(b),
    )
    .map(([name, kind]) => ({ name, kind }));
}
