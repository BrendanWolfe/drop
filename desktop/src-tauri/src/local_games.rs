use database::{
    Database, DownloadableMetadata, GameDownloadStatus, borrow_db_checked, borrow_db_mut_checked,
    db::DATA_ROOT_DIR,
    models::data::{
        ArchiveInstall, ArchiveState, InstalledGameType, InstallerLibraryInput, InstallerLocation,
        LocalGame,
    },
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
    scan_launchers(game.root).await
}

pub(crate) fn installer_package_meta<'a>(
    db: &'a Database,
    id: &str,
) -> Result<&'a DownloadableMetadata, String> {
    db.applications
        .installed_game_version
        .get(id)
        .filter(|meta| {
            db.applications
                .game_versions
                .get(&meta.version)
                .is_some_and(|version| version.locate_after_setup)
        })
        .ok_or_else(|| "This game does not have an installer package.".into())
}

fn installed_package(db: &Database, id: &str) -> Result<(DownloadableMetadata, PathBuf), String> {
    let meta = installer_package_meta(db, id)?;
    if !matches!(
        db.applications.game_statuses.get(id),
        Some(GameDownloadStatus::Installed {
            install_type: InstalledGameType::Installed,
            ..
        })
    ) {
        return Err("Run setup before locating the game.".into());
    }
    Ok((meta.clone(), db.applications.resolve_install_path(meta)?))
}

#[derive(serde::Serialize)]
pub struct InstallerCompanionFile {
    name: String,
    path: Option<String>,
    asset_id: Option<String>,
    queued: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedInstallerLibraryInput {
    #[serde(flatten)]
    input: InstallerLibraryInput,
    copy_path: Option<String>,
    status: LibraryInputStatus,
}

#[derive(serde::Serialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase", tag = "type")]
pub(crate) enum LibraryInputStatus {
    Ready,
    DownloadNeeded,
    WrongVersion,
    Busy,
    Missing,
}

pub(crate) fn installed_library_input(
    db: &Database,
    input: &InstallerLibraryInput,
) -> Result<PathBuf, LibraryInputStatus> {
    let apps = &db.applications;
    if apps.local_games.contains_key(&input.game_id) {
        return Err(LibraryInputStatus::DownloadNeeded);
    }
    if apps.transient_statuses.iter().any(|(meta, status)| {
        meta.id == input.game_id
            && !matches!(status, database::ApplicationTransientStatus::Running {})
    }) {
        return Err(LibraryInputStatus::Busy);
    }
    let source = apps
        .installed_game_version
        .get(&input.game_id)
        .ok_or(LibraryInputStatus::DownloadNeeded)?;
    if source.version != input.version_id {
        return Err(LibraryInputStatus::WrongVersion);
    }
    let complete = matches!(apps.game_statuses.get(&input.game_id), Some(GameDownloadStatus::Installed {
        version_id, install_type: InstalledGameType::Installed | InstalledGameType::SetupRequired, ..
    }) if version_id == &input.version_id);
    let folder = apps
        .resolve_install_path(source)
        .map_err(|_| LibraryInputStatus::Missing)?;
    let data = games::downloads::drop_data::DropData::read(&folder)
        .map_err(|_| LibraryInputStatus::Missing)?;
    if data.game_id != input.game_id
        || data.game_version != input.version_id
        || data.target_platform != source.target_platform
        || data.emulator
        || games::downloads::installer_files::has_inventory(&folder)
    {
        return Err(LibraryInputStatus::Missing);
    }
    if !complete && !data.companion_files.contains(&folder, input) {
        return Err(LibraryInputStatus::DownloadNeeded);
    }
    games::downloads::installer_files::resolve_library_input(&folder, input.kind, &input.path)
        .ok_or(LibraryInputStatus::Missing)
}

#[cfg(test)]
mod library_input_tests {
    use super::*;
    use database::{
        DownloadType,
        models::data::{InstallerLibraryInputKind, UserConfiguration},
    };
    use games::downloads::drop_data::DropData;

