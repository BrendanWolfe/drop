# Development and stable releases

| Branch    | Purpose                        | Desktop releases                      | Server images                                                    |
| --------- | ------------------------------ | ------------------------------------- | ---------------------------------------------------------------- |
| `develop` | Integration and testing        | Prereleases; manual installation only | `develop`, `nightly`, date/build-specific tags                   |
| `main`    | Tested, release-ready versions | Stable releases; automatic updates    | `main`, `latest`, stable version aliases and build-specific tags |

Both release workflows run on pushes to these branches and support manual runs.
Server nightly schedules explicitly check out `develop`, regardless of the
repository's default branch. Server CI checks pushes and pull requests to both
branches. Development images never move `latest` or stable major/minor aliases.

## Promoting a version

1. Develop and test changes on `develop`, using its prerelease installers and
   development server images.
2. Bump application versions when appropriate and open a pull request from
   `develop` into `main`. Require CI and review before merging. Configure branch
   protection in GitHub to prevent unreviewed direct pushes to `main`.
3. Merge the tested changes into `main`. The push builds stable desktop releases
   and stable server images. Automatic desktop releases are published only after
   all platform builds succeed; failed builds leave a draft, not an update.

Keep the existing branch histories. Do not reset or force-push `main` to promote
changes. When first enabling this process, merge the release workflow/script
changes into `main` too; an old `main` will still contain its old workflows.

## Tags and manual releases

Automatic desktop tags are `v<VERSION>-dev.<RUN_NUMBER>` on `develop` and
`v<VERSION>+build.<RUN_NUMBER>` on `main`. Server builds use their own workflow run
number. Docker normalizes unsupported tag characters (such as `+`) to `-`.
Stable major/minor version aliases are generated only for main builds.
Development image version tags are prefixed with `develop-`, even for custom
tags, so a development build named `v0.4.6` cannot overwrite the stable image.

To use a custom version tag, manually dispatch the desktop/server workflow on
the intended branch and set `tagName` (for example, `v0.4.6` on `main`). The
`__VERSION__` placeholder is supported. Always use a new tag for a new build;
the desktop workflow refuses tags pointing at a different commit or published
releases reused outside a release-published event.

For a release created through GitHub's release UI, explicitly set its target to
`main` and leave **prerelease** unchecked, or target `develop` and check
**prerelease**. A target SHA or mismatched prerelease flag is rejected, and the
tagged commit must be an ancestor of the selected branch. These workflows use
the existing desktop signing secrets; see [desktop signing](desktop/README.md#one-time-release-signing-setup).

## Automatic updates

The desktop checker only considers published, non-draft, non-prerelease releases
whose `target_commitish` is `main` and that contain `latest.json`. It never
selects a development release, even if its prerelease flag is accidentally unset.
Development builds omit the updater manifest as additional protection for older
clients whose checker did not filter prereleases.

Existing release assets/metadata are not changed by this configuration. Clients
with the old update policy need one updated stable build to acquire the stricter
checker. Until then, an already-published legacy development manifest can still
be visible to them. Do not relabel an untested legacy release as stable merely
to make it eligible for automatic updates.
