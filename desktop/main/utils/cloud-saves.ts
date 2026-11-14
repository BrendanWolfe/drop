export type CloudSnapshot = {
  id: string;
  revision: number;
  definitionRevision: number;
  clientId: string;
  kind: "manual" | "background";
  checksum: string;
  contentHash: string;
  bytes: number;
  createdAt: string;
};

export type CloudSaveStatus = {
  enabled: boolean;
  phase: string;
  message: string;
  namespace: string;
  revision: number;
  definitionRevision: number;
  snapshots: CloudSnapshot[];
  locations: string[];
  recoveryDirectory: string | null;
  interruptedRestore: boolean;
  recoveries: string[];
};

export function snapshotLabel(snapshot: CloudSnapshot): string {
  const time = new Date(snapshot.createdAt).toLocaleString();
  const kind =
    snapshot.kind === "manual"
      ? "Confirmed closed"
      : "Background recovery (game may have been running)";
  return `Revision ${snapshot.revision} · ${time} · ${kind}`;
}

export function cloudActionNeedsChoice(
  status: CloudSaveStatus | null,
): boolean {
  return status?.phase === "conflict";
}