    #[test]
    fn only_the_pinned_complete_install_can_supply_a_rom() {
        let temp = tempfile::tempdir().unwrap();
        let mut db = Database::new(temp.path(), None, temp.path().join("cache"));
        let folder = temp.path().join("roms");
        fs::create_dir(&folder).unwrap();
        let rom = folder.join("game.z64");
        fs::write(&rom, b"fixture").unwrap();
        let meta = DownloadableMetadata::new(
            "source".into(),
            "v1".into(),
            Platform::HOST,
            DownloadType::Game,
        );
        let mut input = InstallerLibraryInput {
            name: None,
            game_id: meta.id.clone(),
            version_id: "v2".into(),
            game_name: "ROM".into(),
            version_name: "Second version".into(),
            kind: InstallerLibraryInputKind::File,
            path: "game.z64".into(),
        };
        assert_eq!(
            installed_library_input(&db, &input),
            Err(LibraryInputStatus::DownloadNeeded)
        );
        db.applications.record_installation(
            &meta,
            GameDownloadStatus::Installed {
                install_type: InstalledGameType::Installed,
                version_id: meta.version.clone(),
                install_dir: folder.to_string_lossy().into_owned(),
                update_available: false,
            },
        );
        assert_eq!(
            installed_library_input(&db, &input),
            Err(LibraryInputStatus::WrongVersion)
        );
        input.version_id = meta.version.clone();
        assert_eq!(
            installed_library_input(&db, &input),
            Err(LibraryInputStatus::Missing)
        );
        DropData::new(
            meta.id.clone(),
            meta.version.clone(),
            meta.target_platform,
            folder,
            UserConfiguration::default(),
            None,
        )
        .write();
        assert_eq!(
            installed_library_input(&db, &input),
            Ok(fs::canonicalize(rom).unwrap())
        );
        db.applications.transient_statuses.insert(
            meta.clone(),
            database::ApplicationTransientStatus::Updating {
                version_id: "v2".into(),
            },
        );
        assert_eq!(
            installed_library_input(&db, &input),
            Err(LibraryInputStatus::Busy)
        );
    }
}

#[tauri::command]
pub fn fetch_installer_library_inputs(
    id: String,
) -> Result<Vec<ResolvedInstallerLibraryInput>, String> {
    let db = borrow_db_checked();
    let meta = installer_package_meta(&db, &id)?;
    let version = &db.applications.game_versions[&meta.version];
    Ok(version
        .installer_library_inputs
        .iter()
        .filter(|input| input.game_id != id)
        .map(|input| {
            let (copy_path, status) = match installed_library_input(&db, input) {
                Ok(path) => (
                    Some(installer_picker_path(&path, &id, meta.target_platform)),
                    LibraryInputStatus::Ready,
                ),
                Err(status) => (None, status),
            };
            ResolvedInstallerLibraryInput {
                input: input.clone(),
                copy_path,
                status,
            }
        })
        .collect())
}

#[tauri::command]
pub fn fetch_installer_companion_files(id: String) -> Result<Vec<InstallerCompanionFile>, String> {
    let db = borrow_db_checked();
    let meta = installer_package_meta(&db, &id)?;
    if !matches!(
        db.applications.game_statuses.get(&id),
        Some(GameDownloadStatus::Installed {
            install_type: InstalledGameType::Installed | InstalledGameType::SetupRequired,
            ..
        })
    ) {
        return Ok(vec![]);
    }
    let root = db.applications.resolve_install_path(meta)?;
    let version = &db.applications.game_versions[&meta.version];
    let legacy_executable = version
        .setups
        .iter()
        .find(|setup| setup.platform == meta.target_platform)
        .and_then(|setup| ParsedCommand::parse(setup.command.clone()).ok())
        .and_then(|command| {
            Path::new(&command.command)
                .file_name()?
                .to_str()
                .map(str::to_owned)
        });
    let mut files: Vec<_> = games::downloads::installer_files::companion_files(
        &root,
        &id,
        version.installer_companion_files.as_deref(),
        legacy_executable.as_deref(),
    )
    .map_err(|error| error.to_string())?
    .into_iter()
    .map(|(name, path)| InstallerCompanionFile {
        name,
        path: Some(installer_picker_path(&path, &id, meta.target_platform)),
        asset_id: None,
        queued: false,
    })
    .collect();
    for asset in &version.installer_assets {
        let queued = db.applications.transient_statuses.keys().any(|item| {
            item.id == id
                && item.version == asset.id
                && item.download_type == database::DownloadType::InstallerAsset
        });
        let path = if queued {
            None
        } else {
            games::downloads::installer_files::ready_asset(&root, &id, asset)
                .map(|path| installer_picker_path(&path, &id, meta.target_platform))
        };
        files.push(InstallerCompanionFile {
            name: asset.name.clone(),
            path,
            asset_id: Some(asset.id.clone()),
            queued,
        });
    }
    Ok(files)
}

fn installer_picker_path(path: &Path, _id: &str, _platform: Platform) -> String {
    #[cfg(target_os = "linux")]
    if _platform == Platform::Windows {
        let drives = DATA_ROOT_DIR.join("pfx").join(_id).join("dosdevices");
        if let Some(path) = wine_picker_path(path, &drives) {
            return path;
        }
    }
    dunce::simplified(path).to_string_lossy().into_owned()
}

#[cfg(target_os = "linux")]
fn wine_picker_path(path: &Path, drives: &Path) -> Option<String> {
    let (letter, relative) = match fs::read_dir(drives) {
        Ok(entries) => entries
            .flatten()
            .filter_map(|entry| {
                let name = entry.file_name();
                let bytes = name.as_encoded_bytes();
                if bytes.len() != 2 || !bytes[0].is_ascii_alphabetic() || bytes[1] != b':' {
                    return None;
                }
                let root = fs::canonicalize(entry.path()).ok()?;
                let relative = path.strip_prefix(&root).ok()?;
                Some((
                    root.components().count(),
                    bytes[0].to_ascii_uppercase() as char,
                    relative,
                ))
            })
            .max_by_key(|(depth, letter, _)| (*depth, std::cmp::Reverse(*letter)))
            .map(|(_, letter, path)| (letter, path))?,
        // Before the first launch Proton has not created its default Z: drive yet.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            ('Z', path.strip_prefix("/").ok()?)
        }
        Err(_) => return None,
    };
    Some(format!(
        "{}:\\{}",
        letter,
        relative.to_str()?.replace('/', "\\")
    ))
}

