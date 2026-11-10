use database::{
    ApplicationTransientStatus, Database, DownloadableMetadata, GameDownloadStatus,
    models::data::InstalledGameType, platform::Platform,
};

/// Legacy fallback allows upgrades to resolve installs before startup migration.
pub fn installed_emulator<'a>(
    db: &'a Database,
    game_id: &str,
    version_id: &str,
    platform: Platform,
) -> Option<(&'a DownloadableMetadata, &'a GameDownloadStatus)> {
    if let Some(install) = db
        .applications
        .emulator_installation(game_id, version_id, platform)
    {
        return Some((&install.metadata, &install.status));
    }
    let meta = db.applications.installed_game_version.get(game_id)?;
    if meta.version != version_id || meta.target_platform != platform {
        return None;
    }
    Some((meta, db.applications.game_statuses.get(game_id)?))
}

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
    let (meta, status) = installed_emulator(db, game_id, version_id, platform)?;
    if meta.version != version_id
        || meta.target_platform != platform
        || db
            .applications
            .transient_statuses
            .iter()
            .any(|(meta, status)| {
                meta.id == game_id
                    && meta.version == version_id
                    && meta.target_platform == platform
                    && !matches!(status, ApplicationTransientStatus::Running { .. })
            })
    {
        return None;
    }
    let GameDownloadStatus::Installed {
        install_type: InstalledGameType::Installed,
        version_id: installed_version,
        install_dir,
        ..
    } = status
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
    let installed_game = db
        .applications
        .installed_game_version
        .get(game_id)
        .is_some_and(|meta| {
            meta.version == expected_version && meta.target_platform == expected_platform
        });
    let installed_emulator = db
        .applications
        .emulator_installation(game_id, expected_version, expected_platform)
        .is_some();
    if remote.game_id != game_id
        || remote.version_id != expected_version
        || !(installed_game || installed_emulator)
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

/// Resolve only explicit standalone commands for the installed build/platform.
pub fn direct_open_launches<'a>(
    version: &'a database::GameVersion,
    platform: Platform,
    host: Platform,
) -> impl Iterator<Item = &'a database::models::data::LaunchConfiguration> {
    version.launches.iter().filter(move |launch| {
        launch.platform == platform
            && launch.platform.can_run_on(host, launch.proton_compatible)
            && launch.emulator.is_none()
            && launch
                .emulator_profile
                .as_ref()
                .and_then(|profile| profile.direct_open_command.as_ref())
                .is_some_and(|command| !command.trim().is_empty())
    })
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

/// Never delete an install containing another package (or inside another one).
/// Legacy paths are retained, so validate them as well as new managed paths.
pub fn safe_emulator_removal(
    db: &Database,
    meta: &DownloadableMetadata,
    path: &std::path::Path,
) -> Result<(), String> {
    let target = match path.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if target.parent().is_none() {
        return Err("Refusing to remove a filesystem root.".into());
    }
    for (other, status) in db
        .applications
        .emulator_installations
        .values()
        .map(|install| (&install.metadata, &install.status))
        .chain(
            db.applications
                .installed_game_version
                .values()
                .filter_map(|meta| {
                    db.applications
                        .game_statuses
                        .get(&meta.id)
                        .map(|status| (meta, status))
                }),
        )
    {
        if other.id == meta.id
            && other.version == meta.version
            && other.target_platform == meta.target_platform
        {
            continue;
        }
        if let GameDownloadStatus::Installed { install_dir, .. } = status {
            if let Ok(other_path) = std::path::Path::new(install_dir).canonicalize() {
                if target.starts_with(&other_path) || other_path.starts_with(&target) {
                    return Err("This emulator shares a folder with another installation. Move it to a separate folder before uninstalling.".into());
                }
            }
        }
    }
    Ok(())
}

pub fn emulator_in_use(db: &Database, meta: &DownloadableMetadata) -> bool {
    let dependents = required_by(db, meta);
    db.applications
        .transient_statuses
        .iter()
        .any(|(item, status)| {
            item == meta
                || (matches!(status, ApplicationTransientStatus::Running { .. })
                    && dependents.iter().any(|game| game.id == item.id))
        })
}

