import type { VersionGuess } from ".";

const redistDirectoryPattern =
  /^(?:_?(?:common)?redist(?:ributables?)?|vc_?redist)$/i;

const redistFilePatterns = [
  /^oalinst/i,
  /^vc_?redist/i,
  /^redist/i,
  /^dxwebsetup/i,
  /^dxsetup/i,
  /^directx/i,
  /^dotnetfx/i,
];

const installerNamePatterns = [
  /^(?:install|installer|uninstall|uninstaller|unins\d*)(?:[._-].*)?$/i,
  /(?:install|installer|uninstall|uninstaller|unins\d*)$/i,
  /^setup(?:\d+|[._ -].*)?$/i,
  /[._ -]setup(?:[._ -].*)?$/i,
  /^installshield/i,
];

const crashToolPatterns = [
  /^(?:crashreport(?:client|er)?|crashhandler|crashsender|crashpad|unitycrashhandler\d*|werfault|bugreport(?:er)?|errorreport(?:er)?|bugsplat|crashrpt)(?:[._-].*)?$/i,
];

const penalizedTerms = [
  "launcher",
  "compat",
  "editor",
  "bench",
  "helper",
  "update",
];

function normalizeFilename(filename: string) {
  // Preload filenames are shell-escaped; escaped spaces are not path separators.
  return filename
    .replace(/\\([^\w/\\])/g, "$1")
    .replace(/\\/g, "/")
    .replace(/^["']|["']$/g, "");
}

function scoreCandidate(
  candidate: Pick<VersionGuess, "filename" | "match">,
  setupMode: boolean,
) {
  const filename = normalizeFilename(candidate.filename);
  const segments = filename.split("/").filter(Boolean);
  const basename = segments.at(-1) ?? filename;
  const name = basename.replace(/\.[^.]+$/, "").toLowerCase();
  const directories = segments
    .slice(0, -1)
    .map((segment) => segment.toLowerCase());

  const isInRedistDirectory = directories.some((directory) =>
    redistDirectoryPattern.test(directory),
  );
  const isRedistFile = redistFilePatterns.some((pattern) => pattern.test(name));
  const isInstaller = installerNamePatterns.some((pattern) =>
    pattern.test(name),
  );
  const isCrashTool = crashToolPatterns.some((pattern) => pattern.test(name));

  // Installers are only useful when importing in setup mode.
  if (
    isInRedistDirectory ||
    isRedistFile ||
    isCrashTool ||
    (!setupMode && isInstaller)
  )
    return undefined;

  let score = (Number.isFinite(candidate.match) ? candidate.match : 0) * 150;

  if (setupMode && isInstaller) score += 200;

  for (const term of penalizedTerms) {
    if (name.includes(term)) score -= 50;
  }

  const depth = Math.max(0, segments.length - 1);
  score -= depth * 12;
  score -= Math.min(filename.length, 160) * 0.15;

  return score;
}

/**
 * Ranks auto-discovered executables so mass imports prefer the game binary.
 * Tooling candidates are excluded and no fallback is returned when none remain.
 */
export function rankPreloadCandidates(
  candidates: Array<VersionGuess>,
  options: { setupMode?: boolean } = {},
) {
  const setupMode = options.setupMode ?? false;

  return candidates
    .map((candidate, index) => ({
      candidate,
      index,
      score: scoreCandidate(candidate, setupMode),
    }))
    .filter(
      (
        entry,
      ): entry is {
        candidate: VersionGuess;
        index: number;
        score: number;
      } => entry.score !== undefined,
    )
    .sort((a, b) => b.score - a.score || a.index - b.index)
    .map((entry) => entry.candidate);
}
