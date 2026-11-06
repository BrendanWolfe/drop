export function canStartStoreDownload(
  status: { type: string } | undefined,
  versions: readonly { protonSetupRequired?: boolean }[] | undefined,
  installDirs: readonly string[] | undefined,
): boolean {
  return (
    status?.type === "Remote" &&
    !!versions?.length &&
    !versions[0]?.protonSetupRequired &&
    installDirs?.length === 1
  );
}
