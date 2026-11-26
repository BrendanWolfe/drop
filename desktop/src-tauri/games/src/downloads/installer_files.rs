use database::{
    DownloadableMetadata,
    install_records::INSTALLER_ASSET_DIR,
    models::data::{InstallerAsset, InstallerLibraryInputKind},
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    drop_data::{DROPDATA_PATH, DropData},
    server_archive::{read_named_inventory, write_named_inventory},
};

const INVENTORY: &str = ".drop-installer-files";

fn asset_storage(
    package: &Path,
    meta: &DownloadableMetadata,
    asset: &InstallerAsset,
) -> io::Result<(PathBuf, DownloadableMetadata)> {
    if let Some(key) = &asset.filesystem_key {
        if key.len() != 64 || !key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(io::Error::other("Invalid filesystem companion key"));
        }
        let unit = format!(
            "{:x}",
            Sha256::digest(format!("{key}/{}", asset.target_path))
        );
        let base = package
            .parent()
            .ok_or_else(|| io::Error::other("Missing install directory"))?;
        Ok((
            base.join(INSTALLER_ASSET_DIR).join(&unit),
            DownloadableMetadata::new(
                "filesystem-companion".into(),
                unit,
                database::platform::Platform::HOST,
                database::DownloadType::InstallerAsset,
            ),
        ))
    } else {
        Ok((asset_root(package, &meta.version)?, meta.clone()))
    }
}

pub fn asset_root(package: &Path, id: &str) -> io::Result<PathBuf> {
    let suffix = id.strip_prefix("installer-asset-").unwrap_or("");
    if suffix.len() != 36
        || !suffix.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
    {
        return Err(io::Error::other("Invalid installer companion ID"));
    }
    Ok(package.join(INSTALLER_ASSET_DIR).join(id))
}

pub(super) fn prepare_asset_root(
    package: &Path,
    meta: &DownloadableMetadata,
    asset: &InstallerAsset,
) -> io::Result<(PathBuf, DownloadableMetadata)> {
    validate_root(package)?;
    let (root, meta) = asset_storage(package, meta, asset)?;
    let parent = root.parent().unwrap();
    if let Err(error) = fs::create_dir(parent)
        && error.kind() != io::ErrorKind::AlreadyExists
    {
        return Err(error);
    }
    validate_root(parent)?;
    match fs::create_dir(&root) {
        Ok(()) => DropData::new(
            meta.id.clone(),
            meta.version.clone(),
            meta.target_platform,
            root.clone(),
            Default::default(),
            None,
        )
        .write(),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => validate_root(&root)?,
        Err(error) => return Err(error),
    }
    let data = DropData::read(&root)?;
    if data.game_id != meta.id
        || data.game_version != meta.version
        || data.target_platform != meta.target_platform
    {
        return Err(io::Error::other(
            "Companion folder belongs to a different download",
        ));
    }
    Ok((root, meta))
}

pub fn ready_asset(package: &Path, game: &str, asset: &InstallerAsset) -> Option<PathBuf> {
    validate_root(package).ok()?;
    let meta = DownloadableMetadata::new(
        game.into(),
        asset.id.clone(),
        database::platform::Platform::HOST,
        database::DownloadType::InstallerAsset,
    );
    let (root, meta) = asset_storage(package, &meta, asset).ok()?;
    ready_asset_payload(&root, &meta.id, &meta.version, &asset.target_path)
}

pub(super) fn ready_asset_payload(
    root: &Path,
    owner: &str,
    version: &str,
    target_path: &str,
) -> Option<PathBuf> {
    validate_root(root.parent()?).ok()?;
    validate_root(root).ok()?;
    let data = DropData::read(root).ok()?;
    if data.game_id != owner || data.game_version != version {
        return None;
    }
    let files = data.installer_asset_files?;
    if files.is_empty()
        || !files.iter().all(|(name, length)| {
            super::integrity::payload_path(root, name)
                .and_then(fs::symlink_metadata)
                .is_ok_and(|info| info.is_file() && info.len() == *length)
        })
    {
        return None;
    }
    let target = super::integrity::payload_path(root, target_path).ok()?;
    target.canonicalize().ok()
}

