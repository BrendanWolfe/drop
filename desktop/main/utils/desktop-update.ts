export type DesktopUpdatePhase =
  | "idle"
  | "checking"
  | "downloading"
  | "ready"
  | "installing"
  | "error"
  | "disabled";

export interface DesktopUpdateStatus {
  phase: DesktopUpdatePhase;
  tag: string | null;
  downloaded: number;
  total: number | null;
  error: string | null;
}

export function updateButtonLabel(status: DesktopUpdateStatus): string {
  switch (status.phase) {
    case "checking":
      return "Checking for updates";
    case "downloading": {
      if (!status.total || status.total <= 0) return "Downloading update";
      const percent = Math.min(
        100,
        Math.max(0, Math.floor((status.downloaded / status.total) * 100)),
      );
      return `Downloading update ${percent}%`;
    }
    case "ready":
      return status.error ? "Retry update and restart" : "Update and restart";
    case "installing":
      return "Installing update";
    case "error":
      return "Retry update download";
    default:
      return "Check for updates";
  }
}
