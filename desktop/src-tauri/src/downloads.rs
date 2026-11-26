use std::{
    path::PathBuf,
    sync::{Arc, nonpoison::Mutex},
    time::Duration,
};

use crate::{
    AppState,
    games::fetch_game_version_options_logic,
    version_options::{configuration_for_build, contains_build},
};
use database::{
    DownloadType, DownloadableMetadata, GameDownloadStatus, borrow_db_checked,
    borrow_db_mut_checked, install_locations::InstallLocationKind, models::data::InstalledGameType,
    platform::Platform,
};
use download_manager::{
    DOWNLOAD_MANAGER, downloadable::Downloadable, error::ApplicationDownloadError,
};
use games::downloads::download_agent::{CompanionRequest, GameDownloadAgent};
use remote::{
    auth::generate_authorization_header, requests::generate_url, utils::DROP_CLIENT_ASYNC,
};
use tauri::Manager;

// Only bounds the small metadata requests used to submit companion jobs;
// archive indexing happens later in the download worker.
const COMPANION_SUBMIT_TIMEOUT: Duration = Duration::from_secs(30);

#[tauri::command]
#[allow(clippy::too_many_arguments)] // Keep the flat IPC fields backwards compatible.
pub async fn download_game(
    game_id: String,
    version_id: String,
    target_platform: Platform,
    install_dir: Option<usize>,
    install_path: Option<PathBuf>,
    reuse_installed: Option<bool>,
    required_launch_ids: Option<Vec<String>>,
    enable_updates: bool,
    state: tauri::State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), ApplicationDownloadError> {
    let report_game_id = game_id.clone();
    let result = async {
        // Keep the old IPC field accepted; immutable emulator builds now always reuse.
        let _ = reuse_installed;
        if borrow_db_checked()
            .applications
            .local_games
            .contains_key(&game_id)
        {
            return Err(ApplicationDownloadError::Archive(
                "Local games are not downloaded from the Drop server.".into(),
            ));
        }
        // Cache metadata for automatically installed dependencies as well as games.
        let game = crate::games::fetch_game(game_id.clone(), state.clone()).await?;
        let location_kind = if game.game.game_type == "Emulator" {
            InstallLocationKind::Emulator
        } else {
            InstallLocationKind::Game
        };
        let is_emulator = game.game.game_type == "Emulator";
        if is_emulator {
            borrow_db_mut_checked()
                .applications
                .migrate_emulator(&game_id);
        }
        // Complete immutable builds are always reused, including repair requests.
        if is_emulator
            && games::emulators::emulator_install_path(
                &borrow_db_checked(),
                &game_id,
                &version_id,
                target_platform,
                required_launch_ids.as_deref().unwrap_or(&[]),
                Platform::HOST,
            )
            .is_some()
        {
            return Ok(());
        }
        let options = fetch_game_version_options_logic(game_id.clone(), state.clone()).await?;
        if !contains_build(&options, &version_id, target_platform) {
            return Err(ApplicationDownloadError::BuildUnavailable);
        }
        let sender = { DOWNLOAD_MANAGER.get_sender().clone() };

        let meta = DownloadableMetadata {
            id: game_id,
            version: version_id,
            target_platform,
            download_type: if is_emulator {
                DownloadType::Emulator
            } else {
                DownloadType::Game
            },
        };

        let version_name = options
            .iter()
            .find(|option| option.version_id == meta.version && option.platform == target_platform)
            .map(|option| option.folder_version_name().to_owned());
        let installer_flow = options.iter().any(|option| {
            option.version_id == meta.version
                && option.platform == target_platform
                && option.installer_flow()
        });

        {
            let db = borrow_db_checked();
            if !is_emulator
                && let Some(installed) = db.applications.installed_game_version.get(&meta.id)
                && let Ok(path) = db.applications.resolve_install_path(installed)
            {
                let is_installer = db.applications.game_versions.get(&installed.version)
                    .is_some_and(|version| version.locate_after_setup)
                    || games::downloads::installer_files::has_inventory(&path);
                if is_installer != installer_flow {
                    return Err(ApplicationDownloadError::Archive(
                        "Remove the current download before switching between a game and an installer package.".into(),
                    ));
                }
            }
            let status = db.applications.transient_statuses.get(&meta);

            if matches!(
                status,
                Some(database::ApplicationTransientStatus::Uninstalling {})
            ) {
                return Err(ApplicationDownloadError::Archive(
                    "Wait for the current uninstall to finish, then try Update again.".into(),
                ));
            }
            if status.is_some() {
                return Ok(());
            }
        };

        let configuration = {
            let db = borrow_db_checked();
            let existing = db
                .applications
                .installed_game_version
                .get(&meta.id)
                .and_then(|installed| {
                    db.applications
                        .game_versions
                        .get(&installed.version)
                        .map(|version| (installed.target_platform, &version.user_configuration))
                });
            configuration_for_build(existing, target_platform, enable_updates && !is_emulator)
        };

        let base_dir = {
            let db_lock = borrow_db_checked();

            db_lock
                .applications
                .resolve_install_directory(location_kind, install_dir, install_path.as_deref())
                .map_err(ApplicationDownloadError::Archive)?
        };

        {
            let mut db = borrow_db_mut_checked();
            if is_emulator {
                if !games::emulators::reserve_emulator_download(&mut db, &meta)
                    .map_err(ApplicationDownloadError::Archive)?
                {
                    return Ok(());
                }
            } else {
                games::downloads::integrity::reserve(&mut db, &meta)
                    .map_err(ApplicationDownloadError::Archive)?;
            }
        }
        let game_download_agent = if installer_flow {
            GameDownloadAgent::new_installer(
                meta.clone(),
                base_dir,
                sender,
                DOWNLOAD_MANAGER.clone_depot_manager(),
                configuration,
            )
            .await
        } else {
            GameDownloadAgent::new(
                meta.clone(),
                base_dir,
                version_name,
                sender,
                DOWNLOAD_MANAGER.clone_depot_manager(),
                configuration,
            )
            .await
        };
        let game_download_agent = match game_download_agent {
            Ok(agent) => agent,
            Err(error) => {
                {
                    borrow_db_mut_checked()
                        .applications
                        .transient_statuses
                        .remove(&meta);
                }
                return Err(error);
            }
        };

        let package = installer_flow.then(|| (game_download_agent.dropdata.base_path.clone(), target_platform));
        let game_download_agent =
            Arc::new(Box::new(game_download_agent) as Box<dyn Downloadable + Send + Sync>);

        if let Err(error) = DOWNLOAD_MANAGER.queue_download(game_download_agent).await {
            borrow_db_mut_checked().applications.transient_statuses.remove(&meta);
            return Err(ApplicationDownloadError::Archive(error.to_string()));
        }

        if let Some(package) = package {
            let app = app.clone();
            tauri::async_runtime::spawn(async move {
                let failures = prepare_installer_companions(&meta.id, &meta.version, package, app.state(), &app).await;
                if !failures.is_empty() {
                    utils::app_emit!(&app, "installer_companion_error", (&meta.id, failures.join("\n")));
                }
            });
        }
        Ok(())
    }
    .await;
    if let Err(error) = &result {
        crate::activity_reporting::report_client_error(
            app,
            "download".into(),
            error.to_string(),
            Some("Preparing download".into()),
            Some(report_game_id),
        );
    }
    result
}

