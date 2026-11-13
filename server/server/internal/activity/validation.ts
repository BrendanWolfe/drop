import { type } from "arktype";

export const DownloadActivityBody = type({
  sessionId: "0 < string <= 100",
  gameId: "0 < string <= 100",
  versionId: "string <= 100",
  platform: "string <= 40",
  state: "'Queued' | 'Downloading' | 'Completed' | 'Cancelled' | 'Failed'",
  occurredAt: "number.integer >= 0",
  bytesDownloaded: "number.integer >= 0",
  bytesTotal: "number.integer >= 0",
});

export const ErrorReportBody = type({
  reportId: "0 < string <= 100",
  source:
    "'download' | 'vue' | 'javascript' | 'unhandled-rejection' | 'client'",
  message: "0 < string <= 2000",
  details: "string <= 12000",
  "gameId?": "string <= 100",
  clientVersion: "string <= 50",
  occurredAt: "number.integer >= 0",
});

export function validTimestamp(value: number) {
  return (
    Number.isSafeInteger(value) && value >= 0 && value <= 8_640_000_000_000_000
  );
}

// A second boundary in addition to client-side redaction. Never store credentials
// or URL queries that may contain signed download tokens.
export function redactReport(value: string) {
  return value
    .replace(
      /\b(Bearer|JWT|Basic)\s+[^\s,;]+(?:\s+[^\s,;]+)?/gi,
      "$1 [redacted]",
    )
    .replace(/(https?:\/\/)[^\s/@]+:[^\s/@]+@/gi, "$1[redacted]@")
    .replace(/(https?:\/\/[^\s?#]+)[?#][^\s]*/gi, "$1?[redacted]")
    .replace(
      /\b(password|token|secret|authorization|cookie)\s*[:=]\s*[^\s,;]+/gi,
      "$1=[redacted]",
    );
}

export function historyPage(query: Record<string, unknown>) {
  const page = Number(query.page ?? 1);
  if (!Number.isSafeInteger(page) || page < 1 || page > 100_000)
    return undefined;
  return { skip: (page - 1) * 50, take: 50, page };
}
