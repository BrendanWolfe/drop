# Desktop client activity

Updated servers and desktop clients provide three administrator views:

- `/admin/downloads`: live progress and speed (unchanged, in memory).
- `/admin/download-history`: saved download attempts, user/device names, game/build,
  queued/start/end times, byte counts, and completed/cancelled/failed outcomes.
- `/admin/errors`: automatically submitted download and uncaught frontend errors,
  with user/device, time, client version, message, and available stack details.

Download views require `downloads:read`; error reports require `errors:read`.
Both history views support search and pagination. Records persist across server
restarts and retain snapshot names if an account, device, or game is removed.

## Deployment

Deploy migration `20261113000000_client_activity` along with the updated server,
and rebuild/update desktop clients. The normal server launcher applies pending
migrations before starting. For a manual deployment, run `prisma migrate deploy`
using the server's configured database. Existing live snapshots and downloads
from old clients cannot be backfilled into history.

## Reporting behavior

Reporting is automatic: no consent dialog, setting, or error-submission button is
added. Changing the active download still requires confirmation.

Download workers emit explicit lifecycle events; a missing live heartbeat is
never treated as a successful download. A record with no end time shows its
last reported state, which may be left behind by a crash or lost connection.
Resuming a partially installed game after restarting creates a new attempt.
Local/external archive downloads are excluded from server download history.

The client retries through a bounded, on-disk outbox of 256 events. Reporting
failures never fail a download. Retries are idempotent, and pending records are
not sent to a different server or device identity after sign-out/reconfiguration.
Older servers without the reporting endpoints are tolerated.

Error reports strip credentials, URL queries, and the local home-directory path
before submission; the server also redacts common credentials and signed URLs.
Reports contain error text and available stacks, not full logs, screenshots, or
file contents. Frontend reports are deduplicated/rate-limited; the server accepts
at most 30 new error reports per device per minute. Reports and download history
are retained in the database without automatic expiry.
