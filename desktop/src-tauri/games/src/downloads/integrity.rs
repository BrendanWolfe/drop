//! Explicit integrity checks only. Library discovery must never call this module.
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::Arc,
};

use download_manager::util::{
    download_thread_control_flag::{DownloadThreadControl, DownloadThreadControlFlag},
    progress_object::{ProgressHandle, ProgressObject},
};
use droplet_types::Manifest;
use sha2::{Digest, Sha256};

/// Reserve synchronously before fetching a manifest or creating an agent.
pub fn reserve(
    db: &mut database::Database,
    meta: &database::DownloadableMetadata,
) -> Result<(), String> {
    if db.applications.local_games.contains_key(&meta.id) {
        return Err(
            "Integrity verification is available for server-managed installations only.".into(),
        );
    }
    if db.applications.transient_statuses.contains_key(meta)
        || (meta.download_type == database::DownloadType::Emulator
            && crate::emulators::emulator_in_use(db, meta))
    {
        return Err(
            "Stop this game and wait for its current operation to finish before verifying.".into(),
        );
    }
    db.applications.transient_statuses.insert(
        meta.clone(),
        database::ApplicationTransientStatus::Queued {
            version_id: meta.version.clone(),
        },
    );
    Ok(())
}

/// Reject traversal, metadata overwrites and links before reading/writing payloads.
pub fn payload_path(root: &Path, name: &str) -> io::Result<PathBuf> {
    let relative = PathBuf::from(name.replace('\\', "/"));
    if !database::install_records::valid_relative(&relative)
        || relative.components().any(|component| {
            let name = component.as_os_str().to_string_lossy();
            name.ends_with('.')
                || name.ends_with(' ')
                || [
                    ".dropdata",
                    ".dropdata.tmp",
                    ".drop-local-game",
                    ".drop-archive-cache",
                    ".drop-archive-files",
                    ".drop-archive-files.tmp",
                ]
                .iter()
                .any(|reserved| name.eq_ignore_ascii_case(reserved))
        })
    {
        return Err(io::Error::other("Unsafe installation payload path"));
    }
    if !fs::symlink_metadata(root)?.is_dir() {
        return Err(io::Error::other("Installation root is not a directory"));
    }
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(io::Error::other(
                    "Installation path contains a symbolic link",
                ));
            }
            Ok(meta) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if meta.is_file() && meta.nlink() > 1 {
                        return Err(io::Error::other("Installation payload is a hard link"));
                    }
                }
                #[cfg(not(unix))]
                let _ = meta;
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    Ok(current)
}

pub fn expected_lengths(
    file_list: &HashMap<String, String>,
    manifests: &HashMap<String, Manifest>,
) -> io::Result<HashMap<String, u64>> {
    let mut ranges: HashMap<String, Vec<(u64, u64)>> = HashMap::new();
    for (version, manifest) in manifests {
        for chunk in manifest.chunks.values() {
            for file in &chunk.files {
                if file_list.get(&file.filename) != Some(version) {
                    continue;
                }
                let start = u64::try_from(file.start).map_err(io::Error::other)?;
                let length = u64::try_from(file.length).map_err(io::Error::other)?;
                let end = start
                    .checked_add(length)
                    .ok_or_else(|| io::Error::other("File is too large"))?;
                ranges
                    .entry(file.filename.clone())
                    .or_default()
                    .push((start, end));
            }
        }
    }
    let mut lengths = HashMap::new();
    for name in file_list.keys() {
        let parts = ranges
            .get_mut(name)
            .ok_or_else(|| io::Error::other("Manifest is missing file ranges"))?;
        parts.sort_unstable();
        let mut end = 0;
        for (start, next) in parts {
            if *start != end {
                return Err(io::Error::other(
                    "Manifest file ranges overlap or are incomplete",
                ));
            }
            end = *next;
        }
        lengths.insert(name.clone(), end);
    }
    Ok(lengths)
}

