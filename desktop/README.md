# Desktop

The official desktop client for Drop.

See [Local games and archive downloads](LOCAL_GAMES.md) for usage, supported
formats, download-monitoring behavior, and native build dependencies.

See [Steam integration on Windows](STEAM_INTEGRATION.md) for non-Steam shortcuts,
launch preferences, safe updates, deferred cleanup and launch limitations.

## Releases

This fork builds desktop installers for Windows x64 and Linux x64/ARM64 only.
The release workflow does not build macOS, AUR, Arch Linux, or Flatpak packages.
Manually dispatched runs default to a distinct `v<VERSION>-build.<RUN_NUMBER>`
pre-release tag when no tag is supplied. Published releases retain their tag,
release notes, and pre-release flag.

## Desktop updates

Packaged clients check `BrendanWolfe/drop` on startup and every 30 minutes,
independently of the configured Drop server. The newest published release with a
`latest.json` asset is used (including build pre-releases); drafts and releases
without desktop update artifacts are ignored. Only the most recent 100 releases
are inspected. A failed check or download is retried after five minutes.

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
(for example, `v0.4.5-build.1` to `v0.4.5-build.2`). The installed tag is never
offered again, and lower application versions are rejected. Releases should not
be republished under an existing tag; publish a new tag for a new build.