/// Reserve before awaiting manifests so uninstall/duplicate requests cannot race.
pub fn reserve_emulator_download(
    db: &mut Database,
    meta: &DownloadableMetadata,
) -> Result<bool, String> {
    match db.applications.transient_statuses.get(meta) {
        Some(ApplicationTransientStatus::Uninstalling { .. }) => {
            return Err("Wait for this build's uninstall to finish, then try again.".into());
        }
        Some(_) => return Ok(false),
        None => {}
    }
    if emulator_in_use(db, meta) {
        return Err("Stop games using this build before repairing it.".into());
    }
    db.applications.transient_statuses.insert(
        meta.clone(),
        ApplicationTransientStatus::Queued {
            version_id: meta.version.clone(),
        },
    );
    Ok(true)
}

pub fn begin_emulator_uninstall(
    db: &mut Database,
    id: &str,
    version: &str,
    platform: Platform,
    confirmed: &[String],
) -> Result<(DownloadableMetadata, std::path::PathBuf), String> {
    let installation = db
        .applications
        .emulator_installation(id, version, platform)
        .ok_or("This emulator build is no longer installed. Refresh and try again.")?;
    let meta = installation.metadata.clone();
    let GameDownloadStatus::Installed { install_dir, .. } = &installation.status else {
        return Err("This build is not installed.".into());
    };
    let path = std::path::PathBuf::from(install_dir);
    if !uninstall_confirmed(&required_by(db, &meta), Some(confirmed)) {
        return Err("The games using this build changed. Refresh and confirm again.".into());
    }
    if emulator_in_use(db, &meta) {
        return Err(
            "This build is in use. Stop its games or wait for its download to finish.".into(),
        );
    }
    safe_emulator_removal(db, &meta, &path)?;
    db.applications
        .transient_statuses
        .insert(meta.clone(), ApplicationTransientStatus::Uninstalling {});
    Ok((meta, path))
}

