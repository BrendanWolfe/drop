//! Best-effort reporting. Network activity is never awaited by a download worker.
use database::borrow_db_checked;
use download_manager::frontend_updates::{DownloadStatsUpdateEvent, QueueUpdateEvent};
use remote::{
    auth::generate_authorization_header, proxy::PROXY_AUTH, requests::generate_url,
    utils::get_client_async_for,
};
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Listener};
use tokio::sync::Notify;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    reporter_id: String,
    sequence: u64,
    downloads: Vec<DownloadReport>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadReport {
    session_id: String,
    game_id: String,
    version_id: String,
    state: &'static str,
    bytes_downloaded: usize,
    bytes_total: usize,
    bytes_per_second: usize,
}

#[derive(Default)]
struct Snapshot {
    queue: Option<QueueUpdateEvent>,
    speed: usize,
    last_stats: Option<Instant>,
}

pub fn start(app: &AppHandle) {
    let snapshot = Arc::new(Mutex::new(Snapshot::default()));
    let notify = Arc::new(Notify::new());
    let queue_snapshot = snapshot.clone();
    let queue_notify = notify.clone();
    app.listen("update_queue", move |event| {
        if let Ok(queue) = serde_json::from_str::<QueueUpdateEvent>(event.payload()) {
            let mut snapshot = queue_snapshot.lock().unwrap_or_else(|e| e.into_inner());
            let changed = snapshot.queue.as_ref().is_none_or(|previous| {
                previous.status != queue.status
                    || previous.queue.len() != queue.queue.len()
                    || previous
                        .queue
                        .iter()
                        .zip(&queue.queue)
                        .any(|(a, b)| a.meta != b.meta || a.status != b.status)
            });
            if changed {
                snapshot.speed = 0;
            }
            snapshot.queue = Some(queue);
            if changed {
                queue_notify.notify_one();
            }
        }
    });
    let stats_snapshot = snapshot.clone();
    app.listen("update_stats", move |event| {
        if let Ok(stats) = serde_json::from_str::<DownloadStatsUpdateEvent>(event.payload()) {
            let mut snapshot = stats_snapshot.lock().unwrap_or_else(|e| e.into_inner());
            snapshot.speed = stats.speed.saturating_mul(1000);
            snapshot.last_stats = Some(Instant::now());
        }
    });
    tauri::async_runtime::spawn(async move {
        let client = get_client_async_for(PROXY_AUTH.clone()); // Does not affect online/offline status.
        let reporter_id = uuid::Uuid::new_v4().to_string();
        let mut sequence = 0;
        let mut sessions = HashMap::<String, String>::new();
        let mut had_downloads = false;
        let mut empty_since: Option<Instant> = None;
        let mut unsupported_until = Instant::now();
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        loop {
            tokio::select! { _ = interval.tick() => (), _ = notify.notified() => () }
            if Instant::now() < unsupported_until {
                continue;
            }
            let downloads = {
                let snapshot = snapshot.lock().unwrap_or_else(|e| e.into_inner());
                let db = borrow_db_checked();
                if db.auth.is_none() || db.base_url.is_empty() || db.settings.force_offline {
                    continue;
                }
                let Some(queue) = &snapshot.queue else {
                    continue;
                };
                let server_items = queue
                    .queue
                    .iter()
                    .filter(|item| !db.applications.local_games.contains_key(&item.meta.id))
                    .collect::<Vec<_>>();
                sessions.retain(|id, _| server_items.iter().any(|item| &item.meta.id == id));
                server_items
                    .iter()
                    .take(100)
                    .map(|item| {
                        use download_manager::download_manager_frontend::{
                            DownloadManagerStatus, DownloadStatus,
                        };
                        let state = match item.status {
                            _ if queue.status == DownloadManagerStatus::Paused => "Paused",
                            DownloadStatus::Downloading => "Downloading",
                            DownloadStatus::Validating => "Validating",
                            _ => "Queued",
                        };
                        DownloadReport {
                            session_id: sessions
                                .entry(item.meta.id.clone())
                                .or_insert_with(|| uuid::Uuid::new_v4().to_string())
                                .clone(),
                            game_id: item.meta.id.clone(),
                            version_id: item.meta.version.clone(),
                            state,
                            bytes_downloaded: item.dl_current,
                            bytes_total: item.dl_max,
                            bytes_per_second: if state == "Downloading"
                                && snapshot
                                    .last_stats
                                    .is_some_and(|time| time.elapsed() < Duration::from_secs(3))
                            {
                                snapshot.speed
                            } else {
                                0
                            },
                        }
                    })
                    .collect::<Vec<_>>()
            };
            if downloads.is_empty() && !had_downloads {
                continue;
            }
            if downloads.is_empty() {
                if empty_since.get_or_insert_with(Instant::now).elapsed() >= Duration::from_secs(60)
                {
                    had_downloads = false;
                    continue;
                }
            } else {
                empty_since = None;
            }
            sequence += 1;
            let empty = downloads.is_empty();
            let Ok(endpoint) = generate_url(&["/api/v1/client/downloads"], &[]) else {
                continue;
            };
            let result = client
                .post(endpoint)
                .header("Authorization", generate_authorization_header())
                .timeout(Duration::from_secs(4))
                .json(&Report {
                    reporter_id: reporter_id.clone(),
                    sequence,
                    downloads,
                })
                .send()
                .await;
            // Ignore old servers, temporary outages, and telemetry errors. Retry a
            // final empty snapshot until acknowledged so finished cards disappear.
            match result {
                Ok(response) if matches!(response.status().as_u16(), 404 | 405) => {
                    unsupported_until = Instant::now() + Duration::from_secs(300);
                    had_downloads = !empty;
                }
                result => {
                    had_downloads =
                        !empty || !result.is_ok_and(|response| response.status().is_success())
                }
            }
        }
    });
}
