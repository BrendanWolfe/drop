export function canStartStoreDownload(
  status: { type: string } | undefined,
  versions:
    | readonly {
        protonSetupRequired?: boolean;
        requiredContent?: readonly unknown[];
      }[]
    | undefined,
  installDirs: readonly string[] | undefined,
  defaultIndex?: number | null,
): boolean {
  return (
    status?.type === "Remote" &&
    !!versions?.length &&
    !versions[0]?.protonSetupRequired &&
    !versions[0]?.requiredContent?.length &&
    (installDirs?.length === 1 ||
      (defaultIndex != null &&
        Number.isInteger(defaultIndex) &&
        defaultIndex >= 0 &&
        !!installDirs?.[defaultIndex]))
  );
}
