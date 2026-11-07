use database::{
    borrow_db_checked, borrow_db_mut_checked,
    db::DATA_ROOT_DIR,
    models::data::{ArchiveInstall, ArchiveState, LocalGame},
    platform::Platform,
};
use download_manager::{DOWNLOAD_MANAGER, downloadable::Downloadable};
use games::{
    downloads::archive::{ArchiveDownloadAgent, is_busy, parse_external_url},
    local::{self, OWNER_FILE},
};
use process::ParsedCommand;
use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use tauri::AppHandle;
use utils::app_emit;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalGameInput {
    id: Option<String>,
    name: String,
    root: PathBuf,
    launcher: PathBuf,
    arguments: String,
    working_directory: PathBuf,
    platform: Platform,
}

fn ensure_idle(id: &str) -> Result<(), String> {
    let db = borrow_db_checked();
    if db
        .applications
        .transient_statuses
        .keys()
        .any(|meta| meta.id == id)
        || is_busy(id)
    {
        return Err("Stop the game or cancel its download before editing or removing it.".into());
    }
    Ok(())
}

#[tauri::command]
pub fn fetch_local_game(id: String) -> Result<LocalGame, String> {
    borrow_db_checked()
        .applications
        .local_games
        .get(&id)
        .cloned()
        .ok_or("Local game not found".into())
}

#[tauri::command]
pub fn save_local_game(input: LocalGameInput, app_handle: AppHandle) -> Result<String, String> {
    let name = input.name.trim();
    if name.is_empty() || name.len() > 200 {
        return Err("Enter a game name (up to 200 characters).".into());
    }
    let root = dunce::canonicalize(input.root).map_err(|_| "Choose an existing game folder.")?;
    let launcher =
        dunce::canonicalize(input.launcher).map_err(|_| "Choose an existing launch file.")?;
    let working_directory = dunce::canonicalize(input.working_directory)
        .map_err(|_| "Choose an existing working directory.")?;
    if !root.is_dir() || !working_directory.is_dir() || !launcher.is_file() {
        return Err("Choose valid game, launch file, and working directory paths.".into());
    }
    let parsed = ParsedCommand::for_file(&launcher, &input.arguments, input.platform)
        .map_err(|e| e.to_string())?;
    let command = parsed.reconstruct();
    let id = input
        .id
        .unwrap_or_else(|| format!("local-{}", uuid::Uuid::new_v4()));
    ensure_idle(&id)?;
    let mut db = borrow_db_mut_checked();
    let previous = db.applications.local_games.get(&id).cloned();
    if !id.starts_with("local-")
        || (previous.is_none() && db.applications.game_statuses.contains_key(&id))
    {
        return Err("Invalid local game ID".into());
    }
    if let Some(previous) = &previous {
        if previous.archive.as_ref().is_some_and(|a| !a.extracted) {
            return Err("Finish extracting the archive before choosing a launcher.".into());
        }
        if previous.archive.is_some() && previous.root != root {
            return Err("The folder of a downloaded game cannot be changed.".into());
        }
    }
    let mut archive = previous.as_ref().and_then(|g| g.archive.clone());
    if let Some(a) = &mut archive {
        a.state = ArchiveState::Ready;
        a.error = None;
    }
    let game = LocalGame {
        id: id.clone(),
        name: name.into(),
        root,
        launcher: Some(launcher),
        arguments: input.arguments,
        working_directory,
        platform: input.platform,
        playtime_seconds: previous.map_or(0, |g| g.playtime_seconds),
        archive,
    };
    local::register(&mut db, game, Some(command));
    let version = db.applications.game_versions.get(&id).cloned();
    games::library::push_game_update(
        &app_handle,
        &id,
        version,
        games::state::GameStatusManager::fetch_state(&id, &db),
    );
    drop(db);
    app_emit!(&app_handle, "update_local_game", id.clone());
    app_emit!(&app_handle, "update_library", ());
    Ok(id)
}

