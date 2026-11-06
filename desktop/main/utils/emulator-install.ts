type PackageVersion = { gameId: string; versionId: string };

/** Required emulator packages are pinned, deduplicated, and queued first. */
export function gameInstallPlan(
  game: PackageVersion,
  dependencies: readonly PackageVersion[],
  enableUpdates: boolean,
): Array<PackageVersion & { enableUpdates: boolean }> {
  const versions = new Map<string, string>();
  for (const dependency of dependencies) {
    const previous = versions.get(dependency.gameId);
    if (
      dependency.gameId === game.gameId ||
      (previous !== undefined && previous !== dependency.versionId)
    )
      throw new Error(
        "This game requires conflicting emulator versions. Ask your administrator to update its launch options.",
      );
    versions.set(dependency.gameId, dependency.versionId);
  }
  return [
    ...Array.from(versions, ([gameId, versionId]) => ({
      gameId,
      versionId,
      enableUpdates: false,
    })),
    { ...game, enableUpdates },
  ];
}