fn remove_assets(package: &Path, game: &str) -> io::Result<()> {
    let dir = package.join(INSTALLER_ASSET_DIR);
    if validate_root(&dir).is_err() {
        return Ok(());
    }
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if asset_root(package, name).is_ok()
            && entry.file_type()?.is_dir()
            && DropData::read(&entry.path())
                .is_ok_and(|data| data.game_id == game && data.game_version == name)
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    let _ = fs::remove_dir(dir);
    Ok(())
}

pub fn has_inventory(root: &Path) -> bool {
    root.join(INVENTORY).symlink_metadata().is_ok()
}

fn validate_root(root: &Path) -> io::Result<()> {
    if !root.symlink_metadata()?.is_dir() {
        return Err(io::Error::other(
            "The installer folder is not a regular directory",
        ));
    }
    Ok(())
}

fn validate_names(files: impl Iterator<Item = impl AsRef<str>>) -> io::Result<()> {
    for file in files {
        let file = file.as_ref();
        if file.split(['/', '\\']).any(|part| {
            part.eq_ignore_ascii_case(INVENTORY) || part.eq_ignore_ascii_case(INSTALLER_ASSET_DIR)
        }) {
            return Err(io::Error::other(
                "Installer content uses a reserved Drop path",
            ));
        }
        super::extract::safe_relative(file)?;
    }
    Ok(())
}

/// List only declared files still present in the package. Older downloads have
/// only the installer inventory, so they use it as a fallback.
pub fn companion_files(
    root: &Path,
    id: &str,
    current: Option<&[String]>,
    legacy_executable: Option<&str>,
) -> io::Result<Vec<(String, std::path::PathBuf)>> {
    validate_root(root)?;
    let inventory = read_named_inventory(root, id, INVENTORY)?;
    let names = current.unwrap_or(&inventory);
    validate_names(names.iter())?;
    let root = fs::canonicalize(root)?;
    Ok(names
        .iter()
        .filter(|name| current.is_some() || Some(name.as_str()) != legacy_executable)
        .filter_map(|name| {
            let path = root.join(name);
            if !path.symlink_metadata().is_ok_and(|meta| meta.is_file()) {
                return None;
            }
            let path = fs::canonicalize(path).ok()?;
            (path.parent() == Some(root.as_path())).then(|| (name.clone(), path))
        })
        .collect())
}

pub(super) fn record<'a>(
    root: &Path,
    id: &str,
    files: impl IntoIterator<Item = &'a String>,
) -> io::Result<()> {
    validate_root(root)?;
    let mut all: BTreeSet<String> = read_named_inventory(root, id, INVENTORY)?
        .into_iter()
        .collect();
    all.extend(files.into_iter().cloned());
    validate_names(all.iter())?;
    write_named_inventory(root, id, all.into_iter().collect(), INVENTORY)
}

pub fn resolve_library_input(
    root: &Path,
    kind: InstallerLibraryInputKind,
    path: &str,
) -> Option<PathBuf> {
    validate_root(root).ok()?;
    let root = fs::canonicalize(root).ok()?;
    if path.is_empty() {
        return (kind == InstallerLibraryInputKind::Folder).then_some(root);
    }
    if path.len() > 4096
        || path.split('/').any(|part| {
            part.is_empty()
                || part.starts_with('.')
                || part.ends_with(['.', ' '])
                || part
                    .chars()
                    .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
        })
    {
        return None;
    }
    let relative = droplet_types::archive_relative_path(path).ok()?;
    let mut candidate = root.clone();
    for part in relative.components() {
        candidate.push(part);
        if candidate.symlink_metadata().ok()?.is_symlink() {
            return None;
        }
    }
    let target = fs::canonicalize(candidate).ok()?;
    if !target.starts_with(&root) || target == root {
        return None;
    }
    let matches = match kind {
        InstallerLibraryInputKind::File => target.is_file(),
        InstallerLibraryInputKind::Folder => target.is_dir(),
    };
    matches.then_some(target)
}

