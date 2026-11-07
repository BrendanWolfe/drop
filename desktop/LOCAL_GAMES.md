# Local games, archive downloads, and download monitoring

## Desktop

Use **Library → Add game** to select an existing executable/script or paste a
direct archive download URL. Local games stay on this device and work offline
after the normal Drop client setup. No standalone/no-server setup mode is added.

Existing folders are never copied or deleted by Drop. Set the launch file,
arguments, game folder, working directory, and target platform separately.
Windows games can use the existing Proton support on Linux. Use **Launch
settings** for compatibility/handler overrides. Local playtime stays local;
server updates and cloud saves are not offered.

The first local game enables a new desktop database format tag to prevent older
clients from treating imported folders as orphaned server installations. Do not
downgrade a desktop client with local games without first backing up its
`drop.db`; older clients cannot read this format. Old database files migrate
automatically when opening the updated client.

Archive downloads support ZIP, 7z, RAR4, RAR5 (including solid archives), tar,
tar.gz, tar.bz2, and tar.xz through libarchive. Use direct HTTP/HTTPS file links,
not a host's web download page. Password-protected archives, multipart downloads,
and archive links/special files are not supported. Files extract directly into a
new Drop-owned game folder, without a staging directory. Nothing executes until
you choose a launcher and press Play.

Downloads share Drop's queue and speed limit. An interrupted job remains in the
local library; choose **Resume download** or **Retry extraction**. Byte-range
resume requires an ETag or Last-Modified validator and a cooperating host;
otherwise Drop restarts the transfer. Failed extractions keep the completed
archive for retry; successful extraction removes the cached archive.

Removing a downloaded game keeps its installed files unless you explicitly
select the delete-files checkbox. Deletion requires the folder's ownership
marker. Removing an existing-folder import can never delete its files.

External HTTP requests use a separate client with no Drop authorization tokens,
client certificates, cookies, or proxy-auth/custom headers. Those credentials
are not inherited on redirects, even if the URL points to the Drop server.
Signed download URLs are stored locally to support resume, not sent as admin
telemetry or included in network error logs.

## Administrator downloads page

**Admin → Downloads** lists server-hosted game downloads, with the game title,
authenticated user/device, progress, status, and reported speed. It requires
administrator access or the `downloads:read` system API permission. External
downloads and imported game names are never reported.

Updated clients send best-effort snapshots approximately every two seconds.
Reporting failures never stop downloads. Older clients do not appear. Sessions
become disconnected after 15 seconds without a heartbeat and disappear after
60 seconds. There is no history, remote download control, or authoritative
server-bandwidth accounting.

The active registry is in memory and intended for a single server process,
matching the existing real-time services. Multiple server replicas require a
shared registry/pub-sub implementation. Server restarts recover active cards on
the next client heartbeat.

## Native build dependencies

RAR5 requires **libarchive 3.4 or newer** (a current patched release is recommended).
Linux builds need `pkg-config` and `libarchive-dev` alongside Tauri's GTK/WebKit
dependencies. The deb/rpm packages declare the libarchive runtime dependency.
The RPM dependency is unversioned because Tauri treats each entry as a literal
dependency name rather than parsing version constraints. RPM installations still
need libarchive 3.4 or newer for RAR5 support.

For Windows/MSVC, install the static libraries through vcpkg and set these
environment variables before building (release CI does this automatically):

```powershell
vcpkg install "libarchive[core,bzip2,lz4,lzma,zstd]:x64-windows-static-md"
$env:VCPKGRS_TRIPLET = "x64-windows-static-md"
$env:LIBARCHIVE_STATIC = "1"
$env:LIBARCHIVE_LIB_DIR = "$env:VCPKG_ROOT/installed/x64-windows-static-md/lib"
$env:LIBARCHIVE_INCLUDE_DIR = "$env:VCPKG_ROOT/installed/x64-windows-static-md/include"
```

On macOS, install a current `libarchive` with Homebrew and add its
`lib/pkgconfig` directory to `PKG_CONFIG_PATH` when building.
