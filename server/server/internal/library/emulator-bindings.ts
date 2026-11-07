type Binding = {
  launchId: string;
  emulatorId: string | null;
  previousEmulatorId: string | null;
};
type Launch = {
  launchId: string;
  platform: string;
  protonCompatible: boolean;
  emulatorId: string | null;
};
type Emulator = {
  launchId: string;
  platform: string;
  protonCompatible: boolean;
  emulatorId: string | null;
  versionId: string;
  gameVersion: { gameId: string; onlySetup: boolean; game: { type: string } };
};

/** Validate the complete proposed set before changing any launch or game files. */
export function validateEmulatorBindings(
  launches: Launch[],
  bindings: Binding[],
  emulators: Emulator[],
  required: boolean,
): string | undefined {
  const updates = new Map(
    bindings.map((binding) => [binding.launchId, binding]),
  );
  if (
    updates.size !== bindings.length ||
    bindings.length !== launches.length ||
    launches.some((launch) => !updates.has(launch.launchId))
  )
    return "The launch options changed. Reopen the editor and try again.";
  const versions = new Map<string, string>();
  for (const launch of launches) {
    const binding = updates.get(launch.launchId)!;
    if (binding.previousEmulatorId !== launch.emulatorId)
      return "The emulator requirements changed. Reopen the editor and try again.";
    if (!binding.emulatorId) {
      if (required)
        return "Select an emulator version for every emulated game launch.";
      continue;
    }
    const emulator = emulators.find(
      (emulator) => emulator.launchId === binding.emulatorId,
    );
    if (
      !emulator ||
      emulator.gameVersion.game.type !== "Emulator" ||
      emulator.gameVersion.onlySetup ||
      emulator.emulatorId ||
      emulator.platform !== launch.platform
    )
      return "Select a portable emulator launch for the same platform.";
    if (
      launch.platform === "Windows" &&
      launch.protonCompatible &&
      !emulator.protonCompatible
    )
      return "Enable Proton compatibility on the selected emulator launch, or disable it for the game launch first.";
    const key = `${launch.platform}:${emulator.gameVersion.gameId}`;
    const version = versions.get(key);
    if (version && version !== emulator.versionId)
      return "Launch options for the same platform must use the same version of an emulator. Update all affected launch options before saving.";
    versions.set(key, emulator.versionId);
  }
}
