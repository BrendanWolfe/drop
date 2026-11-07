use std::sync::nonpoison::Mutex;

use async_trait::async_trait;
use client::app_state::AppState;
use database::{
    GameDownloadStatus, GameVersion, borrow_db_checked, borrow_db_mut_checked,
};
use log::warn;
use remote::utils::DROP_APP_HANDLE;
use tauri::Manager;

use crate::{
    games::fetch_game_version_options,
    scheduler::ScheduleTask,
    version_options::has_update,
};

mod policy;

pub struct GameUpdater {
    no_internet: bool,
}

impl GameUpdater {
    pub fn new() -> Self {
        GameUpdater { no_internet: false }
    }
}

/*
This implementation is kinda inefficient because we can't hold the locks across await boundaries,
which means we constantly lock and unlock certain objects. It doesn't matter though, because this
doesn't have to be fast.
*/
#[async_trait]
impl ScheduleTask for GameUpdater {
    fn timeframe(&mut self) -> usize {
        if self.no_internet { 5 } else { 30 }
    }

    async fn call(&mut self) -> Result<(), anyhow::Error> {
        let app_handle = DROP_APP_HANDLE.lock().await;
        let app_handle = app_handle
            .as_ref()
            .ok_or(anyhow::anyhow!("game update task ran before setup"))?;
        let state = app_handle.state::<Mutex<AppState>>();
        let status = state.lock().status;

        let to_check: Vec<(GameVersion, database::platform::Platform)> = {
            let db_lock = borrow_db_checked();
            if !policy::can_check_game_updates(status, &db_lock) {
                self.no_internet = true;
                return Ok(());
            }
            self.no_internet = false;
            db_lock
                .applications
                .game_statuses
                .values()
                .map(|v| match v {
                    GameDownloadStatus::Installed { version_id, .. } => Some(version_id),
                    _ => None,
                })
                .map(|v| {
                    v.and_then(|version_id| db_lock.applications.game_versions.get(version_id))
                })
                .filter(|v| {
                    v.map(|v| v.user_configuration.enable_updates && !db_lock.applications.local_games.contains_key(&v.game_id))
                        .unwrap_or(false)
                })
                .filter_map(|v| {
                    let version = v?;
                    let meta = db_lock.applications.installed_game_version
                        .get(&version.game_id)?;
                    Some((version.clone(), meta.target_platform))
                })
                .collect()
        };

        for (version, installed_platform) in to_check {
            let version_options =
                match fetch_game_version_options(version.game_id.clone(), state.clone()).await {
                    Ok(v) => v,
                    Err(err) => {
                        warn!(
                            "failed to check for update for game id {}: {:?}",
                            version.game_id, err
                        );
                        continue;
                    }
                };

            let has_update = has_update(
                &version_options,
                installed_platform,
                &version.version_id,
                version.version_index,
            );
            let mut db_lock = borrow_db_mut_checked();
            if !db_lock.applications.installed_game_version.get(&version.game_id)
                .is_some_and(|meta| {
                    meta.target_platform == installed_platform && meta.version == version.version_id
                })
            {
                continue;
            }
            let game_status = db_lock
                .applications
                .game_statuses
                .get_mut(&version.game_id)
                .ok_or(anyhow::anyhow!(""))?;

            if let GameDownloadStatus::Installed {
                    update_available, ..
                } = game_status {
                *update_available = has_update;
            };
        }

        Ok(())
    }
}
