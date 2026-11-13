use bitcode::{Decode, Encode};
use database::{
    ApplicationTransientStatus, Database, DownloadableMetadata, GameDownloadStatus, GameVersion,
    borrow_db_checked, borrow_db_mut_checked,
    models::data::{InstalledGameType, UserConfiguration},
};
use log::{debug, error, warn};
use remote::{
    auth::generate_authorization_header,
    error::{DropServerError, RemoteAccessError},
    requests::{generate_url, make_authenticated_post},
    utils::DROP_CLIENT_ASYNC,
};
use serde::{Deserialize, Serialize};
use std::fs::remove_dir_all;
use std::thread::spawn;
use tauri::AppHandle;
use utils::app_emit;

use crate::state::{GameStatusManager, GameStatusWithTransient};

#[derive(Serialize, Deserialize, Debug)]
pub struct FetchGameStruct {
    pub game: Game,
    pub status: GameStatusWithTransient,
    pub version: Option<GameVersion>,
}

impl FetchGameStruct {
    pub fn new(game: Game, status: GameStatusWithTransient, version: Option<GameVersion>) -> Self {
        Self {
            game,
            status,
            version,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, Encode, Decode)]
#[serde(rename_all = "camelCase")]
pub struct Game {
    pub id: String,
    #[serde(rename = "type")]
    pub game_type: String,
    pub m_name: String,
    pub m_short_description: String,
    pub m_description: String,
    // mDevelopers
    // mPublishers
    pub m_icon_object_id: String,
    pub m_banner_object_id: String,
    pub m_cover_object_id: String,
    pub m_image_library_object_ids: Vec<String>,
    pub m_image_carousel_object_ids: Vec<String>,
    pub library_path: String,
    #[serde(default)]
    pub playtime: Vec<GamePlaytime>,
}
#[derive(Serialize, Deserialize, Clone, Debug, Default, Encode, Decode)]
#[serde(rename_all = "camelCase")]
pub struct GamePlaytime {
    pub seconds: i32,
}
#[derive(Serialize)]
struct PlaytimeUpdate {
    seconds: u64,
}
impl Game {
    pub fn id(&self) -> &String {
        &self.id
    }
}
#[derive(serde::Serialize, Clone)]
pub struct GameUpdateEvent {
    pub game_id: String,
    pub status: (
        Option<GameDownloadStatus>,
        Option<ApplicationTransientStatus>,
    ),
    pub version: Option<GameVersion>,
}

/**
 * Called by:
 *  - on_cancel, when cancelled, for obvious reasons
 *  - when downloading, so if drop unexpectedly quits, we can resume the download. hidden by the "Downloading..." transient state, though
 *  - when scanning, to import the game
 */
pub fn set_partially_installed(
    meta: &DownloadableMetadata,
    install_dir: String,
    app_handle: Option<&AppHandle>,
    configuration: UserConfiguration,
) {
    set_partially_installed_db(
        &mut borrow_db_mut_checked(),
        meta,
        install_dir,
        app_handle,
        configuration,
    );
}

pub fn set_partially_installed_db(
    db_lock: &mut Database,
    meta: &DownloadableMetadata,
    install_dir: String,
    app_handle: Option<&AppHandle>,
    configuration: UserConfiguration,
) {
    db_lock.applications.transient_statuses.remove(meta);
    db_lock.applications.record_installation(
        meta,
        GameDownloadStatus::Installed {
            install_type: InstalledGameType::PartiallyInstalled { configuration },
            version_id: meta.version.clone(),
            install_dir,
            update_available: false,
        },
    );

    if meta.download_type == database::DownloadType::Emulator {
        if let Some(app_handle) = app_handle {
            app_emit!(app_handle, "update_library", ());
        }
        return;
    }

    if let Some(app_handle) = app_handle {
        push_game_update(
            app_handle,
            &meta.id,
            None,
            GameStatusManager::fetch_state(&meta.id, db_lock),
        );
    }
}

pub fn uninstall_game_logic(meta: DownloadableMetadata, app_handle: &AppHandle) {
    debug!("triggered uninstall for agent");
    let mut db_handle = borrow_db_mut_checked();
    // Imported folders must never enter the server-managed deletion path.
    if db_handle.applications.local_games.contains_key(&meta.id) {
        warn!("refusing server uninstall for a local game");
        return;
    }
    if db_handle
        .applications
        .transient_statuses
        .contains_key(&meta)
    {
        warn!("refusing uninstall while an installation is in use");
        return;
    }
    let install_path = match db_handle.applications.resolve_install_path(&meta) {
        Ok(path) => path,
        Err(error) => {
            warn!("{error}");
            return;
        }
    };
    if let Err(error) = validate_managed_directory(&db_handle, &meta, &install_path) {
        warn!("refusing unsafe uninstall: {error}");
        return;
    }
    db_handle
        .applications
        .transient_statuses
        .insert(meta.clone(), ApplicationTransientStatus::Uninstalling {});

    push_game_update(
        app_handle,
        &meta.id,
        None,
        GameStatusManager::fetch_state(&meta.id, &db_handle),
    );

    let previous_state = db_handle.applications.game_statuses.get(&meta.id).cloned();

    let previous_state = if let Some(state) = previous_state {
        state
    } else {
        warn!("uninstall job doesn't have previous state, failing silently");
        return;
    };

    if matches!(previous_state, GameDownloadStatus::Installed { .. }) {
        db_handle
            .applications
            .transient_statuses
            .insert(meta.clone(), ApplicationTransientStatus::Uninstalling {});

        drop(db_handle);

        let app_handle = app_handle.clone();
        spawn(move || {
            if let Err(e) = remove_dir_all(install_path) {
                error!("{e}");
                let mut db = borrow_db_mut_checked();
                db.applications.transient_statuses.remove(&meta);
                let version = db.applications.game_versions.get(&meta.version).cloned();
                push_game_update(
                    &app_handle,
                    &meta.id,
                    version,
                    GameStatusManager::fetch_state(&meta.id, &db),
                );
                return;
            }
            let mut db_handle = borrow_db_mut_checked();
            db_handle.applications.transient_statuses.remove(&meta);
            db_handle
                .applications
                .installed_game_version
                .remove(&meta.id);
            db_handle
                .applications
                .game_statuses
                .insert(meta.id.clone(), GameDownloadStatus::Remote {});
            let _ = db_handle.applications.transient_statuses.remove(&meta);

            push_game_update(
                &app_handle,
                &meta.id,
                None,
                GameStatusManager::fetch_state(&meta.id, &db_handle),
            );

            debug!("uninstalled game id {}", &meta.id);
            app_emit!(&app_handle, "update_library", ());
        });
    } else {
        warn!("invalid previous state for uninstall, failing silently.");
    }
}

pub fn get_current_meta(game_id: &String) -> Option<DownloadableMetadata> {
    borrow_db_checked()
        .applications
        .installed_game_version
        .get(game_id)
        .cloned()
}

pub async fn on_game_complete(
    meta: &DownloadableMetadata,
    configuration: UserConfiguration,
    install_dir: String,
    app_handle: &AppHandle,
) -> Result<(), RemoteAccessError> {
    // Fetch game version information from remote
    let response = generate_url(
        &["/api/v1/client/game", &meta.id, "version", &meta.version],
        &[],
    )?;
    let response = DROP_CLIENT_ASYNC
        .get(response)
        .header("Authorization", generate_authorization_header()?)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(RemoteAccessError::InvalidResponse(response.json().await?));
    }

