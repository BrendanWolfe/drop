import type { InstallLocationList } from "../types.ts";

export function defaultInstallIndex(locations: InstallLocationList): number {
  const index = locations.defaultIndex;
  return index !== null &&
    Number.isInteger(index) &&
    index >= 0 &&
    index < locations.directories.length
    ? index
    : 0;
}

/** Resolve both destinations before queueing anything, including dependencies. */
export function installPlanLocations<T extends { gameId: string }>(
  plan: readonly T[],
  mainGameId: string,
  mainPath: string | undefined,
  emulatorPath: string | undefined,
): Array<T & { installPath: string }> {
  return plan.map((item) => {
    const installPath = item.gameId === mainGameId ? mainPath : emulatorPath;
    if (!installPath)
      throw new Error(
        item.gameId === mainGameId
          ? "Select an installation location."
          : "Select an emulator installation location.",
      );
    return { ...item, installPath };
  });
}