#[cfg(all(test, target_os = "linux"))]
mod installer_path_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn uses_the_default_drive_before_setup_and_the_most_specific_existing_drive_after() {
        let temp = tempfile::tempdir().unwrap();
        let drives = temp.path().join("dosdevices");
        let files = temp.path().join("downloads");
        let package_file = files.join("game data.bin");
        assert!(
            wine_picker_path(&package_file, &drives)
                .unwrap()
                .starts_with("Z:\\")
        );
        fs::create_dir(&drives).unwrap();
        fs::create_dir(&files).unwrap();
        symlink("/", drives.join("z:")).unwrap();
        symlink(&files, drives.join("d:")).unwrap();
        assert_eq!(
            wine_picker_path(&package_file, &drives).as_deref(),
            Some("D:\\game data.bin")
        );
    }
}

#[tauri::command]
pub fn fetch_installer_location(id: String) -> Option<InstallerLocation> {
    let db = borrow_db_checked();
    let (meta, _) = installed_package(&db, &id).ok()?;
    process::resolution::located_installer_game(&db, &meta)
}

#[tauri::command]
pub async fn find_installer_launchers(id: String, root: PathBuf) -> Result<Vec<String>, String> {
    let (meta, package, expected, emulated) = {
        let db = borrow_db_checked();
        let (meta, package) = installed_package(&db, &id)?;
        let expected = db.applications.game_versions[&meta.version]
            .installer_launch_path
            .clone();
        let emulated = db.applications.game_versions[&meta.version]
            .setups
            .iter()
            .any(|setup| setup.platform == meta.target_platform && setup.emulator.is_some());
        (meta, package, expected, emulated)
    };
    let root = process::resolution::installer_game_folder(&root, &package)?;
    let hint = expected.and_then(|path| {
        let relative = PathBuf::from(path.replace('\\', "/"));
        if !database::install_records::valid_relative(&relative) {
            return None;
        }
        let location = process::resolution::installer_location(
            &root,
            &root.join(relative),
            &package,
            meta.target_platform,
            emulated,
        )
        .ok()?;
        if !emulated {
            ParsedCommand::for_file(&location.launcher, "", meta.target_platform).ok()?;
        }
        Some(location.launcher.to_string_lossy().into_owned())
    });
    let mut candidates = scan_launchers(root).await?;
    if let Some(hint) = hint {
        candidates.retain(|path| path != &hint);
        candidates.insert(0, hint);
    }
    Ok(candidates)
}

#[tauri::command]
pub fn save_installer_location(
    id: String,
    root: PathBuf,
    launcher: PathBuf,
) -> Result<InstallerLocation, String> {
    let mut db = borrow_db_mut_checked();
    let (meta, package) = installed_package(&db, &id)?;
    if db.applications.transient_statuses.contains_key(&meta) {
        return Err("Wait for this game's current operation to finish.".into());
    }
    let emulated = db.applications.game_versions[&meta.version]
        .setups
        .iter()
        .any(|setup| setup.platform == meta.target_platform && setup.emulator.is_some());
    let location = process::resolution::installer_location(
        &root,
        &launcher,
        &package,
        meta.target_platform,
        emulated,
    )?;
    if !emulated {
        ParsedCommand::for_file(&location.launcher, "", meta.target_platform)
            .map_err(|error| error.to_string())?;
    }
    db.applications
        .installer_locations
        .insert(id, location.clone());
    Ok(location)
}

async fn scan_launchers(root: PathBuf) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || {
        walkdir::WalkDir::new(root)
            .follow_links(false)
            .max_depth(12)
            .sort_by_file_name()
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
    let base = borrow_db_checked().applications.resolve_install_directory(
        database::install_locations::InstallLocationKind::Game,
        install_dir,
        install_path.as_deref(),
    )?;
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
