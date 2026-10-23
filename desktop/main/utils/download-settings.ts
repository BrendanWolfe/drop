export const BYTES_PER_MEGABYTE = 1_000_000;

export type SpeedLimitParseResult =
  | { valid: true; bytesPerSecond: number }
  | { valid: false; error: string };

export function formatDownloadSpeedMegabytes(bytesPerSecond: number): string {
  if (!Number.isFinite(bytesPerSecond) || bytesPerSecond <= 0) return "0";

  const megabytesPerSecond = bytesPerSecond / BYTES_PER_MEGABYTE;
  return Number(megabytesPerSecond.toFixed(6)).toString();
}

export function parseDownloadSpeedMegabytes(
  value: string | number,
): SpeedLimitParseResult {
  const rawValue = typeof value === "string" ? value.trim() : String(value);
  if (rawValue === "") {
    return { valid: false, error: "Enter a download speed limit." };
  }

  const megabytesPerSecond = Number(rawValue);
  if (!Number.isFinite(megabytesPerSecond)) {
    return { valid: false, error: "Enter a valid number of MB/s." };
  }
  if (megabytesPerSecond < 0) {
    return { valid: false, error: "Download speed cannot be negative." };
  }

  const bytesPerSecond = Math.round(megabytesPerSecond * BYTES_PER_MEGABYTE);
  if (megabytesPerSecond > 0 && bytesPerSecond === 0) {
    return {
      valid: false,
      error:
        "Download speed limit must be at least 0.000001 MB/s, or 0 for unlimited.",
    };
  }
  if (!Number.isSafeInteger(bytesPerSecond)) {
    return { valid: false, error: "Download speed limit is too large." };
  }

  return { valid: true, bytesPerSecond };
}
