use async_trait::async_trait;
use database::models::data::UserConfiguration;
use database::{
    ApplicationTransientStatus, DownloadableMetadata, borrow_db_checked, borrow_db_mut_checked,
};
use download_manager::depot_manager::DepotManager;
use download_manager::download_manager_frontend::{DownloadManagerSignal, DownloadStatus};
use download_manager::downloadable::Downloadable;
use download_manager::error::ApplicationDownloadError;
use download_manager::util::download_thread_control_flag::{
    DownloadThreadControl, DownloadThreadControlFlag,
};
use download_manager::util::progress_object::{ProgressHandle, ProgressObject, ProgressType};
use droplet_types::{ChunkData, Manifest};
use futures_util::StreamExt;
use futures_util::stream::FuturesUnordered;
use log::{debug, error, info, warn};
use remote::auth::generate_authorization_header;
use remote::cache::get_cached_object;
use remote::error::RemoteAccessError;
use remote::requests::generate_url;
use remote::utils::DROP_CLIENT_ASYNC;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::fs::{create_dir_all, remove_file};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tokio::sync::mpsc::Sender;
use utils::{app_emit, lock};

use crate::downloads::utils::get_disk_available;
use crate::library::{
    Game, on_game_complete, push_game_update, set_partially_installed, set_partially_installed_db,
};
use crate::state::GameStatusManager;

use super::download_logic::{download_game_chunk, wait_for_stop};
use super::drop_data::DropData;
use super::server_archive;

static RETRY_COUNT: usize = 3;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInformation {
    file_list: HashMap<String, String>,
    manifests: HashMap<String, Manifest>,
    install_size: u64,
    download_size: u64,
    #[serde(default)]
    installer_package: bool,
    #[serde(default)]
    installer_flow: bool,
}

pub struct GameDownloadAgent {
    pub metadata: DownloadableMetadata,
    pub configuration: UserConfiguration,
    pub control_flag: DownloadThreadControl,
    pub dl_info: Mutex<Option<DownloadInformation>>,
    pub download_progress: Arc<ProgressObject>,
    pub disk_progress: Arc<ProgressObject>,
    depot_manager: Arc<DepotManager>,
    pub dropdata: DropData,
    status: Mutex<DownloadStatus>,
    verification: bool,
    companion_request: Option<CompanionRequest>,
    companion: OnceLock<CompanionPlan>,
    new_companion_folder: bool,
    started: AtomicBool,
    sender: Sender<DownloadManagerSignal>,
    label: Option<String>,
    asset: Option<database::models::data::InstallerAsset>,
    reused_asset: AtomicBool,
}

pub struct CompanionRequest {
    pub inputs: Vec<database::models::data::InstallerLibraryInput>,
    pub missing_dependencies: bool,
    pub already_complete: bool,
}

struct CompanionPlan {
    inputs: Vec<database::models::data::InstallerLibraryInput>,
    files: HashMap<String, u64>,
    checkpointable: HashSet<String>,
    // A mixed chunk can be resumed by this exact job, but must not be saved
    // as fully installed for a later download with a different selection.
    completed: Mutex<HashMap<String, bool>>,
    complete_install: bool,
}

impl Debug for GameDownloadAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameDownloadAgent").finish()
    }
}

impl GameDownloadAgent {
    pub fn new_installer_asset(
        metadata: DownloadableMetadata,
        package: &Path,
        asset: &database::models::data::InstallerAsset,
        sender: Sender<DownloadManagerSignal>,
        depot_manager: Arc<DepotManager>,
    ) -> Result<Self, ApplicationDownloadError> {
        let (directory, storage_meta) =
            super::installer_files::prepare_asset_root(package, &metadata, asset)?;
        let mut agent = Self::at_path(
            storage_meta,
            directory,
            sender,
            depot_manager,
            Default::default(),
            false,
            None,
        )?;
        agent.metadata = metadata;
        agent.asset = Some(asset.clone());
        agent.label = Some(asset.name.clone());
        Ok(agent)
    }

    fn is_installer_asset(&self) -> bool {
        self.metadata.download_type == database::DownloadType::InstallerAsset
    }

    fn notify_asset_status(&self, app: &AppHandle, done: bool) {
        if done {
            borrow_db_mut_checked()
                .applications
                .transient_statuses
                .remove(&self.metadata);
        }
        app_emit!(app, "installer_assets_changed", &self.metadata.id);
    }

    pub async fn new_installer(
        metadata: DownloadableMetadata,
        base_dir: PathBuf,
        sender: Sender<DownloadManagerSignal>,
        depot_manager: Arc<DepotManager>,
        configuration: UserConfiguration,
    ) -> Result<Self, ApplicationDownloadError> {
        let existing = {
            let db = borrow_db_checked();
            db.applications
                .installed_game_version
                .get(&metadata.id)
                .and_then(|installed| db.applications.resolve_install_path(installed).ok())
        };
        let directory = if let Some(existing) = existing {
            existing
        } else {
            let name = get_cached_object::<Game>(&format!("game/{}", metadata.id))
                .map(|game| game.library_path)
                .unwrap_or_else(|_| metadata.id.clone());
            super::install_path::create_installer_directory(&base_dir, &name)?
        };
        super::installer_files::record(&directory, &metadata.id, std::iter::empty())?;
        Self::new_at_path(metadata, directory, sender, depot_manager, configuration).await
    }

    pub async fn new(
        metadata: DownloadableMetadata,
        base_dir: PathBuf,
        version_name: Option<String>,
        sender: Sender<DownloadManagerSignal>,
        depot_manager: Arc<DepotManager>,
        configuration: UserConfiguration,
    ) -> Result<Self, ApplicationDownloadError> {
        let game_name = get_cached_object::<Game>(&format!("game/{}", metadata.id))
            .map(|v| v.library_path)
            .unwrap_or(metadata.id.clone());

        let base_dir_path = Path::new(&base_dir);
        info!("base dir {}", base_dir_path.display());
        let data_base_dir_path = if metadata.download_type == database::DownloadType::Emulator {
            // Existing exact builds (including legacy folders) resume in place.
            let db = borrow_db_checked();
            match db
                .applications
                .emulator_installation(&metadata.id, &metadata.version, metadata.target_platform)
                .map(|install| &install.status)
            {
                Some(database::GameDownloadStatus::Installed { .. }) => db
                    .applications
                    .resolve_install_path(&metadata)
                    .map_err(server_archive::fail)?,
                _ => super::install_path::create_emulator_directory(
                    base_dir_path,
                    &game_name,
                    version_name.as_deref().unwrap_or("version"),
                    &metadata,
                )?,
            }
        } else {
            let db = borrow_db_checked();
            db.applications
                .installed_game_version
                .get(&metadata.id)
                .and_then(|installed| db.applications.resolve_install_path(installed).ok())
                .unwrap_or_else(|| base_dir_path.join(game_name))
        };
        info!("data dir path {}", data_base_dir_path.display());

        Self::new_at_path(
            metadata,
            data_base_dir_path,
            sender,
            depot_manager,
            configuration,
        )
        .await
    }

