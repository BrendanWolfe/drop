# How to create Flamegraph

Run this in `src-tauri`:

```
WEBKIT_DISABLE_DMABUF_RENDERER=1 CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph --release
```

You can leave out `WEBKIT_DISABLE_DMABUF_RENDERER=1` if you're not on NVIDIA/Linux

And then run this in the root dir:

```
yarn dev --port 1432
```

And then do what you want, and it'll create the flamegraph for you

# Linux Store Rendering Crashes

On Fedora 44 with WebKitGTK 2.54 and Mesa 26.1, Store rendering has caused
excessive memory use and compositor stalls. Check both the WebKit crash dump
and `journalctl -k` for OOM kills: a WebKit watchdog abort during shutdown may
be a consequence rather than the original failure.

The Store should avoid nested backdrop filters, especially inside carousel
slides, and only load further catalog pages when the document scrolls near
the bottom. These fixes are served by the Drop server, so updating only the
desktop package will not apply them.

To diagnose a remaining compositor stall with an installed desktop package,
fully quit Drop (including its tray icon), then launch:

```sh
WEBKIT_DISABLE_COMPOSITING_MODE=1 /usr/bin/drop-app
```

This changes WebKit's compositor path, not graphics settings for launched
games. It can change rendering performance and is a workaround, not proof
that excessive memory use is fixed. Do not disable WebKit's memory-pressure
monitor or sandbox to hide the crash.
