import { type } from "arktype";

export const EmulatorProfile = type({
  mode: "'cli' | 'ui'",
  configFiles: type({ path: "string", content: "string" }).array(),
  "directOpenCommand?": "string",
});

export function validateDirectOpenCommand(command: string): string | undefined {
  if (/\{(?:rom|gameDir|configDir)(?::json)?\}/.test(command))
    return "Direct-open commands cannot use game or per-game configuration placeholders.";
}

// Apply the same cross-platform path rules on the server and desktop. Templates
// may only create files inside Drop's isolated configuration directory.
export function validConfigPath(path: string): boolean {
  return (
    path.length > 0 &&
    !/[\\:]/.test(path) &&
    !/\p{Cc}/u.test(path) &&
    path
      .split("/")
      .every((part) => part.length > 0 && part !== "." && part !== "..")
  );
}

export function validateEmulatorProfile(
  profile: typeof EmulatorProfile.infer,
  command: string,
): string | undefined {
  const directOpenError = validateDirectOpenCommand(
    profile.directOpenCommand ?? "",
  );
  if (directOpenError) return directOpenError;
  if (profile.mode === "cli" && !command.includes("{rom}"))
    return "CLI emulator commands must contain {rom}.";
  if (profile.mode === "ui" && command.includes("{rom}"))
    return "UI-first commands must not contain {rom}; use {gameDir} for the game folder.";
  if (profile.configFiles.length > 32)
    return "An emulator profile can contain at most 32 configuration files.";
  const paths = new Set<string>();
  for (const file of profile.configFiles) {
    if (!validConfigPath(file.path))
      return "Configuration paths must be relative paths without traversal.";
    const key = file.path.toLowerCase();
    if (paths.has(key)) return "Configuration file paths must be unique.";
    paths.add(key);
    if (Buffer.byteLength(file.content, "utf8") > 262144)
      return "Configuration templates must be smaller than 256 KiB.";
  }
  if (profile.configFiles.length && !command.includes("{configDir}"))
    return "Commands with configuration templates must pass {configDir} to the emulator.";
}
