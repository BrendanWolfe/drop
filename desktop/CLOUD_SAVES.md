# Cloud saves

Cloud saves require **two explicit opt-ins**, both off by default:

1. An administrator enables a game under **Admin → Library → Game → Cloud Saves** and configures its save locations.
2. The player enables **Settings → General → Cloud saves** in the desktop client. This is one device-wide preference for all supported games, not a per-game setting.

Apply the server's database migrations (`pnpm exec prisma migrate deploy` from `server/`) before deploying this feature. Existing games and desktop settings remain opted out. Cloud saves require the game to be in the player's library and the client to have the CloudSaves capability; the desktop requests that capability after opt-in.

## Configuring a game

Each location has a stable ID, platform, file/directory type, and path. Directories include all subdirectories and empty folders. Use forward slashes, no wildcards in the root path, and a game-specific child of a supported placeholder:

| Placeholder | Meaning |
| --- | --- |
| `<base>` | Resolved game installation directory, including relocated installations |
| `<home>` | Current operating-system user's home |
| `<winAppData>` | Windows roaming application data |
| `<winLocalAppData>` | Windows local application data |
| `<winLocalAppDataLow>` | Windows LocalLow application data |
| `<winDocuments>` | Windows Documents |
| `<xdgData>` | Linux user data directory, respecting XDG configuration |
| `<xdgConfig>` | Linux user configuration directory, respecting XDG configuration |

Examples: `<winLocalAppData>/ExampleGame/Saves`, `<xdgData>/ExampleGame/progress.dat`.

Exclusions are relative globs, one per line: `Cache/**`, `Logs/**`, `**/*.lock`. Exclude credentials, caches and logs. Overlapping save roots, symbolic links, special files, unsafe filenames, traversal paths, and case-colliding filenames are rejected. Save paths must not include Drop's own cloud-save storage.

Windows, Linux and macOS have separate histories. Windows games run using Drop's Proton/UMU launcher use the Windows history and Drop's per-game prefix. External Steam/Wine prefixes are not guessed. Wine directories linked to another location, such as a symlinked Documents folder, are rejected rather than followed. Registry-based saves and cross-platform format conversion are not supported.

Keep location IDs stable when editing paths. Configuration changes pause automatic synchronization until reviewed. Explicit restores map the snapshot's stable IDs and types to the current configured paths; adding/removing IDs or changing types can make older snapshots incompatible. Review the displayed local paths before restoring an older configuration.

## Automatic snapshots and manual synchronization

While Drop is running, the native background task reconciles save directories every minute. Files are content-hashed, so same-size edits and clock differences do not hide changes. Two unchanged scans at least 60 seconds apart allow capture. A private staged copy is compared with a fresh scan before compression.

Automatic captures are labeled **Background recovery**. A game may still have been running: filesystem quietness and process exit are not guarantees of a consistent multi-file save. Launchers, Steam handoffs, and external launches therefore do not trigger automatic restores or claims that a game is closed. Drop must be running to observe external game saves.

For a confirmed snapshot, close the game and launcher, open its library page, confirm they are closed, then choose **Upload local saves**. To move to another device, close the game there, check cloud saves, and explicitly restore a snapshot before playing. Restores are never automatic in this first release.

If cloud data differs from this device's last synchronized revision, automatic publishing pauses. Choose **Use local saves** or restore a complete cloud snapshot. Files from conflicting snapshots are never automatically merged. First-time enablement with existing, different local and cloud saves also requires a choice. A changed cloud revision after review requires checking again.

When offline, previously fetched save definitions can be used to queue one immutable local snapshot per game/platform. Upload retries require a fresh server opt-in/access check and the original expected cloud revision. Queues and baselines are partitioned by server and client identity; switching accounts never sends another account's queue. A missing previously present save root pauses automatic capture, rather than interpreting an unplugged drive as deleted saves.

## Storage, limits and recovery

Snapshots are full streamed `tar.zst` archives, not incremental/deduplicated backups. The manifest contains relative paths, per-file SHA-256 checksums, game/platform identity and the definition revision; absolute device paths are not uploaded. The server verifies the archive checksum and conditionally advances the cloud revision only after upload completion. Failed uploads cannot replace a published revision.

- Compressed archive size uses the existing server **save-slot size limit** (default 10 MiB), capped at 512 MiB. Adjust size and retention in **Admin → Settings → General → Cloud save storage** for games with large save directories.
- Expanded snapshots are limited to 1 GiB and 20,000 files/directories, with bounded nesting and decompressor memory.
- Server history uses the **save-slot history limit** (default 3), bounded to 1–20 recent snapshots. The most recent confirmed/manual snapshot is retained additionally if necessary, so background captures cannot prune the last confirmed copy.
- Pruned objects and interrupted server uploads are durably queued for garbage collection. Subsequent save requests reclaim old queued objects after a one-hour grace period, including objects from deleted users/games.

Restoring verifies and stages the entire archive first, backs up existing local saves, then replaces individual files through same-directory temporary files. Only configured, non-excluded files are managed. Deleted slots are removed, excluded/unrelated files remain, and an incomplete snapshot is not merged with a populated local root it lacks. There is no portable atomic transaction across multiple save directories.

A durable restore journal pauses background synchronization after an interrupted restore. Use **Recover previous local backup** or retry the cloud restore after closing the game. Successful restores also leave a local backup accessible under **Local recovery backups**. There is room for 10 local recovery archives per game/platform; move older archives elsewhere when full. Recovery archives are not silently deleted.

Disabling either opt-in stops synchronization without deleting existing saves or backups. Saves are private to the account, but are stored on the Drop server without additional application-level encryption at rest; trust the server administrator and use HTTPS for remote access.
