use std::{
    collections::HashMap,
    fs::{OpenOptions, create_dir_all},
    io,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    sync::Arc,
    thread::spawn,
    time::{Duration, SystemTime},
};

use database::{
    ApplicationTransientStatus, Database, DownloadableMetadata, GameDownloadStatus, GameVersion,
    borrow_db_checked, borrow_db_mut_checked, db::DATA_ROOT_DIR, models::data::InstalledGameType,
    platform::Platform,
};
use games::{library::{push_game_update, report_playtime}, state::GameStatusManager};
use log::{debug, info, warn};
use serde::Serialize;
use shared_child::SharedChild;
use tauri::{AppHandle, Emitter as _};

use crate::{
    PROCESS_MANAGER,
    error::ProcessError,
    parser::ParsedCommand,
    process_handlers::{
        AsahiMuvmLauncher, LinuxNativeLauncher, MacLauncher, UMUCompatLauncher, UMUNativeLauncher,
        WindowsCmdLauncher, WindowsDirectLauncher, WindowsLauncher, WindowsPowershellLauncher,
    },
};

pub struct RunningProcess {
    handle: Arc<SharedChild>,
    start: SystemTime,
    manually_killed: bool,
}

pub struct ProcessManager<'a> {
    current_platform: Platform,
    log_output_dir: PathBuf,
    processes: HashMap<String, RunningProcess>,
    game_launchers: Vec<(
        (Platform, Platform),
        &'a (dyn ProcessHandler + Sync + Send + 'static),
    )>,
    app_handle: AppHandle,
}

#[derive(Serialize)]
pub struct LaunchOption {
    name: String,
}

#[derive(Serialize)]
pub struct ProcessHandlerOption {
    id: String,
    name: String,
    description: String,
}

