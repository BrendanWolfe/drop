type Launch = {
  platform: string;
  protonCompatible?: boolean;
  emulator?: { platform: string; protonCompatible?: boolean } | null;
};

/** Permission to try Proton, not a claim that the game has been tested. */
export function allowsProton(launch: Launch): boolean {
  return (
    launch.platform === "Windows" &&
    launch.protonCompatible === true &&
    (!launch.emulator ||
      (launch.emulator.platform === "Windows" &&
        launch.emulator.protonCompatible === true))
  );
}

/** Installers must not advertise a native build when only Windows launches exist. */
export function downloadLaunches<L extends Launch, S extends Launch>(version: {
  onlySetup: boolean;
  launches: L[];
  setups: S[];
}): Array<L | S> {
  return version.onlySetup ? version.setups : version.launches;
}
