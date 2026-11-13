use std::fs;

use database::{
    Database, DownloadType, DownloadableMetadata, GameDownloadStatus, borrow_db_mut_checked,
};
use log::warn;

use crate::downloads::drop_data::{DROPDATA_PATH, DropData};
use database::models::data::InstalledGameType;
use std::path::PathBuf;

pub fn scan_install_dirs() -> Vec<String> {
    let directories = registered_directories(&database::borrow_db_checked());
    // Discovery reads only directory entries and .dropdata, never game payloads.
    // Do not hold the database write lock during filesystem IO.
    let discovered = discover(directories);
    let mut db_lock = borrow_db_mut_checked();
    reconcile(&mut db_lock, discovered)
}

#[cfg(test)]
fn scan_install_dirs_db(db: &mut Database) {
    let discovered = discover(registered_directories(db));
    reconcile(db, discovered);
}

fn registered_directories(db: &Database) -> Vec<PathBuf> {
    let mut directories = db.applications.install_dirs.clone();
    for directory in db
        .applications
        .install_directories(database::install_locations::InstallLocationKind::Emulator)
    {
        if !directories.contains(directory) {
            directories.push(directory.clone());
        }
    }
    directories
}

fn discover(directories: Vec<PathBuf>) -> Vec<(PathBuf, DropData)> {
    let mut discovered = Vec::new();
    for install_dir in directories {
        let Ok(files) = fs::read_dir(&install_dir) else {
            continue;
        };
        let mut paths = files
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        let legacy_emulators = install_dir.join(".drop-emulators");
        if fs::symlink_metadata(&legacy_emulators).is_ok_and(|meta| meta.is_dir()) {
            if let Ok(builds) = fs::read_dir(legacy_emulators) {
                paths.extend(builds.flatten().map(|entry| entry.path()));
            }
        }
        paths.sort();
        for path in paths {
            // Do not adopt a symlink to an installation outside this root.
            if !fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) {
                continue;
            }
            if !path.join(DROPDATA_PATH).exists() {
                continue;
            }
            match DropData::read(&path) {
                Ok(data) => discovered.push((path, data)),
                Err(error) => warn!("Couldn't read .dropdata in {}: {error}", path.display()),
            }
        }
    }
    discovered
}