    let mut game_version: GameVersion = response.json().await?;
    game_version.user_configuration = configuration;

    let mut handle = borrow_db_mut_checked();
    handle
        .applications
        .game_versions
        .insert(meta.version.clone(), game_version.clone());

    drop(handle);

    let setup_configuration = game_version
        .setups
        .iter()
        .find(|v| v.platform == meta.target_platform);

    if let Ok(mut data) =
        crate::downloads::drop_data::DropData::read(std::path::Path::new(&install_dir))
    {
        data.installed_version = Some(game_version.clone());
        data.setup_completed = Some(setup_configuration.is_none());
        data.configuration = game_version.user_configuration.clone();
        data.write();
    }

    let status = GameDownloadStatus::Installed {
        version_id: meta.version.clone(),
        install_dir,
        install_type: if setup_configuration.is_none() {
            InstalledGameType::Installed
        } else {
            InstalledGameType::SetupRequired
        },
        update_available: false,
    };

    let mut db_handle = borrow_db_mut_checked();
    db_handle
        .applications
        .record_installation(meta, status.clone());
    db_handle.applications.transient_statuses.remove(meta);
    drop(db_handle);
    if meta.download_type == database::DownloadType::Emulator {
        app_emit!(app_handle, "update_library", ());
        return Ok(());
    }
    app_emit!(
        app_handle,
        &format!("update_game/{}", meta.id),
        GameUpdateEvent {
            game_id: meta.id.clone(),
            status: (Some(status), None),
            version: Some(game_version),
        }
    );

    app_emit!(app_handle, "update_library", ());

    Ok(())
}

pub fn validate_managed_directory(
    db: &Database,
    meta: &DownloadableMetadata,
    path: &std::path::Path,
) -> Result<(), String> {
    if !std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir()) {
        return Err("Installation root is not a directory.".into());
    }
    let data =
        crate::downloads::drop_data::DropData::read(path).map_err(|error| error.to_string())?;
    if data.game_id != meta.id
        || data.game_version != meta.version
        || data.target_platform != meta.target_platform
    {
        return Err("This directory belongs to a different installation.".into());
    }
    let resolved = path.canonicalize().map_err(|error| error.to_string())?;
    if resolved.parent().is_none()
        || db
            .applications
            .install_dirs
            .iter()
            .chain(
                db.applications.install_directories(
                    database::install_locations::InstallLocationKind::Emulator,
                ),
            )
            .any(|root| root.canonicalize().is_ok_and(|root| root == resolved))
    {
        return Err("Refusing to modify an installation location root.".into());
    }
    crate::emulators::safe_emulator_removal(db, meta, path)
}

pub async fn report_playtime(game_id: String, seconds: u64) -> Result<(), RemoteAccessError> {
    let endpoint = generate_url(&["/api/v1/client/playtime", &game_id], &[])?;
    let response = make_authenticated_post(endpoint, &PlaytimeUpdate { seconds }).await?;

    if !response.status().is_success() {
        let error: DropServerError = response.json().await?;
        return Err(RemoteAccessError::InvalidResponse(error));
    }

    Ok(())
}

pub fn push_game_update(
    app_handle: &AppHandle,
    game_id: &String,
    version: Option<GameVersion>,
    status: GameStatusWithTransient,
) {
    if let Some(GameDownloadStatus::Installed {
        install_type: InstalledGameType::Installed | InstalledGameType::SetupRequired,
        ..
    }) = &status.0
        && version.is_none()
    {
        panic!("pushed game for installed game that doesn't have version information");
    }

    app_emit!(
        app_handle,
        &format!("update_game/{game_id}"),
        GameUpdateEvent {
            game_id: game_id.clone(),
            status,
            version,
        }
    );
}