async fn companion_version(game: &str, version: &str) -> Result<database::GameVersion, String> {
    let url = generate_url(&["/api/v1/client/game", game, "version", version], &[])
        .map_err(|e| e.to_string())?;
    DROP_CLIENT_ASYNC
        .get(url)
        .header(
            "Authorization",
            generate_authorization_header().map_err(|e| e.to_string())?,
        )
        .timeout(COMPANION_SUBMIT_TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())
}

async fn prepare_installer_companions(
    id: &str,
    version: &str,
    package: (PathBuf, Platform),
    state: tauri::State<'_, Mutex<AppState>>,
    app: &tauri::AppHandle,
) -> Vec<String> {
    let version = match companion_version(id, version).await {
        Ok(version) if version.locate_after_setup => version,
        Ok(_) => return vec![],
        Err(error) => return vec![format!("Could not load installer companions: {error}")],
    };
    let mut failures = vec![];
    for asset in &version.installer_assets {
        if let Err(error) =
            queue_installer_asset(id, &version.version_id, &package.0, package.1, asset).await
        {
            failures.push(format!("{}: {error}", asset.name));
        }
    }
    let mut groups = std::collections::BTreeMap::<_, Vec<_>>::new();
    for input in version
        .installer_library_inputs
        .into_iter()
        .filter(|input| input.game_id != id)
    {
        groups
            .entry((input.game_id.clone(), input.version_id.clone()))
            .or_default()
            .push(input);
    }
    for ((source, pinned), inputs) in groups {
        let name = inputs[0].game_name.clone();
        if let Err(error) = queue_companion_source(source, pinned, inputs, state.clone(), app).await
        {
            failures.push(format!("{name}: {error}"));
        }
    }
    failures
}

