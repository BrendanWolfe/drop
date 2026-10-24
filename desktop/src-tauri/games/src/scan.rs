use std::fs;

use database::{
    Database, DownloadType, DownloadableMetadata, GameDownloadStatus, borrow_db_mut_checked,
};
use log::warn;

use crate::{
    downloads::drop_data::{DROPDATA_PATH, DropData},
    library::set_partially_installed_db,
};

pub fn scan_install_dirs() {
    let mut db_lock = borrow_db_mut_checked();
    scan_install_dirs_db(&mut db_lock);
}

fn scan_install_dirs_db(db: &mut Database) {
    for install_dir in db.applications.install_dirs.clone() {
        let Ok(files) = fs::read_dir(install_dir) else {
            continue;
        };
        for game in files.into_iter().flatten() {
            let drop_data_file = game.path().join(DROPDATA_PATH);
            if !drop_data_file.exists() {
                continue;
            }
            let drop_data = match DropData::read(&game.path()) {
                Ok(v) => v,
                Err(err) => {
                    warn!(
                        ".dropdata exists for {}, but couldn't read it. is it corrupted? {:?}",
                        game.file_name().display(),
                        err
                    );
                    continue;
                }
            };
            if matches!(
                db.applications.game_statuses.get(&drop_data.game_id),
                Some(GameDownloadStatus::Installed { .. })
            ) {
                continue;
            }

            let metadata = DownloadableMetadata::new(
                drop_data.game_id,
                drop_data.game_version,
                drop_data.target_platform,
                DownloadType::Game,
            );
            set_partially_installed_db(
                db,
                &metadata,
                drop_data.base_path.to_str().unwrap().to_string(),
                None,
                drop_data.configuration,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use database::{
        Database, GameDownloadStatus,
        models::data::{InstalledGameType, UserConfiguration},
        platform::Platform,
    };

    use super::*;

    fn test_configuration() -> UserConfiguration {
        UserConfiguration {
            launch_template: "proton run {}".to_owned(),
            override_proton_path: Some("/custom/proton".to_owned()),
            override_handler: Some("custom-handler".to_owned()),
            enable_updates: true,
        }
    }

    fn create_drop_data(
        install_dir: &Path,
        game_id: &str,
        version: &str,
        configuration: UserConfiguration,
    ) {
        let game_dir = install_dir.join(game_id);
        std::fs::create_dir_all(&game_dir).unwrap();
        DropData::new(
            game_id.to_owned(),
            version.to_owned(),
            Platform::HOST,
            game_dir.clone(),
            configuration,
            None,
        )
        .write();
        assert!(
            game_dir.join(DROPDATA_PATH).exists(),
            "drop data should be written for {game_id}"
        );
    }

    fn database_with_install_dir(install_dir: &Path) -> Database {
        let mut db = Database::new(install_dir.to_path_buf(), None, install_dir.join("cache"));
        db.applications.install_dirs = vec![install_dir.to_path_buf()];
        db
    }

    fn assert_recovered(
        db: &Database,
        install_dir: &Path,
        game_id: &str,
        version: &str,
        expected_configuration: &UserConfiguration,
    ) {
        let game_dir = install_dir.join(game_id);
        match db.applications.game_statuses.get(game_id) {
            Some(GameDownloadStatus::Installed {
                install_type: InstalledGameType::PartiallyInstalled { configuration },
                version_id,
                install_dir: recovered_install_dir,
                update_available,
            }) => {
                assert_eq!(configuration, expected_configuration);
                assert_eq!(version_id, version);
                assert_eq!(
                    recovered_install_dir,
                    &game_dir.to_str().unwrap().to_string()
                );
                assert!(!update_available);
            }
            other => panic!("expected partially installed status for {game_id}, got {other:?}"),
        }

        let metadata = db
            .applications
            .installed_game_version
            .get(game_id)
            .unwrap_or_else(|| panic!("expected metadata for {game_id}"));
        assert_eq!(metadata.id, game_id);
        assert_eq!(metadata.version, version);
        assert_eq!(metadata.target_platform, Platform::HOST);
        assert_eq!(metadata.download_type, DownloadType::Game);
    }

    #[test]
    fn scan_recovers_remote_game_from_drop_data() {
        let temp = tempfile::tempdir().unwrap();
        let configuration = test_configuration();
        create_drop_data(temp.path(), "remote-game", "1.2.3", configuration.clone());

        let mut db = database_with_install_dir(temp.path());
        db.applications
            .game_statuses
            .insert("remote-game".to_owned(), GameDownloadStatus::Remote {});

        scan_install_dirs_db(&mut db);

        assert_recovered(&db, temp.path(), "remote-game", "1.2.3", &configuration);
    }

    #[test]
    fn scan_recovers_absent_game_from_drop_data() {
        let temp = tempfile::tempdir().unwrap();
        let configuration = test_configuration();
        create_drop_data(temp.path(), "absent-game", "4.5.6", configuration.clone());

        let mut db = database_with_install_dir(temp.path());
        assert!(!db.applications.game_statuses.contains_key("absent-game"));

        scan_install_dirs_db(&mut db);

        assert_recovered(&db, temp.path(), "absent-game", "4.5.6", &configuration);
    }

    #[test]
    fn scan_preserves_installed_game_status() {
        let temp = tempfile::tempdir().unwrap();
        create_drop_data(
            temp.path(),
            "installed-game",
            "2.0.0",
            UserConfiguration::default(),
        );

        let existing_configuration = test_configuration();
        let existing_install_dir = "/existing/install".to_owned();
        let mut db = database_with_install_dir(temp.path());
        db.applications.game_statuses.insert(
            "installed-game".to_owned(),
            GameDownloadStatus::Installed {
                install_type: InstalledGameType::PartiallyInstalled {
                    configuration: existing_configuration.clone(),
                },
                version_id: "1.0.0".to_owned(),
                install_dir: existing_install_dir.clone(),
                update_available: true,
            },
        );
        db.applications.installed_game_version.insert(
            "installed-game".to_owned(),
            DownloadableMetadata::new(
                "installed-game".to_owned(),
                "1.0.0".to_owned(),
                Platform::HOST,
                DownloadType::Game,
            ),
        );

        scan_install_dirs_db(&mut db);

        match db.applications.game_statuses.get("installed-game") {
            Some(GameDownloadStatus::Installed {
                install_type: InstalledGameType::PartiallyInstalled { configuration },
                version_id,
                install_dir,
                update_available,
            }) => {
                assert_eq!(configuration, &existing_configuration);
                assert_eq!(version_id, "1.0.0");
                assert_eq!(install_dir, &existing_install_dir);
                assert!(update_available);
            }
            other => panic!("expected installed status to be preserved, got {other:?}"),
        }

        let metadata = db
            .applications
            .installed_game_version
            .get("installed-game")
            .unwrap();
        assert_eq!(metadata.version, "1.0.0");
    }
}
