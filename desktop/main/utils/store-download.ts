export function canStartStoreDownload(
  status: { type: string } | undefined,
  versions: readonly unknown[] | undefined,
  installDirs: readonly string[] | undefined,
): boolean {
  return (
    status?.type === "Remote" && !!versions?.length && installDirs?.length === 1
  );
}