async fn queue_installer_asset(
    id: &str,
    version: &str,
    package: &std::path::Path,
    platform: Platform,
    asset: &database::models::data::InstallerAsset,
) -> Result<(), ApplicationDownloadError> {
    use games::downloads::installer_files::{has_inventory, ready_asset};
    let meta = DownloadableMetadata::new(
        id.into(),
        asset.id.clone(),
        platform,
        DownloadType::InstallerAsset,
    );
    {
        let mut db = borrow_db_mut_checked();
        if db.applications.transient_statuses.contains_key(&meta)
            || ready_asset(package, id, asset).is_some()
        {
            return Ok(());
        }
        let owner =
            DownloadableMetadata::new(id.into(), version.into(), platform, DownloadType::Game);
        let active = db.applications.transient_statuses.get(&owner);
        let same_install =
            db.applications
                .installed_game_version
                .get(id)
                .is_some_and(|installed| {
                    installed.version == owner.version
                        && installed.target_platform == owner.target_platform
                        && installed.download_type == owner.download_type
                })
                && db
                    .applications
                    .resolve_install_path(&owner)
                    .is_ok_and(|path| path == package);
        if (!same_install
            && !matches!(
                active,
                Some(
                     database::ApplicationTransientStatus::Queued { version_id }
                         | database::ApplicationTransientStatus::Downloading { version_id }
                         | database::ApplicationTransientStatus::Extracting { version_id }
                         | database::ApplicationTransientStatus::Validating { version_id }
                ) if version_id == &owner.version
            ))
            || matches!(
                active,
                Some(database::ApplicationTransientStatus::Uninstalling {})
            )
            || !has_inventory(package)
        {
            return Err(ApplicationDownloadError::Archive(
                "The matching installer download is no longer available.".into(),
            ));
        }
        db.applications.transient_statuses.insert(
            meta.clone(),
            database::ApplicationTransientStatus::Queued {
                version_id: meta.version.clone(),
            },
        );
    }
    let result = async {
        let agent = GameDownloadAgent::new_installer_asset(
            meta.clone(),
            package,
            asset,
            DOWNLOAD_MANAGER.get_sender(),
            DOWNLOAD_MANAGER.clone_depot_manager(),
        )?;
        DOWNLOAD_MANAGER
            .queue_download(Arc::new(
                Box::new(agent) as Box<dyn Downloadable + Send + Sync>
            ))
            .await
            .map_err(|error| ApplicationDownloadError::Archive(error.to_string()))
    }
    .await;
    if result.is_err() {
        borrow_db_mut_checked()
            .applications
            .transient_statuses
            .remove(&meta);
    }
    result
}

