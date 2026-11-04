# Reverse proxy authentication

In desktop setup, or **Settings → Server**, enable **Reverse Proxy Authentication**.
Enter two custom header names and their token values. The defaults work with Pangolin:

- `P-Access-Token-Id`
- `P-Access-Token`

These headers are independent of Drop's login. The desktop client still sends its normal
JWT/Bearer `Authorization` headers. Drop's server needs no changes. Authentication,
cookie, routing, and transport header names cannot be used as custom token headers.

## Storage and recovery

**Remember tokens** uses Windows Credential Manager, macOS Keychain, or the Linux
keyring/Secret Service backend. Only the origin and header names go in `drop.db`.
Saved secrets are never returned to the frontend: blank fields retain saved values;
changing a header name requires a new value. Disabling authentication removes saved
credentials for that origin when storage is available. Signing out of Drop retains the
server's proxy settings.

If OS credential storage is locked or unavailable, unlock it or uncheck **Remember
tokens** for explicit session-only use. There is no plaintext fallback. A failed load
of remembered credentials blocks requests to that server rather than sending requests
without the configured tokens. Re-enter tokens through setup's **Switch instance**
link if startup authentication fails. Disabling authentication in Settings does not
require a working server connection.

Session-only recovery and disabling authentication also work when old credentials cannot
be deleted because the vault is locked. The old record is immediately removed from the
enabled configuration and is never reused. A separate cleanup marker (origin only) is
persisted, and the UI explicitly reports the deferred deletion. Unlock credential storage
and save again, or restart Drop, to retry. Saving new remembered credentials replaces the
old record and cancels its cleanup marker.

## Network boundary

- Tokens are sent only to the active server's exact origin (scheme, host, and effective
  port), including sign-in, native APIs, background work, Store proxying, metadata,
  same-origin downloads, and WebSocket handshakes.
- The built-in depot is advertised as `/api/v1/depot/` and resolved against the server
  address entered in desktop setup, not the server's `EXTERNAL_URL`. LAN connections
  download over the LAN; public connections download through the public reverse proxy.
  Manifest, speed-test, and chunk requests all use that resolved endpoint. Absolute
  URLs remain supported for external depots. Both server and desktop must be updated;
  older desktop versions cannot resolve the new relative built-in endpoint.
- External depot/download hosts do not receive these tokens. Each separately protected
  depot must be made accessible independently; the desktop client never guesses that
  another host is trusted.
- Redirect destinations are evaluated at every hop, including external download URLs
  redirecting into the protected server. External hops receive no proxy tokens; tokens
  are attached before the first request to the protected origin. Standard Drop
  Authorization/cookie headers are stripped on cross-origin hops, as with reqwest.
- Same-origin redirects work. **Cross-origin redirects from requests carrying proxy
  tokens are blocked**, including HTTP/HTTPS changes and port changes. Use the final
  server address rather than an address that redirects to another origin. Unconfigured
  connections retain reqwest's normal redirect behavior.
- Server addresses cannot contain URL credentials, queries, or fragments. Use HTTPS:
  custom headers over HTTP are not encrypted.
- OS-browser sign-in is external to the desktop transport. The browser must have its
  own access to the protected server; tokens are never embedded in its login URL.
- HTTP 401/403 responses with proxy authentication configured produce a fixed access
  error without exposing response bodies or token values. Check both proxy tokens and
  Drop account permissions; the desktop cannot reliably identify which layer denied access.
- All log records (file and console), crash reports, and native error Display/Debug and
  serialization paths redact configured token values, including tokens reflected by
  non-authentication failures such as HTTP 400/500. Raw, JSON/debug-escaped, URL-encoded,
  and HTML-escaped representations are covered. Redaction patterns remain in memory for
  the process lifetime so replaced tokens and failed probes cannot leak through late errors.

The backend stores headers as a validated list (up to 16), allowing additional header
rows to be exposed in the UI later without changing request injection or storage.

## Validation

From `desktop/src-tauri`, run `cargo test -p proxy_auth` for local HTTP transport,
WebSocket upgrade, origin/redirect isolation, redaction, auth compatibility, and failure
tests. Run `cargo test -p database` for database backward compatibility. Linux builds
need the normal desktop development libraries, including D-Bus, GTK, and WebKitGTK.
From `desktop/main`, run `pnpm test` and `pnpm build`.
