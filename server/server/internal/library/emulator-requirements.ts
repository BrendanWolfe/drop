type Version<P extends string> = {
  versionId: string;
  displayName: string | null;
  versionPath: string | null;
  launches: Array<{
    platform: P;
    emulator: {
      gameVersion: {
        versionId: string;
        displayName: string | null;
        versionPath: string | null;
        game: { id: string; mName: string };
      };
    } | null;
  }>;
};

/** Keep the game-version/platform association; launch options may share a pin. */
export function emulatorRequirements<P extends string>(versions: Version<P>[]) {
  return versions.flatMap((version) => {
    const seen = new Set<string>();
    return version.launches.flatMap((launch) => {
      const emulator = launch.emulator?.gameVersion;
      if (!emulator) return [];
      const key = `${emulator.versionId}:${launch.platform}`;
      if (seen.has(key)) return [];
      seen.add(key);
      return [
        {
          gameVersionId: version.versionId,
          gameVersionName:
            version.displayName || version.versionPath || version.versionId,
          emulatorId: emulator.game.id,
          emulatorName: emulator.game.mName,
          versionId: emulator.versionId,
          versionName:
            emulator.displayName || emulator.versionPath || emulator.versionId,
          platform: launch.platform,
        },
      ];
    });
  });
}
