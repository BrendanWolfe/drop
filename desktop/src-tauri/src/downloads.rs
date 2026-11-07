use std::{
    path::PathBuf,
    sync::{Arc, nonpoison::Mutex},
};

use crate::{
    AppState,
    games::fetch_game_version_options_logic,
    version_options::{configuration_for_build, contains_build},
};
use database::{
    DownloadType, DownloadableMetadata, GameDownloadStatus, borrow_db_checked,
    install_locations::InstallLocationKind, models::data::InstalledGameType, platform::Platform,
};
use download_manager::{
    DOWNLOAD_MANAGER, downloadable::Downloadable, error::ApplicationDownloadError,
};
use games::downloads::download_agent::GameDownloadAgent;

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
) -> Result<(), ApplicationDownloadError> {
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
    if reuse_installed == Some(true)
        && game.game.game_type == "Emulator"
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
    let options = fetch_game_version_options_logic(game_id.clone(), state).await?;
    if !contains_build(&options, &version_id, target_platform) {
        return Err(ApplicationDownloadError::BuildUnavailable);
    }
    let sender = { DOWNLOAD_MANAGER.get_sender().clone() };

    let meta = DownloadableMetadata {
        id: game_id,
        version: version_id,
        target_platform,
        download_type: DownloadType::Game,
    };

    {
        let db = borrow_db_checked();
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
        configuration_for_build(existing, target_platform, enable_updates)
    };

    let base_dir = {
        let db_lock = borrow_db_checked();

        db_lock
            .applications
            .resolve_install_directory(location_kind, install_dir, install_path.as_deref())
            .map_err(ApplicationDownloadError::Archive)?
    };

    let game_download_agent = GameDownloadAgent::new(
        meta,
        base_dir,
        sender,
        DOWNLOAD_MANAGER.clone_depot_manager(),
        configuration,
    )
    .await?;

    let game_download_agent =
        Arc::new(Box::new(game_download_agent) as Box<dyn Downloadable + Send + Sync>);

    DOWNLOAD_MANAGER
        .queue_download(game_download_agent.clone())
        .await
        .unwrap();

    Ok(())
}

#[tauri::command]
pub async fn resume_download(game_id: String) -> Result<(), ApplicationDownloadError> {
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
        let status = db_lock
            .applications
            .game_statuses
            .get(&game_id)
            .ok_or(ApplicationDownloadError::InvalidCommand)?
            .clone();

        let meta = db_lock
            .applications
            .installed_game_version
            .get(&game_id)
            .ok_or(ApplicationDownloadError::InvalidCommand)?
            .clone();

        let install_dir = match status {
            GameDownloadStatus::Installed {
                install_type: InstalledGameType::PartiallyInstalled { configuration },
                install_dir,
                ..
            } => Ok((install_dir, configuration)),
            _ => Err(ApplicationDownloadError::InvalidCommand),
        }?;
        (meta, install_dir)
    };

    let sender = DOWNLOAD_MANAGER.get_sender();

    let game_download_agent = Arc::new(Box::new(
        GameDownloadAgent::new_at_path(
            meta,
            PathBuf::from(install_dir),
            sender,
            DOWNLOAD_MANAGER.clone_depot_manager(),
            configuration,
        )
        .await?,
    ) as Box<dyn Downloadable + Send + Sync>);

    DOWNLOAD_MANAGER
        .queue_download(game_download_agent)
        .await
        .unwrap();
    Ok(())
}
