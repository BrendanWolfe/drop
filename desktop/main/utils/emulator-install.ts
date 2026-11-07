type PackageVersion = {
  gameId: string;
  versionId: string;
  installedPath?: string | null;
};

/** Required emulator packages are pinned, deduplicated, and queued first. */
export function gameInstallPlan(
  game: PackageVersion,
  dependencies: readonly PackageVersion[],
  enableUpdates: boolean,
): Array<PackageVersion & { enableUpdates: boolean }> {
  const versions = new Map<string, { versionId: string; installed: boolean }>();
  for (const dependency of dependencies) {
    const previous = versions.get(dependency.gameId);
    if (
      dependency.gameId === game.gameId ||
      (previous !== undefined && previous.versionId !== dependency.versionId)
    )
      throw new Error(
        "This game requires conflicting emulator versions. Ask your administrator to update its launch options.",
      );
    versions.set(dependency.gameId, {
      versionId: dependency.versionId,
      installed:
        Boolean(dependency.installedPath) && (previous?.installed ?? true),
    });
  }
  return [
    ...Array.from(versions)
      .filter(([, dependency]) => !dependency.installed)
      .map(([gameId, { versionId }]) => ({
        gameId,
        versionId,
        enableUpdates: false,
      })),
    { ...game, enableUpdates },
  ];
}

/** Require review when server pins change, or an installed dependency disappears. */
export function emulatorRequirementsChanged(
  previous: readonly (PackageVersion & { launchIds?: string[] })[],
  current: readonly (PackageVersion & { launchIds?: string[] })[],
): boolean {
  const identity = (item: PackageVersion & { launchIds?: string[] }) =>
    `${item.gameId}:${item.versionId}:${[...(item.launchIds ?? [])].sort().join(",")}`;
  const before = new Set(previous.map(identity));
  return (
    before.size !== current.length ||
    current.some(
      (item) =>
        !before.has(identity(item)) ||
        (!item.installedPath &&
          previous.some(
            (old) => identity(old) === identity(item) && old.installedPath,
          )),
    )
  );
}