    pub async fn new_at_path(
        metadata: DownloadableMetadata,
        data_base_dir_path: PathBuf,
        sender: Sender<DownloadManagerSignal>,
        depot_manager: Arc<DepotManager>,
        configuration: UserConfiguration,
    ) -> Result<Self, ApplicationDownloadError> {
        let agent = Self::at_path(
            metadata,
            data_base_dir_path,
            sender,
            depot_manager,
            configuration,
            false,
            None,
        )?;
        agent.prepare().await?;
        Ok(agent)
    }

    pub fn new_companion(
        metadata: DownloadableMetadata,
        directory: PathBuf,
        name_for_new_folder: Option<&str>,
        sender: Sender<DownloadManagerSignal>,
        depot_manager: Arc<DepotManager>,
        configuration: UserConfiguration,
        request: CompanionRequest,
    ) -> Result<Self, ApplicationDownloadError> {
        let directory = match name_for_new_folder {
            Some(name) => {
                super::install_path::create_companion_directory(&directory, name, &metadata)?
            }
            None => directory,
        };
        let result = Self::at_path(
            metadata,
            directory.clone(),
            sender,
            depot_manager,
            configuration,
            false,
            Some(request),
        );
        match result {
            Ok(mut agent) => {
                agent.new_companion_folder = name_for_new_folder.is_some();
                Ok(agent)
            }
            Err(error) => {
                if name_for_new_folder.is_some() {
                    let _ = std::fs::remove_dir(directory);
                }
                Err(error)
            }
        }
    }

    pub async fn verify_at_path(
        metadata: DownloadableMetadata,
        path: PathBuf,
        sender: Sender<DownloadManagerSignal>,
        depot_manager: Arc<DepotManager>,
        configuration: UserConfiguration,
    ) -> Result<Self, ApplicationDownloadError> {
        let mut agent = Self::at_path(
            metadata,
            path,
            sender,
            depot_manager,
            configuration,
            true,
            None,
        )?;
        agent.prepare().await?;
        let cached = borrow_db_checked()
            .applications
            .game_versions
            .contains_key(&agent.metadata.version);
        if !cached {
            let portable = agent.dropdata.installed_version.clone().filter(|version| {
                version.game_id == agent.metadata.id && version.version_id == agent.metadata.version
            });
            let mut version: database::GameVersion = if let Some(version) = portable {
                version
            } else {
                let url = generate_url(
                    &[
                        "/api/v1/client/game",
                        &agent.metadata.id,
                        "version",
                        &agent.metadata.version,
                    ],
                    &[],
                )?;
                let response = DROP_CLIENT_ASYNC
                    .get(url)
                    .header("Authorization", generate_authorization_header()?)
                    .send()
                    .await
                    .map_err(|error| ApplicationDownloadError::Communication(error.into()))?;
                if !response.status().is_success() {
                    return Err(server_archive::fail(
                        "The installed version's launch metadata is unavailable",
                    ));
                }
                response
                    .json()
                    .await
                    .map_err(|error| ApplicationDownloadError::Communication(error.into()))?
            };
            version.user_configuration = agent.configuration.clone();
            agent.dropdata.installed_version = Some(version.clone());
            borrow_db_mut_checked()
                .applications
                .game_versions
                .entry(agent.metadata.version.clone())
                .or_insert(version);
        }
        Ok(agent)
    }

    fn at_path(
        metadata: DownloadableMetadata,
        data_base_dir_path: PathBuf,
        sender: Sender<DownloadManagerSignal>,
        depot_manager: Arc<DepotManager>,
        configuration: UserConfiguration,
        verification: bool,
        companion: Option<CompanionRequest>,
    ) -> Result<Self, ApplicationDownloadError> {
        // Don't run by default
        let control_flag = DownloadThreadControl::new(DownloadThreadControlFlag::Stop);
        if verification {
            let data = DropData::read(&data_base_dir_path)?;
            if data.game_id != metadata.id
                || data.game_version != metadata.version
                || data.target_platform != metadata.target_platform
            {
                return Err(server_archive::fail(
                    "This folder belongs to a different installation",
                ));
            }
            if !std::fs::symlink_metadata(&data_base_dir_path)?.is_dir() {
                return Err(server_archive::fail("Installation root is not a directory"));
            }
        } else {
            create_dir_all(data_base_dir_path.clone())?;
        }

        let mut stored_manifest = DropData::generate(
            metadata.id.clone(),
            metadata.version.clone(),
            metadata.target_platform,
            data_base_dir_path.clone(),
            configuration.clone(),
        );
        stored_manifest.emulator = metadata.download_type == database::DownloadType::Emulator;
        if metadata.download_type == database::DownloadType::InstallerAsset
            && stored_manifest.installer_asset_files.take().is_some()
        {
            // A previously finished asset was removed or changed. Rebuild it.
            stored_manifest.set_contexts(&[]);
        }
        if verification {
            stored_manifest.previously_installed_version = None;
        }
        if companion.is_some() {
            // Until this job finishes discovery must never mark the source playable.
            stored_manifest.companion_only = true;
        }

        Ok(Self {
            metadata,
            control_flag,
            dl_info: Mutex::new(None),
            download_progress: Arc::new(ProgressObject::new(
                0,
                0,
                sender.clone(),
                ProgressType::Download,
            )),
            disk_progress: Arc::new(ProgressObject::new(
                0,
                0,
                sender.clone(),
                ProgressType::Disk,
            )),
            dropdata: stored_manifest,
            status: Mutex::new(DownloadStatus::Queued),
            depot_manager,
            configuration,
            verification,
            companion_request: companion,
            companion: OnceLock::new(),
            new_companion_folder: false,
            started: AtomicBool::new(false),
            sender,
            label: None,
            asset: None,
            reused_asset: AtomicBool::new(false),
        })
    }