fn reconcile(db: &mut Database, discovered: Vec<(PathBuf, DropData)>) -> Vec<String> {
    let mut recovered = Vec::new();
    // Adopt old emulator records in place; never relocate their existing files.
    let legacy = db
        .applications
        .installed_game_version
        .values()
        .filter(|meta| {
            remote::cache::get_cached_object_db::<crate::library::Game>(
                &format!("game/{}", meta.id),
                db,
            )
            .is_ok_and(|game| game.game_type == "Emulator")
                || db
                    .applications
                    .game_versions
                    .get(&meta.version)
                    .is_some_and(|version| {
                        version
                            .launches
                            .iter()
                            .any(|launch| launch.emulator_profile.is_some())
                    })
                || !crate::emulators::required_by(db, meta).is_empty()
        })
        .map(|meta| meta.id.clone())
        .collect::<Vec<_>>();
    for id in legacy {
        db.applications.migrate_emulator(&id);
    }
    db.applications.migrate_install_records();
    for (path, drop_data) in discovered {
        let emulator = drop_data.emulator
            || remote::cache::get_cached_object_db::<crate::library::Game>(
                &format!("game/{}", drop_data.game_id),
                db,
            )
            .is_ok_and(|game| game.game_type == "Emulator");
        let metadata = DownloadableMetadata::new(
            drop_data.game_id.clone(),
            drop_data.game_version.clone(),
            drop_data.target_platform,
            if emulator {
                DownloadType::Emulator
            } else {
                DownloadType::Game
            },
        );
        if db.applications.local_games.contains_key(&metadata.id)
            || db.applications.transient_statuses.contains_key(&metadata)
        {
            continue;
        }
        let previous = if emulator {
            db.applications
                .emulator_installation(&metadata.id, &metadata.version, metadata.target_platform)
                .map(|item| item.status.clone())
        } else {
            if let Some(existing) = db.applications.installed_game_version.get(&metadata.id) {
                if existing.version != metadata.version
                    || existing.target_platform != metadata.target_platform
                {
                    continue;
                }
            }
            db.applications.game_statuses.get(&metadata.id).cloned()
        };
        if let Some(GameDownloadStatus::Installed { install_dir, .. }) = &previous {
            // Keep an existing exact, live binding when duplicate copies exist.
            if PathBuf::from(install_dir) != path
                && DropData::read(&PathBuf::from(install_dir)).is_ok_and(|data| {
                    data.game_id == metadata.id
                        && data.game_version == metadata.version
                        && data.target_platform == metadata.target_platform
                })
            {
                continue;
            }
        }
        if !db
            .applications
            .game_versions
            .contains_key(&metadata.version)
        {
            if let Some(version) = drop_data.installed_version.filter(|version| {
                version.game_id == metadata.id && version.version_id == metadata.version
            }) {
                db.applications
                    .game_versions
                    .insert(metadata.version.clone(), version);
            }
        }
        if let Some(version) = db.applications.game_versions.get_mut(&metadata.version) {
            // Existing device configuration wins over a copied checkpoint.
            if let Some(GameDownloadStatus::Installed {
                install_type: InstalledGameType::PartiallyInstalled { configuration },
                ..
            }) = &previous
            {
                version.user_configuration = configuration.clone();
            } else if previous.is_none() || matches!(previous, Some(GameDownloadStatus::Remote {}))
            {
                version.user_configuration = drop_data.configuration;
            }
        }
        let setup_required = matches!(
            &previous,
            Some(GameDownloadStatus::Installed {
                install_type: InstalledGameType::SetupRequired,
                ..
            })
        ) || (!matches!(
            &previous,
            Some(GameDownloadStatus::Installed {
                install_type: InstalledGameType::Installed,
                ..
            })
        ) && drop_data.setup_completed == Some(false));
        let update_available = matches!(
            previous,
            Some(GameDownloadStatus::Installed {
                update_available: true,
                ..
            })
        );
        db.applications.record_installation(
            &metadata,
            GameDownloadStatus::Installed {
                version_id: metadata.version.clone(),
                install_dir: path.to_string_lossy().into_owned(),
                install_type: if setup_required {
                    InstalledGameType::SetupRequired
                } else {
                    InstalledGameType::Installed
                },
                update_available,
            },
        );
        if !emulator {
            recovered.push(metadata.id);
        }
    }
    recovered
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
                install_type: InstalledGameType::Installed,
                version_id,
                install_dir: recovered_install_dir,
                update_available,
            }) => {
                assert_eq!(version_id, version);
                assert_eq!(
                    recovered_install_dir,
                    &game_dir.to_str().unwrap().to_string()
                );
                assert!(!update_available);
            }
            other => panic!("expected trusted installed status for {game_id}, got {other:?}"),
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
        let data = DropData::read(&game_dir).unwrap();
        assert_eq!(&data.configuration, expected_configuration);
    }

    #[test]
    fn scan_recovers_emulators_from_their_separate_locations() {
        let games = tempfile::tempdir().unwrap();
        let emulators = tempfile::tempdir().unwrap();
        let configuration = test_configuration();
        create_drop_data(emulators.path(), "emulator", "1.2.3", configuration.clone());
        let mut db = database_with_install_dir(games.path());
        db.applications.emulator_install_dirs = Some(vec![emulators.path().to_path_buf()]);
        scan_install_dirs_db(&mut db);
        assert_recovered(&db, emulators.path(), "emulator", "1.2.3", &configuration);
    }

    #[test]
    fn scan_recovers_visible_side_by_side_and_legacy_emulator_builds() {
        let root = tempfile::tempdir().unwrap();
        let mut db = database_with_install_dir(root.path());
        for (version, folder) in [
            ("v1", "dolphin-5.0"),
            ("v2", "dolphin-6.0"),
            ("v3", ".drop-emulators/legacy-hex-build-Linux"),
        ] {
            let path = root.path().join(folder);
            fs::create_dir_all(&path).unwrap();
            let mut data = DropData::new(
                "dolphin".into(),
                version.into(),
                Platform::Linux,
                path,
                test_configuration(),
                None,
            );
            data.emulator = true;
            data.write();
        }
        scan_install_dirs_db(&mut db);
        scan_install_dirs_db(&mut db);
        assert_eq!(db.applications.emulator_installations.len(), 3);
        assert!(db.applications.game_statuses.is_empty());
        for (version, folder) in [
            ("v1", "dolphin-5.0"),
            ("v2", "dolphin-6.0"),
            ("v3", ".drop-emulators/legacy-hex-build-Linux"),
        ] {
            let installation = db
                .applications
                .emulator_installation("dolphin", version, Platform::Linux)
                .unwrap();
            assert!(matches!(&installation.status,
                GameDownloadStatus::Installed { install_dir, .. }
                    if std::path::Path::new(install_dir) == root.path().join(folder)));
        }
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

    #[test]
    fn one_scan_rebinds_a_moved_library_and_trusts_markers_without_payloads() {
        let base = tempfile::tempdir().unwrap();
        let old = base.path().join("old");
        let new = base.path().join("new");
        fs::create_dir(&old).unwrap();
        let mut db = database_with_install_dir(&old);
        for (id, partial) in [("complete", false), ("partial", true)] {
            create_drop_data(&old, id, "v1", test_configuration());
            let meta = DownloadableMetadata::new(
                id.into(),
                "v1".into(),
                Platform::HOST,
                DownloadType::Game,
            );
            db.applications.record_installation(
                &meta,
                GameDownloadStatus::Installed {
                    install_type: if partial {
                        InstalledGameType::PartiallyInstalled {
                            configuration: test_configuration(),
                        }
                    } else {
                        InstalledGameType::Installed
                    },
                    version_id: "v1".into(),
                    install_dir: old.join(id).to_string_lossy().into_owned(),
                    update_available: false,
                },
            );
        }
        fs::rename(&old, &new).unwrap();
        db.applications
            .add_install_directory(
                database::install_locations::InstallLocationKind::Game,
                new.clone(),
            )
            .unwrap();
        scan_install_dirs_db(&mut db);
        for id in ["complete", "partial"] {
            assert_recovered(&db, &new, id, "v1", &test_configuration());
            let meta = &db.applications.installed_game_version[id];
            assert_eq!(
                db.applications.resolve_install_path(meta).unwrap(),
                new.join(id)
            );
            assert_eq!(
                fs::read_dir(new.join(id)).unwrap().count(),
                1,
                "no payload or manifest is required"
            );
        }
        assert_eq!(db.applications.install_references.len(), 2);
        assert!(db.applications.transient_statuses.is_empty());
        let before = serialized_snapshot(&db);
        scan_install_dirs_db(&mut db);
        assert_eq!(db.applications.install_references.len(), 2);
        let mut restarted: Database = serde_json::from_str(&before).unwrap();
        restarted.applications.migrate_install_records();
        for id in ["complete", "partial"] {
            assert_recovered(&restarted, &new, id, "v1", &test_configuration());
        }
    }

    fn serialized_snapshot(db: &Database) -> String {
        let mut snapshot = db.clone();
        snapshot.applications.prepare_install_records_for_save();
        serde_json::to_string(&snapshot).unwrap()
    }

    #[test]
    fn scan_does_not_promote_an_active_download() {
        let root = tempfile::tempdir().unwrap();
        create_drop_data(root.path(), "game", "v1", test_configuration());
        let mut db = database_with_install_dir(root.path());
        let meta = DownloadableMetadata::new(
            "game".into(),
            "v1".into(),
            Platform::HOST,
            DownloadType::Game,
        );
        db.applications.transient_statuses.insert(
            meta,
            database::ApplicationTransientStatus::Downloading {
                version_id: "v1".into(),
            },
        );
        scan_install_dirs_db(&mut db);
        assert!(db.applications.game_statuses.is_empty());
    }
}
