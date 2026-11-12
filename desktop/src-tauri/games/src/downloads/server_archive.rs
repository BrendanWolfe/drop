//! Staging for authenticated, server-managed archive installs. The normal
//! download agent still owns queueing, depots, verification and completion.
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
};

use download_manager::{
    error::ApplicationDownloadError,
    util::download_thread_control_flag::{DownloadThreadControl, DownloadThreadControlFlag},
};
use droplet_types::{
    ARCHIVE_CACHE_DIR, ARCHIVE_FILENAME, ARCHIVE_INVENTORY, ARCHIVE_MANIFEST_VERSION, Manifest,
};
use sha2::{Digest, Sha256};
use utils::lock;

static BUSY: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

pub struct Lease(PathBuf);
impl Lease {
    pub fn acquire(root: &Path) -> Result<Arc<Self>, ApplicationDownloadError> {
        let root = fs::canonicalize(root)?;
        if !lock!(BUSY).insert(root.clone()) {
            return Err(fail(
                "This archive installation is still stopping. Try again shortly.",
            ));
        }
        Ok(Arc::new(Self(root)))
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        lock!(BUSY).remove(&self.0);
    }
}

/// If the async worker is aborted, the blocking extractor must stop as well.
pub struct StopOnDrop(pub Option<DownloadThreadControl>);
impl Drop for StopOnDrop {
    fn drop(&mut self) {
        if let Some(control) = &self.0 {
            control.set(DownloadThreadControlFlag::Stop);
        }
    }
}

pub fn fail(message: impl Into<String>) -> ApplicationDownloadError {
    ApplicationDownloadError::Archive(message.into())
}

pub fn validate(
    manifests: &HashMap<String, Manifest>,
    version: &str,
) -> Result<Option<u64>, ApplicationDownloadError> {
    if !manifests
        .values()
        .any(|manifest| manifest.version == ARCHIVE_MANIFEST_VERSION)
    {
        if manifests.values().any(|manifest| manifest.version != "2") {
            return Err(fail(
                "Unsupported installation manifest. Update Drop Desktop.",
            ));
        }
        return Ok(None);
    }
    if manifests.len() != 1 {
        return Err(fail(
            "Archive installations cannot be combined with delta versions",
        ));
    }
    let manifest = manifests
        .get(version)
        .ok_or_else(|| fail("Archive version does not match this installation"))?;
    let metadata = manifest
        .archive
        .as_ref()
        .ok_or_else(|| fail("Archive installation metadata is missing"))?;
    let mut ranges = Vec::new();
    for chunk in manifest.chunks.values() {
        if chunk.files.len() != 1 {
            return Err(fail("Invalid archive chunk"));
        }
        let file = &chunk.files[0];
        if file.filename != ARCHIVE_FILENAME || file.length == 0 {
            return Err(fail("Invalid archive payload"));
        }
        ranges.push((file.start as u64, file.length as u64));
    }
    ranges.sort_unstable();
    let mut end = 0u64;
    for (start, length) in ranges {
        if start != end {
            return Err(fail("Archive byte ranges overlap or are incomplete"));
        }
        end = end
            .checked_add(length)
            .ok_or_else(|| fail("Archive is too large"))?;
    }
    if end == 0 || end != manifest.size {
        return Err(fail("Archive size does not match its byte ranges"));
    }
    Ok(Some(metadata.install_size))
}

fn reject_symlink(path: &Path) -> Result<(), ApplicationDownloadError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(fail("Archive staging paths cannot be symbolic links"))
        }
        Ok(metadata) => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                if metadata.is_file() && metadata.nlink() > 1 {
                    return Err(fail("Archive staging files cannot be hard links"));
                }
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct Inventory {
    game_id: String,
    files: Vec<String>,
}

/// Keep this separate from .dropdata: a 187k-file inventory must not be
/// re-serialized for every downloaded chunk checkpoint.
pub fn read_inventory(root: &Path, id: &str) -> std::io::Result<Vec<String>> {
    let path = root.join(ARCHIVE_INVENTORY);
    reject_symlink(&path).map_err(|error| std::io::Error::other(error.to_string()))?;
    let reader = match fs::File::open(path) {
        Ok(reader) => reader,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let inventory: Inventory = serde_json::from_reader(std::io::BufReader::new(reader))?;
    if inventory.game_id != id {
        return Err(std::io::Error::other(
            "Archive inventory belongs to another game",
        ));
    }
    for file in &inventory.files {
        droplet_types::archive_relative_path(file)?;
    }
    Ok(inventory.files)
}

pub fn write_inventory(root: &Path, id: &str, files: Vec<String>) -> std::io::Result<()> {
    let path = root.join(ARCHIVE_INVENTORY);
    let temporary = root.join(format!("{ARCHIVE_INVENTORY}.tmp"));
    reject_symlink(&path).map_err(|error| std::io::Error::other(error.to_string()))?;
    reject_symlink(&temporary).map_err(|error| std::io::Error::other(error.to_string()))?;
    let mut writer = std::io::BufWriter::new(fs::File::create(&temporary)?);
    serde_json::to_writer(
        &mut writer,
        &Inventory {
            game_id: id.to_string(),
            files,
        },
    )?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);
    fs::rename(temporary, path)
}

