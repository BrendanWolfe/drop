use std::{
    fs, io,
    path::{Path, PathBuf},
};

use database::DownloadableMetadata;

use super::drop_data::DropData;

// Keep names readable, bounded, and safe on every supported filesystem.
fn folder_component(value: &str, fallback: &str) -> String {
    let mut result = String::new();
    for character in value.chars().flat_map(char::to_lowercase) {
        let character = if character.is_alphanumeric() || character == '.' {
            character
        } else {
            '-'
        };
        if character == '-' && (result.is_empty() || result.ends_with('-')) {
            continue;
        }
        if result.len() + character.len_utf8() > 80 {
            break;
        }
        result.push(character);
    }
    let result = result.trim_matches(['-', '.']);
    if result.is_empty() {
        fallback.into()
    } else {
        result.into()
    }
}

/// Reserve a visible sibling folder atomically. Never take over a game,
/// unrelated emulator build, unmarked user folder, or symlink.
pub(super) fn create_emulator_directory(
    base: &Path,
    name: &str,
    version: &str,
    metadata: &DownloadableMetadata,
) -> io::Result<PathBuf> {
    fs::create_dir_all(base)?;
    let name = folder_component(name, "emulator");
    let version = folder_component(version, "version");
    let stem = format!("{name}-{version}");
    for number in 1.. {
        let folder = if number == 1 {
            stem.clone()
        } else {
            format!("{stem}-{number}")
        };
        let path = base.join(folder);
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if fs::symlink_metadata(&path)?.file_type().is_dir()
                    && DropData::read(&path).is_ok_and(|data| {
                        data.emulator
                            && data.game_id == metadata.id
                            && data.game_version == metadata.version
                            && data.target_platform == metadata.target_platform
                    })
                {
                    return Ok(path);
                }
            }
            Err(error) => return Err(error),
        }
    }
    unreachable!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{DownloadType, models::data::UserConfiguration, platform::Platform};

    fn metadata(version: &str, platform: Platform) -> DownloadableMetadata {
        DownloadableMetadata::new(
            "dolphin".into(),
            version.into(),
            platform,
            DownloadType::Emulator,
        )
    }

    #[test]
    fn builds_use_readable_distinct_sibling_folders() {
        let root = tempfile::tempdir().unwrap();
        let first = create_emulator_directory(
            root.path(),
            "Dolphin",
            "5.0",
            &metadata("v1", Platform::Linux),
        )
        .unwrap();
        let second = create_emulator_directory(
            root.path(),
            "Dolphin",
            "6.0",
            &metadata("v2", Platform::Linux),
        )
        .unwrap();
        let windows = create_emulator_directory(
            root.path(),
            "Dolphin",
            "5.0",
            &metadata("v1", Platform::Windows),
        )
        .unwrap();
        assert_eq!(first, root.path().join("dolphin-5.0"));
        assert_eq!(second, root.path().join("dolphin-6.0"));
        assert_eq!(windows, root.path().join("dolphin-5.0-2"));
        assert!(!root.path().join(".drop-emulators").exists());
    }

    #[test]
    fn duplicate_labels_and_unowned_folders_are_not_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let original = root.path().join("dolphin-5.0");
        fs::create_dir(&original).unwrap();
        fs::write(original.join("keep"), "user files").unwrap();
        let first = create_emulator_directory(
            root.path(),
            "Dolphin",
            "5.0",
            &metadata("v1", Platform::Linux),
        )
        .unwrap();
        let second = create_emulator_directory(
            root.path(),
            "Dolphin",
            "5.0",
            &metadata("v2", Platform::Linux),
        )
        .unwrap();
        assert_ne!(first, second);
        assert_eq!(first, root.path().join("dolphin-5.0-2"));
        assert_eq!(
            fs::read_to_string(original.join("keep")).unwrap(),
            "user files"
        );
    }

    #[test]
    fn matching_owned_build_can_resume_but_game_marker_cannot() {
        let root = tempfile::tempdir().unwrap();
        let meta = metadata("v1", Platform::Linux);
        let path = create_emulator_directory(root.path(), "Dolphin", "5.0", &meta).unwrap();
        let mut data = DropData::new(
            meta.id.clone(),
            meta.version.clone(),
            meta.target_platform,
            path.clone(),
            UserConfiguration::default(),
            None,
        );
        data.emulator = true;
        data.write();
        assert_eq!(
            create_emulator_directory(root.path(), "Dolphin", "5.0", &meta).unwrap(),
            path
        );
        data.emulator = false;
        data.write();
        assert_ne!(
            create_emulator_directory(root.path(), "Dolphin", "5.0", &meta).unwrap(),
            path
        );
    }

    #[test]
    fn server_names_cannot_escape_or_hide_the_folder() {
        let root = tempfile::tempdir().unwrap();
        let path = create_emulator_directory(
            root.path(),
            "../Dolphin\\bad:",
            "../../5.0?*",
            &metadata("v1", Platform::Linux),
        )
        .unwrap();
        assert_eq!(path.parent(), Some(root.path()));
        let name = path.file_name().unwrap().to_str().unwrap();
        assert!(!name.starts_with('.'));
        assert!(!name.contains(['/', '\\', ':', '?', '*']));
        assert_eq!(folder_component("...", "emulator"), "emulator");
        assert!(folder_component(&"🐬".repeat(100), "emulator").len() <= 80);
        assert_eq!(
            folder_component(" Dolphin   Emulator ", "emulator"),
            "dolphin-emulator"
        );
        assert_eq!(
            folder_component("5.0 Beta / RC1", "version"),
            "5.0-beta-rc1"
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_never_used_as_install_folders() {
        let root = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let link = root.path().join("dolphin-5.0");
        std::os::unix::fs::symlink(target.path(), &link).unwrap();
        let path = create_emulator_directory(
            root.path(),
            "Dolphin",
            "5.0",
            &metadata("v1", Platform::Linux),
        )
        .unwrap();
        assert_ne!(path, link);
        assert!(fs::read_dir(target.path()).unwrap().next().is_none());
    }
}