#[tauri::command]
pub async fn remove_local_game(
    id: String,
    delete_files: bool,
    app_handle: AppHandle,
) -> Result<(), String> {
    ensure_idle(&id)?;
    let game = fetch_local_game(id.clone())?;
    if delete_files {
        if game.archive.is_none() {
            return Err("Imported game folders cannot be deleted by Drop.".into());
        }
        let game = game.clone();
        tokio::task::spawn_blocking(move || local::delete_owned_files(&game))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
    }
    if let Some(archive) = &game.archive {
        // Only this job's cache file, never an arbitrary parent directory.
        let _ = fs::remove_file(&archive.archive_path);
    }
    let mut db = borrow_db_mut_checked();
    db.applications.local_games.remove(&id);
    db.applications.game_versions.remove(&id);
    db.applications.game_statuses.remove(&id);
    db.applications.installed_game_version.remove(&id);
    drop(db);
    app_emit!(&app_handle, "update_library", ());
    Ok(())
}

#[tauri::command]
pub async fn find_local_launchers(id: String) -> Result<Vec<String>, String> {
    let game = fetch_local_game(id)?;
    tokio::task::spawn_blocking(move || {
        walkdir::WalkDir::new(&game.root)
            .follow_links(false)
            .max_depth(12)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .filter(|entry| {
                let ext = entry
                    .path()
                    .extension()
                    .and_then(|v| v.to_str())
                    .unwrap_or("")
                    .to_ascii_lowercase();
                matches!(
                    ext.as_str(),
                    "exe" | "com" | "bat" | "cmd" | "ps1" | "sh" | "appimage"
                ) || executable(entry.path())
            })
            .take(200)
            .map(|entry| entry.path().to_string_lossy().into_owned())
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
}

fn executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        path.metadata()
            .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        false
    }
}

async fn queue_archive(game: LocalGame) -> Result<(), String> {
    let meta = local::metadata(&game);
    {
        let mut db = borrow_db_mut_checked();
        if db.applications.transient_statuses.contains_key(&meta) {
            return Err("This game already has an active download.".into());
        }
        db.applications.transient_statuses.insert(
            meta.clone(),
            database::ApplicationTransientStatus::Queued {
                version_id: meta.version.clone(),
            },
        );
    }
    let agent = Arc::new(Box::new(ArchiveDownloadAgent::new(
        game,
        DOWNLOAD_MANAGER.get_sender(),
    )) as Box<dyn Downloadable + Send + Sync>);
    if DOWNLOAD_MANAGER.queue_download(agent).await.is_err() {
        borrow_db_mut_checked()
            .applications
            .transient_statuses
            .remove(&meta);
        return Err("Download queue is unavailable".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn download_archive(
    name: String,
    url: String,
    install_dir: Option<usize>,
    install_path: Option<PathBuf>,
    app_handle: AppHandle,
) -> Result<String, String> {
    parse_external_url(&url).map_err(|e| e.to_string())?;
    let name = name.trim();
    if name.is_empty() || name.len() > 200 {
        return Err("Enter a game name (up to 200 characters).".into());
    }
    let base = borrow_db_checked()
        .applications
        .resolve_install_directory(database::install_locations::InstallLocationKind::Game, install_dir, install_path.as_deref())?;
    let id = format!("local-{}", uuid::Uuid::new_v4());
    let folder_name = name
        .chars()
        .take(60)
        .map(|ch| {
            if ch.is_alphanumeric() || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    let root = base.join(format!("{folder_name}-{}", &id[6..14]));
    fs::create_dir(&root).map_err(|e| e.to_string())?;
    let root = dunce::canonicalize(root).map_err(|e| e.to_string())?;
    fs::write(root.join(OWNER_FILE), &id).map_err(|e| e.to_string())?;
    let game = LocalGame {
        id: id.clone(),
        name: name.into(),
        root: root.clone(),
        launcher: None,
        arguments: String::new(),
        working_directory: root,
        platform: Platform::HOST,
        playtime_seconds: 0,
        archive: Some(ArchiveInstall {
            url,
            archive_path: DATA_ROOT_DIR.join("archive-downloads").join(&id),
            validator: None,
            downloaded: false,
            extracted: false,
            state: ArchiveState::Incomplete,
            error: None,
        }),
    };
    local::register(&mut borrow_db_mut_checked(), game.clone(), None);
    queue_archive(game).await?;
    app_emit!(&app_handle, "update_library", ());
    Ok(id)
}

#[tauri::command]
pub async fn resume_archive(id: String) -> Result<(), String> {
    ensure_idle(&id)?;
    let game = fetch_local_game(id)?;
    if !game.archive.as_ref().is_some_and(|a| !a.extracted) {
        return Err("This game has no unfinished archive installation.".into());
    }
    queue_archive(game).await
}
