use crate::AppState;
use database::{
    GameDownloadStatus, GameVersion, borrow_db_checked, borrow_db_mut_checked,
    models::data::InstalledGameType, platform::Platform,
};
use games::{emulators::required_by, library::Game};
use remote::cache::get_cached_object_db;
use serde::Serialize;
use std::{sync::nonpoison::Mutex, time::Duration};
use tauri::AppHandle;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmulatorDependent {
    game_id: String,
    name: String,
    version_name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledEmulator {
    game_id: String,
    name: String,
    icon_object_id: Option<String>,
    banner_object_id: Option<String>,
    short_description: Option<String>,
    version_id: String,
    version_name: String,
    platform: Platform,
    install_dir: String,
    install_type: InstalledGameType,
    busy: bool,
    direct_open_launches: Vec<DirectOpenLaunch>,
    required_by: Vec<EmulatorDependent>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectOpenLaunch {
    launch_id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MissingEmulator {
    #[serde(flatten)]
    dependency: games::emulators::MissingEmulator,
    name: String,
    version_name: String,
}

// Tauri requires Result for async commands with a borrowed State argument.
#[tauri::command]
pub async fn fetch_missing_emulators(
    game_id: String,
    state: tauri::State<'_, Mutex<AppState>>,
    app_handle: AppHandle,
) -> Result<Vec<MissingEmulator>, String> {
    if let Err(error) = refresh_launch_requirements(&game_id, &state, &app_handle).await {
        log::warn!("Could not refresh emulator requirements for {game_id}: {error:?}");
    }
    let db = borrow_db_checked();
    Ok(
        games::emulators::missing_emulators(&db, &game_id, Platform::HOST)
            .into_iter()
            .map(|dependency| {
                let game =
                    get_cached_object_db::<Game>(&format!("game/{}", dependency.game_id), &db).ok();
                let version = db.applications.game_versions.get(&dependency.version_id);
                MissingEmulator {
                    name: game
                        .map(|game| game.m_name)
                        .unwrap_or_else(|| dependency.game_id.clone()),
                    version_name: version
                        .map(|version| {
                            version
                                .display_name
                                .clone()
                                .unwrap_or_else(|| version.version_path.clone())
                        })
                        .unwrap_or_else(|| dependency.version_id.clone()),
                    dependency,
                }
            })
            .collect(),
    )
}

async fn refresh_launch_requirements(
    game_id: &str,
    state: &tauri::State<'_, Mutex<AppState>>,
    app_handle: &AppHandle,
) -> Result<(), remote::error::RemoteAccessError> {
    if state.lock().status != ::client::app_status::AppStatus::SignedIn {
        return Ok(());
    }
    let meta = {
        let db = borrow_db_checked();
        if db.settings.force_offline || db.applications.local_games.contains_key(game_id) {
            return Ok(());
        }
        db.applications.installed_game_version.get(game_id).cloned()
    };
    let Some(meta) = meta else { return Ok(()) };
    let version = fetch_remote_version(game_id, &meta.version).await?;
    let mut db = borrow_db_mut_checked();
    if games::emulators::refresh_installed_launches(
        &mut db,
        game_id,
        &meta.version,
        meta.target_platform,
        version,
    ) {
        games::library::push_game_update(
            app_handle,
            &meta.id,
            db.applications.game_versions.get(&meta.version).cloned(),
            games::state::GameStatusManager::fetch_state(&meta.id, &db),
        );
    }
    Ok(())
}

async fn fetch_remote_version(
    game_id: &str,
    version_id: &str,
) -> Result<GameVersion, remote::error::RemoteAccessError> {
    let url = remote::requests::generate_url(
        &["/api/v1/client/game", game_id, "version", version_id],
        &[],
    )?;
    let response = remote::utils::DROP_CLIENT_ASYNC
        .get(url)
        .header(
            "Authorization",
            remote::auth::generate_authorization_header()?,
        )
        .timeout(Duration::from_secs(4))
        .send()
        .await?;
    if !response.status().is_success() {
        return Err(remote::error::RemoteAccessError::InvalidResponse(
            response.json().await?,
        ));
    }
    Ok(response.json().await?)
}

/// The installation database is authoritative, so this also works offline and
/// includes emulators that are no longer in the user's server library.
#[tauri::command]
pub async fn fetch_installed_emulators(
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<Vec<InstalledEmulator>, String> {
    // Refresh commands for existing installs, but keep cached commands offline.
    let signed_in = state.lock().status == ::client::app_status::AppStatus::SignedIn;
    let refresh = {
        let db = borrow_db_checked();
        if signed_in && !db.settings.force_offline {
            db.applications
                .emulator_installations
                .values()
                .filter(|install| !games::emulators::emulator_in_use(&db, &install.metadata))
                .map(|install| install.metadata.clone())
                .collect::<Vec<_>>()
        } else {
            vec![]
        }
    };
    // An unreachable server should cost one timeout, not one per installed build.
    let requests = refresh
        .into_iter()
        .map(|meta| {
            tauri::async_runtime::spawn(async move {
                match fetch_remote_version(&meta.id, &meta.version).await {
                    Ok(version) => {
                        games::emulators::refresh_installed_launches(
                            &mut borrow_db_mut_checked(),
                            &meta.id,
                            &meta.version,
                            meta.target_platform,
                            version,
                        );
                    }
                    Err(error) => log::warn!(
                        "Could not refresh emulator commands for {}: {error:?}",
                        meta.id
                    ),
                }
            })
        })
        .collect::<Vec<_>>();
    for request in requests {
        if let Err(error) = request.await {
            log::warn!("Could not refresh emulator commands: {error}");
        }
    }
    let db = borrow_db_checked();
    let mut emulators = Vec::new();
    for installation in db.applications.emulator_installations.values() {
        let meta = &installation.metadata;
        let GameDownloadStatus::Installed {
            version_id,
            install_dir,
            install_type,
            ..
        } = &installation.status
        else {
            continue;
        };
        if version_id != &meta.version || !std::path::Path::new(install_dir).is_dir() {
            continue;
        }
        let game = get_cached_object_db::<Game>(&format!("game/{}", meta.id), &db).ok();
        let version = db.applications.game_versions.get(&meta.version);
        let dependents = required_by(&db, meta);
        let mut dependents = dependents
            .into_iter()
            .map(|dependent| {
                let game =
                    get_cached_object_db::<Game>(&format!("game/{}", dependent.id), &db).ok();
                let version = db.applications.game_versions.get(&dependent.version);
                EmulatorDependent {
                    name: game
                        .map(|game| game.m_name)
                        .unwrap_or_else(|| dependent.id.clone()),
                    version_name: version
                        .map(|v| {
                            v.display_name
                                .clone()
                                .unwrap_or_else(|| v.version_path.clone())
                        })
                        .unwrap_or_else(|| dependent.version.clone()),
                    game_id: dependent.id,
                }
            })
            .collect::<Vec<_>>();
        dependents.sort_by(|a, b| a.name.cmp(&b.name));
        emulators.push(InstalledEmulator {
            game_id: meta.id.clone(),
            icon_object_id: game.as_ref().map(|game| game.m_icon_object_id.clone()),
            banner_object_id: game.as_ref().map(|game| game.m_banner_object_id.clone()),
            short_description: game.as_ref().map(|game| game.m_short_description.clone()),
            name: game
                .map(|game| game.m_name)
                .unwrap_or_else(|| meta.id.clone()),
            version_id: meta.version.clone(),
            version_name: version
                .map(|v| {
                    v.display_name
                        .clone()
                        .unwrap_or_else(|| v.version_path.clone())
                })
                .unwrap_or_else(|| meta.version.clone()),
            platform: meta.target_platform,
            install_dir: install_dir.clone(),
            install_type: install_type.clone(),
            busy: games::emulators::emulator_in_use(&db, meta),
            direct_open_launches: version
                .into_iter()
                .flat_map(|version| {
                    games::emulators::direct_open_launches(
                        version,
                        meta.target_platform,
                        Platform::HOST,
                    )
                })
                .map(|launch| DirectOpenLaunch {
                    launch_id: launch.launch_id.clone(),
                    name: launch.name.clone(),
                })
                .collect(),
            required_by: dependents,
        });
    }
    emulators.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(emulators)
}

#[tauri::command]
pub fn open_emulator(
    game_id: String,
    version_id: String,
    target_platform: Platform,
    launch_id: String,
) -> Result<(), String> {
    process::PROCESS_MANAGER
        .lock()
        .launch_emulator(&game_id, &version_id, target_platform, &launch_id)
        .map_err(|error| error.to_string())
}

/// Reserve and remove precisely one build; failures retain its database record.
#[tauri::command]
pub async fn uninstall_emulator(
    game_id: String,
    version_id: String,
    target_platform: Platform,
    confirmed_dependencies: Vec<String>,
    app_handle: AppHandle,
) -> Result<(), String> {
    let (meta, install_dir) = {
        let mut db = borrow_db_mut_checked();
        games::emulators::begin_emulator_uninstall(
            &mut db,
            &game_id,
            &version_id,
            target_platform,
            &confirmed_dependencies,
        )?
    };
    utils::app_emit!(&app_handle, "update_library", ());
    let result =
        tauri::async_runtime::spawn_blocking(move || match std::fs::remove_dir_all(&install_dir) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        })
        .await
        .map_err(|error| error.to_string())
        .and_then(|result| result);
    {
        let mut db = borrow_db_mut_checked();
        games::emulators::finish_emulator_uninstall(&mut db, &meta, result.is_ok());
    }
    utils::app_emit!(&app_handle, "update_library", ());
    result
}
