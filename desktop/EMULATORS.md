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

The normal install flow queues required emulator versions automatically, with
automatic updates **disabled** for the emulator so the game's version pin cannot
silently change. Launch verifies the installed version/platform and requests the
required dependency if missing or different. Close the emulator to stop tracking
the game's play session. Prefer non-single-instance/non-detaching launch modes.

The current library supports one active installation per emulator. Games pinned
to different versions may request an emulator version switch; simultaneous
side-by-side installations of the same emulator are not yet supported. Existing
legacy `{rom}` emulator commands continue working without a profile.

Apply the server database migration before running the updated server, and ship
the updated desktop client to use UI-first profiles and generated templates.

## Testing development builds

Install the desktop prerelease manually; development releases intentionally have
no automatic-update manifest. Run the matching development server image
`ghcr.io/brendanwolfe/drop:develop` to access the emulator admin section and profile
fields. Back up your database before updating a test server. The server startup
applies the new emulator migration automatically. Stable `main` installations do
not receive this development build automatically.
