export type SaveLocation = {
  id: string;
  platform: "Windows" | "Linux" | "macOS";
  path: string;
  type: "file" | "directory";
  exclude: string[];
};

export class CloudSaveValidationError extends Error {}

function invalid(message: string): never {
  throw new CloudSaveValidationError(message);
}

export const MAX_ARCHIVE_BYTES = 512 * 1024 * 1024;
export const MAX_EXPANDED_BYTES = 1024 * 1024 * 1024;
export const MAX_SAVE_FILES = 20_000;
const identifier = /^[a-zA-Z0-9_-]{1,40}$/;
const hash = /^[a-f0-9]{64}$/;
const uuid = /^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$/i;
// Reject control characters in administrator-provided paths and patterns.
// eslint-disable-next-line no-control-regex
const unsafePathCharacters = /[\\\x00-\x1f*?]/;
// eslint-disable-next-line no-control-regex
const unsafePatternCharacters = /[\\\x00-\x1f<>:]/;
const placeholders = new Set([
  "<base>",
  "<home>",
  "<winAppData>",
  "<winLocalAppData>",
  "<winLocalAppDataLow>",
  "<winDocuments>",
  "<xdgData>",
  "<xdgConfig>",
]);

export function parseLocations(value: unknown): SaveLocation[] {
  if (!Array.isArray(value) || value.length > 16)
    invalid("Configure at most 16 save locations.");
  const ids = new Set<string>();
  return value.map((entry) => {
    if (!entry || typeof entry !== "object" || Array.isArray(entry))
      invalid("Invalid save location.");
    const { id, platform, path, type, exclude = [] } = entry;
    if (typeof id !== "string" || !identifier.test(id) || ids.has(id))
      invalid(
        "Location IDs must be unique letters, numbers, underscores or hyphens (1–40 characters).",
      );
    ids.add(id);
    if (!["Windows", "Linux", "macOS"].includes(platform))
      invalid("Invalid platform.");
    if (type !== "file" && type !== "directory")
      invalid("Choose a file or directory.");
    if (
      typeof path !== "string" ||
      path.length > 1024 ||
      unsafePathCharacters.test(path)
    )
      invalid(
        "Use forward slashes and a supported placeholder, without wildcards.",
      );
    const [root, ...parts] = path.split("/");
    if (
      !placeholders.has(root!) ||
      !parts.length ||
      parts.some((p) => !p || p === "." || p === ".." || /[<>:]/.test(p))
    )
      invalid(
        "Save paths must identify a game-specific child of a supported placeholder.",
      );
    if (platform !== "Windows" && root!.startsWith("<win"))
      invalid("Windows placeholders require a Windows save location.");
    if (platform !== "Linux" && root!.startsWith("<xdg"))
      invalid("XDG placeholders require a Linux save location.");
    if (
      !Array.isArray(exclude) ||
      exclude.length > 32 ||
      exclude.some(
        (p) =>
          typeof p !== "string" ||
          !p ||
          p.length > 200 ||
          p.startsWith("/") ||
          /[[\]{}]/.test(p) ||
          p
            .split("/")
            .some(
              (part) =>
                !part ||
                part === "." ||
                part === ".." ||
                (part.includes("**") && part !== "**"),
            ) ||
          unsafePatternCharacters.test(p),
      )
    )
      invalid(
        "Use relative exclusion patterns with *, ? and whole-directory ** only.",
      );
    return { id, platform, path, type, exclude } as SaveLocation;
  });
}

export function parseConfiguration(value: unknown) {
  if (!value || typeof value !== "object")
    invalid("Invalid cloud save configuration.");
  const body = value as Record<string, unknown>;
  if (typeof body.enabled !== "boolean") invalid("enabled must be a boolean.");
  const locations = parseLocations(body.locations);
  if (body.enabled && !locations.length)
    invalid("Add a save location before enabling cloud saves.");
  if (
    !Number.isSafeInteger(body.revision) ||
    (body.revision as number) < 0 ||
    (body.revision as number) > 2_147_483_646
  )
    invalid("Invalid configuration revision.");
  return {
    enabled: body.enabled,
    locations,
    revision: body.revision as number,
  };
}

export function parseUploadHeaders(get: (name: string) => string | undefined) {
  const number = (key: string) => {
    const raw = get(key);
    if (!raw || !/^\d{1,10}$/.test(raw)) invalid(`Invalid ${key}.`);
    const n = Number(raw);
    if (n > 2_147_483_646) invalid(`Invalid ${key}.`);
    return n;
  };
  const id = get("x-snapshot-id") ?? "";
  const checksum = get("x-archive-sha256") ?? "";
  const contentHash = get("x-content-sha256") ?? "";
  const kind = get("x-capture-kind");
  if (!uuid.test(id) || !hash.test(checksum) || !hash.test(contentHash))
    invalid("Invalid snapshot identity or checksum.");
  if (kind !== "manual" && kind !== "background")
    invalid("Invalid capture kind.");
  return {
    id,
    checksum,
    contentHash,
    kind,
    expectedRevision: number("x-expected-revision"),
    definitionRevision: number("x-definition-revision"),
  };
}

export function parseNamespace(value: unknown): string {
  if (
    typeof value !== "string" ||
    !["Windows", "Linux", "macOS"].includes(value)
  )
    invalid("Invalid save namespace.");
  return value;
}

export function parseStorageLimits(value: unknown) {
  if (!value || typeof value !== "object") invalid("Invalid storage limits.");
  const { sizeLimitMiB, history } = value as Record<string, unknown>;
  if (
    typeof sizeLimitMiB !== "number" ||
    !Number.isInteger(sizeLimitMiB) ||
    sizeLimitMiB < 1 ||
    sizeLimitMiB > 512
  )
    invalid("Archive size must be an integer between 1 and 512 MiB.");
  if (
    typeof history !== "number" ||
    !Number.isInteger(history) ||
    history < 1 ||
    history > 20
  )
    invalid("History must be an integer between 1 and 20.");
  return { sizeLimitMiB, history };
}
