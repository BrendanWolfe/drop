use database::{
    ApplicationTransientStatus, Database, DownloadableMetadata, GameDownloadStatus,
    models::data::InstalledGameType, platform::Platform,
};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingEmulator {
    pub game_id: String,
    pub version_id: String,
    pub platform: Platform,
}

/// Reuse only a complete, idle installation of the exact build and launches.
pub fn emulator_install_path(
    db: &Database,
    game_id: &str,
    version_id: &str,
    platform: Platform,
    launch_ids: &[String],
    host: Platform,
) -> Option<String> {
    let meta = db.applications.installed_game_version.get(game_id)?;
    if meta.version != version_id
        || meta.target_platform != platform
        || db
            .applications
            .transient_statuses
            .iter()
            .any(|(meta, status)| {
                meta.id == game_id && !matches!(status, ApplicationTransientStatus::Running { .. })
            })
    {
        return None;
    }
    let GameDownloadStatus::Installed {
        install_type: InstalledGameType::Installed,
        version_id: installed_version,
        install_dir,
        ..
    } = db.applications.game_statuses.get(game_id)?
    else {
        return None;
    };
    if installed_version != version_id {
        return None;
    }
    if !std::path::Path::new(install_dir).is_dir() {
        return None;
    }
    let version = db.applications.game_versions.get(version_id)?;
    let valid_launch = |id: Option<&str>| {
        version.launches.iter().any(|launch| {
            id.is_none_or(|id| launch.launch_id == id)
                && launch.platform == platform
                && launch.platform.can_run_on(host, launch.proton_compatible)
        })
    };
    if launch_ids.is_empty() {
        valid_launch(None).then(|| install_dir.clone())
    } else {
        launch_ids
            .iter()
            .all(|id| valid_launch(Some(id)))
            .then(|| install_dir.clone())
    }
}

/// Refresh launch metadata only. Never replace installed files, the active build,
/// pinned game version, or the user's launch/runtime/update preferences.
pub fn refresh_installed_launches(
    db: &mut Database,
    game_id: &str,
    expected_version: &str,
    expected_platform: Platform,
    remote: database::GameVersion,
) -> bool {
    if remote.game_id != game_id
        || remote.version_id != expected_version
        || !db
            .applications
            .installed_game_version
            .get(game_id)
            .is_some_and(|meta| {
                meta.version == expected_version && meta.target_platform == expected_platform
            })
        || db
            .applications
            .transient_statuses
            .keys()
            .any(|meta| meta.id == game_id)
    {
        return false;
    }
    let Some(current) = db.applications.game_versions.get_mut(expected_version) else {
        return false;
    };
    if current.launches == remote.launches {
        return false;
    }
    current.launches = remote.launches;
    true
}

/// Match the installed game's pinned launches, not the latest server version.
pub fn missing_emulators(db: &Database, game_id: &str, host: Platform) -> Vec<MissingEmulator> {
    let Some(meta) = db.applications.installed_game_version.get(game_id) else {
        return vec![];
    };
    let Some(version) = db.applications.game_versions.get(&meta.version) else {
        return vec![];
    };
    let mut missing = Vec::<MissingEmulator>::new();
    for launch in &version.launches {
        if launch.platform != meta.target_platform
            || !launch.platform.can_run_on(host, launch.proton_compatible)
        {
            continue;
        }
        let Some(dependency) = &launch.emulator else {
            continue;
        };
        let installed = emulator_install_path(
            db,
            &dependency.game_id,
            &dependency.version_id,
            meta.target_platform,
            std::slice::from_ref(&dependency.launch_id),
            host,
        )
        .is_some();
        if !installed
            && !missing.iter().any(|item| {
                item.game_id == dependency.game_id && item.version_id == dependency.version_id
            })
        {
            missing.push(MissingEmulator {
                game_id: dependency.game_id.clone(),
                version_id: dependency.version_id.clone(),
                platform: meta.target_platform,
            });
        }
    }
    missing
}

/// Only the active installation and build count, never stale cached versions.
pub fn required_by(db: &Database, emulator: &DownloadableMetadata) -> Vec<DownloadableMetadata> {
    let mut games = db.applications.installed_game_version.values().filter(|meta| {
        meta.id != emulator.id
            && matches!(db.applications.game_statuses.get(&meta.id),
                Some(GameDownloadStatus::Installed { version_id, .. }) if version_id == &meta.version)
            && db.applications.game_versions.get(&meta.version).is_some_and(|version| {
                version.launches.iter().any(|launch| {
                    launch.platform == meta.target_platform
                        && meta.target_platform == emulator.target_platform
                        && launch.emulator.as_ref().is_some_and(|dependency| {
                            dependency.game_id == emulator.id
                                && dependency.version_id == emulator.version
                        })
                })
            })
    }).cloned().collect::<Vec<_>>();
    games.sort_by(|a, b| a.id.cmp(&b.id));
    games
}

