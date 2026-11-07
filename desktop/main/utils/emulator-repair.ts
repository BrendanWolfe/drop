type Requirement = { gameId: string; versionId: string; platform: string };

/** Repair the installed game's pins, without updating or downloading the game. */
export function emulatorRepairPlan<T extends Requirement>(
  requirements: T[],
): T[] {
  const plan = new Map<string, T>();
  for (const requirement of requirements) {
    plan.set(
      JSON.stringify([
        requirement.gameId,
        requirement.versionId,
        requirement.platform,
      ]),
      requirement,
    );
  }
  return [...plan.values()];
}