    async fn prepare(&self) -> Result<(), ApplicationDownloadError> {
        self.accept_manifest(self.fetch_manifest().await?)
    }

    fn accept_manifest(
        &self,
        mut info: DownloadInformation,
    ) -> Result<(), ApplicationDownloadError> {
        let companion = self
            .companion_request
            .as_ref()
            .map(|request| Self::prepare_companion_plan(&mut info, &self.dropdata, request))
            .transpose()?;

        let required_space = if self.verification {
            0
        } else {
            let archive = server_archive::validate(&info.manifests, &self.metadata.version)?;
            if self.is_installer_asset() {
                let completed = self.dropdata.get_contexts();
                let cached_bytes: u64 = info
                    .manifests
                    .values()
                    .flat_map(|manifest| &manifest.chunks)
                    .filter(|(id, _)| completed.get(*id) == Some(&true))
                    .flat_map(|(_, chunk)| &chunk.files)
                    .map(|file| file.length as u64)
                    .sum();
                info.install_size.saturating_sub(cached_bytes)
            } else if archive.is_some() {
                server_archive::required_space(
                    &self.dropdata.base_path,
                    &self.metadata.version,
                    &info.manifests[&self.metadata.version],
                )?
            } else {
                info.install_size
            }
        };

        let available_space = get_disk_available(self.dropdata.base_path.clone())? as u64;

        if required_space > available_space {
            return Err(ApplicationDownloadError::DiskFull(
                required_space,
                available_space,
            ));
        }

        if let Some(plan) = companion {
            self.companion
                .set(plan)
                .map_err(|_| ApplicationDownloadError::Lock)?;
        }
        *lock!(self.dl_info) = Some(info);
        Ok(())
    }

    async fn prepare_queued(
        &self,
        manifest: impl Future<Output = Result<DownloadInformation, ApplicationDownloadError>>,
    ) -> Result<bool, ApplicationDownloadError> {
        *lock!(self.status) = DownloadStatus::Preparing;
        let _ = self.sender.try_send(DownloadManagerSignal::UpdateUIQueue);
        // The server may be indexing an archive. Don't let the request block
        // pause, cancel, or switching to another queued download.
        let info = tokio::select! {
            biased;
            _ = wait_for_stop(&self.control_flag) => return Ok(false),
            result = manifest => result?,
        };
        self.accept_manifest(info)?;
        Ok(true)
    }

    fn prepare_companion_plan(
        info: &mut DownloadInformation,
        data: &DropData,
        request: &CompanionRequest,
    ) -> Result<CompanionPlan, ApplicationDownloadError> {
        if info.installer_package
            || info.installer_flow
            || server_archive::validate(&info.manifests, &data.game_version)?.is_some()
        {
            return Err(server_archive::fail(
                "This server cannot download individual files from this version",
            ));
        }
        let full_file_list = &info.file_list;
        for input in &request.inputs {
            if !full_file_list
                .keys()
                .any(|name| super::companion::matches(input, name))
            {
                return Err(server_archive::fail(format!(
                    "Companion path is not in the pinned version: {}",
                    input.path
                )));
            }
        }
        let selected = full_file_list
            .iter()
            .filter(|(name, _)| {
                request
                    .inputs
                    .iter()
                    .any(|input| super::companion::matches(input, name))
            })
            .map(|(name, version)| (name.clone(), version.clone()))
            .collect::<HashMap<_, _>>();
        let files = super::integrity::expected_lengths(&selected, &info.manifests)?;
        let needed = selected
            .iter()
            .filter(|(name, _)| {
                !data
                    .companion_files
                    .has_file(&data.base_path, name, files[*name])
            })
            .map(|(name, version)| (name.clone(), version.clone()))
            .collect::<HashMap<_, _>>();
        let mut checkpointable = HashSet::new();
        info.install_size = needed.keys().map(|name| files[name]).sum();
        info.download_size = 0;
        for (version, manifest) in &mut info.manifests {
            manifest.chunks.retain(|id, chunk| {
                if !chunk
                    .files
                    .iter()
                    .any(|file| needed.get(&file.filename) == Some(version))
                {
                    return false;
                }
                if chunk.files.iter().all(|file| {
                    full_file_list.get(&file.filename) != Some(version)
                        || selected.contains_key(&file.filename)
                }) {
                    checkpointable.insert(id.clone());
                } else {
                    // A selected file may be missing from a previously completed install.
                    data.set_context(id.clone(), false);
                }
                info.download_size += chunk
                    .files
                    .iter()
                    .map(|file| file.length as u64)
                    .sum::<u64>();
                true
            });
        }
        let complete_install = request.already_complete
            || (selected.len() == full_file_list.len() && !request.missing_dependencies);
        info.file_list = needed;
        Ok(CompanionPlan {
            inputs: request.inputs.clone(),
            files,
            checkpointable,
            completed: Mutex::new(HashMap::new()),
            complete_install,
        })
    }

    fn scan_filetree(&self, path: &Path) -> Result<Vec<PathBuf>, io::Error> {
        if !std::fs::symlink_metadata(path)?.is_dir() {
            return Err(io::Error::other("Cleanup root is not a directory"));
        }

        let subdirs = path.read_dir()?;
        let mut results = Vec::new();
        for subdir in subdirs {
            let subdir = subdir?;
            // Never follow symlinks into another installation or a save folder.
            if subdir.file_type()?.is_dir() {
                results.extend(self.scan_filetree(&subdir.path())?);
            } else {
                results.push(subdir.path());
            }
        }
        Ok(results)
    }

    // Blocking
    pub fn setup_download(&self, app_handle: &AppHandle) -> Result<(), ApplicationDownloadError> {
        if !self.check_manifest_exists() {
            return Err(ApplicationDownloadError::NotInitialized);
        }

        self.started.store(true, Ordering::Relaxed);
        self.dropdata.write();
        if self.is_installer_asset() {
            return Ok(());
        }
        let mut db_lock = borrow_db_mut_checked();
        if !self.verification {
            set_partially_installed_db(
                &mut db_lock,
                &self.metadata,
                self.dropdata.base_path.display().to_string(),
                None,
                self.configuration.clone(),
                self.dropdata.companion_only,
            );
        }
        let status = ApplicationTransientStatus::Downloading {
            version_id: self.metadata.version.clone(),
        };
        db_lock
            .applications
            .transient_statuses
            .insert(self.metadata(), status.clone());
        if self.metadata.download_type == database::DownloadType::Emulator {
            app_emit!(app_handle, "update_library", ());
        } else {
            push_game_update(
                app_handle,
                &self.metadata.id,
                None,
                GameStatusManager::fetch_state(&self.metadata.id, &db_lock),
            );
        }

        Ok(())
    }

