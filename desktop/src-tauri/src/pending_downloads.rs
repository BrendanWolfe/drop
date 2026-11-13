use std::{
    path::PathBuf,
    sync::{Arc, nonpoison::Mutex},
};

use database::{
    DownloadType, DownloadableMetadata, GameDownloadStatus, borrow_db_checked,
    install_locations::InstallLocationKind, models::data::InstalledGameType,
};
use download_manager::{
    DOWNLOAD_MANAGER, downloadable::Downloadable, error::ApplicationDownloadError,
};
use games::{downloads::download_agent::GameDownloadAgent, library::Game};
use serde::Serialize;

use crate::{
    AppState,
    downloads::{download_game, resume_download},
    games::fetch_game_version_options_logic,
};

#[derive(Clone, Copy, Serialize, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum PendingAction {
    Resume,
    Update,
}

fn pending_action(status: &GameDownloadStatus) -> Option<PendingAction> {
    match status {
        GameDownloadStatus::Installed {
            install_type: InstalledGameType::PartiallyInstalled { .. },
            ..
        } => Some(PendingAction::Resume),
        GameDownloadStatus::Installed {
            update_available: true,
            ..
        } => Some(PendingAction::Update),
        _ => None,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingDownload {
    meta: DownloadableMetadata,
    name: String,
    cover_object_id: Option<String>,
    action: PendingAction,
}

#[tauri::command]
pub fn fetch_pending_downloads() -> Vec<PendingDownload> {
    let queued = DOWNLOAD_MANAGER.read_queue();
    let db = borrow_db_checked();
    let mut pending = db
        .applications
        .installed_game_version
        .values()
        .filter_map(|meta| {
            if meta.download_type != DownloadType::Game
                || queued.iter().any(|item| item.id == meta.id)
                || db
                    .applications
                    .transient_statuses
                    .keys()
                    .any(|item| item.id == meta.id)
            {
                return None;
            }
            let action = pending_action(db.applications.game_statuses.get(&meta.id)?)?;
            // Read through this DB handle: acquiring the global DB lock again would deadlock.
            let game = if let Some(local) = db.applications.local_games.get(&meta.id) {
                // A local game without a launcher also looks partial, but only
                // unfinished archive imports have anything to download.
                if !local
                    .archive
                    .as_ref()
                    .is_some_and(|archive| !archive.extracted)
                {
                    return None;
                }
                Some(games::local::as_game(local))
            } else {
                remote::cache::get_cached_object_db::<Game>(&format!("game/{}", meta.id), &db).ok()
            };
            if game.as_ref().is_some_and(|game| game.game_type != "Game") {
                return None;
            }
            Some(PendingDownload {
                meta: meta.clone(),
                name: game
                    .as_ref()
                    .map(|game| game.m_name.clone())
                    .unwrap_or_else(|| meta.id.clone()),
                cover_object_id: game.map(|game| game.m_cover_object_id),
                action,
            })
        })
        .collect::<Vec<_>>();
    pending.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    pending
}

// Return the exact queued builds, including any required emulators, so the UI can
// promote the whole batch without putting a game ahead of its dependencies.
#[tauri::command]
pub async fn queue_pending_download(
    meta: DownloadableMetadata,
    state: tauri::State<'_, Mutex<AppState>>,
    app: tauri::AppHandle,
) -> Result<Vec<DownloadableMetadata>, ApplicationDownloadError> {
    let (action, install_dir, configuration, version_index, local_archive) = {
        let db = borrow_db_checked();
        if meta.download_type != DownloadType::Game
            || db.applications.installed_game_version.get(&meta.id) != Some(&meta)
            || db
                .applications
                .transient_statuses
                .keys()
                .any(|item| item.id == meta.id)
        {
            return Err(ApplicationDownloadError::InvalidCommand);
        }
        let status = db
            .applications
            .game_statuses
            .get(&meta.id)
            .ok_or(ApplicationDownloadError::InvalidCommand)?;
        let action = pending_action(status).ok_or(ApplicationDownloadError::InvalidCommand)?;
        let local_archive = if let Some(local) = db.applications.local_games.get(&meta.id) {
            if action != PendingAction::Resume
                || !local
                    .archive
                    .as_ref()
                    .is_some_and(|archive| !archive.extracted)
            {
                return Err(ApplicationDownloadError::InvalidCommand);
            }
            true
        } else {
            false
        };
        let GameDownloadStatus::Installed {
            install_dir,
            version_id,
            ..
        } = status
        else {
            return Err(ApplicationDownloadError::InvalidCommand);
        };
        let version = db.applications.game_versions.get(version_id);
        (
            action,
            PathBuf::from(install_dir),
            version.map(|version| version.user_configuration.clone()),
            version.map(|version| version.version_index),
            local_archive,
        )
    };
    if DOWNLOAD_MANAGER
        .read_queue()
        .iter()
        .any(|item| item.id == meta.id)
    {
        return Err(ApplicationDownloadError::InvalidCommand);
    }
    if action == PendingAction::Resume {
        if local_archive {
            crate::local_games::resume_archive(meta.id.clone())
                .await
                .map_err(ApplicationDownloadError::Archive)?;
        } else {
            resume_download(meta.id.clone(), None, None, app).await?;
        }
        return Ok(vec![meta]);
    }

    let options = fetch_game_version_options_logic(meta.id.clone(), state.clone()).await?;
    if !crate::version_options::has_update(
        &options,
        meta.target_platform,
        &meta.version,
        version_index.ok_or(ApplicationDownloadError::InvalidCommand)?,
    ) {
        return Err(ApplicationDownloadError::BuildUnavailable);
    }
    let latest = crate::version_options::latest_for_platform(&options, meta.target_platform)
        .filter(|option| option.version_id != meta.version)
        .ok_or(ApplicationDownloadError::BuildUnavailable)?;
    let next_meta = DownloadableMetadata {
        version: latest.version_id.clone(),
        ..meta.clone()
    };
    // Construct in the existing folder, not a newly selected library location.
    let agent = GameDownloadAgent::new_at_path(
        next_meta.clone(),
        install_dir,
        DOWNLOAD_MANAGER.get_sender(),
        DOWNLOAD_MANAGER.clone_depot_manager(),
        configuration.ok_or(ApplicationDownloadError::InvalidCommand)?,
    )
    .await?;

    let missing = latest
        .required_content
        .iter()
        .filter(|dependency| dependency.installed_path.is_none())
        .collect::<Vec<_>>();
    let emulator_path = if missing.is_empty() {
        None
    } else {
        Some(
            borrow_db_checked()
                .applications
                .resolve_install_directory(InstallLocationKind::Emulator, None, None)
                .map_err(ApplicationDownloadError::Archive)?,
        )
    };
    // Revalidate after remote preparation; never enqueue over an uninstall or a
    // different installation that appeared while the request was in flight.
    {
        let db = borrow_db_checked();
        if db.applications.installed_game_version.get(&meta.id) != Some(&meta)
            || db
                .applications
                .transient_statuses
                .keys()
                .any(|item| item.id == meta.id)
            || DOWNLOAD_MANAGER
                .read_queue()
                .iter()
                .any(|item| item.id == meta.id)
        {
            return Err(ApplicationDownloadError::InvalidCommand);
        }
    }
    let mut queued = Vec::new();
    for dependency in missing {
        let dependency_meta = DownloadableMetadata {
            id: dependency.game_id.clone(),
            version: dependency.version_id.clone(),
            target_platform: meta.target_platform,
            download_type: DownloadType::Emulator,
        };
        if queued.contains(&dependency_meta) {
            continue;
        }
        download_game(
            dependency.game_id.clone(),
            dependency.version_id.clone(),
            meta.target_platform,
            None,
            emulator_path.clone(),
            Some(true),
            Some(dependency.launch_ids.clone()),
            false,
            state.clone(),
            app.clone(),
        )
        .await?;
        queued.push(dependency_meta);
    }
    DOWNLOAD_MANAGER
        .queue_download(Arc::new(
            Box::new(agent) as Box<dyn Downloadable + Send + Sync>
        ))
        .await
        .map_err(|error| ApplicationDownloadError::Archive(error.to_string()))?;
    queued.push(next_meta);
    Ok(queued)
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::models::data::UserConfiguration;

    fn installed(install_type: InstalledGameType, update_available: bool) -> GameDownloadStatus {
        GameDownloadStatus::Installed {
            install_type,
            update_available,
            version_id: "v1".into(),
            install_dir: "games/game".into(),
        }
    }

    #[test]
    fn partial_installs_resume_even_when_an_update_exists() {
        assert_eq!(
            pending_action(&installed(
                InstalledGameType::PartiallyInstalled {
                    configuration: UserConfiguration::default()
                },
                true
            )),
            Some(PendingAction::Resume)
        );
    }

    #[test]
    fn complete_installs_only_appear_when_an_update_is_available() {
        assert_eq!(
            pending_action(&installed(InstalledGameType::Installed, true)),
            Some(PendingAction::Update)
        );
        assert_eq!(
            pending_action(&installed(InstalledGameType::SetupRequired, true)),
            Some(PendingAction::Update)
        );
        assert_eq!(
            pending_action(&installed(InstalledGameType::Installed, false)),
            None
        );
        assert_eq!(
            pending_action(&installed(InstalledGameType::SetupRequired, false)),
            None
        );
        assert_eq!(pending_action(&GameDownloadStatus::Remote {}), None);
    }
}
