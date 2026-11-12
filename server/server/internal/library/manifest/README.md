# Server-managed archive installs

When importing a local single-file archive, select **Download archive and extract
on client**. The game remains a normal server-library game, including its launch
and setup commands, permissions, queue, and version selection.

- Supported: single-file RAR4/RAR5 (including solid RAR), ZIP/ZIP64, 7z, and tar
  (including gzip, bzip2 and xz wrappers).
- Not supported: multipart or password-protected archives, links, special files,
  unsafe/reserved paths, or delta/update-mode overlays.
- Both the server/Torrential and Drop Desktop need this feature. Older desktop
  clients receive an update-required error rather than installing a raw payload.

## Storage and transfer

The manifest format is `archive-v1`. Import inspects archive headers once for the
extracted size and safety checks, then hashes fixed-size ranges of the original
compressed bytes. Some solid/filtered formats may need decompression during
header traversal; there is no repeated per-file lookup or server-side extraction.
No database migration or second server-side copy is required.

The original archive must remain unchanged and available at its imported library
path. Existing authenticated, encrypted depot chunk requests serve its byte
ranges directly. SHA-256 verification and chunk checkpoints allow interrupted
downloads to resume. The desktop re-verifies checkpointed ranges from its local
cache before trusting them.

The desktop stages the archive under `.drop-archive-cache` inside the install
directory, then switches to extraction progress. A fresh installation needs free
space for the compressed archive **plus** the extracted game (and filesystem
overhead). Resuming accounts for existing staged bytes; extraction checks free
space again. Failed or paused extraction retains the archive. Successful game
registration removes it.

Archive upgrades download the complete replacement package, not individual
changed files. Files tracked by a previous archive install but absent from the
replacement are removed after extraction; unrelated files, such as saves, are
preserved. Switching from a legacy folder/chunk install does not delete files
that were never recorded in the archive inventory.
The inventory is saved once in `.drop-archive-files`, separately from transfer
checkpoints, so a large file list is not rewritten for every downloaded chunk.

## Checks

- `cargo test --manifest-path libraries/droplet/Cargo.toml --lib` includes RAR4,
  RAR5, solid RAR, byte-range verification, and a generated 187,260-entry ZIP64.
- `cargo test --manifest-path desktop/src-tauri/Cargo.toml -p games --lib` covers
  staging, checksummed resume, safe extraction, ownership, and interrupted jobs.
- The server test suite covers archive download manifests, delta rejection, and
  compressed-versus-extracted sizes.