    // Blocking
    pub async fn download(&self, app_handle: &AppHandle) -> Result<bool, ApplicationDownloadError> {
        if let Some(asset) = &self.asset {
            if super::installer_files::ready_asset_payload(
                &self.dropdata.base_path,
                &self.dropdata.game_id,
                &self.dropdata.game_version,
                &asset.target_path,
            )
            .is_some()
            {
                self.reused_asset.store(true, Ordering::Relaxed);
                return Ok(true);
            }
        }
        if !self.check_manifest_exists() && !self.prepare_queued(self.fetch_manifest()).await? {
            return Ok(false);
        }
        if self.control_flag.get() == DownloadThreadControlFlag::Stop {
            return Ok(false);
        }
        *lock!(self.status) = DownloadStatus::Downloading;
        let _ = self.sender.try_send(DownloadManagerSignal::UpdateUIQueue);
        let archive = self.archive_size()?;
        let lease = if archive.is_some() {
            Some(server_archive::Lease::acquire(&self.dropdata.base_path)?)
        } else {
            None
        };
        self.setup_download(app_handle)?;
        let timer = Instant::now();

        info!("beginning download for {}...", self.metadata().id);

        let res = self.run(app_handle, archive, lease).await;

        debug!(
            "{} took {}ms to download",
            self.metadata.id,
            timer.elapsed().as_millis()
        );
        res
    }

    pub fn check_manifest_exists(&self) -> bool {
        lock!(self.dl_info).is_some()
    }

    async fn fetch_manifest(&self) -> Result<DownloadInformation, ApplicationDownloadError> {
        let client = DROP_CLIENT_ASYNC.clone();
        let url = generate_url(
            &["/api/v1/client/game/manifest"],
            &[
                ("id", &self.metadata.id),
                ("version", &self.metadata.version),
                ("archive", "true"),
                ("installer", "true"),
                ("installerAssets", "true"),
                ("installerFlow", "true"),
                (
                    "companionFiles",
                    if self.dropdata.companion_only {
                        "true"
                    } else {
                        "false"
                    },
                ),
                (
                    "previous",
                    if self.dropdata.companion_only {
                        ""
                    } else {
                        self.dropdata
                            .previously_installed_version
                            .as_deref()
                            .unwrap_or("")
                    },
                ),
            ],
        )
        .map_err(ApplicationDownloadError::Communication)?;

        let response = client
            .get(url)
            .header("Authorization", generate_authorization_header()?)
            .timeout(Duration::from_secs(600))
            .send()
            .await
            .map_err(|e| ApplicationDownloadError::Communication(e.into()))?;

        if response.status() != 200 {
            return Err(ApplicationDownloadError::Communication(
                RemoteAccessError::ManifestDownloadFailed(
                    response.status(),
                    response.text().await.unwrap_or_default(),
                ),
            ));
        }

        response
            .json()
            .await
            .map_err(|e| ApplicationDownloadError::Communication(e.into()))
    }

    // Sets up progress for download writes
    fn setup_progress(&self) {
        let dl_info = lock!(self.dl_info);
        let dl_info = dl_info.as_ref().unwrap();

        let total_chunks = dl_info
            .manifests
            .iter()
            .map(|v| v.1.chunks.len())
            .sum::<usize>();

        self.download_progress
            .set_max(dl_info.download_size.try_into().unwrap());
        self.download_progress.set_size(total_chunks);
        self.download_progress.reset();

        self.disk_progress
            .set_max(dl_info.install_size.try_into().unwrap());
        self.disk_progress.set_size(total_chunks);
        self.disk_progress.reset();
    }

    fn archive_size(&self) -> Result<Option<u64>, ApplicationDownloadError> {
        let info = lock!(self.dl_info);
        let info = info.as_ref().ok_or(ApplicationDownloadError::Lock)?;
        server_archive::validate(&info.manifests, &self.metadata.version)
    }

