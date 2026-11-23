use database::models::data::InstallerLibraryInputKind;
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

use super::{
    drop_data::DROPDATA_PATH,
    server_archive::{read_named_inventory, write_named_inventory},
};

const INVENTORY: &str = ".drop-installer-files";

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
        if file.starts_with('.') || file.contains(['/', '\\']) {
            return Err(io::Error::other("Invalid installer file name"));
        }
        droplet_types::archive_relative_path(file)?;
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

pub fn remove(root: &Path, id: &str) -> io::Result<()> {
    validate_root(root)?;
    let files = read_named_inventory(root, id, INVENTORY)?;
    validate_names(files.iter())?;
    for name in files
        .iter()
        .map(String::as_str)
        .chain([DROPDATA_PATH, INVENTORY])
    {
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
        assert!(remove(&package, "other game").is_err());
        assert!(package.join("setup.exe").exists());
        remove(&package, "game").unwrap();
        assert_eq!(fs::read_dir(&package).unwrap().count(), 1);
        assert!(package.join("user.txt").is_file());
    }
}
