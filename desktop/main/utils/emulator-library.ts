import type { InstalledEmulator } from "../types.ts";

export type EmulatorLibraryEntry = {
  gameId: string;
  name: string;
  iconObjectId?: string | null;
  bannerObjectId?: string | null;
  shortDescription?: string | null;
  versions: InstalledEmulator[];
  requiredGameCount: number;
};

export function emulatorLibrary(
  installed: InstalledEmulator[],
): EmulatorLibraryEntry[] {
  const entries = new Map<string, EmulatorLibraryEntry>();
  for (const version of installed) {
    let entry = entries.get(version.gameId);
    if (!entry) {
      entry = {
        gameId: version.gameId,
        name: version.name,
        iconObjectId: version.iconObjectId,
        bannerObjectId: version.bannerObjectId,
        shortDescription: version.shortDescription,
        versions: [],
        requiredGameCount: 0,
      };
      entries.set(version.gameId, entry);
    }
    entry.versions.push(version);
  }
  for (const entry of entries.values()) {
    entry.versions.sort((a, b) =>
      b.versionName.localeCompare(a.versionName, undefined, { numeric: true }),
    );
    entry.requiredGameCount = new Set(
      entry.versions.flatMap((version) =>
        version.requiredBy.map((game) => game.gameId),
      ),
    ).size;
  }
  return [...entries.values()].sort((a, b) =>
    a.name.localeCompare(b.name, undefined, { numeric: true }),
  );
}

export function filterEmulatorLibrary(
  entries: EmulatorLibraryEntry[],
  query: string,
) {
  const search = query.trim().toLocaleLowerCase();
  return entries.filter((entry) =>
    entry.name.toLocaleLowerCase().includes(search),
  );
}

export function emulatorSelection(
  entries: EmulatorLibraryEntry[],
  selectedId: string | undefined,
) {
  return entries.find((entry) => entry.gameId === selectedId) ?? entries[0];
}

export function emulatorVersionStatus(version: InstalledEmulator) {
  if (version.busy) return "Operation in progress";
  if (version.installType.type === "PartiallyInstalled")
    return "Installation incomplete";
  if (version.installType.type === "SetupRequired") return "Setup required";
  return "Installed";
}

export function emulatorCanOpen(version: InstalledEmulator, opening = false) {
  return (
    (version.directOpenLaunches?.length ?? 0) > 0 &&
    version.installType.type === "Installed" &&
    !version.busy &&
    !opening
  );
}
