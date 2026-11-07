type Requirement = { gameId: string; versionId: string; platform: string };

/** Repair the installed game's pins, without updating or downloading the game. */
export function emulatorRepairPlan<T extends Requirement>(
  requirements: T[],
): T[] {
  const plan = new Map<string, T>();
  for (const requirement of requirements) {
    const previous = plan.get(requirement.gameId);
    if (
      previous &&
      (previous.versionId !== requirement.versionId ||
        previous.platform !== requirement.platform)
    ) {
      throw new Error(
        "This game requires conflicting emulator builds. Ask your administrator to update its launch options.",
      );
    }
    plan.set(requirement.gameId, requirement);
  }
  return [...plan.values()];
}
