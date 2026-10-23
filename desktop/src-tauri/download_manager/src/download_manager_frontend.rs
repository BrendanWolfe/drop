use std::{
    collections::VecDeque,
    fmt::Debug,
    sync::{Arc, Mutex, MutexGuard},
};

use database::DownloadableMetadata;
use log::{debug, info, warn};
use serde::Serialize;
use tauri::async_runtime::JoinHandle;
use tokio::sync::mpsc::Sender;
use tokio::sync::mpsc::error::SendError;
use utils::{lock, send};

use crate::{depot_manager::DepotManager, error::ApplicationDownloadError};

use super::{
    download_manager_builder::{CurrentProgressObject, DownloadAgent},
    util::queue::Queue,
};

#[derive(Debug)]
pub enum DownloadManagerSignal {
    /// Starts the next queued download unless the user has paused the queue.
    Go,
    /// Explicitly resumes a user-paused queue.
    Resume,
    /// Pauses the `DownloadManager`
    Stop,
    /// Called when a `DownloadAgent` has fully completed a download.
    Completed(DownloadableMetadata),
    /// Generates and appends a `DownloadAgent`
    /// to the registry and queue
    Queue(DownloadAgent),
    /// Tells the Manager to stop the current
    /// download, sync everything to disk, and
    /// then exit
    Finish,
    /// Stops, removes, and tells a download to cleanup
    Cancel(DownloadableMetadata),
    /// Moves the item at `current_index` to `new_index`.
    ///
    /// The manager performs the edit so it can stop the active worker first
    /// when the queue front is involved.
    Rearrange {
        current_index: usize,
        new_index: usize,
    },
    /// Any error which occurs in the agent
    Error(DownloadableMetadata, ApplicationDownloadError),
    /// Pushes UI update
    UpdateUIQueue,
    UpdateUIDownloadStats(usize, usize), //kb/s and seconds
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadManagerStatus {
    Downloading,
    Paused,
    Empty,
    Error,
}

impl Serialize for DownloadManagerStatus {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&format!["{self:?}"])
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub enum DownloadStatus {
    Queued,
    Downloading,
    Validating,
    Error,
}

/// Accessible front-end for the `DownloadManager`
///
/// The system works entirely through signals, both internally and externally,
/// all of which are accessible through the `DownloadManagerSignal` type, but
/// should not be used directly. Rather, signals are abstracted through this
/// interface.
///
/// The actual download queue may be accessed through the .`edit()` function,
/// which provides raw access to the underlying queue.
/// THIS EDITING IS BLOCKING!!!
pub struct DownloadManager {
    terminator: Mutex<Option<JoinHandle<()>>>,
    download_queue: Queue,
    progress: CurrentProgressObject,
    command_sender: Sender<DownloadManagerSignal>,
    depot_manager: Arc<DepotManager>,
}

impl Debug for DownloadManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadManager").finish()
    }
}

#[allow(dead_code)]
impl DownloadManager {
    pub fn new(
        terminator: JoinHandle<()>,
        download_queue: Queue,
        progress: CurrentProgressObject,
        command_sender: Sender<DownloadManagerSignal>,
        depot_manager: Arc<DepotManager>,
    ) -> Self {
        Self {
            terminator: Mutex::new(Some(terminator)),
            download_queue,
            progress,
            command_sender,
            depot_manager,
        }
    }

    pub async fn queue_download(
        &self,
        download: DownloadAgent,
    ) -> Result<(), SendError<DownloadManagerSignal>> {
        info!("creating download with meta {:?}", download.metadata());
        self.command_sender
            .send(DownloadManagerSignal::Queue(download))
            .await?;
        self.command_sender.send(DownloadManagerSignal::Go).await
    }
    pub fn edit(&self) -> MutexGuard<'_, VecDeque<DownloadableMetadata>> {
        self.download_queue.edit()
    }
    pub fn read_queue(&self) -> VecDeque<DownloadableMetadata> {
        self.download_queue.read()
    }
    pub fn get_current_download_progress(&self) -> Option<f64> {
        let progress_object = (*lock!(self.progress)).clone()?;
        Some(progress_object.get_progress())
    }
    pub async fn rearrange_string(&self, meta: &DownloadableMetadata, new_index: usize) {
        let current_index = self
            .read_queue()
            .iter()
            .position(|download_agent| download_agent == meta);

        match current_index {
            Some(current_index) => self.rearrange(current_index, new_index).await,
            None => warn!("cannot rearrange missing download {meta:?}"),
        }
    }
    pub async fn cancel(&self, meta: DownloadableMetadata) {
        send!(self.command_sender, DownloadManagerSignal::Cancel(meta));
    }
    pub async fn rearrange(&self, current_index: usize, new_index: usize) {
        if current_index == new_index {
            return;
        }

        debug!("moving download at index {current_index} to index {new_index}");
        send!(
            self.command_sender,
            DownloadManagerSignal::Rearrange {
                current_index,
                new_index,
            }
        );
    }
    pub async fn pause_downloads(&self) {
        send!(self.command_sender, DownloadManagerSignal::Stop);
    }
    pub async fn resume_downloads(&self) {
        send!(self.command_sender, DownloadManagerSignal::Resume);
    }
    pub async fn ensure_terminated(&self) -> Result<(), tauri::Error> {
        send!(self.command_sender, DownloadManagerSignal::Finish);
        let terminator = lock!(self.terminator).take();
        terminator.unwrap().await
    }
    pub fn get_sender(&self) -> Sender<DownloadManagerSignal> {
        self.command_sender.clone()
    }
    pub fn clone_depot_manager(&self) -> Arc<DepotManager> {
        self.depot_manager.clone()
    }
}
