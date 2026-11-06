# Native builds and Proton

A launch's platform describes its files, not the computer downloading them:

| Build                    | Windows desktop | Linux desktop          |
| ------------------------ | --------------- | ---------------------- |
| Linux                    | Not offered     | Native Linux           |
| Windows, Proton disabled | Native Windows  | Not offered            |
| Windows, Proton enabled  | Native Windows  | Windows through Proton |

## Administration

Use **Linux** only for a native Linux executable. Do not add a Linux launch
pointing to a Windows executable to make it available through Proton.

For Windows launches, enable **Allow on Linux using Proton** during import or in
**Admin → Library → Game → Versions**. Existing imported launch/setup cards save
this setting without reimporting the archive. Setup-only versions have the same
setting on their Windows setup command. Bulk imports default to Proton disabled;
enable it on the relevant Windows commands afterward.

This is permission to try Proton, not a verified compatibility rating. Native
Linux builds ignore this flag. Windows emulator launches and the game launches
referencing them must both allow Proton to be offered on Linux.

The migration adds opt-in fields without relabelling or deleting any existing
builds. Review incorrectly Linux-labelled Windows executables separately and
correct those imports. Already installed legacy launches keep their previous
local launch behavior until their metadata is refreshed; explicit server flags
take precedence in newly fetched metadata. New download options from servers
without the eligibility field are not assumed to support Proton.

## Desktop selection

The install dialog separates **Build** from **Version**, with labels such as
**Linux — Native** and **Windows — Proton**. New installations prefer a native
build when one exists. Existing installations keep their selected platform;
switching to another build requires an explicit selection.

**Latest** enables update notifications for the selected build platform only.
Pinned versions do not receive those notifications. Updates do not silently
switch native Linux to Windows/Proton or the reverse, and retain the current
per-game Proton/launcher overrides when staying on the same platform. If that
platform is no longer offered, the client does not substitute a different one.

Eligible Windows downloads remain visible on Linux when the runtime is missing.
The dialog shows **Proton setup required**, allows downloading, and directs the
user to **Settings → Compatibility**. Install `umu-run` and select a valid Proton
installation before playing. Store quick-download actions pause at this warning
instead of skipping it.

Emulator dependency recovery matches both the pinned version and package platform,
so a Windows/Proton game cannot accidentally install a native Linux emulator.