    async fn run(
        &self,
        app_handle: &AppHandle,
        archive_size: Option<u64>,
        lease: Option<Arc<server_archive::Lease>>,
    ) -> Result<bool, ApplicationDownloadError> {
        super::initialize_download_speed_limit_from_db();
        if self.verification && archive_size.is_none() {
            *lock!(self.status) = DownloadStatus::Validating;
            let status = ApplicationTransientStatus::Validating {
                version_id: self.metadata.version.clone(),
            };
            borrow_db_mut_checked()
                .applications
                .transient_statuses
                .insert(self.metadata(), status.clone());
            push_game_update(app_handle, &self.metadata.id, None, (None, Some(status)));
            app_emit!(app_handle, "update_library", ());
            let (files, manifests) = {
                let info = lock!(self.dl_info);
                let info = info.as_ref().unwrap();
                (info.file_list.clone(), info.manifests.clone())
            };
            let total_chunks = manifests
                .values()
                .map(|manifest| manifest.chunks.len())
                .sum::<usize>();
            let root = self.dropdata.base_path.clone();
            let control = self.control_flag.clone();
            let progress = self.disk_progress.clone();
            let mut stop = server_archive::StopOnDrop(Some(control.clone()));
            let valid = tokio::task::spawn_blocking(move || {
                super::integrity::verified_contexts(&root, &files, &manifests, &control, &progress)
            })
            .await
            .map_err(|_| server_archive::fail("Integrity verification worker failed"))??;
            stop.0 = None;
            if self.control_flag.get() == DownloadThreadControlFlag::Stop {
                return Ok(false);
            }
            self.dropdata
                .set_contexts(&valid.into_iter().collect::<Vec<_>>());
            self.dropdata.write();
            if self.dropdata.get_contexts().len() == total_chunks {
                let info = lock!(self.dl_info);
                let info = info.as_ref().unwrap();
                super::integrity::finish_lengths(
                    &self.dropdata.base_path,
                    &info.file_list,
                    &info.manifests,
                )?;
                return Ok(true);
            }
            *lock!(self.status) = DownloadStatus::Downloading;
            let status = ApplicationTransientStatus::Downloading {
                version_id: self.metadata.version.clone(),
            };
            borrow_db_mut_checked()
                .applications
                .transient_statuses
                .insert(self.metadata(), status.clone());
            push_game_update(app_handle, &self.metadata.id, None, (None, Some(status)));
        }
        self.setup_progress();
        info!("setup progress objects");
        let manifests_chunks: Vec<(String, Vec<(String, ChunkData)>, [u8; 16])> = {
            let dl_info = lock!(self.dl_info);
            dl_info
                .as_ref()
                .unwrap()
                .manifests
                .iter()
                .map(|(id, manifest)| {
                    let mut chunks: Vec<_> = manifest.chunks.clone().into_iter().collect();
                    if archive_size.is_some() {
                        chunks.sort_by_key(|(_, chunk)| chunk.files[0].start);
                    }
                    (id.clone(), chunks, manifest.key)
                })
                .collect()
        };
        let file_list = {
            let dl_info = lock!(self.dl_info);
            dl_info.as_ref().unwrap().file_list.clone()
        };
        let mut completed_chunks = self.companion.get().map_or_else(
            || self.dropdata.get_contexts(),
            |plan| lock!(plan.completed).clone(),
        );
        let current_chunks = manifests_chunks
            .iter()
            .flat_map(|(_, chunks, _)| chunks.iter().map(|(id, _)| id))
            .collect::<std::collections::HashSet<_>>();
        completed_chunks.retain(|id, complete| *complete && current_chunks.contains(id));
        info!("started with {} existing chunks", completed_chunks.len());
        let chunk_len = manifests_chunks.iter().map(|v| v.1.len()).sum::<usize>();
        let mut max_download_threads = borrow_db_checked().settings.max_download_threads;
        if max_download_threads == 0 {
            max_download_threads = 1;
        }

        let archive_cache = if archive_size.is_some() {
            let cache = server_archive::prepare(&self.dropdata.base_path, &self.metadata.version)?;
            let manifest =
                lock!(self.dl_info).as_ref().unwrap().manifests[&self.metadata.version].clone();
            // Verification below runs on a blocking worker, not the UI/runtime.
            let cached = completed_chunks.clone();
            let stage = cache.clone();
            let control = self.control_flag.clone();
            completed_chunks = tokio::task::spawn_blocking(move || {
                server_archive::verified_contexts(&stage, &manifest, &cached, &control)
            })
            .await
            .map_err(|_| server_archive::fail("Archive verification worker failed"))??;
            // A pause during verification must not discard checkpoints for
            // ranges we haven't checked yet; leave them for the next resume.
            if self.control_flag.get() == DownloadThreadControlFlag::Stop {
                return Ok(false);
            }
            self.dropdata.set_contexts(
                &completed_chunks
                    .iter()
                    .map(|(id, valid)| (id.clone(), *valid))
                    .collect::<Vec<_>>(),
            );
            self.dropdata.write();
            // During transfer the disk counter measures the staged archive.
            self.disk_progress
                .set_max(lock!(self.dl_info).as_ref().unwrap().download_size as usize);
            Some(cache)
        } else {
            None
        };

        let file_list = &file_list;
        let base_path = archive_cache.as_ref().unwrap_or(&self.dropdata.base_path);
        let installer_flow = {
            let info = lock!(self.dl_info);
            let info = info.as_ref().unwrap();
            info.installer_package || info.installer_flow
        };
        if installer_flow && archive_size.is_none() && !self.verification {
            super::installer_files::record(
                &self.dropdata.base_path,
                &self.metadata.id,
                file_list.keys(),
            )?;
        }
        let current_file_tree = if archive_size.is_none()
            && !self.verification
            && !installer_flow
            && self.companion.get().is_none()
        {
            self.scan_filetree(base_path)?
        } else {
            Vec::new()
        };

        for file in current_file_tree {
            if !base_path.is_dir() {
                return Err(io::Error::other("Cleanup root is not a directory").into());
            }
            let relative = file.strip_prefix(base_path)?;
            if relative.as_os_str().is_empty() {
                return Err(io::Error::other("Refusing to delete the installation root").into());
            }
            let filename = relative.to_string_lossy().replace('\\', "/");
            let needed = file_list.contains_key(&filename) || filename == ".dropdata";
            if !needed {
                debug!("deleted {}", file.display());
                remove_file(file)?;
            }
        }

        let local_completed_chunks = completed_chunks.clone();
        if completed_chunks.values().filter(|valid| **valid).count() < chunk_len {
            self.depot_manager.sync_depots().await?;
        }

        // Restore all completed bytes before starting any network work.
        for (index, (version_id, chunk_id, chunk_data)) in manifests_chunks
            .iter()
            .flat_map(|(version_id, chunks, _)| {
                chunks.iter().map(move |(id, data)| (version_id, id, data))
            })
            .enumerate()
        {
            if !local_completed_chunks
                .get(chunk_id)
                .copied()
                .unwrap_or(false)
            {
                continue;
            }
            ProgressHandle::new(
                self.download_progress.get(index),
                self.download_progress.clone(),
            )
            .skip(chunk_data.files.iter().map(|file| file.length).sum());
            ProgressHandle::new(self.disk_progress.get(index), self.disk_progress.clone()).skip(
                chunk_data
                    .files
                    .iter()
                    .filter(|file| file_list.get(&file.filename) == Some(version_id))
                    .map(|file| file.length)
                    .sum(),
            );
        }

        let mut chunk_completions = FuturesUnordered::new();

        let mut handle_output =
            |value: Result<Option<String>, ApplicationDownloadError>| match value {
                Ok(value) => {
                    if let Some(chunk_id) = value {
                        completed_chunks.insert(chunk_id.clone(), true);
                        if let Some(plan) = self.companion.get() {
                            lock!(plan.completed).insert(chunk_id.clone(), true);
                        }
                        if self
                            .companion
                            .get()
                            .is_none_or(|plan| plan.checkpointable.contains(&chunk_id))
                        {
                            self.dropdata.set_context(chunk_id, true);
                            self.dropdata.write();
                        }
                    }
                    Ok(())
                }
                Err(err) => Err(err),
            };

        let mut index = 0;
        for (version_id, chunks, key) in manifests_chunks.into_iter() {
            let version_id = &version_id;
            for (chunk_id, chunk_data) in chunks.into_iter() {
                let download_progress_handle = ProgressHandle::new(
                    self.download_progress.get(index),
                    self.download_progress.clone(),
                );
                let disk_progress_handle =
                    ProgressHandle::new(self.disk_progress.get(index), self.disk_progress.clone());
                index += 1;

                if *local_completed_chunks.get(&chunk_id).unwrap_or(&false) {
                    continue;
                }

                let (depot, permit) = match self
                    .depot_manager
                    .next_depot(&self.metadata.id, &self.metadata.version)
                {
                    Ok(v) => v,
                    Err(err) => {
                        return Err(err.into());
                    }
                };

                let local_version_id = version_id.clone();
                while chunk_completions.len() >= max_download_threads {
                    handle_output(
                        chunk_completions
                            .next()
                            .await
                            .expect("max download threads is zero?"),
                    )?;
                }
                chunk_completions.push(async move {
                    for i in 0..RETRY_COUNT {
                        match download_game_chunk(
                            &self.metadata.id,
                            &local_version_id,
                            &chunk_id,
                            &depot,
                            &key,
                            &chunk_data,
                            file_list,
                            base_path,
                            max_download_threads,
                            &self.control_flag,
                            &download_progress_handle,
                            &disk_progress_handle,
                        )
                        .await
                        {
                            Ok(true) => {
                                drop(permit);
                                return Ok(Some(chunk_id.clone()));
                            }
                            Ok(false) => return Ok(None),
                            Err(e) => {
                                warn!("got error for chunk id {}: {e:?}", chunk_id);

                                let retry = true; /*matches!(
                                &e,
                                ApplicationDownloadError::Communication(_)
                                | ApplicationDownloadError::Checksum
                                | ApplicationDownloadError::Lock
                                | ApplicationDownloadError::IoError(_)
                                );*/

                                if i == RETRY_COUNT - 1 || !retry {
                                    warn!("retry logic failed, not re-attempting.");
                                    return Err(e);
                                }
                            }
                        }
                    }
                    Ok(None)
                });
            }
        }

        while let Some(value) = chunk_completions.next().await {
            handle_output(value)?
        }

        info!("completed {} chunks", completed_chunks.len());

        // If there are any contexts left which are false
        if completed_chunks.values().filter(|valid| **valid).count() != chunk_len {
            info!(
                "download agent for {} exited without completing ({}/{})",
                self.metadata.id.clone(),
                completed_chunks.len(),
                chunk_len,
            );
            return Ok(false);
        }
        if let Some(expected_size) = archive_size {
            if self.control_flag.get() == DownloadThreadControlFlag::Stop {
                return Ok(false);
            }
            *lock!(self.status) = DownloadStatus::Extracting;
            let status = ApplicationTransientStatus::Extracting {
                version_id: self.metadata.version.clone(),
            };
            borrow_db_mut_checked()
                .applications
                .transient_statuses
                .insert(self.metadata(), status.clone());
            push_game_update(app_handle, &self.metadata.id, None, (None, Some(status)));
            let path = base_path.join(droplet_types::ARCHIVE_FILENAME);
            // A resumed cache may have stale trailing bytes. Extract exactly
            // the verified archive, never bytes outside its manifest ranges.
            let archive_length = lock!(self.dl_info).as_ref().unwrap().download_size;
            tokio::fs::OpenOptions::new()
                .write(true)
                .open(&path)
                .await?
                .set_len(archive_length)
                .await?;
            let root = self.dropdata.base_path.clone();
            let id = self.metadata.id.clone();
            let version = self.metadata.version.clone();
            let control = self.control_flag.clone();
            let disk = self.disk_progress.clone();
            let mut stop_on_drop = server_archive::StopOnDrop(Some(control.clone()));
            let extraction_lease = lease.clone();
            let verification = self.verification;
            let extracted = tokio::task::spawn_blocking(move || {
                let _lease = extraction_lease;
                let extract = if verification {
                    super::extract::verify_managed
                } else {
                    super::extract::extract_managed
                };
                extract(&path, &root, &id, &version, expected_size, &control, &disk)
            })
            .await
            .map_err(|_| server_archive::fail("Archive extraction worker failed"))??;
            stop_on_drop.0 = None;
            if !extracted {
                return Ok(false);
            }
            self.dropdata.write();
        } else if self.verification || self.companion.get().is_some() {
            let info = lock!(self.dl_info);
            let info = info.as_ref().unwrap();
            super::integrity::finish_lengths(
                &self.dropdata.base_path,
                &info.file_list,
                &info.manifests,
            )?;
        }
        Ok(true)
    }