/// A stale dialog must not silently consent to newly affected games.
pub fn uninstall_confirmed(
    dependents: &[DownloadableMetadata],
    confirmed: Option<&[String]>,
) -> bool {
    dependents
        .iter()
        .all(|game| confirmed.is_some_and(|ids| ids.contains(&game.id)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{
        DownloadType, GameVersion, models::data::InstalledGameType, platform::Platform,
    };
    use serde_json::json;

    #[test]
    fn uninstall_requires_confirmation_for_every_affected_game() {
        let game = DownloadableMetadata::new(
            "game".into(),
            "v1".into(),
            Platform::Linux,
            DownloadType::Game,
        );
        let other = DownloadableMetadata::new(
            "other".into(),
            "v1".into(),
            Platform::Linux,
            DownloadType::Game,
        );
        assert!(uninstall_confirmed(&[], None));
        assert!(!uninstall_confirmed(std::slice::from_ref(&game), None));
        assert!(!uninstall_confirmed(std::slice::from_ref(&game), Some(&[])));
        let confirmed = ["game".to_owned()];
        assert!(uninstall_confirmed(
            std::slice::from_ref(&game),
            Some(&confirmed)
        ));
        assert!(!uninstall_confirmed(&[game, other], Some(&confirmed)));
    }

    #[test]
    fn metadata_refresh_preserves_files_build_and_user_preferences() {
        let mut db = Database::new("games", None, "cache".into());
        let mut original: database::GameVersion = serde_json::from_value(json!({
            "gameId": "game", "versionId": "game-v1", "displayName": "Installed game",
            "versionPath": "game.zip", "onlySetup": false, "versionIndex": 1, "delta": false,
            "launches": [], "setups": []
        }))
        .unwrap();
        original.user_configuration.launch_template = "custom {}".into();
        original.user_configuration.enable_updates = false;
        let meta = DownloadableMetadata::new(
            "game".into(),
            "game-v1".into(),
            Platform::Linux,
            DownloadType::Game,
        );
        db.applications
            .installed_game_version
            .insert("game".into(), meta.clone());
        db.applications
            .game_versions
            .insert("game-v1".into(), original.clone());
        let mut remote = original.clone();
        remote.user_configuration = Default::default();
        remote.launches = serde_json::from_value(json!([{
            "launchId": "play", "name": "Play", "command": "unchanged.rom", "platform": "Linux", "umuIdOverride": null,
            "emulator": { "gameId": "emu", "versionId": "new-emulator-version", "launchId": "new-launch" }
        }])).unwrap();
        assert!(refresh_installed_launches(
            &mut db,
            "game",
            "game-v1",
            Platform::Linux,
            remote.clone()
        ));
        let saved = &db.applications.game_versions["game-v1"];
        assert_eq!(saved.user_configuration, original.user_configuration);
        assert_eq!(saved.version_id, original.version_id);
        assert_eq!(saved.version_path, original.version_path);
        assert_eq!(
            db.applications.installed_game_version["game"].target_platform,
            Platform::Linux
        );
        assert_eq!(
            saved.launches[0].emulator.as_ref().unwrap().version_id,
            "new-emulator-version"
        );
        assert!(!refresh_installed_launches(
            &mut db,
            "game",
            "game-v1",
            Platform::Windows,
            original.clone()
        ));
        db.applications
            .transient_statuses
            .insert(meta, ApplicationTransientStatus::Running {});
        assert!(!refresh_installed_launches(
            &mut db,
            "game",
            "game-v1",
            Platform::Linux,
            original
        ));
    }

    #[test]
    fn dependencies_match_active_version_and_build_without_duplicates() {
        let emulator_directory = tempfile::tempdir().unwrap();
        let mut db = Database::new("games", None, "cache".into());
        let emulator = DownloadableMetadata::new(
            "emu".into(),
            "emu-v1".into(),
            Platform::Linux,
            DownloadType::Game,
        );
        let launch = json!({
            "launchId": "launch", "name": "Play", "command": "rom", "platform": "Linux",
            "umuIdOverride": null,
            "emulator": { "launchId": "emu-launch", "gameId": "emu", "versionId": "emu-v1" }
        });
        let version: GameVersion = serde_json::from_value(json!({
            "gameId": "game", "versionId": "game-v1", "displayName": null,
            "versionPath": "1.0", "onlySetup": false, "versionIndex": 0, "delta": false,
            "launches": [launch.clone(), launch], "setups": []
        }))
        .unwrap();
        db.applications
            .game_versions
            .insert("game-v1".into(), version);
        db.applications.installed_game_version.insert(
            "game".into(),
            DownloadableMetadata::new(
                "game".into(),
                "game-v1".into(),
                Platform::Linux,
                DownloadType::Game,
            ),
        );
        db.applications.game_statuses.insert(
            "game".into(),
            GameDownloadStatus::Installed {
                install_type: InstalledGameType::Installed,
                version_id: "game-v1".into(),
                install_dir: "games/game".into(),
                update_available: false,
            },
        );

        assert_eq!(required_by(&db, &emulator).len(), 1);
        // Deleting the emulator leaves the game installed but needing Update.
        let missing = missing_emulators(&db, "game", Platform::Linux);
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].game_id, "emu");
        assert_eq!(missing[0].version_id, "emu-v1");
        assert_eq!(missing[0].platform, Platform::Linux);

        let mut emulator_version = db.applications.game_versions["game-v1"].clone();
        emulator_version.game_id = "emu".into();
        emulator_version.version_id = "emu-v1".into();
        emulator_version.launches.truncate(1);
        emulator_version.launches[0].launch_id = "emu-launch".into();
        emulator_version.launches[0].emulator = None;
        db.applications
            .game_versions
            .insert("emu-v1".into(), emulator_version);
        db.applications
            .installed_game_version
            .insert("emu".into(), emulator.clone());
        db.applications.game_statuses.insert(
            "emu".into(),
            GameDownloadStatus::Installed {
                install_type: InstalledGameType::Installed,
                version_id: "emu-v1".into(),
                install_dir: emulator_directory.path().to_string_lossy().into_owned(),
                update_available: false,
            },
        );
        // Reinstalling exactly the pinned build makes the game playable again.
        assert!(missing_emulators(&db, "game", Platform::Linux).is_empty());
        assert_eq!(
            emulator_install_path(
                &db,
                "emu",
                "emu-v1",
                Platform::Linux,
                &["emu-launch".into()],
                Platform::Linux
            ),
            Some(emulator_directory.path().to_string_lossy().into_owned())
        );
        assert!(
            emulator_install_path(
                &db,
                "emu",
                "emu-v1",
                Platform::Linux,
                &["missing-launch".into()],
                Platform::Linux
            )
            .is_none()
        );
        assert!(
            emulator_install_path(
                &db,
                "emu",
                "different-version",
                Platform::Linux,
                &[],
                Platform::Linux
            )
            .is_none()
        );
        assert!(
            emulator_install_path(
                &db,
                "emu",
                "emu-v1",
                Platform::Windows,
                &[],
                Platform::Linux
            )
            .is_none()
        );
        let installed_status = db.applications.game_statuses["emu"].clone();
        if let Some(GameDownloadStatus::Installed { install_type, .. }) =
            db.applications.game_statuses.get_mut("emu")
        {
            *install_type = InstalledGameType::PartiallyInstalled {
                configuration: Default::default(),
            };
        }
        assert!(
            emulator_install_path(&db, "emu", "emu-v1", Platform::Linux, &[], Platform::Linux)
                .is_none()
        );
        db.applications
            .game_statuses
            .insert("emu".into(), installed_status.clone());
        if let Some(GameDownloadStatus::Installed { install_dir, .. }) =
            db.applications.game_statuses.get_mut("emu")
        {
            *install_dir = emulator_directory
                .path()
                .join("missing")
                .to_string_lossy()
                .into_owned();
        }
        assert_eq!(missing_emulators(&db, "game", Platform::Linux).len(), 1);
        db.applications
            .game_statuses
            .insert("emu".into(), installed_status);
        db.applications
            .transient_statuses
            .insert(emulator.clone(), ApplicationTransientStatus::Running {});
        assert!(missing_emulators(&db, "game", Platform::Linux).is_empty());
        db.applications.transient_statuses.insert(
            emulator.clone(),
            ApplicationTransientStatus::Queued {
                version_id: "emu-v2".into(),
            },
        );
        assert_eq!(missing_emulators(&db, "game", Platform::Linux).len(), 1);
        db.applications.transient_statuses.insert(
            emulator.clone(),
            ApplicationTransientStatus::Uninstalling {},
        );
        assert_eq!(missing_emulators(&db, "game", Platform::Linux).len(), 1);
        db.applications.transient_statuses.remove(&emulator);
        db.applications
            .installed_game_version
            .get_mut("emu")
            .unwrap()
            .target_platform = Platform::Windows;
        assert_eq!(missing_emulators(&db, "game", Platform::Linux).len(), 1);
        db.applications
            .installed_game_version
            .get_mut("emu")
            .unwrap()
            .target_platform = Platform::Linux;
        db.applications
            .installed_game_version
            .get_mut("emu")
            .unwrap()
            .version = "emu-v2".into();
        assert_eq!(missing_emulators(&db, "game", Platform::Linux).len(), 1);
        db.applications.installed_game_version.remove("emu");
        db.applications
            .game_statuses
            .insert("emu".into(), GameDownloadStatus::Remote {});
        assert_eq!(missing_emulators(&db, "game", Platform::Linux).len(), 1);
        assert!(db.applications.installed_game_version.contains_key("game"));
        let mut different = emulator.clone();
        different.version = "emu-v2".into();
        assert!(required_by(&db, &different).is_empty());
        different = emulator.clone();
        different.target_platform = Platform::Windows;
        assert!(required_by(&db, &different).is_empty());
        db.applications
            .installed_game_version
            .get_mut("game")
            .unwrap()
            .target_platform = Platform::Windows;
        assert!(required_by(&db, &emulator).is_empty());
        db.applications
            .installed_game_version
            .get_mut("game")
            .unwrap()
            .target_platform = Platform::Linux;
        db.applications
            .game_statuses
            .insert("game".into(), GameDownloadStatus::Remote {});
        assert!(required_by(&db, &emulator).is_empty());
    }
}
