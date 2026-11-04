# Desktop

The official desktop client for Drop.

See [Local games and archive downloads](LOCAL_GAMES.md) for usage, supported
formats, download-monitoring behavior, and native build dependencies.

## Releases

This fork builds desktop installers for Windows x64 and Linux x64/ARM64 only.
The release workflow does not build macOS, AUR, Arch Linux, or Flatpak packages.
Manually dispatched runs default to a distinct `v<VERSION>-build.<RUN_NUMBER>`
pre-release tag when no tag is supplied. Published releases retain their tag,
release notes, and pre-release flag.
