# Portable emulators

## Administration

1. Put the complete portable emulator ZIP in a filesystem library, for example
   `Library/Dolphin/5.0.zip`. Archives use the same import/depot pipeline as games;
   there is no separate browser archive uploader.
2. Open **Admin → Emulators → Add emulator**, select its library folder, and import
   it as an **Emulator**. Import each version and add platform-specific launches.
3. Choose **CLI** to start a ROM directly, or **UI** to open the emulator's game
   browser. The command is relative to the extracted emulator install directory.
4. Import the ROM/game normally, checking **This game requires an emulator**.
   When importing its version, select the emulator, exact version and launch.
   Set the game launch path to the ROM's relative filename, or `.` for a whole
   game directory. The game stays a normal, discoverable store entry.

Example CLI command (adapt the executable/flags to your emulator):

```text
emulator.exe --fullscreen --run "{rom}"
```

### UI-first configuration

For an emulator accepting `--config`, configure this UI launch command:

```text
emulator.exe --config "{configDir}/settings.json"
```

Add a template named `settings.json`, using the emulator's **actual** schema:

```text
{"gameDirectories": [{gameDir:json}]}
```

This is a template, not valid JSON until Drop substitutes the path. A `:json`
placeholder emits a complete quoted JSON string, including Windows path escaping.
For INI/text templates, use raw `{gameDir}` and the emulator's expected syntax.
These generic examples are not universal emulator configuration formats.

Supported placeholders in emulator commands and templates:

| Placeholder     | Value                                                               |
| --------------- | ------------------------------------------------------------------- |
| `{rom}`         | Absolute game file or folder selected by the game launch            |
| `{gameDir}`     | Absolute game installation directory                                |
| `{emulatorDir}` | Absolute emulator installation directory                            |
| `{configDir}`   | Private per-game, per-emulator-version, per-launch config directory |

In templates, each placeholder also supports `:json`. Quote path placeholders as
one command argument. Drop substitutes already-parsed arguments, never shell code.
UI launches must not contain `{rom}`; CLI profiles must contain it.

Configuration files are created once under
`<game>/.drop-emulator/<emulator-version>/<launch>/`. Existing files are preserved,
including user edits and settings saved by the emulator. Remove a generated file
to recreate it from its template on the next launch. Use the emulator's config
flag or environment variable to direct it to this directory. Drop does not write
to system/user-wide settings or silently patch arbitrary existing config files.
Emulators without a config override can still use a preconfigured portable ZIP
or a UI command that accepts a game directory. BIOS, firmware and keys are not
downloaded automatically; supply legally obtained files where required.

## Desktop behavior

Linux emulator launches must contain native Linux executables. To offer a Windows
emulator through Proton, enable **Allow on Linux using Proton** on both its Windows
launch and the game's referencing Windows launch. See [Native builds and Proton](PLATFORMS.md).

The normal install flow queues required emulator versions automatically, with
automatic updates **disabled** for the emulator so the game's version pin cannot
silently change. Launch verifies the installed version/platform and requests the
required dependency if missing or different. Close the emulator to stop tracking
the game's play session. Prefer non-single-instance/non-detaching launch modes.

Emulator builds install side by side, keyed by emulator, version and platform.
Each game uses its exact required build; there is no global active version to
switch. Complete installed builds are reused, so only missing dependencies are
downloaded. Existing legacy `{rom}` commands continue working without a profile.

Choose separate game and emulator install locations/defaults in **Settings →
Downloads**. New emulator folders are directly inside the selected location,
with lowercase, hyphenated names such as `dolphin-5.0`. Name collisions receive
numeric suffixes. Existing installs remain in place, including older hidden
folders, and can still be resumed and uninstalled.

The **Emulators** library shows each installed version and the games that rely on
it. Removing a build requires confirmation; affected games offer **Update** to
restore only their missing emulator dependencies before playing. Admins can edit
a game version's emulator requirements without re-importing its game files.

Apply the server database migration before running the updated server, and ship
the updated desktop client to use UI-first profiles and generated templates.

## Releases and development builds

Stable desktop installers are available from the repository's latest stable
release. Pair them with `ghcr.io/brendanwolfe/drop:latest` for the stable server.
Back up your server database before upgrading; startup applies migrations
automatically. Desktop client update settings show the installed version,
manual update check, automatic-update preference and update timestamps.

Install the desktop prerelease manually; development releases intentionally have
no automatic-update manifest. Run the matching development server image
`ghcr.io/brendanwolfe/drop:develop` to access the emulator admin section and profile
fields. Stable `main` installations do not receive development builds
automatically.
