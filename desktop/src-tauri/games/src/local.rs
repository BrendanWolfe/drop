use crate::library::{Game, GamePlaytime};
use database::{
    Database, DownloadType, DownloadableMetadata, GameDownloadStatus, GameVersion,
    models::data::{
        ArchiveState, InstalledGameType, LaunchConfiguration, LocalGame, UserConfiguration,
    },
};

pub const OWNER_FILE: &str = ".drop-local-game";

pub fn delete_owned_files(game: &LocalGame) -> std::io::Result<()> {
    use std::{fs, io};
    if game.archive.is_none() {
        return Err(io::Error::other(
            "Imported game folders cannot be deleted by Drop.",
        ));
    }
    if !game.root.exists() {
        return Ok(());
    }
    if fs::symlink_metadata(&game.root)?.file_type().is_symlink()
        || fs::read_to_string(game.root.join(OWNER_FILE))? != game.id
    {
        return Err(io::Error::other(
            "Folder ownership changed; files were not deleted.",
        ));
    }
    fs::remove_dir_all(&game.root)
}

pub fn as_game(local: &LocalGame) -> Game {
    Game {
        id: local.id.clone(),
        game_type: "Game".into(),
        m_name: local.name.clone(),
        m_short_description: "Local game".into(),
        m_description: "This game is managed on this device.".into(),
        playtime: vec![GamePlaytime {
            seconds: local.playtime_seconds.min(i32::MAX as u64) as i32,
        }],
        ..Default::default()
    }
}

pub fn metadata(local: &LocalGame) -> DownloadableMetadata {
    DownloadableMetadata::new(
        local.id.clone(),
        local.id.clone(),
        local.platform,
        DownloadType::Game,
    )
}

/// Adapt a local launch to the existing platform handlers, without server lookups.
pub fn register(db: &mut Database, local: LocalGame, command: Option<String>) {
    let meta = metadata(&local);
    let configuration = db
        .applications
        .game_versions
        .get(&meta.version)
        .map(|v| v.user_configuration.clone())
        .unwrap_or_default();
    let configuration = UserConfiguration {
        enable_updates: false,
        ..configuration
    };
    let launches = command
        .map(|command| LaunchConfiguration {
            launch_id: local.id.clone(),
            name: "Play".into(),
            command,
            platform: local.platform,
            umu_id_override: None,
            emulator: None,
            emulator_profile: None,
        })
        .into_iter()
        .collect();
    let version = GameVersion {
        game_id: local.id.clone(),
        version_id: local.id.clone(),
        display_name: None,
        version_path: String::new(),
        only_setup: false,
        version_index: 0,
        delta: false,
        user_configuration: configuration,
        launches,
        setups: vec![],
    };
    let ready = local.launcher.is_some() && local.archive.as_ref().is_none_or(|a| a.extracted);
    db.applications
        .game_versions
        .insert(meta.version.clone(), version);
    db.applications
        .installed_game_version
        .insert(local.id.clone(), meta);
    db.applications.game_statuses.insert(
        local.id.clone(),
        GameDownloadStatus::Installed {
            install_type: if ready {
                InstalledGameType::Installed
            } else {
                InstalledGameType::PartiallyInstalled {
                    configuration: UserConfiguration::default(),
                }
            },
            version_id: local.id.clone(),
            install_dir: local.root.to_string_lossy().into_owned(),
            update_available: false,
        },
    );
    db.applications.local_games.insert(local.id.clone(), local);
}

pub fn reset_interrupted(db: &mut Database) {
    for local in db.applications.local_games.values_mut() {
        if let Some(archive) = &mut local.archive
            && matches!(
                archive.state,
                ArchiveState::Downloading | ArchiveState::Extracting
            )
        {
            archive.state = ArchiveState::Incomplete;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::platform::Platform;

    #[test]
    fn downloaded_folders_require_the_matching_owner_marker_to_delete() {
        let directory = tempfile::tempdir().unwrap();
        let mut db = Database::new("games", None, "cache".into());
        let game = LocalGame {
            id: "local-owned".into(),
            name: "Owned game".into(),
            root: directory.path().to_path_buf(),
            launcher: None,
            arguments: String::new(),
            working_directory: directory.path().to_path_buf(),
            platform: Platform::Linux,
            playtime_seconds: 0,
            archive: Some(database::models::data::ArchiveInstall {
                url: "https://example.com/game.rar?token=secret".into(),
                archive_path: "cache/archive".into(),
                validator: None,
                downloaded: false,
                extracted: false,
                state: ArchiveState::Downloading,
                error: None,
            }),
        };
        std::fs::write(directory.path().join("keep.txt"), "data").unwrap();
        assert!(delete_owned_files(&game).is_err());
        assert!(directory.path().join("keep.txt").is_file());
        std::fs::write(directory.path().join(OWNER_FILE), "someone-else").unwrap();
        assert!(delete_owned_files(&game).is_err());
        register(&mut db, game.clone(), None);
        reset_interrupted(&mut db);
        assert_eq!(
            db.applications.local_games["local-owned"]
                .archive
                .as_ref()
                .unwrap()
                .state,
            ArchiveState::Incomplete
        );
        assert!(!format!("{game:?}").contains("token=secret"));
        std::fs::write(directory.path().join(OWNER_FILE), &game.id).unwrap();
        delete_owned_files(&game).unwrap();
        assert!(!directory.path().exists());
    }

    #[test]
    fn local_records_survive_serialization_without_server_cache() {
        let mut db = Database::new("games", None, "cache".into());
        let game = LocalGame {
            id: "local-test".into(),
            name: "My game".into(),
            root: "games/test".into(),
            launcher: Some("games/test/play.exe".into()),
            arguments: String::new(),
            working_directory: "games/test".into(),
            platform: Platform::Windows,
            playtime_seconds: 42,
            archive: None,
        };
        register(&mut db, game, Some("play.exe".into()));
        assert!(
            !db.applications.game_versions["local-test"]
                .user_configuration
                .enable_updates
        );
        assert_eq!(
            as_game(&db.applications.local_games["local-test"]).m_name,
            "My game"
        );
        let directory = tempfile::tempdir().unwrap();
        let mut imported = db.applications.local_games["local-test"].clone();
        imported.root = directory.path().to_path_buf();
        std::fs::write(directory.path().join("keep.txt"), "user files").unwrap();
        assert!(delete_owned_files(&imported).is_err());
        assert!(directory.path().join("keep.txt").is_file());
    }
}