async fn queue_companion_source(
    id: String,
    version: String,
    inputs: Vec<database::models::data::InstallerLibraryInput>,
    state: tauri::State<'_, Mutex<AppState>>,
    app: &tauri::AppHandle,
) -> Result<(), String> {
    use crate::local_games::{LibraryInputStatus, installed_library_input};
    let pending_inputs = |db: &database::Database| -> Result<Vec<_>, String> {
        let mut pending = vec![];
        for input in &inputs {
            match installed_library_input(db, input) {
                Ok(_) => (),
                Err(LibraryInputStatus::WrongVersion) => return Err("A different version is installed. Switch to the pinned version before retrying.".into()),
                Err(LibraryInputStatus::Busy) => return Err("Wait for the source game's current operation, then retry missing files.".into()),
                Err(_) => pending.push(input.clone()),
            }
        }
        Ok(pending)
    };
    if pending_inputs(&borrow_db_checked())?.is_empty() {
        return Ok(());
    }

    let url = generate_url(&["/api/v1/client/collection/default/entry"], &[])
        .map_err(|e| e.to_string())?;
    let response = DROP_CLIENT_ASYNC
        .post(url)
        .header(
            "Authorization",
            generate_authorization_header().map_err(|e| e.to_string())?,
        )
        .json(&serde_json::json!({"id": &id}))
        .timeout(COMPANION_SUBMIT_TIMEOUT)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err("Could not add the source game to your library.".into());
    }
    utils::app_emit!(app, "update_library", ());
    let game = tokio::time::timeout(
        COMPANION_SUBMIT_TIMEOUT,
        crate::games::fetch_game(id.clone(), state.clone()),
    )
    .await
    .map_err(|_| "Timed out loading the source game.")?
    .map_err(|e| e.to_string())?;
    if game.game.game_type != "Game" {
        return Err("This source is not a library game.".into());
    }
    let options = tokio::time::timeout(
        COMPANION_SUBMIT_TIMEOUT,
        fetch_game_version_options_logic(id.clone(), state),
    )
    .await
    .map_err(|_| "Timed out loading the source game’s versions.")?
    .map_err(|e| e.to_string())?;
    let option = options
        .iter()
        .find(|option| option.version_id == version && !option.installer_flow())
        .ok_or("The pinned version is no longer available for this device.")?;
    let meta = DownloadableMetadata::new(id, version, option.platform, DownloadType::Game);
    let (directory, new_folder, configuration, request) = {
        let mut db = borrow_db_mut_checked();
        let inputs = pending_inputs(&db)?;
        if inputs.is_empty() {
            return Ok(());
        }
        let apps = &db.applications;
        let (directory, new_folder) =
            if let Some(existing) = apps.installed_game_version.get(&meta.id) {
                if existing.version != meta.version
                    || existing.target_platform != meta.target_platform
                    || existing.download_type != meta.download_type
                {
                    return Err("A different build is installed.".into());
                }
                let path = apps.resolve_install_path(existing)?;
                games::library::validate_managed_directory(&db, existing, &path)?;
                (path, None)
            } else {
                let base = apps.resolve_install_directory(InstallLocationKind::Game, None, None)?;
                (base, Some(game.game.library_path.as_str()))
            };
        let current = apps.game_statuses.get(&meta.id);
        let already_complete = matches!(
            current,
            Some(GameDownloadStatus::Installed {
                install_type: InstalledGameType::Installed | InstalledGameType::SetupRequired,
                ..
            })
        );
        let configuration = match current {
            Some(GameDownloadStatus::Installed {
                install_type: InstalledGameType::PartiallyInstalled { configuration, .. },
                ..
            }) => configuration.clone(),
            _ => apps
                .game_versions
                .get(&meta.version)
                .map(|item| item.user_configuration.clone())
                .unwrap_or_default(),
        };
        games::downloads::integrity::reserve(&mut db, &meta)?;
        (
            directory,
            new_folder,
            configuration,
            CompanionRequest {
                inputs,
                missing_dependencies: option
                    .required_content
                    .iter()
                    .any(|item| item.installed_path.is_none()),
                already_complete,
            },
        )
    };
    let result = async {
        let agent = GameDownloadAgent::new_companion(
            meta.clone(),
            directory,
            new_folder,
            DOWNLOAD_MANAGER.get_sender(),
            DOWNLOAD_MANAGER.clone_depot_manager(),
            configuration,
            request,
        )
        .map_err(|e| e.to_string())?;
        DOWNLOAD_MANAGER
            .queue_download(Arc::new(
                Box::new(agent) as Box<dyn Downloadable + Send + Sync>
            ))
            .await
            .map_err(|e| e.to_string())
    }
    .await;
    if result.is_err() {
        borrow_db_mut_checked()
            .applications
            .transient_statuses
            .remove(&meta);
    }
    utils::app_emit!(app, "update_library", ());
    result
}