pub fn prepare(root: &Path, version: &str) -> Result<PathBuf, ApplicationDownloadError> {
    reject_symlink(root)?;
    let cache = root.join(ARCHIVE_CACHE_DIR);
    reject_symlink(&cache)?;
    fs::create_dir_all(&cache)?;
    let marker = cache.join(".version");
    let payload = cache.join(ARCHIVE_FILENAME);
    reject_symlink(&marker)?;
    reject_symlink(&payload)?;
    // Do not take ownership of an arbitrary pre-existing directory.
    if fs::read_dir(&cache)?.any(|entry| {
        entry.map_or(true, |entry| {
            ![".version", ARCHIVE_FILENAME]
                .iter()
                .any(|name| entry.file_name() == *name)
        })
    }) {
        return Err(fail("Archive cache contains unexpected files"));
    }
    if fs::read_to_string(&marker).ok().as_deref() != Some(version) {
        if payload.exists() && !marker.is_file() {
            return Err(fail("Archive cache is not owned by this installation"));
        }
        if payload.exists() {
            fs::remove_file(&payload)?;
        }
        fs::write(marker, version)?;
    }
    Ok(cache)
}

pub fn required_space(
    root: &Path,
    version: &str,
    manifest: &Manifest,
) -> Result<u64, ApplicationDownloadError> {
    let install = manifest
        .archive
        .as_ref()
        .ok_or_else(|| fail("Missing archive metadata"))?
        .install_size;
    let cache = root.join(ARCHIVE_CACHE_DIR);
    reject_symlink(&cache)?;
    reject_symlink(&cache.join(".version"))?;
    let payload = cache.join(ARCHIVE_FILENAME);
    reject_symlink(&payload)?;
    if fs::read_to_string(cache.join(".version")).ok().as_deref() == Some(version) {
        if let Ok(metadata) = fs::metadata(payload) {
            // Extraction has its own per-file free-space check. On resume do
            // not demand space again for archive bytes already on disk, or
            // files written by an interrupted extraction.
            #[cfg(unix)]
            let allocated = {
                use std::os::unix::fs::MetadataExt;
                metadata.blocks().saturating_mul(512)
            };
            #[cfg(not(unix))]
            let allocated = metadata.len();
            return Ok(manifest.size.saturating_sub(allocated));
        }
    }
    manifest
        .size
        .checked_add(install)
        .ok_or_else(|| fail("Installation is too large"))
}

/// Checkpointed ranges must still match their checksums after a restart or
/// interrupted write. This reads only the cached, compressed bytes locally.
pub fn verified_contexts(
    cache: &Path,
    manifest: &Manifest,
    completed: &HashMap<String, bool>,
    control: &DownloadThreadControl,
) -> Result<HashMap<String, bool>, ApplicationDownloadError> {
    let mut valid = HashMap::new();
    let path = cache.join(ARCHIVE_FILENAME);
    if !path.is_file() {
        return Ok(valid);
    }
    let mut reader = fs::File::open(path)?;
    let mut chunks: Vec<_> = manifest.chunks.iter().collect();
    chunks.sort_by_key(|(_, chunk)| chunk.files[0].start);
    let mut buffer = vec![0; 1024 * 1024];
    for (id, chunk) in chunks {
        if !completed.get(id).copied().unwrap_or(false) {
            continue;
        }
        let entry = &chunk.files[0];
        reader.seek(SeekFrom::Start(entry.start as u64))?;
        let mut remaining = entry.length;
        let mut hasher = Sha256::new();
        while remaining > 0 {
            if control.get() == DownloadThreadControlFlag::Stop {
                return Ok(valid);
            }
            let length = remaining.min(buffer.len());
            let amount = reader.read(&mut buffer[..length])?;
            if amount == 0 {
                break;
            }
            hasher.update(&buffer[..amount]);
            remaining -= amount;
        }
        if remaining == 0 && hex::encode(hasher.finalize()) == chunk.checksum {
            valid.insert(id.clone(), true);
        }
    }
    Ok(valid)
}

