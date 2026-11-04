export type DownloadPhase = "Queued" | "Downloading" | "Paused" | "Validating";

export interface DownloadReport {
  sessionId: string;
  gameId: string;
  versionId: string;
  state: DownloadPhase;
  bytesDownloaded: number;
  bytesTotal: number;
  bytesPerSecond: number;
}

export interface ActiveDownload extends DownloadReport {
  clientId: string;
  userId: string;
  userName: string;
  deviceName: string;
  gameTitle: string;
  startedAt: number;
  updatedAt: number;
  stale: boolean;
}

interface ClientSnapshot {
  reporterId: string;
  sequence: number;
  updatedAt: number;
  downloads: ActiveDownload[];
}

/** Ephemeral, authoritative-per-client snapshots. Never write progress to SQL. */
export class DownloadRegistry {
  private clients = new Map<string, ClientSnapshot>();

  replace(
    clientId: string,
    reporterId: string,
    sequence: number,
    downloads: Omit<ActiveDownload, "startedAt" | "updatedAt" | "stale">[],
    now = Date.now(),
  ) {
    this.expire(now);
    const previous = this.clients.get(clientId);
    if (previous?.reporterId === reporterId && sequence <= previous.sequence)
      return;
    if (!previous && this.clients.size >= 10_000) return;
    const oldSessions = new Map(
      previous?.downloads.map((entry) => [entry.sessionId, entry]) ?? [],
    );
    this.clients.set(clientId, {
      reporterId,
      sequence,
      updatedAt: now,
      downloads: downloads.map((entry) => ({
        ...entry,
        startedAt: oldSessions.get(entry.sessionId)?.startedAt ?? now,
        updatedAt: now,
        stale: false,
      })),
    });
  }

  snapshot(now = Date.now()): ActiveDownload[] {
    this.expire(now);
    return [...this.clients.values()]
      .flatMap((client) =>
        client.downloads.map((entry) => ({
          ...entry,
          stale: now - entry.updatedAt >= 15_000,
          bytesPerSecond:
            now - entry.updatedAt >= 15_000 || entry.state !== "Downloading"
              ? 0
              : entry.bytesPerSecond,
        })),
      )
      .sort(
        (a, b) =>
          a.startedAt - b.startedAt || a.sessionId.localeCompare(b.sessionId),
      );
  }

  private expire(now: number) {
    for (const [id, client] of this.clients) {
      if (now - client.updatedAt >= 60_000) this.clients.delete(id);
    }
  }
}

export const downloadRegistry = new DownloadRegistry();