    #[allow(dead_code)]
    fn setup_validate(&self, app_handle: &AppHandle) {
        self.setup_progress();

        self.control_flag.set(DownloadThreadControlFlag::Go);

        let status = ApplicationTransientStatus::Validating {
            version_id: self.metadata.version.clone(),
        };

        let mut db_lock = borrow_db_mut_checked();
        db_lock
            .applications
            .transient_statuses
            .insert(self.metadata(), status.clone());
        push_game_update(app_handle, &self.metadata().id, None, (None, Some(status)));
    }

    pub fn validate(&self, _app_handle: &AppHandle) -> Result<bool, ApplicationDownloadError> {
        /*
        self.setup_validate(app_handle);

        let buckets = lock!(self.buckets);
        let contexts: Vec<DropValidateContext> = buckets
            .clone()
            .into_iter()
            .flat_map(|e| -> Vec<DropValidateContext> { e.into() })
            .collect();
        let max_download_threads = borrow_db_checked().settings.max_download_threads;

        info!("{} validation contexts", contexts.len());
        let pool = ThreadPoolBuilder::new()
            .num_threads(max_download_threads)
            .build()
            .unwrap_or_else(|_| {
                panic!("failed to build thread pool with {max_download_threads} threads")
            });

        let invalid_chunks = Arc::new(boxcar::Vec::new());
        pool.scope(|scope| {
            for (index, context) in contexts.iter().enumerate() {
                let current_progress = self.progress.get(index);
                let progress_handle = ProgressHandle::new(current_progress, self.progress.clone());
                let invalid_chunks_scoped = invalid_chunks.clone();
                let sender = self.sender.clone();

                scope.spawn(move |_| {
                    match validate_game_chunk(context, &self.control_flag, progress_handle) {
                        Ok(true) => {}
                        Ok(false) => {
                            invalid_chunks_scoped.push(context.checksum.clone());
                        }
                        Err(e) => {
                            error!("{e}");
                            send!(sender, DownloadManagerSignal::Error(e));
                        }
                    }
                });
            }
        });

        // If there are any contexts left which are false
        if !invalid_chunks.is_empty() {
            info!("validation of game id {} failed", self.id);

            for context in invalid_chunks.iter() {
                self.dropdata.set_context(context.1.clone(), false);
            }

            self.dropdata.write();

            return Ok(false);
        }
         */

        Ok(true)
    }