pub fn cleanup(root: &Path) {
    let cache = root.join(ARCHIVE_CACHE_DIR);
    if reject_symlink(&cache).is_err() {
        return;
    }
    let _ = fs::remove_file(cache.join(ARCHIVE_FILENAME));
    let _ = fs::remove_file(cache.join(".version"));
    let _ = fs::remove_dir(cache); // Deliberately non-recursive.
}

#[cfg(test)]
mod tests {
    use super::*;
    use droplet_types::{ArchiveMetadata, ChunkData, FileEntry};

    fn manifest() -> Manifest {
        Manifest {
            version: ARCHIVE_MANIFEST_VERSION.into(),
            size: 6,
            key: [0; 16],
            archive: Some(ArchiveMetadata { install_size: 100 }),
            chunks: [
                ("first".into(), chunk(0, b"abc")),
                ("last".into(), chunk(3, b"def")),
            ]
            .into(),
        }
    }
    fn chunk(start: usize, data: &[u8]) -> ChunkData {
        ChunkData {
            files: vec![FileEntry {
                filename: ARCHIVE_FILENAME.into(),
                start,
                length: data.len(),
                permissions: 0o600,
            }],
            checksum: hex::encode(Sha256::digest(data)),
            iv: [0; 16],
        }
    }

    #[test]
    fn rejects_mixed_versions_missing_metadata_and_incomplete_ranges() {
        assert_eq!(
            validate(&[("version".into(), manifest())].into(), "version").unwrap(),
            Some(100)
        );
        let mut data = manifest();
        data.chunks.get_mut("last").unwrap().files[0].start = 2;
        assert!(validate(&[("version".into(), data)].into(), "version").is_err());
        let mut data = manifest();
        data.archive = None;
        assert!(validate(&[("version".into(), data)].into(), "version").is_err());
        assert!(
            validate(
                &[("version".into(), manifest()), ("other".into(), manifest())].into(),
                "version"
            )
            .is_err()
        );
    }

    #[test]
    fn resumes_only_verified_ranges_and_resets_cache_on_version_change() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(required_space(root.path(), "v1", &manifest()).unwrap(), 106);
        let cache = prepare(root.path(), "v1").unwrap();
        fs::write(cache.join(ARCHIVE_FILENAME), b"abcBAD").unwrap();
        let control = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
        let completed = [("first".into(), true), ("last".into(), true)].into();
        let valid = verified_contexts(&cache, &manifest(), &completed, &control).unwrap();
        assert_eq!(valid, [("first".into(), true)].into());
        assert_eq!(required_space(root.path(), "v1", &manifest()).unwrap(), 0);
        prepare(root.path(), "v2").unwrap();
        assert!(!cache.join(ARCHIVE_FILENAME).exists());
        assert!(
            verified_contexts(&cache, &manifest(), &completed, &control)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn cache_cleanup_preserves_unexpected_files_and_busy_lease_blocks_reentry() {
        let root = tempfile::tempdir().unwrap();
        let cache = prepare(root.path(), "version").unwrap();
        fs::write(cache.join("unrelated"), b"keep").unwrap();
        assert!(prepare(root.path(), "version").is_err());
        cleanup(root.path());
        assert_eq!(fs::read(cache.join("unrelated")).unwrap(), b"keep");
        let lease = Lease::acquire(root.path()).unwrap();
        assert!(Lease::acquire(root.path()).is_err());
        drop(lease);
        assert!(Lease::acquire(root.path()).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn cache_refuses_symlink_targets() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join(ARCHIVE_CACHE_DIR)).unwrap();
        assert!(prepare(root.path(), "version").is_err());
    }

    #[test]
    fn inventory_is_owned_and_checkpointed_separately_from_transfer_progress() {
        let root = tempfile::tempdir().unwrap();
        assert!(read_inventory(root.path(), "game").unwrap().is_empty());
        write_inventory(
            root.path(),
            "game",
            vec!["bin/play.exe".into(), "assets/data.bin".into()],
        )
        .unwrap();
        assert_eq!(read_inventory(root.path(), "game").unwrap().len(), 2);
        assert!(read_inventory(root.path(), "different-game").is_err());
        assert!(
            !root
                .path()
                .join(format!("{ARCHIVE_INVENTORY}.tmp"))
                .exists()
        );
        cleanup(root.path());
        assert_eq!(read_inventory(root.path(), "game").unwrap().len(), 2);
    }

    #[test]
    fn aborted_extraction_stops_but_success_can_disarm_the_guard() {
        let control = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
        drop(StopOnDrop(Some(control.clone())));
        assert!(control.get() == DownloadThreadControlFlag::Stop);
        control.set(DownloadThreadControlFlag::Go);
        let mut guard = StopOnDrop(Some(control.clone()));
        guard.0 = None;
        drop(guard);
        assert!(control.get() == DownloadThreadControlFlag::Go);
    }
}