/// A mixed delta chunk cannot be hashed from the effective files alone. Let
/// the checksum-validating download path repair it rather than trusting it.
pub fn verified_contexts(
    root: &Path,
    file_list: &HashMap<String, String>,
    manifests: &HashMap<String, Manifest>,
    control: &DownloadThreadControl,
    progress: &Arc<ProgressObject>,
) -> io::Result<HashMap<String, bool>> {
    let lengths = expected_lengths(file_list, manifests)?;
    let mut file_lengths = HashMap::new();
    for name in lengths.keys() {
        let path = payload_path(root, name)?;
        let length = match fs::metadata(&path) {
            Ok(meta) if meta.is_file() => Some(meta.len()),
            Ok(_) => None,
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        file_lengths.insert(name.clone(), length);
    }
    let total: usize = manifests
        .values()
        .flat_map(|manifest| manifest.chunks.values())
        .flat_map(|chunk| &chunk.files)
        .map(|file| file.length)
        .sum();
    progress.set_size(1);
    progress.set_max(total);
    progress.reset();
    let handle = ProgressHandle::new(progress.get(0), progress.clone());
    let mut valid = HashMap::new();
    let mut buffer = vec![0; 1024 * 1024];
    for (version, manifest) in manifests {
        for (id, chunk) in &manifest.chunks {
            if control.get() == DownloadThreadControlFlag::Stop {
                return Ok(valid);
            }
            if chunk.files.iter().any(|file| {
                file_list.get(&file.filename) != Some(version)
                    || !file_lengths
                        .get(&file.filename)
                        .copied()
                        .flatten()
                        .is_some_and(|length| {
                            (file.start as u64).saturating_add(file.length as u64) <= length
                        })
            }) {
                handle.add(chunk.files.iter().map(|file| file.length).sum());
                continue;
            }
            let mut hasher = Sha256::new();
            for entry in &chunk.files {
                let mut reader = File::open(payload_path(root, &entry.filename)?)?;
                reader.seek(SeekFrom::Start(entry.start as u64))?;
                let mut remaining = entry.length;
                while remaining > 0 {
                    if control.get() == DownloadThreadControlFlag::Stop {
                        return Ok(valid);
                    }
                    let length = remaining.min(buffer.len());
                    reader.read_exact(&mut buffer[..length])?;
                    hasher.update(&buffer[..length]);
                    handle.add(length);
                    remaining -= length;
                }
            }
            if hex::encode(hasher.finalize()) == chunk.checksum {
                valid.insert(id.clone(), true);
            }
        }
    }
    Ok(valid)
}

pub fn finish_lengths(
    root: &Path,
    file_list: &HashMap<String, String>,
    manifests: &HashMap<String, Manifest>,
) -> io::Result<()> {
    for (name, length) in expected_lengths(file_list, manifests)? {
        let path = payload_path(root, &name)?;
        let actual = fs::metadata(&path)?.len();
        if actual < length {
            return Err(io::Error::other(
                "A repaired file is still shorter than its manifest",
            ));
        }
        if actual > length {
            fs::OpenOptions::new()
                .write(true)
                .open(path)?
                .set_len(length)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use download_manager::util::progress_object::ProgressType;
    use droplet_types::{ChunkData, FileEntry};

    fn fixture() -> (HashMap<String, String>, HashMap<String, Manifest>) {
        let chunk = |name: &str, start, bytes: &[u8]| ChunkData {
            files: vec![FileEntry {
                filename: name.into(),
                start,
                length: bytes.len(),
                permissions: 0o600,
            }],
            checksum: hex::encode(Sha256::digest(bytes)),
            iv: [0; 16],
        };
        (
            [("game.bin".into(), "v1".into())].into(),
            [(
                "v1".into(),
                Manifest {
                    version: "2".into(),
                    chunks: [
                        ("first".into(), chunk("game.bin", 0, b"abc")),
                        ("last".into(), chunk("game.bin", 3, b"def")),
                    ]
                    .into(),
                    size: 6,
                    key: [0; 16],
                    archive: None,
                },
            )]
            .into(),
        )
    }

    #[test]
    fn detects_missing_truncated_and_same_length_corruption_at_chunk_granularity() {
        let root = tempfile::tempdir().unwrap();
        let (files, manifests) = fixture();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1500);
        let progress = Arc::new(ProgressObject::new(0, 1, sender, ProgressType::Disk));
        let control = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
        assert!(
            verified_contexts(root.path(), &files, &manifests, &control, &progress)
                .unwrap()
                .is_empty()
        );
        fs::write(root.path().join("game.bin"), b"abcdef").unwrap();
        assert_eq!(
            verified_contexts(root.path(), &files, &manifests, &control, &progress)
                .unwrap()
                .len(),
            2
        );
        fs::write(root.path().join("game.bin"), b"abcXYZ").unwrap();
        let valid =
            verified_contexts(root.path(), &files, &manifests, &control, &progress).unwrap();
        assert_eq!(valid.len(), 1);
        assert_eq!(valid.get("first"), Some(&true));
        fs::write(root.path().join("game.bin"), b"abc").unwrap();
        let valid =
            verified_contexts(root.path(), &files, &manifests, &control, &progress).unwrap();
        assert_eq!(valid.len(), 1);
        assert_eq!(valid.get("first"), Some(&true));
    }

    #[test]
    fn refuses_traversal_metadata_and_symlink_escapes() {
        let root = tempfile::tempdir().unwrap();
        for name in [
            "",
            "../save",
            "/save",
            "C:\\save",
            ".dropdata",
            ".dropdata.tmp",
            ".drop-local-game",
        ] {
            assert!(payload_path(root.path(), name).is_err(), "{name}");
        }
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
            assert!(payload_path(root.path(), "linked/save").is_err());
        }
    }

    #[test]
    fn repairs_trailing_bytes_without_removing_unrelated_files() {
        let root = tempfile::tempdir().unwrap();
        let (files, manifests) = fixture();
        fs::write(root.path().join("game.bin"), b"abcdeftrailing").unwrap();
        fs::write(root.path().join("save.dat"), b"keep").unwrap();
        finish_lengths(root.path(), &files, &manifests).unwrap();
        assert_eq!(fs::read(root.path().join("game.bin")).unwrap(), b"abcdef");
        assert_eq!(fs::read(root.path().join("save.dat")).unwrap(), b"keep");
    }

    #[test]
    fn reservations_block_conflicting_operations_without_changing_install_state() {
        use database::{
            ApplicationTransientStatus, Database, DownloadType, DownloadableMetadata,
            GameDownloadStatus, models::data::InstalledGameType, platform::Platform,
        };
        let mut db = Database::new("games", None, "cache".into());
        let meta = DownloadableMetadata::new(
            "game".into(),
            "v1".into(),
            Platform::Linux,
            DownloadType::Game,
        );
        db.applications.record_installation(
            &meta,
            GameDownloadStatus::Installed {
                install_type: InstalledGameType::Installed,
                version_id: "v1".into(),
                install_dir: "games/game".into(),
                update_available: true,
            },
        );
        db.applications
            .transient_statuses
            .insert(meta.clone(), ApplicationTransientStatus::Running {});
        assert!(reserve(&mut db, &meta).is_err());
        db.applications.transient_statuses.remove(&meta);
        reserve(&mut db, &meta).unwrap();
        assert!(reserve(&mut db, &meta).is_err());
        assert!(matches!(
            &db.applications.game_statuses["game"],
            GameDownloadStatus::Installed {
                install_type: InstalledGameType::Installed,
                update_available: true,
                ..
            }
        ));
    }

    #[test]
    fn cancellation_does_not_mark_unchecked_chunks_as_complete() {
        let root = tempfile::tempdir().unwrap();
        let (files, manifests) = fixture();
        fs::write(root.path().join("game.bin"), b"abcdef").unwrap();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1500);
        let progress = Arc::new(ProgressObject::new(0, 1, sender, ProgressType::Disk));
        let control = DownloadThreadControl::new(DownloadThreadControlFlag::Stop);
        assert!(
            verified_contexts(root.path(), &files, &manifests, &control, &progress)
                .unwrap()
                .is_empty()
        );
        assert_eq!(fs::read(root.path().join("game.bin")).unwrap(), b"abcdef");
    }

    #[test]
    fn delta_checks_use_effective_versions_and_do_not_trust_mixed_chunks() {
        let root = tempfile::tempdir().unwrap();
        let entry = |filename: &str| FileEntry {
            filename: filename.into(),
            start: 0,
            length: 3,
            permissions: 0o600,
        };
        let manifest = |files: Vec<FileEntry>, bytes: &[u8], id: &str| Manifest {
            version: "2".into(),
            size: bytes.len() as u64,
            key: [0; 16],
            archive: None,
            chunks: [(
                id.into(),
                ChunkData {
                    files,
                    checksum: hex::encode(Sha256::digest(bytes)),
                    iv: [0; 16],
                },
            )]
            .into(),
        };
        let files = [
            ("keep.bin".into(), "base".into()),
            ("changed.bin".into(), "delta".into()),
        ]
        .into();
        let manifests = [
            (
                "base".into(),
                manifest(
                    vec![entry("keep.bin"), entry("changed.bin")],
                    b"abcold",
                    "mixed",
                ),
            ),
            (
                "delta".into(),
                manifest(vec![entry("changed.bin")], b"new", "current"),
            ),
        ]
        .into();
        fs::write(root.path().join("keep.bin"), b"abc").unwrap();
        fs::write(root.path().join("changed.bin"), b"new").unwrap();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1500);
        let progress = Arc::new(ProgressObject::new(0, 1, sender, ProgressType::Disk));
        let control = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
        let valid =
            verified_contexts(root.path(), &files, &manifests, &control, &progress).unwrap();
        assert_eq!(valid.get("current"), Some(&true));
        assert!(!valid.contains_key("mixed"));
        assert_eq!(expected_lengths(&files, &manifests).unwrap().len(), 2);
    }
}