    pub fn cancel(&self, app_handle: &AppHandle) {
        if self.is_installer_asset() {
            self.notify_asset_status(app_handle, true);
            return;
        }
        if self.verification {
            self.finish_verification(app_handle, false);
            return;
        }
        if self.companion_request.is_some() && !self.started.load(Ordering::Relaxed) {
            // A queued companion cancelled before the first write should not
            // create a partial installation or take over an existing one.
            self.clear_failed_download(app_handle);
            return;
        }
        // See docs on usage
        set_partially_installed(
            &self.metadata(),
            self.dropdata.base_path.display().to_string(),
            Some(app_handle),
            self.configuration.clone(),
            self.dropdata.companion_only,
        );

        self.dropdata.write();
        if self.companion_request.is_some() {
            app_emit!(app_handle, "update_library", ());
        }
    }

    fn clear_failed_download(&self, app: &AppHandle) {
        if self.new_companion_folder {
            // Never delete payloads or checkpoints, only an unused directory.
            let _ = std::fs::remove_dir(&self.dropdata.base_path);
        }
        let mut db = borrow_db_mut_checked();
        db.applications.transient_statuses.remove(&self.metadata);
        if self.metadata.download_type == database::DownloadType::Game {
            push_game_update(
                app,
                &self.metadata.id,
                None,
                GameStatusManager::fetch_state(&self.metadata.id, &db),
            );
        }
        drop(db);
        if self.metadata.download_type == database::DownloadType::Emulator
            || self.companion_request.is_some()
        {
            app_emit!(app, "update_library", ());
        }
    }

    fn finish_verification(&self, app: &AppHandle, success: bool) {
        let mut db = borrow_db_mut_checked();
        db.applications.transient_statuses.remove(&self.metadata);
        if success {
            let status = if self.metadata.download_type == database::DownloadType::Emulator {
                db.applications
                    .emulator_installation(
                        &self.metadata.id,
                        &self.metadata.version,
                        self.metadata.target_platform,
                    )
                    .map(|item| item.status.clone())
            } else {
                db.applications
                    .game_statuses
                    .get(&self.metadata.id)
                    .cloned()
            };
            if let Some(mut status) = status {
                if let database::GameDownloadStatus::Installed { install_type, .. } = &mut status {
                    if matches!(
                        install_type,
                        database::models::data::InstalledGameType::PartiallyInstalled { .. }
                    ) {
                        *install_type = database::models::data::InstalledGameType::Installed;
                    }
                }
                db.applications.record_installation(&self.metadata, status);
            }
        }
        if self.metadata.download_type == database::DownloadType::Game {
            let version = db
                .applications
                .game_versions
                .get(&self.metadata.version)
                .cloned();
            push_game_update(
                app,
                &self.metadata.id,
                version,
                GameStatusManager::fetch_state(&self.metadata.id, &db),
            );
        }
        drop(db);
        app_emit!(app, "update_library", ());
        if success {
            app_emit!(app, "integrity_complete", &self.metadata.id);
        }
    }
}

#[async_trait]
impl Downloadable for GameDownloadAgent {
    async fn download(&self, app_handle: &AppHandle) -> Result<bool, ApplicationDownloadError> {
        self.download(app_handle).await
    }

    fn validate(&self, app_handle: &AppHandle) -> Result<bool, ApplicationDownloadError> {
        *lock!(self.status) = DownloadStatus::Validating;
        self.validate(app_handle)
    }

    fn dl_progress(&self) -> &Arc<ProgressObject> {
        &self.download_progress
    }

    fn disk_progress(&self) -> &Arc<ProgressObject> {
        &self.disk_progress
    }

    fn control_flag(&self) -> DownloadThreadControl {
        self.control_flag.clone()
    }

    fn metadata(&self) -> DownloadableMetadata {
        self.metadata.clone()
    }

    fn label(&self) -> Option<String> {
        self.label.clone()
    }

    fn on_queued(&self, app_handle: &tauri::AppHandle) {
        *self.status.lock().unwrap() = DownloadStatus::Queued;
        if self.is_installer_asset() {
            self.notify_asset_status(app_handle, false);
            return;
        }
        let mut db_lock = borrow_db_mut_checked();
        if !self.verification && self.metadata.download_type == database::DownloadType::Emulator {
            self.dropdata.write();
            set_partially_installed_db(
                &mut db_lock,
                &self.metadata,
                self.dropdata.base_path.to_string_lossy().into_owned(),
                None,
                self.configuration.clone(),
                false,
            );
        }
        let status = ApplicationTransientStatus::Queued {
            version_id: self.metadata.version.clone(),
        };
        db_lock
            .applications
            .transient_statuses
            .insert(self.metadata(), status.clone());
        if self.metadata.download_type == database::DownloadType::Emulator {
            app_emit!(app_handle, "update_library", ());
        } else {
            push_game_update(app_handle, &self.metadata.id, None, (None, Some(status)));
        }
    }

    fn on_error(&self, app_handle: &tauri::AppHandle, error: &ApplicationDownloadError) {
        *lock!(self.status) = DownloadStatus::Error;
        app_emit!(app_handle, "download_error", error.to_string());

        error!("error while managing download: {error:?}");

        if self.is_installer_asset() {
            self.notify_asset_status(app_handle, true);
            return;
        }

        if self.verification {
            self.finish_verification(app_handle, false);
            return;
        }

        self.clear_failed_download(app_handle);
    }