impl ProcessManager<'_> {
    pub fn new(app_handle: AppHandle) -> Self {
        let log_output_dir = DATA_ROOT_DIR.join("logs");

        ProcessManager {
            #[cfg(target_os = "windows")]
            current_platform: Platform::Windows,

            #[cfg(target_os = "macos")]
            current_platform: Platform::macOS,

            #[cfg(target_os = "linux")]
            current_platform: Platform::Linux,

            processes: HashMap::new(),
            log_output_dir,
            game_launchers: vec![
                // Current platform to target platform
                (
                    (Platform::Windows, Platform::Windows),
                    &WindowsLauncher {} as &(dyn ProcessHandler + Sync + Send + 'static),
                ),
                (
                    (Platform::Windows, Platform::Windows),
                    &WindowsDirectLauncher {} as &(dyn ProcessHandler + Sync + Send + 'static),
                ),
                (
                    (Platform::Windows, Platform::Windows),
                    &WindowsCmdLauncher {} as &(dyn ProcessHandler + Sync + Send + 'static),
                ),
                (
                    (Platform::Windows, Platform::Windows),
                    &WindowsPowershellLauncher {} as &(dyn ProcessHandler + Sync + Send + 'static),
                ),
                (
                    (Platform::Linux, Platform::Linux),
                    &LinuxNativeLauncher {} as &(dyn ProcessHandler + Sync + Send + 'static),
                ),
                (
                    (Platform::Linux, Platform::Linux),
                    &UMUNativeLauncher {} as &(dyn ProcessHandler + Sync + Send + 'static),
                ),
                (
                    (Platform::macOS, Platform::macOS),
                    &MacLauncher {} as &(dyn ProcessHandler + Sync + Send + 'static),
                ),
                (
                    (Platform::Linux, Platform::Windows),
                    &AsahiMuvmLauncher {} as &(dyn ProcessHandler + Sync + Send + 'static),
                ),
                (
                    (Platform::Linux, Platform::Windows),
                    &UMUCompatLauncher {} as &(dyn ProcessHandler + Sync + Send + 'static),
                ),
            ],
            app_handle,
        }
    }

    pub fn kill_game(&mut self, game_id: String) -> Result<(), io::Error> {
        match self.processes.get_mut(&game_id) {
            Some(process) => {
                process.manually_killed = true;
                kill_process_tree(&process.handle)?;
                let exit_status = process.handle.wait()?;
                info!("exit status: {:?}", exit_status);
                Ok(())
            }
            None => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Game ID not running",
            )),
        }
    }

    pub fn get_log_dir(&self, game_id: String) -> PathBuf {
        self.log_output_dir.join(game_id)
    }

    fn on_process_finish(
        &mut self,
        game_id: String,
        result: Result<ExitStatus, std::io::Error>,
    ) -> Result<(), ProcessError> {
        if !self.processes.contains_key(&game_id) {
            warn!(
                "process on_finish was called, but game_id is no longer valid. finished with result: {result:?}"
            );
            return Ok(());
        }

        debug!("process for {:?} exited with {:?}", &game_id, result);

        let process = match self.processes.remove(&game_id) {
            Some(process) => process,
            None => {
                info!("Attempted to stop process {game_id} which didn't exist");
                return Ok(());
            }
        };

        let mut db_handle = borrow_db_mut_checked();
        let meta = db_handle
            .applications
            .installed_game_version
            .get(&game_id)
            .cloned()
            .unwrap_or_else(|| panic!("Could not get installed version of {}", &game_id));
        db_handle.applications.transient_statuses.remove(&meta);

        let current_state = db_handle.applications.game_statuses.get_mut(&game_id);
        if let Some(GameDownloadStatus::Installed { install_type, .. }) = current_state
            && let Ok(exit_code) = result
            && exit_code.success()
        {
            *install_type = InstalledGameType::Installed;
        }

        let elapsed = process.start.elapsed().unwrap_or(Duration::ZERO);
        // If we started and ended really quickly, something might've gone wrong
        // Or if the status isn't 0
        // Or if it's an error
        if !process.manually_killed
            && (elapsed.as_secs() <= 2 || result.map_or(true, |r| !r.success()))
        {
            warn!("drop detected that the game {game_id} may have failed to launch properly");
            let _ = self.app_handle.emit("launch_external_error", &game_id);
        }

        let version_data = match db_handle.applications.game_versions.get(&meta.version) {
            // This unwrap here should be resolved by just making the hashmap accept an option rather than just a String
            Some(res) => res,
            None => todo!(),
        };

        let status = GameStatusManager::fetch_state(&game_id, &db_handle);

        push_game_update(
            &self.app_handle,
            &game_id,
            Some(version_data.clone()),
            status,
        );

        let elapsed_seconds = elapsed.as_secs();
        if let Some(local) = db_handle.applications.local_games.get_mut(&game_id) {
            local.playtime_seconds = local.playtime_seconds.saturating_add(elapsed_seconds);
            let _ = self.app_handle.emit("update_local_game", &game_id);
            return Ok(());
        }
        if elapsed_seconds > 2 {
            let playtime_game_id = game_id.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = report_playtime(playtime_game_id.clone(), elapsed_seconds).await
                {
                    warn!("failed to report playtime for game {playtime_game_id}: {error}");
                }
            });
        }

        Ok(())
    }

    fn fetch_process_handler(
        &self,
        db_lock: &Database,
        target_platform: &Platform,
        override_id: Option<&str>,
    ) -> Result<&(dyn ProcessHandler + Send + Sync), ProcessError> {
        // An explicit override wins, as long as it's valid for the current platform.
        if let Some(override_id) = override_id
            && let Some(handler) = self.game_launchers.iter().find(|e| {
                let (e_current, e_target) = e.0;
                e_current == self.current_platform
                    && e_target == *target_platform
                    && e.1.id() == override_id
                    && e.1.valid_for_platform(db_lock, target_platform)
            })
        {
            return Ok(handler.1);
        }

        Ok(self
            .game_launchers
            .iter()
            .find(|e| {
                let (e_current, e_target) = e.0;
                e_current == self.current_platform
                    && e_target == *target_platform
                    && e.1.valid_for_platform(db_lock, target_platform)
            })
            .ok_or(ProcessError::InvalidPlatform)?
            .1)
    }

    pub fn valid_platform(&self, platform: &Platform) -> bool {
        let db_lock = borrow_db_checked();
        let process_handler = self.fetch_process_handler(&db_lock, platform, None);
        process_handler.is_ok()
    }

    pub fn proton_ready(&self, game_id: &str) -> bool {
        #[cfg(target_os = "linux")]
        {
            let db = borrow_db_checked();
            if self
                .fetch_process_handler(&db, &Platform::Windows, None)
                .is_err()
            {
                return false;
            }
            let configuration = db.applications.installed_game_version
                .get(game_id)
                .and_then(|meta| db.applications.game_versions.get(&meta.version))
                .map(|version| &version.user_configuration);
            let path = configuration
                .and_then(|config| config.override_proton_path.as_ref())
                .or(db.applications.default_proton_path.as_ref());
            path.is_some_and(|path| {
                crate::compat::read_proton_path(path.into())
                    .ok()
                    .flatten()
                    .is_some()
            })
        }
        #[cfg(not(target_os = "linux"))]
        {
            let _ = game_id;
            false
        }
    }

    pub fn get_process_handlers(
        &self,
        game_id: String,
    ) -> Result<Vec<ProcessHandlerOption>, ProcessError> {
        let db_lock = borrow_db_checked();

        let meta = db_lock
            .applications
            .installed_game_version
            .get(&game_id)
            .cloned()
            .ok_or(ProcessError::NotInstalled)?;

        let target_platform = meta.target_platform;

        let handlers = self
            .game_launchers
            .iter()
            .filter(|e| {
                let (e_current, e_target) = e.0;
                e_current == self.current_platform
                    && e_target == target_platform
                    && e.1.valid_for_platform(&db_lock, &target_platform)
            })
            .map(|e| ProcessHandlerOption {
                id: e.1.id().to_string(),
                name: e.1.name().to_string(),
                description: e.1.description().to_string(),
            })
            .collect();

        Ok(handlers)
    }

    pub fn get_launch_options(game_id: String) -> Result<Vec<LaunchOption>, ProcessError> {
        let db_lock = borrow_db_checked();

        let meta = db_lock
            .applications
            .installed_game_version
            .get(&game_id)
            .cloned()
            .ok_or(ProcessError::NotInstalled)?;

        let game_version = db_lock
            .applications
            .game_versions
            .get(&meta.version)
            .ok_or(ProcessError::InvalidVersion)?;

        let launch_options = game_version
            .launches
            .iter()
            .filter(|v| v.platform == meta.target_platform)
            .filter(|v| v.platform.can_run_on(Platform::HOST, v.proton_compatible))
            .map(|v| LaunchOption {
                name: v.name.clone(),
            })
            .collect::<Vec<LaunchOption>>();

        Ok(launch_options)
    }

    pub fn launch_process(
        &mut self,
        game_id: String,
        launch_process_index: usize,
    ) -> Result<(), ProcessError> {
        if self.processes.contains_key(&game_id) {
            return Err(ProcessError::AlreadyRunning);
        }

        let mut db_lock = borrow_db_mut_checked();

        let meta = db_lock
            .applications
            .installed_game_version
            .get(&game_id)
            .cloned()
            .ok_or(ProcessError::NotInstalled)?;

        let game_status = db_lock
            .applications
            .game_statuses
            .get(&game_id)
            .ok_or(ProcessError::NotInstalled)?;

        let (version_name, install_dir) = match game_status {
            GameDownloadStatus::Installed {
                version_id: version_name,
                install_dir,
                install_type: InstalledGameType::Installed | InstalledGameType::SetupRequired,
                ..
            } => (version_name, install_dir),
            _ => return Err(ProcessError::NotInstalled),
        };

        debug!(
            "Launching process {:?} with version {:?}",
            &game_id,
            db_lock.applications.game_versions.get(version_name)
        );

        let game_version = db_lock
            .applications
            .game_versions
            .get(version_name)
            .ok_or(ProcessError::InvalidVersion)?;

        if let Some(local) = db_lock.applications.local_games.get(&game_id)
            && !local.launcher.as_ref().is_some_and(|path| path.is_file()) {
            return Err(ProcessError::InvalidArguments("Launcher missing. Choose a new launcher in game settings.".into()));
        }

        let game_log_folder = &self.get_log_dir(game_id);
        create_dir_all(game_log_folder)?;

        let current_time = chrono::offset::Local::now();
        let log_file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .read(true)
            .create(true)
            .open(game_log_folder.join(format!(
                "{}-{}.log",
                &meta.version,
                current_time.timestamp()
            )))?;

        let error_file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .read(true)
            .create(true)
            .open(game_log_folder.join(format!(
                "{}-{}-error.log",
                &meta.version,
                current_time.timestamp()
            )))?;

        let target_platform = meta.target_platform;

        let mut process_handler = self
            .fetch_process_handler(
                &db_lock,
                &target_platform,
                game_version.user_configuration.override_handler.as_deref(),
            )
            .map_err(|error| {
                if Platform::HOST == Platform::Linux && target_platform == Platform::Windows {
                    ProcessError::NoCompat
                } else {
                    error
                }
            })?;
        debug!("using process handler {:?}", process_handler.id());

        let (target_command, emulator) = match game_status {
            GameDownloadStatus::Installed {
                install_type: InstalledGameType::Installed,
                ..
            } => {
                let (_, launch_config) = game_version
                    .launches
                    .iter()
                    .filter(|v| v.platform == target_platform)
                    .filter(|v| v.platform.can_run_on(Platform::HOST, v.proton_compatible))
                    .enumerate()
                    .find(|(i, _)| *i == launch_process_index)
                    .ok_or(ProcessError::NotInstalled)?;
                (
                    launch_config.command.clone(),
                    launch_config.emulator.as_ref(),
                )
            }
            GameDownloadStatus::Installed {
                install_type: InstalledGameType::SetupRequired,
                ..
            } => {
                let setup_config = game_version
                    .setups
                    .iter()
                    .find(|v| {
                        v.platform == target_platform
                            && v.platform.can_run_on(Platform::HOST, v.proton_compatible)
                    })
                    .ok_or(ProcessError::NotInstalled)?;

                (setup_config.command.clone(), None)
            }
            _ => unreachable!("Game registered as 'Partially Installed'"),
        };

        let target_command = ParsedCommand::parse(target_command)?;
        let mut emulator_working_directory = None;

        let target_launch_string = if let Some(emulator) = emulator {
            let err = ProcessError::RequiredDependency(
                emulator.game_id.clone(),
                emulator.version_id.clone(),
                target_platform,
            );

            let (emulator_metadata, _) = games::emulators::installed_emulator(&db_lock, &emulator.game_id, &emulator.version_id, target_platform).ok_or(err.clone())?;
            if matches!(db_lock.applications.transient_statuses.get(emulator_metadata), Some(ApplicationTransientStatus::Running { .. })) {
                return Err(ProcessError::AlreadyRunning);
            }
            let emulator_install_dir = games::emulators::emulator_install_path(
                &db_lock, &emulator.game_id, &emulator.version_id, target_platform,
                std::slice::from_ref(&emulator.launch_id), Platform::HOST,
            ).ok_or(err.clone())?;

            let emulator_game_version = db_lock
                .applications
                .game_versions
                .get(&emulator.version_id)
                .ok_or(err.clone())?;

            let emulator_launch_config = emulator_game_version
                .launches
                .iter()
                .find(|v| v.launch_id == emulator.launch_id)
                .filter(|v| v.platform == target_platform)
                .filter(|v| v.platform.can_run_on(Platform::HOST, v.proton_compatible))
                .ok_or(err)?;

            let exe_command = crate::emulator::prepare(
                emulator_launch_config,
                &target_command,
                Path::new(install_dir),
                Path::new(&emulator_install_dir),
                &emulator.version_id,
            )?;
            emulator_working_directory = Some(PathBuf::from(&emulator_install_dir));
            process_handler = self.fetch_process_handler(
                &db_lock,
                &target_platform,
                emulator_game_version.user_configuration.override_handler.as_deref(),
            )?;

            process_handler.create_launch_process(
                emulator_metadata,
                exe_command.reconstruct(),
                emulator_game_version,
                &emulator_install_dir,
                &db_lock,
            )?
        } else {
            process_handler.create_launch_process(
                &meta,
                target_command.reconstruct(),
                game_version,
                install_dir,
                &db_lock,
            )?
        };

        let resolved = crate::resolution::finish_command(
            target_launch_string,
            install_dir,
            game_version,
            emulator_working_directory.as_deref().or_else(|| {
                db_lock.applications.local_games.get(&meta.id)
                    .map(|local| local.working_directory.as_path())
            }),
        )?;
        let launch_parameters = resolved.command;
        launch_parameters.ensure_executable()?;
        let launch_working_dir = resolved.working_directory;
        info!("launching (in {}): {:?}", launch_working_dir.display(), launch_parameters);

        let mut command = {
            let mut command = Command::new(launch_parameters.command);
            command.args(launch_parameters.args);
            for parts in launch_parameters
                .env
                .into_iter()
                .filter_map(|e| e.split_once('=').map(|(key, value)| (key.to_owned(), value.to_owned())))
            {
                command.env(parts.0, parts.1);
            }
            command
        };

        command
            .stderr(error_file)
            .stdout(log_file)
            .env_remove("RUST_LOG")
            .current_dir(&launch_working_dir);

        process_handler.modify_command(&mut command);

        let child = command.spawn()?;

        let launch_process_handle = Arc::new(SharedChild::new(child)?);

        db_lock
            .applications
            .transient_statuses
            .insert(meta.clone(), ApplicationTransientStatus::Running {});

        push_game_update(
            &self.app_handle,
            &meta.id,
            None,
            (None, Some(ApplicationTransientStatus::Running {})),
        );

        let wait_thread_handle = launch_process_handle.clone();
        let wait_thread_game_id = meta.clone();

        self.processes.insert(
            meta.id,
            RunningProcess {
                handle: wait_thread_handle,
                start: SystemTime::now(),
                manually_killed: false,
            },
        );
        spawn(move || {
            let result: Result<ExitStatus, std::io::Error> = launch_process_handle.wait();

            PROCESS_MANAGER
                .lock()
                .on_process_finish(wait_thread_game_id.id, result)
        });
        Ok(())
    }

    /// Launch one exact emulator build without a game or playtime reporting.
    pub fn launch_emulator(
        &mut self,
        game_id: &str,
        version_id: &str,
        platform: Platform,
        launch_id: &str,
    ) -> Result<(), ProcessError> {
        let mut db = borrow_db_mut_checked();
        let installation = db
            .applications
            .emulator_installation(game_id, version_id, platform)
            .ok_or(ProcessError::NotInstalled)?;
        let meta = installation.metadata.clone();
        if games::emulators::emulator_in_use(&db, &meta) {
            return Err(ProcessError::AlreadyRunning);
        }
        let install_dir = games::emulators::emulator_install_path(
            &db,
            game_id,
            version_id,
            platform,
            &[launch_id.into()],
            self.current_platform,
        )
        .ok_or(ProcessError::NotInstalled)?;
        let version = db
            .applications
            .game_versions
            .get(version_id)
            .ok_or(ProcessError::InvalidVersion)?;
        let launch =
            games::emulators::direct_open_launches(version, platform, self.current_platform)
                .find(|launch| launch.launch_id == launch_id)
                .ok_or_else(|| {
                    ProcessError::InvalidArguments(
                        "No direct-open command is configured for this emulator launch.".into(),
                    )
                })?;
        let parsed = crate::emulator::prepare_direct_open(
            launch
                .emulator_profile
                .as_ref()
                .unwrap()
                .direct_open_command
                .as_deref()
                .unwrap(),
            Path::new(&install_dir),
        )?;
        let handler = self.fetch_process_handler(
            &db,
            &platform,
            version.user_configuration.override_handler.as_deref(),
        )?;
        let target = handler.create_launch_process(
            &meta,
            parsed.reconstruct(),
            version,
            &install_dir,
            &db,
        )?;
        let resolved = crate::resolution::finish_command(
            target,
            &install_dir,
            version,
            Some(Path::new(&install_dir)),
        )?;
        resolved.command.ensure_executable()?;
        let mut command = Command::new(resolved.command.command);
        command
            .args(resolved.command.args)
            .current_dir(resolved.working_directory)
            .env_remove("RUST_LOG");
        for env in resolved.command.env {
            if let Some((key, value)) = env.split_once('=') {
                command.env(key, value);
            }
        }
        let log_dir = self.get_log_dir(meta.id.clone());
        create_dir_all(&log_dir)?;
        let log = OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_dir.join(format!(
                "{}-direct-open.log",
                database::emulator_installations::build_key(&meta.id, &meta.version, platform)
            )))?;
        command.stdout(log.try_clone()?).stderr(log);
        handler.modify_command(&mut command);
        let mut child = command.spawn()?;
        db.applications
            .transient_statuses
            .insert(meta.clone(), ApplicationTransientStatus::Running {});
        utils::app_emit!(&self.app_handle, "update_library", ());
        let app = self.app_handle.clone();
        spawn(move || {
            let result = child.wait();
            if let Err(error) = result {
                warn!("Could not wait for emulator GUI: {error}");
            }
            borrow_db_mut_checked()
                .applications
                .transient_statuses
                .remove(&meta);
            utils::app_emit!(&app, "update_library", ());
        });
        Ok(())
    }
}

fn kill_process_tree(handle: &SharedChild) -> io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        // handle.kill() only terminates the launched process (often a cmd or
        // powershell wrapper), orphaning the actual game. taskkill /T kills the
        // whole process tree.
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        let pid = handle.id().to_string();
        let killed = Command::new("taskkill")
            .args(["/F", "/T", "/PID", pid.as_str()])
            .creation_flags(CREATE_NO_WINDOW)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
        if killed {
            return Ok(());
        }
    }
    handle.kill()
}

pub trait ProcessHandler: Send + 'static {
    fn create_launch_process(
        &self,
        meta: &DownloadableMetadata,
        launch_command: String,
        game_version: &GameVersion,
        current_dir: &str,
        database: &Database,
    ) -> Result<String, ProcessError>;

    fn valid_for_platform(&self, db: &Database, target: &Platform) -> bool;

    fn modify_command(&self, command: &mut Command);

    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
}
