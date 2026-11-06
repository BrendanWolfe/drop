# Desktop

The official desktop client for Drop.

See [Local games and archive downloads](LOCAL_GAMES.md) for usage, supported
formats, download-monitoring behavior, and native build dependencies.

See [Steam integration on Windows](STEAM_INTEGRATION.md) for non-Steam shortcuts,
launch preferences, safe updates, deferred cleanup and launch limitations.

## Releases

This fork builds desktop installers for Windows x64 and Linux x64/ARM64 only.
The release workflow does not build macOS, AUR, Arch Linux, or Flatpak packages.
Pushes to `develop` publish testing prereleases (`v<VERSION>-dev.<RUN_NUMBER>`).
Pushes to `main` publish stable releases (`v<VERSION>+build.<RUN_NUMBER>`).
Both branches also support manual workflow runs with an optional custom tag.
Other branches and tag-based manual runs cannot publish releases.

Automatic releases stay drafts until all desktop platforms finish successfully.
Only stable `main` releases publish `latest.json` and become GitHub's latest
release. Development prereleases have downloadable installers, but no automatic
update manifest. Tags are pinned to the actual build commit before release
creation, and release metadata records the originating branch.

Existing published releases can trigger builds too, but must target `main`
(stable) or `develop` (prerelease), and their tagged commit must belong to that
branch. Existing published tags cannot be reused for a new manual/push build.
See [the repository release process](../RELEASING.md) for promotion instructions.

Development installers must be downloaded and installed manually. Emulator
testing also needs the matching development server; see [portable emulators](EMULATORS.md).

## Desktop updates

Packaged clients check `BrendanWolfe/drop` on startup and every 30 minutes,
independently of the configured Drop server. GitHub's latest stable release with a
`latest.json` asset is used only when it is **not** a prerelease and its GitHub
target branch is **main**. Drafts, development builds, legacy SHA-targeted
releases, and releases without desktop update artifacts are ignored. If GitHub's
latest release is not eligible, the most recent 100 releases are inspected for a
stable main desktop release. A failed check or download is retried after five minutes.

The `v0.4.5+updater.1` bridge release changes only desktop update selection and
the release plumbing needed to ship it. Existing signed-update clients accept
its new tag at the same app version and then follow stable main releases only.

Updates download automatically and are signature-verified before the blue
**Update and restart** button appears immediately left of Downloads. Nothing is
installed until the user clicks it. Windows uses the NSIS installer; Linux uses
the installed DEB/RPM package format and may prompt for administrator permission
through `pkexec`. If installation fails or authorization is declined, Drop stays
open and the same button retries the cached update. The cache is temporary and
is downloaded again if Drop is closed before updating.

### One-time release signing setup

Generate a signing key outside this repository (from `desktop/`):

```sh
pnpm tauri signer generate -w "$HOME/.tauri/drop-updater.key"
```

Back up the key securely, then configure these GitHub Actions repository secrets:

| Secret                               | Value                                      |
| ------------------------------------ | ------------------------------------------ |
| `TAURI_SIGNING_PRIVATE_KEY`          | Contents of the generated private key file |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password selected when generating the key  |
| `DROP_UPDATER_PUBLIC_KEY`            | Contents of the generated `.pub` file      |

The release workflow embeds the public key and release tag in the client,
generates signed installers, and publishes their signatures and `latest.json`.
Never commit the private key. Keep using the same key for subsequent releases;
clients only trust the public key embedded when they were built. The workflow
fails explicitly if signing keys are not configured.

Install the first updater-enabled release manually. Older clients cannot acquire
this feature by themselves. Builds without `DROP_UPDATER_PUBLIC_KEY`, and debug
builds, disable update checks. Unpackaged Linux binaries also disable updates.
Local release packages that should receive updates must set
`DROP_UPDATER_PUBLIC_KEY` and `DROP_RELEASE_TAG` when compiling; the public
key is intentionally supplied at build time rather than downloaded from GitHub.

New release tags can update an existing build with the same application version
(for example, `v0.4.5+build.1` to `v0.4.5+build.2`). The installed tag is never
offered again, and lower application versions are rejected. Releases should not
be republished under an existing tag; publish a new tag for a new build.
