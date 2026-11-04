# Steam integration (Windows)

On an installed game's page choose **Add to Steam…**. This works for both Drop
server games and device-local games. Choose a Steam profile and, if necessary,
a launch option. Drop shows the resolved executable, working directory and
argument list before writing anything.

Exit Steam completely (including its tray icon) before adding, updating,
recreating or removing shortcuts. Drop never kills Steam. Use **Retry / refresh**
after exiting. Start Steam to see the shortcut in Library or Big Picture.

## What launches

Steam launches the game's executable directly. Drop still installs and updates
the game, but does not introduce a helper launcher. A fully installed Windows
game, a Windows host, an existing `.exe`, and an existing working directory are
required. Nested executable directories and explicit local-game working
directories use the same resolution code as ordinary Drop launches.

Batch files, PowerShell/scripts, known interpreter executables, emulator-based
launch configurations, environment assignments, compatibility handlers and
wrapper/replacement templates are refused with an explanation. Templates
starting with `{}` that only append arguments are supported. Configuration
strings are parsed with Drop's existing rules, then the resulting argument list
is encoded with Windows argument quoting for Steam. No executable scanning or
first-`.exe` guessing is performed.

Overlay and per-game Steam Input are game-dependent. No global Steam settings
are changed, and controller layouts are not automatically configured. This does
not grant Steam ownership, achievements, Steam Cloud or other Steamworks features.

## Launch route and limitations

Use **Steam options… → Play through Steam**, or enable **Always launch through
Steam** there or in the game's launch settings. Normal Play then requests the
registered Steam launch option. **Play directly with Drop** remains available.

Drop verifies the registration, its exported configuration and the selected
Steam account before handing off `steam://rungameid/<GameID>` to that Steam
installation. With Steam stopped, its most recently used account must match;
with Steam running, the active Windows Steam account and installation must
match. If the active account cannot be determined, sign in to the intended
account and retry. Drop never switches accounts, silently copies shortcuts, or
silently falls back to a direct launch.

“Launch requested in Steam” confirms only handoff. Drop does **not** mark these
games running, show Stop, prevent duplicate Steam launches, or record playtime.
Errors may need troubleshooting in Steam. Drop's direct-launch/exit lifecycle
is bypassed, including any launch-bound save synchronization. The current process
manager's playtime reports, short-launch error detection, process-tree stopping,
logs and setup-completion handling do not run. Setup-required games cannot be
exported. Existing checks for games launched directly by Drop remain in place.

## Updates, moves and cleanup

Changed executables, arguments, working directories or launch settings mark the
registration as needing an update. Close Steam and explicitly update it. Its
AppID/GameID stay unchanged to retain shortcut-specific mappings and artwork.
If Steam's shortcut is missing, Drop offers explicit recreation; it never
automatically recreates deliberately deleted entries.

When uninstalling, choose whether to remove the shortcut too. If Steam is open,
keep the shortcut and continue; game uninstall is not blocked. Deferred entries
and entries from a different configured Drop server remain in **Settings →
Steam** for later cleanup. Registrations are device-local and source-scoped.
Remove a registration before exporting that game to a different Steam profile.

## File safety and ownership

Steam's binary `userdata/<accountID>/config/shortcuts.vdf` is a private client
format, not a supported Steam management API. Drop preserves unrelated entries,
unknown supported fields and scalar bytes. Unsupported, malformed, truncated,
zero-length or ambiguous files are refused, not replaced with an empty library.
An absent file and a valid empty shortcuts object are supported.

Each shortcut receives a `Drop` tag and a unique `Drop:<token>` ownership tag.
The visible `Drop` tag/name is **not** ownership evidence. Mutation requires the
local registration's AppID and unique token to match; removing that token in
Steam prevents Drop from editing/deleting the entry. In that case, remove the
shortcut manually in Steam. Manual name matches are never adopted. Steam-edited
names, icons, overlay preferences, tags and unrelated settings are preserved
where possible during ordinary updates.

Existing files get unique `shortcuts.vdf.drop-<uuid>.bak` backups in the same
directory before replacement. Drop serializes its own operations with a sibling
file lock, writes and flushes a same-directory temporary file, checks Steam again,
compares the latest source bytes and atomically replaces the destination. Only
after success is the desktop registration updated. Steam does not honor Drop's
lock: keep it closed throughout the operation; no application can eliminate the
race with another program starting immediately after the final check. Back up
both `shortcuts.vdf` and Drop's `drop.db` before testing. A crash between the
shortcut write and registration persistence can leave an unregistered shortcut;
Drop will not claim or delete that entry automatically.

## Validation

Automated tests cover binary round-trips, unknown fields, malformed/empty files,
ownership and collision checks, stable updates, preservation of user edits,
GameID precision, Windows argument quoting, backups, refused writes, concurrent
source changes, Steam-running guards, eligibility, working-directory resolution,
and database migration/persistence. Run from `desktop/src-tauri`:

```sh
cargo test -p steam_shortcuts -p process -p database
```

The desktop release workflow runs the shortcut-file safety suite on its native
Windows and Linux runners before building installers.

Manual Windows validation remains necessary: Steam open/closed and initially
stopped, multiple profiles, nonstandard install roots, local/server games,
nested paths/spaces/Unicode/arguments, spawning launchers, overlay and Steam
Input independently, update/move, remove/recreate, uninstall/deferred cleanup,
unsupported configurations and ordinary Drop launches. The initial manual
go/no-go checkpoint was explicitly waived by the user for implementation.
