type BuildOption = { platform: string; runtime: "native" | "proton" };

export function buildLabel(option: BuildOption): string {
  return `${option.platform} — ${option.runtime === "proton" ? "Proton" : "Native"}`;
}

export function availableBuilds<T extends BuildOption>(
  options: readonly T[],
): T[] {
  return [
    ...new Map(options.map((option) => [option.platform, option])).values(),
  ];
}

export function versionsForBuild<T extends BuildOption>(
  options: readonly T[],
  platform: string,
): T[] {
  return options.filter((option) => option.platform === platform);
}

export function findRequiredBuild<
  T extends { versionId: string; platform: string },
>(
  options: readonly T[],
  required: { versionId: string; platform: string },
): T | undefined {
  return options.find(
    (option) =>
      option.versionId === required.versionId &&
      option.platform === required.platform,
  );
}
