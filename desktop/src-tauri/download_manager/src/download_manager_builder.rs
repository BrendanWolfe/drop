use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use database::DownloadableMetadata;
use log::{debug, error, info, warn};
use tauri::{AppHandle, async_runtime::JoinHandle};
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::{sync::mpsc, time::timeout};
use utils::{app_emit, lock, send};

use crate::{
    depot_manager::DepotManager,
    download_manager_frontend::DownloadStatus,
    error::ApplicationDownloadError,
    frontend_updates::{DownloadStatsUpdateEvent, QueueUpdateEvent, QueueUpdateEventQueueData},
};

use super::{
    download_manager_frontend::{DownloadManager, DownloadManagerSignal, DownloadManagerStatus},
    downloadable::Downloadable,
    util::{
        download_thread_control_flag::{DownloadThreadControl, DownloadThreadControlFlag},
        progress_object::ProgressObject,
        queue::Queue,
    },
};

pub type DownloadAgent = Arc<Box<dyn Downloadable + Send + Sync>>;
pub type CurrentProgressObject = Arc<Mutex<Option<Arc<ProgressObject>>>>;

const DOWNLOAD_STOP_TIMEOUT: Duration = Duration::from_secs(4);

async fn stop_and_wait_download_thread(
    active_control_flag: Option<&DownloadThreadControl>,
    current_download_thread: Option<JoinHandle<()>>,
    timeout_duration: Duration,
) -> bool {
    if let Some(current_flag) = active_control_flag {
        current_flag.set(DownloadThreadControlFlag::Stop);
    }

    let Some(mut current_download_thread) = current_download_thread else {
        return true;
    };

    match timeout(timeout_duration, &mut current_download_thread).await {
        Ok(Ok(())) => true,
        Ok(Err(error)) => {
            warn!("download thread exited with error while stopping: {error}");
            false
        }
        Err(_) => {
            warn!("download thread did not stop within {timeout_duration:?}; aborting it");
            current_download_thread.abort();
            let _ = current_download_thread.await;
            false
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalOutcome {
    Completed,
    Error,
}

struct TerminalRecord {
    metadata: DownloadableMetadata,
    outcome: TerminalOutcome,
}

#[derive(Default)]
struct CurrentDownload {
    control_flag: Option<DownloadThreadControl>,
    thread: Option<JoinHandle<()>>,
    terminal: Arc<Mutex<Option<TerminalRecord>>>,
}

impl CurrentDownload {
    fn is_active(&self) -> bool {
        self.thread.is_some() || self.control_flag.is_some()
    }

    fn terminal_handle(&self) -> Arc<Mutex<Option<TerminalRecord>>> {
        self.terminal.clone()
    }

    fn has_terminal(&self) -> bool {
        lock!(self.terminal).is_some()
    }

    fn take_terminal_for(&self, metadata: &DownloadableMetadata) -> Option<TerminalOutcome> {
        let mut terminal = lock!(self.terminal);

        match terminal.as_ref() {
            Some(record) if &record.metadata == metadata => {
                let outcome = record.outcome;
                *terminal = None;
                Some(outcome)
            }
            _ => None,
        }
    }

    fn start(&mut self, control_flag: DownloadThreadControl, thread: JoinHandle<()>) -> bool {
        if self.is_active() {
            warn!("refusing to overwrite an active download thread");
            thread.abort();
            return false;
        }

        self.control_flag = Some(control_flag);
        self.thread = Some(thread);
        true
    }

    async fn stop_and_wait(&mut self, timeout_duration: Duration) -> bool {
        let control_flag = self.control_flag.as_ref();
        let thread = self.thread.take();

        stop_and_wait_download_thread(control_flag, thread, timeout_duration).await
    }

    async fn cleanup(&mut self, timeout_duration: Duration) {
        if let Some(control_flag) = &self.control_flag {
            control_flag.set(DownloadThreadControlFlag::Stop);
        }
        self.control_flag = None;

        if let Some(thread) = self.thread.take() {
            let _ = stop_and_wait_download_thread(None, Some(thread), timeout_duration).await;
        }
    }
}

fn remove_download_from_queue(queue: &Queue, metadata: &DownloadableMetadata) -> bool {
    if let Some(index) = queue.get_by_meta(metadata) {
        queue.edit().remove(index).is_some()
    } else {
        false
    }
}

/*

Welcome to the download manager, the most overengineered, glorious piece of bullshit.

The download manager takes a queue of ids and their associated
DownloadAgents, and then, one-by-one, executes them. It provides an interface
to interact with the currently downloading agent, and manage the queue.

When the DownloadManager is initialised, it is designed to provide a reference
which can be used to provide some instructions (the DownloadManagerInterface),
but other than that, it runs without any sort of interruptions.

It does this by opening up two data structures. Primarily is the command_receiver,
and mpsc (multi-channel-single-producer) which allows commands to be sent from
the Interface, and queued up for the Manager to process.

These have been mapped in the DownloadManagerSignal docs.

The other way to interact with the DownloadManager is via the donwload_queue,
which is just a collection of ids which may be rearranged to suit
whichever download queue order is required.

+----------------------------------------------------------------------------+
| DO NOT ATTEMPT TO ADD OR REMOVE FROM THE QUEUE WITHOUT USING SIGNALS!!     |
| THIS WILL CAUSE A DESYNC BETWEEN THE DOWNLOAD AGENT REGISTRY AND THE QUEUE |
| WHICH HAS NOT BEEN ACCOUNTED FOR                                           |
+----------------------------------------------------------------------------+

This download queue does not actually own any of the DownloadAgents. It is
simply an id-based reference system. The actual Agents are stored in the
download_agent_registry HashMap, as ordering is no issue here. This is why
appending or removing from the download_queue must be done via signals.

Behold, my madness - quexeky

*/

pub struct DownloadManagerBuilder {
    download_agent_registry: HashMap<DownloadableMetadata, DownloadAgent>,
    download_queue: Queue,
    command_receiver: Receiver<DownloadManagerSignal>,
    sender: Sender<DownloadManagerSignal>,
    progress: CurrentProgressObject,
    status: Arc<Mutex<DownloadManagerStatus>>,
    app_handle: AppHandle,

    current_download: CurrentDownload,
    user_paused: bool,
}
impl DownloadManagerBuilder {
    pub fn build(app_handle: AppHandle) -> DownloadManager {
        let queue = Queue::new();
        let (command_sender, command_receiver) = mpsc::channel(1500);
        let active_progress = Arc::new(Mutex::new(None));
        let status = Arc::new(Mutex::new(DownloadManagerStatus::Empty));

        let depot_manager = Arc::new(DepotManager::new());
        let manager = Self {
            download_agent_registry: HashMap::new(),
            download_queue: queue.clone(),
            command_receiver,
            status: status.clone(),
            sender: command_sender.clone(),
            progress: active_progress.clone(),
            app_handle,

            current_download: CurrentDownload::default(),
            user_paused: false,
        };

        let terminator = tauri::async_runtime::spawn(async move {
            let result = manager.manage_queue().await;
            info!("download manager exited with result: {:?}", result);
        });

        DownloadManager::new(
            terminator,
            queue,
            active_progress,
            command_sender,
            depot_manager,
        )
    }

    fn set_status(&self, status: DownloadManagerStatus) {
        *lock!(self.status) = status;
    }

    fn is_downloading(&self) -> bool {
        matches!(*lock!(self.status), DownloadManagerStatus::Downloading)
    }

    async fn remove_download(&mut self, meta: &DownloadableMetadata) -> Option<DownloadAgent> {
        remove_download_from_queue(&self.download_queue, meta);

        let download_agent = self.download_agent_registry.remove(meta);
        self.cleanup_current_download().await;
        download_agent
    }

    // CAREFUL WITH THIS FUNCTION
    // Make sure the download thread is terminated
    async fn cleanup_current_download(&mut self) {
        *lock!(self.progress) = None;
        self.current_download.cleanup(DOWNLOAD_STOP_TIMEOUT).await;
    }

    async fn stop_and_wait_current_download(&mut self) -> bool {
        self.set_status(DownloadManagerStatus::Paused);
        self.current_download
            .stop_and_wait(DOWNLOAD_STOP_TIMEOUT)
            .await
    }

    async fn pause_current_download(&mut self) -> bool {
        if !self.current_download.is_active() {
            debug!("no active download to pause");
            return false;
        }

        let graceful = self.stop_and_wait_current_download().await;
        if !graceful {
            warn!("download thread was force-stopped after the shutdown timeout");
        }

        let terminal_present = self.current_download.has_terminal();
        self.cleanup_current_download().await;

        // If the worker reached a terminal result while stopping, leave the
        // queued Completed/Error signal to reconcile and remove that item.
        // Re-queueing here would overwrite an installed or failed state.
        if !terminal_present
            && let Some(metadata) = self.download_queue.read().front().cloned()
            && let Some(download_agent) = self.download_agent_registry.get(&metadata)
        {
            download_agent.on_queued(&self.app_handle);
        }

        self.push_ui_queue_update();
        true
    }

    async fn manage_queue(mut self) -> Result<(), ()> {
        self.push_ui_queue_update();

        loop {
            let signal = match self.command_receiver.recv().await {
                Some(signal) => signal,
                None => return Err(()),
            };

            match signal {
                DownloadManagerSignal::Go => {
                    self.manage_go_signal().await;
                }
                DownloadManagerSignal::Resume => {
                    self.manage_resume_signal().await;
                }
                DownloadManagerSignal::Stop => {
                    self.manage_stop_signal().await;
                }
                DownloadManagerSignal::Rearrange {
                    current_index,
                    new_index,
                } => {
                    self.manage_rearrange_signal(current_index, new_index).await;
                }
                DownloadManagerSignal::Completed(meta) => {
                    self.manage_completed_signal(meta).await;
                }
                DownloadManagerSignal::Queue(download_agent) => {
                    self.manage_queue_signal(download_agent).await;
                }
                DownloadManagerSignal::Error(meta, e) => {
                    self.manage_error_signal(meta, e).await;
                }
                DownloadManagerSignal::UpdateUIQueue => {
                    self.push_ui_queue_update();
                }
                DownloadManagerSignal::UpdateUIDownloadStats(kbs, time) => {
                    // Progress objects may still flush a delayed update after the
                    // worker has been stopped. Do not leak those into the next
                    // paused/resumed download.
                    if self.is_downloading() {
                        self.push_ui_download_stats_update(kbs, time);
                    }
                }
                DownloadManagerSignal::Finish => {
                    self.stop_and_wait_current_download().await;
                    return Ok(());
                }
                DownloadManagerSignal::Cancel(meta) => {
                    self.manage_cancel_signal(&meta).await;
                }
            }
        }
    }
    async fn manage_queue_signal(&mut self, download_agent: DownloadAgent) {
        debug!("got signal Queue");
        let meta = download_agent.metadata();

        debug!("queue metadata: {meta:?}");

        if self.download_queue.exists(meta.clone()) {
            warn!("download with same ID already exists");
            return;
        }

        download_agent.on_queued(&self.app_handle);
        self.download_queue.append(meta.clone());
        self.download_agent_registry.insert(meta, download_agent);

        if self.user_paused && !self.current_download.is_active() {
            self.set_status(DownloadManagerStatus::Paused);
        }

        send!(self.sender, DownloadManagerSignal::UpdateUIQueue);
    }

    async fn manage_go_signal(&mut self) {
        debug!("got signal Go");

        if self.user_paused {
            debug!("ignoring Go: the queue is paused by the user");
            if !matches!(*lock!(self.status), DownloadManagerStatus::Paused) {
                self.set_status(DownloadManagerStatus::Paused);
                self.push_ui_queue_update();
            }
            return;
        }

        if self.current_download.is_active() {
            debug!("ignoring Go: a download thread is already active");
            return;
        }

        if self.current_download.has_terminal() {
            debug!("deferring Go until the previous download result is processed");
            return;
        }

        if self.download_agent_registry.is_empty() {
            debug!(
                "Download agent registry: {:?}",
                self.download_agent_registry.len()
            );
            return;
        }

        debug!("current download queue: {:?}", self.download_queue.read());

        let agent_data = if let Some(agent_data) = self.download_queue.read().front() {
            agent_data.clone()
        } else {
            return;
        };

        let download_agent = self
            .download_agent_registry
            .get(&agent_data)
            .unwrap()
            .clone();

        let status = download_agent.status();

        // This download is already going
        if status != DownloadStatus::Queued {
            return;
        }

        // Ensure all others are marked as queued
        for agent in self.download_agent_registry.values() {
            if agent.metadata() != agent_data && agent.status() != DownloadStatus::Queued {
                agent.on_queued(&self.app_handle);
            }
        }

        info!("starting download for {agent_data:?}");
        let control_flag = download_agent.control_flag();
        let download_progress = download_agent.dl_progress().clone();
        let terminal = self.current_download.terminal_handle();
        let sender = self.sender.clone();
        let app_handle = self.app_handle.clone();

        control_flag.set(DownloadThreadControlFlag::Go);
        let download_thread = tauri::async_runtime::spawn(async move {
            loop {
                let download_result = match download_agent.download(&app_handle).await {
                    // Ok(true) is for completed and exited properly
                    Ok(v) => v,
                    Err(e) => {
                        let metadata = download_agent.metadata();
                        error!("download {metadata:?} has error {e}");
                        *lock!(terminal) = Some(TerminalRecord {
                            metadata: metadata.clone(),
                            outcome: TerminalOutcome::Error,
                        });
                        send!(sender, DownloadManagerSignal::Error(metadata, e));
                        return;
                    }
                };

                // The manager invokes the cancellation hook after this worker exits.
                if !download_result {
                    return;
                }

                if download_agent.control_flag().get() == DownloadThreadControlFlag::Stop {
                    return;
                }

                let validate_result = match download_agent.validate(&app_handle) {
                    Ok(v) => v,
                    Err(e) => {
                        let metadata = download_agent.metadata();
                        error!("download {metadata:?} has validation error {e}");
                        *lock!(terminal) = Some(TerminalRecord {
                            metadata: metadata.clone(),
                            outcome: TerminalOutcome::Error,
                        });
                        send!(sender, DownloadManagerSignal::Error(metadata, e));
                        return;
                    }
                };

                if download_agent.control_flag().get() == DownloadThreadControlFlag::Stop {
                    return;
                }

                if validate_result {
                    let metadata = download_agent.metadata();
                    match download_agent.on_complete(&app_handle).await {
                        Ok(()) => {
                            *lock!(terminal) = Some(TerminalRecord {
                                metadata: metadata.clone(),
                                outcome: TerminalOutcome::Completed,
                            });
                            send!(sender, DownloadManagerSignal::Completed(metadata));
                        }
                        Err(e) => {
                            error!("download {metadata:?} failed to finalise: {e}");
                            *lock!(terminal) = Some(TerminalRecord {
                                metadata: metadata.clone(),
                                outcome: TerminalOutcome::Error,
                            });
                            send!(sender, DownloadManagerSignal::Error(metadata, e));
                        }
                    }
                    send!(sender, DownloadManagerSignal::UpdateUIQueue);
                    return;
                }
            }
        });

        if self
            .current_download
            .start(control_flag.clone(), download_thread)
        {
            *lock!(self.progress) = Some(download_progress);
            self.set_status(DownloadManagerStatus::Downloading);
            self.push_ui_queue_update();
        }
    }

    async fn manage_resume_signal(&mut self) {
        debug!("got signal Resume");
        self.user_paused = false;

        if self.download_agent_registry.is_empty() {
            self.set_status(DownloadManagerStatus::Empty);
            self.push_ui_queue_update();
            return;
        }

        self.manage_go_signal().await;
    }

    async fn manage_stop_signal(&mut self) {
        debug!("got signal Stop");
        self.user_paused = true;
        self.set_status(DownloadManagerStatus::Paused);
        self.push_ui_queue_update();

        self.pause_current_download().await;
    }

    async fn manage_rearrange_signal(&mut self, current_index: usize, new_index: usize) {
        let queue_len = self.download_queue.read().len();
        if current_index >= queue_len || new_index >= queue_len {
            warn!(
                "ignoring rearrange from index {current_index} to {new_index} for queue of length {queue_len}"
            );
            self.push_ui_queue_update();
            return;
        }

        if current_index == new_index {
            return;
        }

        let needs_pause = current_index == 0 || new_index == 0;
        if needs_pause {
            self.pause_current_download().await;
        }

        {
            let mut queue = self.download_queue.edit();
            let to_move = queue
                .remove(current_index)
                .expect("validated queue index should exist");
            queue.insert(new_index, to_move);
        }

        self.push_ui_queue_update();

        if needs_pause {
            send!(self.sender, DownloadManagerSignal::Go);
        }
    }
    async fn manage_completed_signal(&mut self, meta: DownloadableMetadata) {
        match self.current_download.take_terminal_for(&meta) {
            Some(TerminalOutcome::Completed) => {}
            Some(TerminalOutcome::Error) => {
                warn!("ignoring Completed signal for {meta:?}: worker recorded an error");
                return;
            }
            None => {
                warn!("ignoring stale Completed signal for {meta:?}");
                return;
            }
        }

        // The item may have been moved away from the front while the worker
        // was finishing, so remove it by metadata rather than popping the front.
        self.remove_download(&meta).await;

        if self.user_paused {
            self.set_status(DownloadManagerStatus::Paused);
        } else if self.download_queue.read().is_empty() {
            self.set_status(DownloadManagerStatus::Empty);
        }

        self.push_ui_queue_update();
        send!(self.sender, DownloadManagerSignal::Go);
    }
    async fn manage_error_signal(
        &mut self,
        meta: DownloadableMetadata,
        error: ApplicationDownloadError,
    ) {
        warn!("got signal Error for {meta:?}: {error}");

        match self.current_download.take_terminal_for(&meta) {
            Some(TerminalOutcome::Error) => {}
            Some(TerminalOutcome::Completed) => {
                warn!("ignoring Error signal for {meta:?}: worker recorded completion");
                return;
            }
            None => {
                warn!("ignoring stale Error signal for {meta:?}");
                return;
            }
        }

        if let Some(download_agent) = self.download_agent_registry.get(&meta).cloned() {
            download_agent.on_error(&self.app_handle, &error);
        }

        self.remove_download(&meta).await;

        if self.user_paused {
            self.set_status(DownloadManagerStatus::Paused);
        } else {
            self.set_status(DownloadManagerStatus::Error);
        }

        self.push_ui_queue_update();
        send!(self.sender, DownloadManagerSignal::Go);
    }
    async fn manage_cancel_signal(&mut self, meta: &DownloadableMetadata) {
        // If the current download is the one we're tryna cancel
        if self.download_queue.read().front() == Some(meta) {
            let current_download = self.download_agent_registry.get(meta).cloned();

            // Stop and join the worker before invoking the cancellation hook.
            // The hook persists DropData, which the worker may also write to.
            self.stop_and_wait_current_download().await;

            let completed =
                self.current_download.take_terminal_for(meta) == Some(TerminalOutcome::Completed);

            // If completion won the race with cancellation, the game is already
            // installed. Never turn that into a partially-installed state.
            if !completed && let Some(current_download) = current_download {
                current_download.on_cancelled(&self.app_handle);
            }

            self.remove_download(meta).await;

            if self.user_paused {
                self.set_status(DownloadManagerStatus::Paused);
            } else if self.download_queue.read().is_empty() {
                self.set_status(DownloadManagerStatus::Empty);
            }
        }
        // else just cancel it
        else if let Some(download_agent) = self.download_agent_registry.get(meta).cloned() {
            let index = self.download_queue.get_by_meta(meta);
            if let Some(index) = index {
                if self.current_download.take_terminal_for(meta) != Some(TerminalOutcome::Completed)
                {
                    download_agent.on_cancelled(&self.app_handle);
                }
                let _ = self.download_queue.edit().remove(index);
                let removed = self.download_agent_registry.remove(meta);
                debug!(
                    "removed {:?} from queue {:?}",
                    removed.map(|x| x.metadata()),
                    self.download_queue.read()
                );
            }
        }
        self.push_ui_queue_update();
        send!(self.sender, DownloadManagerSignal::Go);
    }
    fn push_ui_download_stats_update(&self, kbs: usize, time: usize) {
        let event_data = DownloadStatsUpdateEvent { speed: kbs, time };
        app_emit!(&self.app_handle, "update_stats", event_data);
    }
    fn push_ui_queue_update(&self) {
        let queue = &self.download_queue.read();
        let queue_objs = queue
            .iter()
            .map(|key| {
                let val = self.download_agent_registry.get(key).unwrap();
                QueueUpdateEventQueueData {
                    meta: DownloadableMetadata::clone(key),
                    status: val.status(),
                    dl_progress: val.dl_progress().get_progress(),
                    dl_current: val.dl_progress().sum(),
                    dl_max: val.dl_progress().get_max(),
                    disk_progress: val.disk_progress().get_progress(),
                    disk_current: val.disk_progress().sum(),
                    disk_max: val.disk_progress().get_max(),
                }
            })
            .collect();

        let event_data = QueueUpdateEvent {
            status: *lock!(self.status),
            queue: queue_objs,
        };
        app_emit!(&self.app_handle, "update_queue", event_data);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::pending,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        thread,
        time::Instant,
    };

    use database::{DownloadType, platform::Platform};

    use super::*;

    fn test_metadata(id: &str) -> DownloadableMetadata {
        DownloadableMetadata::new(
            id.to_string(),
            "1".to_string(),
            Platform::Linux,
            DownloadType::Game,
        )
    }

    #[test]
    fn stop_without_active_thread_is_graceful() {
        tauri::async_runtime::block_on(async {
            let flag = DownloadThreadControl::new(DownloadThreadControlFlag::Go);

            assert!(
                stop_and_wait_download_thread(Some(&flag), None, Duration::from_millis(50)).await
            );
            assert!(matches!(flag.get(), DownloadThreadControlFlag::Stop));
        });
    }

    #[test]
    fn cooperative_download_thread_is_joined() {
        tauri::async_runtime::block_on(async {
            let flag = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
            let finished = Arc::new(AtomicBool::new(false));
            let finished_thread = finished.clone();

            let thread = tauri::async_runtime::spawn(async move {
                finished_thread.store(true, Ordering::SeqCst);
            });

            assert!(
                stop_and_wait_download_thread(Some(&flag), Some(thread), Duration::from_secs(1))
                    .await
            );
            assert!(finished.load(Ordering::SeqCst));
            assert!(matches!(flag.get(), DownloadThreadControlFlag::Stop));
        });
    }

    #[test]
    fn uncooperative_download_thread_is_aborted_and_dropped() {
        tauri::async_runtime::block_on(async {
            struct DropGuard(Arc<AtomicBool>);
            impl Drop for DropGuard {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::SeqCst);
                }
            }

            let flag = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
            let dropped = Arc::new(AtomicBool::new(false));
            let dropped_thread = dropped.clone();
            let (started_sender, started_receiver) = tokio::sync::oneshot::channel();

            let thread = tauri::async_runtime::spawn(async move {
                let _drop_guard = DropGuard(dropped_thread);
                let _ = started_sender.send(());
                pending::<()>().await;
            });

            started_receiver.await.unwrap();
            let started = Instant::now();

            assert!(
                !stop_and_wait_download_thread(
                    Some(&flag),
                    Some(thread),
                    Duration::from_millis(50),
                )
                .await
            );
            assert!(started.elapsed() < Duration::from_secs(1));
            assert!(
                dropped.load(Ordering::SeqCst),
                "aborted download thread should be dropped before shutdown returns"
            );
            assert!(matches!(flag.get(), DownloadThreadControlFlag::Stop));
        });
    }

    #[test]
    fn pause_joins_worker_and_allows_resume() {
        tauri::async_runtime::block_on(async {
            let mut current_download = CurrentDownload::default();
            let flag = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
            let worker_flag = flag.clone();
            let finished = Arc::new(AtomicBool::new(false));
            let finished_worker = finished.clone();

            let thread = tauri::async_runtime::spawn(async move {
                while worker_flag.get() != DownloadThreadControlFlag::Stop {
                    tokio::task::yield_now().await;
                }
                finished_worker.store(true, Ordering::SeqCst);
            });

            assert!(current_download.start(flag.clone(), thread));
            assert!(current_download.stop_and_wait(Duration::from_secs(1)).await);
            current_download.cleanup(Duration::from_secs(1)).await;

            assert!(finished.load(Ordering::SeqCst));
            assert!(!current_download.is_active());
            assert!(matches!(flag.get(), DownloadThreadControlFlag::Stop));

            let resumed = Arc::new(AtomicBool::new(false));
            let resumed_worker = resumed.clone();
            let resume_thread = tauri::async_runtime::spawn(async move {
                resumed_worker.store(true, Ordering::SeqCst);
            });

            assert!(current_download.start(
                DownloadThreadControl::new(DownloadThreadControlFlag::Stop),
                resume_thread
            ));
            assert!(current_download.stop_and_wait(Duration::from_secs(1)).await);
            current_download.cleanup(Duration::from_secs(1)).await;

            assert!(resumed.load(Ordering::SeqCst));
            assert!(!current_download.is_active());
        });
    }

    #[test]
    fn start_refuses_to_overwrite_active_download() {
        tauri::async_runtime::block_on(async {
            let mut current_download = CurrentDownload::default();
            let flag = DownloadThreadControl::new(DownloadThreadControlFlag::Go);

            let thread = tauri::async_runtime::spawn(async {
                pending::<()>().await;
            });
            assert!(current_download.start(flag, thread));

            let replacement = tauri::async_runtime::spawn(async {
                pending::<()>().await;
            });
            assert!(!current_download.start(
                DownloadThreadControl::new(DownloadThreadControlFlag::Go),
                replacement
            ));
            assert!(current_download.is_active());

            assert!(
                !current_download
                    .stop_and_wait(Duration::from_millis(50))
                    .await
            );
            current_download.cleanup(Duration::from_millis(50)).await;
            assert!(!current_download.is_active());
        });
    }

    #[test]
    fn cancel_ordering_stops_worker_before_callback() {
        tauri::async_runtime::block_on(async {
            let mut current_download = CurrentDownload::default();
            let flag = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
            let worker_flag = flag.clone();
            let worker_stopped = Arc::new(AtomicBool::new(false));
            let worker_stopped_clone = worker_stopped.clone();

            let thread = tauri::async_runtime::spawn(async move {
                while worker_flag.get() != DownloadThreadControlFlag::Stop {
                    tokio::task::yield_now().await;
                }
                worker_stopped_clone.store(true, Ordering::SeqCst);
            });

            assert!(current_download.start(flag, thread));
            assert!(current_download.stop_and_wait(Duration::from_secs(1)).await);

            // Mirrors manage_cancel_signal: the cancellation hook is invoked
            // only after the worker has stopped and joined.
            assert!(
                worker_stopped.load(Ordering::SeqCst),
                "cancellation hook must not run while the worker is still writing"
            );

            current_download.cleanup(Duration::from_secs(1)).await;
            assert!(!current_download.is_active());
        });
    }

    #[test]
    fn pause_preserves_completion_that_finishes_while_stopping() {
        tauri::async_runtime::block_on(async {
            let mut current_download = CurrentDownload::default();
            let flag = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
            let metadata = test_metadata("completed-during-stop");
            let terminal = current_download.terminal_handle();
            let terminal_metadata = metadata.clone();
            let (release_sender, release_receiver) = tokio::sync::oneshot::channel();

            let thread = tauri::async_runtime::spawn(async move {
                // Simulates on_complete awaiting; Stop does not interrupt it.
                let _ = release_receiver.await;
                *lock!(terminal) = Some(TerminalRecord {
                    metadata: terminal_metadata,
                    outcome: TerminalOutcome::Completed,
                });
            });

            assert!(current_download.start(flag, thread));

            thread::spawn(move || {
                thread::sleep(Duration::from_millis(25));
                let _ = release_sender.send(());
            });

            assert!(current_download.stop_and_wait(Duration::from_secs(1)).await);
            assert!(
                current_download.has_terminal(),
                "a completed worker must not be re-queued by pause"
            );
            assert_eq!(
                current_download.take_terminal_for(&metadata),
                Some(TerminalOutcome::Completed)
            );

            current_download.cleanup(Duration::from_secs(1)).await;
            assert!(!current_download.is_active());
        });
    }

    #[test]
    fn pause_preserves_error_that_finishes_while_stopping() {
        tauri::async_runtime::block_on(async {
            let mut current_download = CurrentDownload::default();
            let flag = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
            let metadata = test_metadata("errored-during-stop");
            let terminal = current_download.terminal_handle();
            let terminal_metadata = metadata.clone();
            let (release_sender, release_receiver) = tokio::sync::oneshot::channel();

            let thread = tauri::async_runtime::spawn(async move {
                let _ = release_receiver.await;
                *lock!(terminal) = Some(TerminalRecord {
                    metadata: terminal_metadata,
                    outcome: TerminalOutcome::Error,
                });
            });

            assert!(current_download.start(flag, thread));

            thread::spawn(move || {
                thread::sleep(Duration::from_millis(25));
                let _ = release_sender.send(());
            });

            assert!(current_download.stop_and_wait(Duration::from_secs(1)).await);
            assert!(
                current_download.has_terminal(),
                "an errored worker must not be re-queued by pause"
            );
            assert_eq!(
                current_download.take_terminal_for(&metadata),
                Some(TerminalOutcome::Error)
            );

            current_download.cleanup(Duration::from_secs(1)).await;
            assert!(!current_download.is_active());
        });
    }

    #[test]
    fn fast_worker_terminal_survives_handle_registration() {
        tauri::async_runtime::block_on(async {
            let mut current_download = CurrentDownload::default();
            let metadata = test_metadata("fast-worker");
            let terminal = current_download.terminal_handle();
            let worker_metadata = metadata.clone();
            let (finished_sender, finished_receiver) = tokio::sync::oneshot::channel();
            let thread = tauri::async_runtime::spawn(async move {
                *lock!(terminal) = Some(TerminalRecord {
                    metadata: worker_metadata,
                    outcome: TerminalOutcome::Completed,
                });
                let _ = finished_sender.send(());
            });

            finished_receiver.await.unwrap();
            assert!(current_download.start(
                DownloadThreadControl::new(DownloadThreadControlFlag::Stop),
                thread
            ));

            assert!(current_download.has_terminal());
            assert_eq!(
                current_download.take_terminal_for(&metadata),
                Some(TerminalOutcome::Completed)
            );

            current_download.cleanup(Duration::from_secs(1)).await;
            assert!(!current_download.is_active());
        });
    }

    #[test]
    fn completed_terminal_removes_nonfront_item_after_rearrange() {
        tauri::async_runtime::block_on(async {
            let current_download = CurrentDownload::default();
            let completed = test_metadata("completed");
            let other = test_metadata("other");
            let queue = Queue::new();

            // The completed item was moved behind another item before its
            // Completed signal was processed.
            queue.append(other.clone());
            queue.append(completed.clone());
            *lock!(current_download.terminal) = Some(TerminalRecord {
                metadata: completed.clone(),
                outcome: TerminalOutcome::Completed,
            });

            assert_eq!(
                current_download.take_terminal_for(&completed),
                Some(TerminalOutcome::Completed)
            );
            assert!(remove_download_from_queue(&queue, &completed));
            assert_eq!(queue.read().front(), Some(&other));
            assert_eq!(queue.read().len(), 1);
        });
    }
}