#[tauri::command]
pub async fn retry_installer_companions(
    id: String,
    state: tauri::State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let (version, package) = {
        let db = borrow_db_checked();
        let meta = crate::local_games::installer_package_meta(&db, &id)?;
        (
            meta.version.clone(),
            (
                db.applications.resolve_install_path(meta)?,
                meta.target_platform,
            ),
        )
    };
    let failures = prepare_installer_companions(&id, &version, package, state, &app).await;
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n"))
    }
}

#[tauri::command]
pub async fn resume_download(
    game_id: String,
    version_id: Option<String>,
    target_platform: Option<Platform>,
    app: tauri::AppHandle,
) -> Result<(), ApplicationDownloadError> {
    let report_game_id = game_id.clone();
    let result = async {
        if borrow_db_checked()
            .applications
            .local_games
            .contains_key(&game_id)
        {
            return Err(ApplicationDownloadError::Archive(
                "Resume this archive from its local game page.".into(),
            ));
        }
        let (meta, (install_dir, configuration)) = {
            let db_lock = borrow_db_checked();
            let exact = match (&version_id, target_platform) {
                (Some(version), Some(platform)) => Some(
                    db_lock
                        .applications
                        .emulator_installation(&game_id, version, platform)
                        .ok_or(ApplicationDownloadError::InvalidCommand)?,
                ),
                (None, None) => None,
                _ => return Err(ApplicationDownloadError::InvalidCommand),
            };
            let status = if let Some(install) = exact {
                install.status.clone()
            } else {
                db_lock
                    .applications
                    .game_statuses
                    .get(&game_id)
                    .ok_or(ApplicationDownloadError::InvalidCommand)?
                    .clone()
            };

            let meta = if let Some(install) = exact {
                install.metadata.clone()
            } else {
                db_lock
                    .applications
                    .installed_game_version
                    .get(&game_id)
                    .ok_or(ApplicationDownloadError::InvalidCommand)?
                    .clone()
            };

            let path = db_lock
                .applications
                .resolve_install_path(&meta)
                .map_err(ApplicationDownloadError::Archive)?;
            let install_dir = match status {
                GameDownloadStatus::Installed {
                    install_type: InstalledGameType::PartiallyInstalled { configuration, .. },
                    ..
                } => Ok((path, configuration)),
                _ => Err(ApplicationDownloadError::InvalidCommand),
            }?;
            (meta, install_dir)
        };

        let sender = DOWNLOAD_MANAGER.get_sender();

        let emulator = meta.download_type == DownloadType::Emulator;
        {
            let mut db = borrow_db_mut_checked();
            if emulator {
                if !games::emulators::reserve_emulator_download(&mut db, &meta)
                    .map_err(ApplicationDownloadError::Archive)?
                {
                    return Ok(());
                }
            } else {
                games::downloads::integrity::reserve(&mut db, &meta)
                    .map_err(ApplicationDownloadError::Archive)?;
            }
        }

        let agent = GameDownloadAgent::new_at_path(
            meta.clone(),
            install_dir,
            sender,
            DOWNLOAD_MANAGER.clone_depot_manager(),
            configuration,
        )
        .await;
        let agent = match agent {
            Ok(agent) => agent,
            Err(error) => {
                {
                    borrow_db_mut_checked()
                        .applications
                        .transient_statuses
                        .remove(&meta);
                }
                return Err(error);
            }
        };
        let game_download_agent = Arc::new(Box::new(agent) as Box<dyn Downloadable + Send + Sync>);

        DOWNLOAD_MANAGER
            .queue_download(game_download_agent)
            .await
            .map_err(|error| ApplicationDownloadError::Archive(error.to_string()))?;
        Ok(())
    }
    .await;
    if let Err(error) = &result {
        crate::activity_reporting::report_client_error(
            app,
            "download".into(),
            error.to_string(),
            Some("Resuming download".into()),
            Some(report_game_id),
        );
    }
    result
}