pub fn finish_emulator_uninstall(db: &mut Database, meta: &DownloadableMetadata, success: bool) {
    db.applications.transient_statuses.remove(meta);
    let key =
        database::emulator_installations::build_key(&meta.id, &meta.version, meta.target_platform);
    if success {
        db.applications.emulator_installations.remove(&key);
    } else if let Some(installation) = db.applications.emulator_installations.get_mut(&key) {
        // A filesystem error may have removed some files. Never reuse the build
        // as complete after a failed removal; keep it resumable instead.
        if let GameDownloadStatus::Installed { install_type, .. } = &mut installation.status {
            *install_type = InstalledGameType::PartiallyInstalled {
                configuration: db
                    .applications
                    .game_versions
                    .get(&meta.version)
                    .map(|version| version.user_configuration.clone())
                    .unwrap_or_default(),
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::{
        DownloadType, GameVersion, models::data::InstalledGameType, platform::Platform,
    };
    use serde_json::json;

    fn add_install(db: &mut Database, id: &str, version: &str, platform: Platform, path: &std::path::Path, emulator_version: Option<&str>) -> DownloadableMetadata {
        let emulator = emulator_version.map(|version| json!({ "gameId": "emu", "versionId": version, "launchId": format!("{version}-launch") }));
        let version_data = serde_json::from_value(json!({
            "gameId": id, "versionId": version, "versionPath": version, "onlySetup": false, "versionIndex": 0, "delta": false,
            "launches": [{ "launchId": format!("{version}-launch"), "name": "Play", "command": "launcher", "platform": platform, "protonCompatible": true, "emulator": emulator }], "setups": []
        })).unwrap();
        db.applications.game_versions.insert(version.into(), version_data);
        let meta = DownloadableMetadata::new(id.into(), version.into(), platform, if id == "emu" { DownloadType::Emulator } else { DownloadType::Game });
        db.applications.record_installation(&meta, GameDownloadStatus::Installed { install_type: InstalledGameType::Installed, install_dir: path.to_string_lossy().into_owned(), version_id: version.into(), update_available: false });
        meta
    }

    #[test]
    fn direct_open_requires_an_explicit_command_for_the_exact_platform() {
        let version: GameVersion = serde_json::from_value(json!({
            "gameId": "emu", "versionId": "v1", "versionPath": "emu.zip", "onlySetup": false, "versionIndex": 0, "delta": false,
            "launches": [
                { "launchId": "legacy", "name": "Legacy", "command": "emulator '{rom}'", "platform": "Linux" },
                { "launchId": "cli", "name": "CLI", "command": "emulator '{rom}'", "platform": "Linux", "emulatorProfile": { "mode": "cli", "configFiles": [], "directOpenCommand": "emulator --gui" } },
                { "launchId": "ui", "name": "UI", "command": "emulator", "platform": "Linux", "emulatorProfile": { "mode": "ui", "configFiles": [], "directOpenCommand": " " } },
                { "launchId": "win", "name": "Windows", "command": "emulator.exe '{rom}'", "platform": "Windows", "protonCompatible": false, "emulatorProfile": { "mode": "cli", "configFiles": [], "directOpenCommand": "emulator.exe" } }
            ], "setups": []
        })).unwrap();
        assert_eq!(
            direct_open_launches(&version, Platform::Linux, Platform::Linux)
                .map(|launch| launch.launch_id.as_str())
                .collect::<Vec<_>>(),
            ["cli"]
        );
        assert_eq!(
            direct_open_launches(&version, Platform::Windows, Platform::Linux).count(),
            0
        );
        assert_eq!(
            direct_open_launches(&version, Platform::Windows, Platform::Windows).count(),
            1
        );
    }

    #[test]
    fn standalone_gui_blocks_duplicate_launch_repair_and_uninstall_without_appearing_missing() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::new(dir.path(), None, dir.path().join("cache"));
        let meta = add_install(&mut db, "emu", "v1", Platform::Linux, dir.path(), None);
        db.applications
            .transient_statuses
            .insert(meta.clone(), ApplicationTransientStatus::Running {});
        assert!(emulator_in_use(&db, &meta));
        assert!(reserve_emulator_download(&mut db, &meta).is_ok_and(|reserved| !reserved));
        assert!(begin_emulator_uninstall(&mut db, "emu", "v1", Platform::Linux, &[]).is_err());
        assert!(
            emulator_install_path(&db, "emu", "v1", Platform::Linux, &[], Platform::Linux)
                .is_some()
        );
        db.applications.transient_statuses.remove(&meta);
        assert!(
            emulator_install_path(&db, "emu", "v1", Platform::Linux, &[], Platform::Linux)
                .is_some()
        );
    }

    #[test]
    fn installed_emulator_commands_refresh_without_redownloading_or_changing_preferences() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = Database::new(dir.path(), None, dir.path().join("cache"));
        let meta = add_install(&mut db, "emu", "v1", Platform::Linux, dir.path(), None);
        let mut remote = db.applications.game_versions["v1"].clone();
        remote.launches[0].command = "new command".into();
        db.applications
            .game_versions
            .get_mut("v1")
            .unwrap()
            .user_configuration
            .enable_updates = false;
        assert!(refresh_installed_launches(
            &mut db,
            "emu",
            "v1",
            Platform::Linux,
            remote.clone()
        ));
        assert!(
            !db.applications.game_versions["v1"]
                .user_configuration
                .enable_updates
        );
        assert_eq!(
            db.applications
                .emulator_installation("emu", "v1", Platform::Linux)
                .unwrap()
                .metadata,
            meta
        );
        assert!(!refresh_installed_launches(
            &mut db,
            "emu",
            "v1",
            Platform::Windows,
            remote.clone()
        ));
        db.applications
            .transient_statuses
            .insert(meta, ApplicationTransientStatus::Running {});
        remote.launches.clear();
        assert!(!refresh_installed_launches(
            &mut db,
            "emu",
            "v1",
            Platform::Linux,
            remote
        ));
    }

    #[test]
    fn games_resolve_different_emulator_versions_without_switching_or_redownloading() {
        let root = tempfile::tempdir().unwrap();
        let mut db = Database::new(root.path(), None, root.path().join("cache"));
        for name in ["v1", "v2", "game-a", "game-b"] { std::fs::create_dir(root.path().join(name)).unwrap(); }
        let v1 = add_install(&mut db, "emu", "v1", Platform::Linux, &root.path().join("v1"), None);
        let v2 = add_install(&mut db, "emu", "v2", Platform::Linux, &root.path().join("v2"), None);
        let a = add_install(&mut db, "game-a", "game-a-v1", Platform::Linux, &root.path().join("game-a"), Some("v1"));
        add_install(&mut db, "game-b", "game-b-v1", Platform::Linux, &root.path().join("game-b"), Some("v2"));
        assert!(missing_emulators(&db, "game-a", Platform::Linux).is_empty());
        assert!(missing_emulators(&db, "game-b", Platform::Linux).is_empty());
        assert_eq!(emulator_install_path(&db, "emu", "v1", Platform::Linux, &["v1-launch".into()], Platform::Linux).unwrap(), root.path().join("v1").to_str().unwrap());
        assert_eq!(emulator_install_path(&db, "emu", "v2", Platform::Linux, &["v2-launch".into()], Platform::Linux).unwrap(), root.path().join("v2").to_str().unwrap());
        assert!(emulator_install_path(&db, "emu", "v1", Platform::Windows, &[], Platform::Linux).is_none());
        assert!(emulator_install_path(&db, "emu", "v1", Platform::Linux, &["v2-launch".into()], Platform::Linux).is_none());
        assert_eq!(required_by(&db, &v1)[0].id, "game-a");
        assert_eq!(required_by(&db, &v2)[0].id, "game-b");
        // Running one build blocks its deletion/repair, not other builds.
        db.applications.transient_statuses.insert(a, ApplicationTransientStatus::Running {});
        assert!(begin_emulator_uninstall(&mut db, "emu", "v1", Platform::Linux, &["game-a".into()]).is_err());
        assert!(reserve_emulator_download(&mut db, &v1).is_err());
        assert!(reserve_emulator_download(&mut db, &v2).unwrap());
        assert!(!reserve_emulator_download(&mut db, &v2).unwrap());
        assert!(missing_emulators(&db, "game-a", Platform::Linux).is_empty());
        assert_eq!(missing_emulators(&db, "game-b", Platform::Linux).len(), 1);
        db.applications.transient_statuses.clear();
        assert!(begin_emulator_uninstall(&mut db, "emu", "v1", Platform::Linux, &[]).is_err());
        let (meta, path) = begin_emulator_uninstall(&mut db, "emu", "v1", Platform::Linux, &["game-a".into()]).unwrap();
        assert!(reserve_emulator_download(&mut db, &v1).is_err());
        std::fs::remove_dir_all(path).unwrap();
        finish_emulator_uninstall(&mut db, &meta, true);
        assert_eq!(missing_emulators(&db, "game-a", Platform::Linux).len(), 1);
        assert!(missing_emulators(&db, "game-b", Platform::Linux).is_empty());
        assert!(root.path().join("v2").exists());
        assert!(root.path().join("game-a").exists());
        assert_eq!(db.applications.installed_game_version["game-a"].version, "game-a-v1");
    }

    #[test]
    fn failed_uninstall_is_resumable_and_never_reused_as_a_complete_build() {
        let root = tempfile::tempdir().unwrap();
        let mut db = Database::new(root.path(), None, root.path().join("cache"));
        let meta = add_install(&mut db, "emu", "v1", Platform::Linux, root.path(), None);
        let (reserved, _) = begin_emulator_uninstall(&mut db, "emu", "v1", Platform::Linux, &[]).unwrap();
        finish_emulator_uninstall(&mut db, &reserved, false);
        assert!(db.applications.emulator_installation("emu", "v1", Platform::Linux).is_some());
        assert!(emulator_install_path(&db, "emu", "v1", Platform::Linux, &[], Platform::Linux).is_none());
        assert!(reserve_emulator_download(&mut db, &meta).unwrap());
    }

    #[test]
    fn uninstall_refuses_overlapping_legacy_folders() {
        let root = tempfile::tempdir().unwrap();
        let nested = root.path().join("nested"); std::fs::create_dir(&nested).unwrap();
        let mut db = Database::new(root.path(), None, root.path().join("cache"));
        add_install(&mut db, "emu", "v1", Platform::Linux, root.path(), None);
        add_install(&mut db, "emu", "v2", Platform::Linux, &nested, None);
        assert!(begin_emulator_uninstall(&mut db, "emu", "v1", Platform::Linux, &[]).is_err());
        assert!(begin_emulator_uninstall(&mut db, "emu", "v2", Platform::Linux, &[]).is_err());
    }

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
