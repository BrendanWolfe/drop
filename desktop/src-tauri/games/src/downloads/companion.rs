use std::{collections::HashMap, fs, io, path::Path};

use database::models::data::{InstallerLibraryInput, InstallerLibraryInputKind};
use serde::{Deserialize, Serialize};

use super::integrity::payload_path;

pub fn matches(input: &InstallerLibraryInput, filename: &str) -> bool {
    let filename = filename.strip_prefix("./").unwrap_or(filename);
    match input.kind {
        InstallerLibraryInputKind::File => filename == input.path,
        InstallerLibraryInputKind::Folder => in_folder(&input.path, filename),
    }
}

fn in_folder(folder: &str, path: &str) -> bool {
    folder.is_empty()
        || path
            .strip_prefix(folder)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CompanionFiles {
    files: HashMap<String, u64>,
    folders: Vec<String>,
}

impl CompanionFiles {
    pub fn has_file(&self, root: &Path, name: &str, size: u64) -> bool {
        self.files.get(name) == Some(&size) && on_disk(root, name, size)
    }

    pub fn contains(&self, root: &Path, input: &InstallerLibraryInput) -> bool {
        if input.kind == InstallerLibraryInputKind::Folder
            && !self
                .folders
                .iter()
                .any(|folder| folder == &input.path || in_folder(folder, &input.path))
        {
            return false;
        }
        let mut files = self
            .files
            .iter()
            .filter(|(name, _)| matches(input, name))
            .peekable();
        files.peek().is_some() && files.all(|(name, size)| on_disk(root, name, *size))
    }

    pub fn record(
        &mut self,
        root: &Path,
        inputs: &[InstallerLibraryInput],
        files: HashMap<String, u64>,
    ) -> io::Result<()> {
        if !files.iter().all(|(name, size)| on_disk(root, name, *size)) {
            return Err(io::Error::other(
                "One or more companion files are incomplete",
            ));
        }
        self.files.extend(files);
        for input in inputs {
            if input.kind == InstallerLibraryInputKind::Folder
                && !self.folders.contains(&input.path)
            {
                self.folders.push(input.path.clone());
            }
        }
        Ok(())
    }
}

fn on_disk(root: &Path, name: &str, size: u64) -> bool {
    payload_path(root, name)
        .and_then(fs::symlink_metadata)
        .is_ok_and(|meta| meta.is_file() && meta.len() == size)
}

#[cfg(test)]
mod tests {
    use super::*;
    const FILE: InstallerLibraryInputKind = InstallerLibraryInputKind::File;
    const FOLDER: InstallerLibraryInputKind = InstallerLibraryInputKind::Folder;

    fn input(kind: InstallerLibraryInputKind, path: &str) -> InstallerLibraryInput {
        InstallerLibraryInput {
            name: None,
            game_id: "source".into(),
            version_id: "pinned".into(),
            game_name: "Source".into(),
            version_name: "Pinned".into(),
            kind,
            path: path.into(),
        }
    }

    #[test]
    fn file_folder_and_root_are_distinct_selectors() {
        assert!(matches(&input(FILE, "roms/game.z64"), "./roms/game.z64"));
        assert!(!matches(&input(FILE, "roms/game.z64"), "roms/game.z64.bak"));
        assert!(matches(&input(FOLDER, "roms"), "roms/sub/game.z64"));
        assert!(!matches(&input(FOLDER, "roms"), "roms-extras/game.z64"));
        assert!(matches(&input(FOLDER, ""), "anything.n64"));
    }

    #[test]
    fn only_completed_files_and_folders_are_reused_from_a_partial_install() {
        let temporary = tempfile::tempdir().unwrap();
        let root = temporary.path();
        fs::create_dir_all(root.join("roms/sub")).unwrap();
        fs::write(root.join("roms/sub/game.z64"), b"rom").unwrap();
        fs::write(root.join("roms/extra.z64"), b"rom").unwrap();
        let mut files = CompanionFiles::default();
        let rom = input(FILE, "roms/sub/game.z64");
        let folder = input(FOLDER, "roms");
        assert!(!files.contains(root, &rom));
        files
            .record(root, &[rom.clone()], [(rom.path.clone(), 3)].into())
            .unwrap();
        assert!(files.contains(root, &rom));
        assert!(!files.contains(root, &folder));
        files
            .record(
                root,
                &[folder.clone()],
                [(rom.path.clone(), 3), ("roms/extra.z64".to_owned(), 3)].into(),
            )
            .unwrap();
        assert!(files.contains(root, &folder));
        assert!(files.contains(root, &input(FOLDER, "roms/sub")));
        assert!(!files.contains(root, &input(FOLDER, "")));
        fs::write(root.join("roms/extra.z64"), b"x").unwrap();
        assert!(!files.contains(root, &folder));
        assert!(files.contains(root, &rom));
    }
}