#[tauri::command]
pub async fn verify_file_integrity(
    game_id: String,
    version_id: Option<String>,
    target_platform: Option<Platform>,
    app: tauri::AppHandle,
) -> Result<(), ApplicationDownloadError> {
    let (meta, path, configuration) = {
        let mut db = borrow_db_mut_checked();
        let (meta, status) = match (version_id.as_deref(), target_platform) {
            (Some(version), Some(platform)) => {
                let install = db
                    .applications
                    .emulator_installation(&game_id, version, platform)
                    .ok_or(ApplicationDownloadError::InvalidCommand)?;
                (install.metadata.clone(), install.status.clone())
            }
            (None, None) => {
                let meta = db
                    .applications
                    .installed_game_version
                    .get(&game_id)
                    .ok_or(ApplicationDownloadError::InvalidCommand)?
                    .clone();
                let status = db
                    .applications
                    .game_statuses
                    .get(&game_id)
                    .ok_or(ApplicationDownloadError::InvalidCommand)?
                    .clone();
                (meta, status)
            }
            _ => return Err(ApplicationDownloadError::InvalidCommand),
        };
        let GameDownloadStatus::Installed { install_type, .. } = status else {
            return Err(ApplicationDownloadError::InvalidCommand);
        };
        let configuration = match install_type {
            InstalledGameType::PartiallyInstalled { configuration, .. } => configuration,
            _ => db
                .applications
                .game_versions
                .get(&meta.version)
                .map(|version| version.user_configuration.clone())
                .unwrap_or_default(),
        };
        let path = db
            .applications
            .resolve_install_path(&meta)
            .map_err(ApplicationDownloadError::Archive)?;
        games::library::validate_managed_directory(&db, &meta, &path)
            .map_err(ApplicationDownloadError::Archive)?;
        games::downloads::integrity::reserve(&mut db, &meta)
            .map_err(ApplicationDownloadError::Archive)?;
        (meta, path, configuration)
    };
    let result = async {
        let agent = GameDownloadAgent::verify_at_path(
            meta.clone(),
            path,
            DOWNLOAD_MANAGER.get_sender(),
            DOWNLOAD_MANAGER.clone_depot_manager(),
            configuration,
        )
        .await?;
        let agent = Arc::new(Box::new(agent) as Box<dyn Downloadable + Send + Sync>);
        DOWNLOAD_MANAGER
            .queue_download(agent)
            .await
            .map_err(|error| ApplicationDownloadError::Archive(error.to_string()))
    }
    .await;
    if result.is_err() {
        borrow_db_mut_checked()
            .applications
            .transient_statuses
            .remove(&meta);
        utils::app_emit!(&app, "update_library", ());
    }
    result
}
