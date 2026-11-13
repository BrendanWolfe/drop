use database::DownloadableMetadata;
use serde::{Deserialize, Serialize};

use crate::download_manager_frontend::{DownloadManagerStatus, DownloadStatus};

/// Explicit lifecycle events: disappearance from a snapshot is not completion.
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DownloadActivityEvent {
    pub session_id: String,
    pub game_id: String,
    pub version_id: String,
    pub platform: String,
    pub state: String,
    pub occurred_at: u64,
    pub bytes_downloaded: usize,
    pub bytes_total: usize,
    pub error: Option<String>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct QueueUpdateEventQueueData {
    pub meta: DownloadableMetadata,
    pub status: DownloadStatus,
    pub dl_progress: f64,
    pub dl_current: usize,
    pub dl_max: usize,
    pub disk_progress: f64,
    pub disk_current: usize,
    pub disk_max: usize,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct QueueUpdateEvent {
    pub status: DownloadManagerStatus,
    pub queue: Vec<QueueUpdateEventQueueData>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct DownloadStatsUpdateEvent {
    pub speed: usize,
    pub time: usize,
}

#[derive(Serialize, Clone)]
pub struct DiskStatsUpdateEvent {
    pub speed: usize,
}
