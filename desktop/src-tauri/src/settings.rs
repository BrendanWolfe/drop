use std::{
    fs::create_dir_all,
    io::{Error, ErrorKind},
    path::{Path, PathBuf},
};

use database::{
    Settings, borrow_db_checked, borrow_db_mut_checked,
    db::DATA_ROOT_DIR,
    debug::SystemData,
    install_locations::{InstallLocationKind, InstallLocations},
};
use download_manager::error::DownloadManagerError;
use games::{downloads::set_download_speed_limit, scan::scan_install_dirs};
use log::error;
use serde_json::Value;

// Will, in future, return disk/remaining size
// Just returns the directories that have been set up
#[tauri::command]
pub fn fetch_download_dir_stats(kind: Option<InstallLocationKind>) -> Vec<PathBuf> {
    let lock = borrow_db_checked();
    lock.applications
        .install_directories(kind.unwrap_or_default())
        .to_vec()
}

#[tauri::command]
pub fn fetch_install_locations() -> InstallLocations {
    borrow_db_checked().applications.install_locations()
}

#[tauri::command]
pub fn set_default_download_dir(
    index: Option<usize>,
    path: Option<PathBuf>,
    kind: InstallLocationKind,
) -> Result<(), String> {
    let mut lock = borrow_db_mut_checked();
    let index = lock
        .applications
        .configured_install_index(kind, index, path.as_deref())?;
    lock.applications.set_default_install_directory(kind, index)
}

#[tauri::command]
pub fn delete_download_dir(
    index: Option<usize>,
    path: Option<PathBuf>,
    kind: Option<InstallLocationKind>,
) -> Result<(), String> {
    let mut lock = borrow_db_mut_checked();
    let kind = kind.unwrap_or_default();
    let index = lock
        .applications
        .configured_install_index(kind, index, path.as_deref())?;
    lock.applications.remove_install_directory(kind, index)
}

#[tauri::command]
pub fn add_download_dir(
    new_dir: PathBuf,
    allow_non_empty: bool,
    kind: Option<InstallLocationKind>,
    app: tauri::AppHandle,
) -> Result<(), DownloadManagerError<()>> {
    // Check the new directory is all good
    let new_dir_path = Path::new(&new_dir);
    if new_dir_path.exists() {
        let dir_contents = new_dir_path.read_dir()?;
        if !allow_non_empty && dir_contents.count() != 0 {
            return Err(Error::new(
                ErrorKind::DirectoryNotEmpty,
                "Selected directory cannot contain any existing files",
            )
            .into());
        }
    } else {
        create_dir_all(new_dir_path)?;
    }

    // Add it to the dictionary
    let mut lock = borrow_db_mut_checked();
    lock.applications
        .add_install_directory(kind.unwrap_or_default(), new_dir)
        .map_err(|message| Error::new(ErrorKind::AlreadyExists, message))?;
    drop(lock);

    let recovered = scan_install_dirs();
    let db = database::borrow_db_checked();
    for id in recovered {
        let version = db
            .applications
            .installed_game_version
            .get(&id)
            .and_then(|meta| db.applications.game_versions.get(&meta.version))
            .cloned();
        utils::app_emit!(
            &app,
            &format!("update_game/{id}"),
            games::library::GameUpdateEvent {
                game_id: id.clone(),
                version,
                status: games::state::GameStatusManager::fetch_state(&id, &db),
            }
        );
    }
    drop(db);
    utils::app_emit!(&app, "update_library", ());

    Ok(())
}

#[tauri::command]
pub fn update_settings(new_settings: Value) {
    let mut db_lock = borrow_db_mut_checked();
    let mut current_settings =
        serde_json::to_value(db_lock.settings.clone()).expect("Failed to parse existing settings");
    let values = match new_settings.as_object() {
        Some(values) => values,
        None => {
            error!("Could not parse settings values into object");
            return;
        }
    };
    for (key, value) in values {
        current_settings[key] = value.clone();
    }
    let new_settings: Settings = match serde_json::from_value(current_settings) {
        Ok(settings) => settings,
        Err(e) => {
            error!("Could not parse settings with error {}", e);
            return;
        }
    };
    let speed_limit = new_settings.max_download_speed;
    db_lock.settings = new_settings;
    drop(db_lock);

    set_download_speed_limit(speed_limit);
}
#[tauri::command]
pub fn fetch_settings() -> Settings {
    borrow_db_checked().settings.clone()
}
#[tauri::command]
pub fn fetch_system_data() -> SystemData {
    let db_handle = borrow_db_checked();
    SystemData::new(
        db_handle.auth.as_ref().unwrap().client_id.clone(),
        db_handle.base_url.clone(),
        DATA_ROOT_DIR.to_string_lossy().to_string(),
        std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
    )
}