pub fn remove(root: &Path, id: &str, installed_game: Option<&Path>) -> io::Result<()> {
    validate_root(root)?;
    let files = read_named_inventory(root, id, INVENTORY)?;
    validate_names(files.iter())?;
    let archive = super::server_archive::read_inventory(root, id)?;
    validate_names(archive.iter())?;
    remove_assets(root, id)?;
    for name in files.iter().chain(&archive) {
        let payload = super::integrity::payload_path(root, name)?;
        if installed_game.is_some_and(|game| payload.starts_with(game)) {
            continue;
        }
        match fs::remove_file(payload) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    super::server_archive::cleanup(root);
    for name in [DROPDATA_PATH, INVENTORY, droplet_types::ARCHIVE_INVENTORY] {
        match fs::remove_file(root.join(name)) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    // An installer may have left other files here. Never remove them.
    match fs::remove_dir(root) {
        Err(error) if error.kind() == io::ErrorKind::DirectoryNotEmpty => Ok(()),
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn separate_companions_have_independent_queue_keys_and_cleanup() {
        let package = tempfile::tempdir().unwrap();
        let game = "fixture".to_owned();
        let files = [
            "installer-asset-07560332-cbb2-4f0b-a763-ebac9856ed00",
            "installer-asset-07560332-cbb2-4f0b-a763-ebac9856ed01",
        ];
        let metas = files.map(|id| {
            DownloadableMetadata::new(
                game.clone(),
                id.into(),
                database::platform::Platform::HOST,
                database::DownloadType::InstallerAsset,
            )
        });
        let jobs: std::collections::HashSet<_> = metas.iter().collect();
        assert_eq!(jobs.len(), 2);
        let asset = InstallerAsset {
            filesystem_key: None,
            id: files[0].into(),
            name: "test".into(),
            target_path: "roms/data.nup".into(),
        };
        let (first, _) = prepare_asset_root(package.path(), &metas[0], &asset).unwrap();
        let (second, _) = prepare_asset_root(package.path(), &metas[1], &asset).unwrap();
        fs::create_dir(first.join("roms")).unwrap();
        fs::write(first.join(&asset.target_path), b"rom").unwrap();
        assert!(ready_asset(package.path(), &game, &asset).is_none());
        let mut data = DropData::read(&first).unwrap();
        data.installer_asset_files = Some([(asset.target_path.clone(), 3)].into());
        data.write();
        assert_eq!(
            ready_asset(package.path(), &game, &asset),
            first.join(&asset.target_path).canonicalize().ok()
        );
        fs::create_dir(package.path().join("Installed Game")).unwrap();
        record(package.path(), &game, std::iter::empty()).unwrap();
        remove(package.path(), &game, None).unwrap();
        assert!(!first.exists() && !second.exists());
        assert!(package.path().join("Installed Game").is_dir());
    }

    #[test]
    fn reuses_an_installed_game_folder_or_the_exact_file_without_copying() {
        let file = InstallerLibraryInputKind::File;
        let folder = InstallerLibraryInputKind::Folder;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let rom = "roms/Game Name.z64";
        fs::create_dir(root.join("roms")).unwrap();
        fs::write(root.join(rom), b"fixture").unwrap();
        assert_eq!(
            resolve_library_input(root, file, rom),
            fs::canonicalize(root.join(rom)).ok()
        );
        assert_eq!(
            resolve_library_input(root, folder, "roms"),
            fs::canonicalize(root.join("roms")).ok()
        );
        assert_eq!(
            resolve_library_input(root, folder, ""),
            fs::canonicalize(root).ok()
        );
        assert!(resolve_library_input(root, file, "missing.z64").is_none());
        assert!(resolve_library_input(root, file, "roms").is_none());
        assert!(resolve_library_input(root, file, "roms/../.dropdata").is_none());
        #[cfg(unix)]
        {
            let outside = tempfile::NamedTempFile::new().unwrap();
            std::os::unix::fs::symlink(outside.path(), root.join("other.z64")).unwrap();
            assert!(resolve_library_input(root, file, "other.z64").is_none());
        }
    }

    #[test]
    fn lists_only_current_downloaded_companions_with_absolute_paths() {
        let package = tempfile::tempdir().unwrap();
        let root = package.path();
        for name in ["setup.exe", "game data.bin", "old.bin", "untracked.dat"] {
            fs::write(root.join(name), b"fixture").unwrap();
        }
        record(
            root,
            "game",
            &["setup.exe".into(), "old.bin".into(), "game data.bin".into()],
        )
        .unwrap();
        let files = companion_files(
            root,
            "game",
            Some(&["game data.bin".into(), "missing.bin".into()]),
            None,
        )
        .unwrap();
        assert_eq!(
            files,
            vec![(
                "game data.bin".into(),
                fs::canonicalize(root.join("game data.bin")).unwrap()
            )]
        );
        let legacy = companion_files(root, "game", None, Some("setup.exe")).unwrap();
        assert_eq!(
            legacy.into_iter().map(|(name, _)| name).collect::<Vec<_>>(),
            ["game data.bin", "old.bin"]
        );
        assert!(companion_files(root, "game", Some(&["../outside".into()]), None).is_err());
        assert!(companion_files(root, "other", None, None).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn does_not_return_a_companion_replaced_by_a_symlink() {
        let package = tempfile::tempdir().unwrap();
        let external = tempfile::NamedTempFile::new().unwrap();
        std::os::unix::fs::symlink(external.path(), package.path().join("data.bin")).unwrap();
        assert!(
            companion_files(package.path(), "game", Some(&["data.bin".into()]), None)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn removes_only_downloaded_files_and_tracks_files_across_package_updates() {
        let temp = tempfile::tempdir().unwrap();
        let package = temp.path().join("package");
        fs::create_dir(&package).unwrap();
        fs::create_dir(package.join("installed game")).unwrap();
        fs::write(package.join("installed game/play.exe"), b"game").unwrap();
        for name in [
            "setup v1.exe",
            "setup.exe",
            "data.bin",
            "user.txt",
            DROPDATA_PATH,
        ] {
            fs::write(package.join(name), b"fixture").unwrap();
        }
        record(&package, "game", &["setup v1.exe".into()]).unwrap();
        record(&package, "game", &["setup.exe".into(), "data.bin".into()]).unwrap();
        assert!(remove(&package, "other game", None).is_err());
        assert!(package.join("setup.exe").exists());
        remove(&package, "game", None).unwrap();
        assert_eq!(fs::read_dir(&package).unwrap().count(), 2);
        assert!(package.join("installed game/play.exe").is_file());
        assert!(package.join("user.txt").is_file());
    }

    #[test]
    fn filesystem_companion_is_reused_by_another_installer_and_survives_removal() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let asset = InstallerAsset {
            id: "asset1".into(),
            name: "Game".into(),
            target_path: "Game".into(),
            filesystem_key: Some("a".repeat(64)),
        };
        let metadata = DownloadableMetadata::new(
            "first".into(),
            asset.id.clone(),
            database::platform::Platform::HOST,
            database::DownloadType::InstallerAsset,
        );
        let (root, _) = prepare_asset_root(&first, &metadata, &asset).unwrap();
        fs::create_dir(root.join("Game")).unwrap();
        fs::write(root.join("Game/eboot.bin"), b"boot").unwrap();
        let mut data = DropData::read(&root).unwrap();
        data.installer_asset_files = Some([("Game/eboot.bin".into(), 4)].into());
        data.write();
        let first_path = ready_asset(&first, "first", &asset).unwrap();
        let second_asset = InstallerAsset {
            id: "asset2".into(),
            ..asset
        };
        assert_eq!(
            ready_asset(&second, "second", &second_asset),
            Some(first_path.clone())
        );
        remove(&first, "first", None).unwrap();
        assert!(first_path.is_dir());
    }

    #[test]
    fn removes_extracted_release_files_but_leaves_the_located_game() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let game = root.join("release/game");
        fs::create_dir_all(&game).unwrap();
        fs::write(root.join("release/setup.exe"), b"setup").unwrap();
        fs::write(game.join("play.exe"), b"game").unwrap();
        super::super::server_archive::write_inventory(
            root,
            "test",
            vec!["release/setup.exe".into(), "release/game/play.exe".into()],
        )
        .unwrap();
        remove(root, "test", Some(&game)).unwrap();
        assert!(!root.join("release/setup.exe").exists());
        assert!(game.join("play.exe").is_file());
    }
}