    async fn on_complete(
        &self,
        app_handle: &tauri::AppHandle,
    ) -> Result<(), ApplicationDownloadError> {
        if self.is_installer_asset() {
            if self.reused_asset.load(Ordering::Relaxed) {
                self.notify_asset_status(app_handle, true);
                return Ok(());
            }
            let info = lock!(self.dl_info);
            let info = info
                .as_ref()
                .ok_or(ApplicationDownloadError::NotInitialized)?;
            super::integrity::finish_lengths(
                &self.dropdata.base_path,
                &info.file_list,
                &info.manifests,
            )?;
            let mut data = DropData::read(&self.dropdata.base_path)?;
            data.installer_asset_files = Some(super::integrity::expected_lengths(
                &info.file_list,
                &info.manifests,
            )?);
            data.write();
            self.notify_asset_status(app_handle, true);
            return Ok(());
        }
        if self.verification {
            self.finish_verification(app_handle, true);
            if self.archive_size()?.is_some() {
                server_archive::cleanup(&self.dropdata.base_path);
            }
            return Ok(());
        }
        if let Some(plan) = self.companion.get() {
            let mut data = DropData::read(&self.dropdata.base_path)?;
            data.companion_files
                .record(&data.base_path, &plan.inputs, plan.files.clone())?;
            data.write();
            if !plan.complete_install {
                set_partially_installed(
                    &self.metadata,
                    data.base_path.to_string_lossy().into_owned(),
                    Some(app_handle),
                    self.configuration.clone(),
                    true,
                );
                app_emit!(app_handle, "update_library", ());
                return Ok(());
            }
        }
        on_game_complete(
            &self.metadata(),
            self.configuration.clone(),
            self.dropdata.base_path.to_string_lossy().to_string(),
            app_handle,
        )
        .await
        .map_err(ApplicationDownloadError::DownloadError)?;
        if self.archive_size()?.is_some() {
            server_archive::cleanup(&self.dropdata.base_path);
        }
        Ok(())
    }

    fn on_cancelled(&self, app_handle: &tauri::AppHandle) {
        self.cancel(app_handle);
    }

    fn status(&self) -> DownloadStatus {
        lock!(self.status).clone()
    }
}

#[cfg(test)]
mod companion_plan_tests {
    use super::*;
    use database::{
        models::data::{InstallerLibraryInput, InstallerLibraryInputKind},
        platform::Platform,
    };
    use droplet_types::FileEntry;

    fn fixture() -> DownloadInformation {
        let chunk = |names: &[&str]| ChunkData {
            files: names
                .iter()
                .map(|name| FileEntry {
                    filename: (*name).into(),
                    start: 0,
                    length: 3,
                    permissions: 0o600,
                })
                .collect(),
            checksum: String::new(),
            iv: [0; 16],
        };
        DownloadInformation {
            file_list: ["roms/game.rom", "other.bin", "unrelated.bin"]
                .map(|name| (name.into(), "pinned".into()))
                .into(),
            manifests: [(
                "pinned".into(),
                Manifest {
                    version: "2".into(),
                    archive: None,
                    key: [0; 16],
                    size: 9,
                    chunks: [
                        ("mixed".into(), chunk(&["roms/game.rom", "other.bin"])),
                        ("unrelated".into(), chunk(&["unrelated.bin"])),
                    ]
                    .into(),
                },
            )]
            .into(),
            install_size: 9,
            download_size: 9,
            installer_package: false,
            installer_flow: false,
        }
    }

    fn selection(path: &str, missing_dependencies: bool) -> CompanionRequest {
        CompanionRequest {
            inputs: vec![InstallerLibraryInput {
                name: None,
                game_id: "game".into(),
                version_id: "pinned".into(),
                game_name: "Game".into(),
                version_name: "Pinned".into(),
                kind: InstallerLibraryInputKind::Folder,
                path: path.into(),
            }],
            missing_dependencies,
            already_complete: false,
        }
    }

    #[tokio::test]
    async fn companion_is_queueable_before_manifest_and_can_resume_after_stopping() {
        let directory = tempfile::tempdir().unwrap();
        let (sender, mut events) = tokio::sync::mpsc::channel(4);
        let agent = GameDownloadAgent::new_companion(
            DownloadableMetadata::new(
                "game".into(),
                "pinned".into(),
                Platform::HOST,
                database::DownloadType::Game,
            ),
            directory.path().to_path_buf(),
            Some("Source Game"),
            sender,
            Arc::new(DepotManager::new()),
            Default::default(),
            selection("roms", true),
        )
        .unwrap();
        assert_eq!(agent.status(), DownloadStatus::Queued);
        assert!(!agent.check_manifest_exists());
        assert!(
            !agent
                .dropdata
                .base_path
                .join(super::super::drop_data::DROPDATA_PATH)
                .exists()
        );

        agent.control_flag.set(DownloadThreadControlFlag::Go);
        let prepare = agent.prepare_queued(std::future::pending());
        let stop = async {
            assert!(matches!(
                events.recv().await,
                Some(DownloadManagerSignal::UpdateUIQueue)
            ));
            assert_eq!(agent.status(), DownloadStatus::Preparing);
            agent.control_flag.set(DownloadThreadControlFlag::Stop);
        };
        let (result, ()) = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::join!(prepare, stop)
        })
        .await
        .unwrap();
        assert!(!result.unwrap());
        assert!(!agent.check_manifest_exists());

        agent.control_flag.set(DownloadThreadControlFlag::Go);
        assert!(agent.prepare_queued(async { Ok(fixture()) }).await.unwrap());
        let info = lock!(agent.dl_info);
        assert_eq!(
            info.as_ref().unwrap().file_list.keys().collect::<Vec<_>>(),
            &["roms/game.rom"]
        );
        assert!(!agent.companion.get().unwrap().complete_install);
    }

    #[test]
    fn companion_plan_keeps_unrelated_files_out_of_mixed_chunks() {
        let directory = tempfile::tempdir().unwrap();
        for (path, deps, complete) in [("roms", false, false), ("", true, false), ("", false, true)]
        {
            let data = DropData::new(
                "game".into(),
                "pinned".into(),
                Platform::HOST,
                directory.path().into(),
                Default::default(),
                None,
            );
            let mut info = fixture();
            let plan =
                GameDownloadAgent::prepare_companion_plan(&mut info, &data, &selection(path, deps))
                    .unwrap();
            assert_eq!(plan.complete_install, complete);
            if path == "roms" {
                assert_eq!(
                    info.file_list.keys().collect::<Vec<_>>(),
                    &["roms/game.rom"]
                );
                assert_eq!(info.manifests["pinned"].chunks.len(), 1);
                assert_eq!((info.install_size, info.download_size), (3, 6));
                assert!(plan.checkpointable.is_empty());
            } else {
                assert_eq!(info.file_list.len(), 3);
                assert_eq!(plan.checkpointable.len(), 2);
            }
        }
    }
}
