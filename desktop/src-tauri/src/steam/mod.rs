mod discovery;
use database::{borrow_db_checked, borrow_db_mut_checked, platform::Platform};
use serde::Serialize;
use sha1::{Digest, Sha1};
use std::sync::Mutex;
use steam_shortcuts::{ExportedLaunch, Registration, Shortcuts, read_source, shortcut_path};
use tauri::{AppHandle, Emitter};

static OPERATIONS: Mutex<()> = Mutex::new(());
pub(super) fn digest(value: &str) -> String {
    hex::encode(Sha1::digest(value.as_bytes()))
}
fn supported() -> Result<(), String> {
    if cfg!(windows) {
        Ok(())
    } else {
        Err("Steam integration is available on Windows only.".into())
    }
}
fn identity(db: &database::Database, id: &str) -> (String, String) {
    let source = if db.applications.local_games.contains_key(id) || id.starts_with("local-") {
        "device-local".into()
    } else {
        db.base_url.trim_end_matches('/').to_string()
    };
    (
        digest(&serde_json::to_string(&(&source, id)).unwrap()),
        source,
    )
}
fn current(
    db: &database::Database,
    id: &str,
    launch_id: Option<&str>,
) -> Result<(ExportedLaunch, String), String> {
    let launch = process::resolution::resolve_steam(db, id, launch_id, Platform::HOST)?;
    Ok((
        ExportedLaunch {
            executable: launch.resolved.command.command,
            arguments: launch.resolved.command.args,
            working_directory: launch
                .resolved
                .working_directory
                .to_string_lossy()
                .into_owned(),
            launch_id: launch.launch_id,
            configuration: launch.configuration,
        },
        launch.launch_name,
    ))
}
fn load(reg: &Registration) -> Result<Shortcuts, String> {
    let path = shortcut_path(&reg.steam_root, &reg.profile_id).map_err(|e| e.to_string())?;
    Shortcuts::parse(read_source(&path).map_err(|e| e.to_string())?.as_deref())
        .map_err(|e| e.to_string())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionView {
    id: String,
    name: String,
    launch: Option<ExportedLaunch>,
    reason: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RegistrationView {
    key: String,
    profile: String,
    name: String,
    launch_id: String,
    game_id64: String,
    prefer_steam: bool,
    state: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    supported: bool,
    profiles: Vec<discovery::Profile>,
    selected_profile: Option<String>,
    running: bool,
    options: Vec<OptionView>,
    registration: Option<RegistrationView>,
}
fn context(id: &str) -> Result<Context, String> {
    if !cfg!(windows) {
        return Ok(Context {
            supported: false,
            profiles: vec![],
            selected_profile: None,
            running: false,
            options: vec![],
            registration: None,
        });
    }
    let discovery = discovery::discover().map_err(|e| e.to_string())?;
    let db = borrow_db_checked();
    let (key, _) = identity(&db, id);
    let reg = db.steam_registrations.get(&key);
    let options = db
        .applications
        .installed_game_version
        .get(id)
        .and_then(|meta| db.applications.game_versions.get(&meta.version))
        .map(|version| {
            version
                .launches
                .iter()
                .filter(|launch| launch.platform == Platform::Windows)
                .map(|launch| match current(&db, id, Some(&launch.launch_id)) {
                    Ok((resolved, _)) => OptionView {
                        id: launch.launch_id.clone(),
                        name: launch.name.clone(),
                        launch: Some(resolved),
                        reason: None,
                    },
                    Err(reason) => OptionView {
                        id: launch.launch_id.clone(),
                        name: launch.name.clone(),
                        launch: None,
                        reason: Some(reason),
                    },
                })
                .collect()
        })
        .unwrap_or_default();
    let registration = reg.map(|reg| {
        let state = match load(reg) {
            Err(error) => error,
            Ok(shortcuts) => match shortcuts.verify(reg) {
                Ok(false) => "Missing shortcut — add it again explicitly to restore it.".into(),
                Err(error) => error.to_string(),
                Ok(true) => match current(&db, id, Some(&reg.exported.launch_id)) {
                    Err(error) => error,
                    Ok((launch, _))
                        if launch != reg.exported
                            || !shortcuts.matches(reg, &launch).unwrap_or(false) =>
                    {
                        "Needs update".into()
                    }
                    Ok(_) => "Ready".into(),
                },
            },
        };
        let profile = discovery
            .profiles
            .iter()
            .find(|p| p.account_id == reg.profile_id && p.steam_root == reg.steam_root)
            .map(|p| p.id.clone())
            .unwrap_or_default();
        RegistrationView {
            key: reg.key.clone(),
            profile,
            name: reg.exported_name.clone(),
            launch_id: reg.exported.launch_id.clone(),
            game_id64: reg.game_id64.clone(),
            prefer_steam: reg.prefer_steam,
            state,
        }
    });
    Ok(Context {
        supported: true,
        profiles: discovery.profiles,
        selected_profile: db.steam_profile.clone(),
        running: discovery.running,
        options,
        registration,
    })
}
#[tauri::command]
pub async fn steam_context(id: String) -> Result<Context, String> {
    tauri::async_runtime::spawn_blocking(move || context(&id))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn steam_save(
    id: String,
    profile_id: String,
    launch_id: String,
    name: String,
    app_handle: AppHandle,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        supported()?; let _lock = OPERATIONS.lock().map_err(|e|e.to_string())?;
        discovery::require_closed().map_err(|e|e.to_string())?;
        let discovery = discovery::discover().map_err(|e|e.to_string())?;
        let profile = discovery.profiles.iter().find(|p|p.id == profile_id).ok_or("Steam profile no longer exists. Refresh and choose another profile.")?;
        let name = name.trim(); if name.is_empty() || name.len()>200 || name.contains('\0') { return Err("Enter a shortcut name of up to 200 characters.".into()); }
        let mut db = borrow_db_mut_checked(); let (key,source) = identity(&db,&id);
        let (exported,_) = current(&db,&id,Some(&launch_id))?;
        let previous = db.steam_registrations.get(&key).cloned();
        if previous.as_ref().is_some_and(|reg|reg.steam_root != profile.steam_root || reg.profile_id != profile.account_id) {
            return Err("Remove the existing shortcut before changing its Steam installation or profile. Drop will not silently copy it to another account.".into());
        }
        let path = shortcut_path(&profile.steam_root,&profile.account_id).map_err(|e|e.to_string())?;
        let mut registration = previous.clone().unwrap_or_else(|| {
            let app_id = steam_shortcuts::initial_app_id(&exported.executable,name);
            Registration { key:key.clone(),game_id:id.clone(),source,steam_root:profile.steam_root.clone(),profile_id:profile.account_id.clone(),app_id,game_id64:steam_shortcuts::game_id(app_id),ownership_token:uuid::Uuid::new_v4().to_string(),exported:exported.clone(),exported_name:name.into(),prefer_steam:false }
        });
        registration.exported = exported;
        steam_shortcuts::mutate(&path,discovery::require_closed,|shortcuts| {
            if previous.is_none() {
                let reserved=db.steam_registrations.values().filter(|r|r.steam_root==registration.steam_root && r.profile_id==registration.profile_id).map(|r|r.app_id).collect::<Vec<_>>();
                registration.app_id=shortcuts.allocate_app_id(registration.app_id,&reserved)?;
            }
            registration.game_id64 = steam_shortcuts::game_id(registration.app_id);
            shortcuts.upsert(&registration,name,previous.as_ref().map(|r|r.exported_name.as_str()))
        }).map_err(|e|e.to_string())?;
        registration.exported_name = name.into();
        db.steam_registrations.insert(key,registration); db.steam_profile=Some(profile.id.clone()); drop(db);
        let _ = app_handle.emit("steam_changed",()); Ok(())
    }).await.map_err(|e|e.to_string())?
}

fn remove_key(key: &str) -> Result<(), String> {
    supported()?;
    let _lock = OPERATIONS.lock().map_err(|e| e.to_string())?;
    let mut db = borrow_db_mut_checked();
    let reg = db
        .steam_registrations
        .get(key)
        .ok_or("Steam registration not found")?
        .clone();
    let path = shortcut_path(&reg.steam_root, &reg.profile_id).map_err(|e| e.to_string())?;
    if read_source(&path).map_err(|e| e.to_string())?.is_none() {
        // Removing an already-missing shortcut must not recreate an uninstalled
        // Steam installation or a deleted profile directory.
        discovery::require_closed().map_err(|e| e.to_string())?;
    } else {
        steam_shortcuts::mutate(&path, discovery::require_closed, |shortcuts| {
            shortcuts.remove(&reg)
        })
        .map_err(|e| e.to_string())?;
    }
    db.steam_registrations.remove(key);
    Ok(())
}
#[tauri::command]
pub async fn steam_remove(key: String, app_handle: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        remove_key(&key)?;
        let _ = app_handle.emit("steam_changed", ());
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub fn steam_prefer(id: String, prefer: bool, app_handle: AppHandle) -> Result<(), String> {
    supported()?;
    let mut db = borrow_db_mut_checked();
    let (key, _) = identity(&db, &id);
    db.steam_registrations
        .get_mut(&key)
        .ok_or("Add this game to Steam first.")?
        .prefer_steam = prefer;
    drop(db);
    let _ = app_handle.emit("steam_changed", ());
    Ok(())
}
#[tauri::command]
pub async fn steam_play(id: String) -> Result<(), String> {
    let _save_guard = crate::cloud_sync::launch_gate().await;
    tauri::async_runtime::spawn_blocking(move || {
        supported()?; let _lock=OPERATIONS.lock().map_err(|e|e.to_string())?;
        let db=borrow_db_checked(); let (key,_)=identity(&db,&id);
        let reg=db.steam_registrations.get(&key).ok_or("Add this game to Steam first.")?;
        let (current,_)=current(&db,&id,Some(&reg.exported.launch_id))?;
        let shortcuts=load(reg)?;
        if !shortcuts.verify(reg).map_err(|e|e.to_string())? { return Err("This shortcut was removed in Steam. Use Add again to recreate it explicitly.".into()); }
        if current!=reg.exported || !shortcuts.matches(reg,&current).map_err(|e|e.to_string())? {
            return Err("The Steam shortcut needs updating. Close Steam completely and choose Update Steam shortcut. You can also explicitly play directly with Drop.".into());
        }
        let discovery=discovery::discover().map_err(|e|e.to_string())?;
        if discovery.running && !discovery.active_root.as_ref().is_some_and(|root|root.to_string_lossy().eq_ignore_ascii_case(&reg.steam_root.to_string_lossy())) {
            return Err("The running Steam installation could not be matched to this shortcut. Exit Steam, then retry with the selected installation.".into());
        }
        if discovery.active_account.as_deref()!=Some(&reg.profile_id) { return Err("Switch Steam to the account selected for this shortcut, then retry. Drop will not switch accounts automatically.".into()); }
        if reg.game_id64!=steam_shortcuts::game_id(reg.app_id) { return Err("Invalid Steam launch identifier. Repair the shortcut first.".into()); }
         let executable=reg.steam_root.join("steam.exe");
         if !executable.is_file() { return Err("The registered Steam installation is missing. Reinstall Steam or remove this registration before choosing another installation.".into()); }
         std::process::Command::new(executable).arg(format!("steam://rungameid/{}",reg.game_id64)).spawn().map_err(|e|e.to_string())?;
        Ok(())
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
pub async fn steam_launch_preferred(id: String) -> Result<bool, String> {
    if !cfg!(windows) {
        return Ok(false);
    }
    let preferred = {
        let db = borrow_db_checked();
        let (key, _) = identity(&db, &id);
        db.steam_registrations
            .get(&key)
            .is_some_and(|reg| reg.prefer_steam)
    };
    if !preferred {
        return Ok(false);
    }
    steam_play(id).await?;
    Ok(true)
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cleanup {
    key: String,
    name: String,
    profile: String,
}
#[tauri::command]
pub fn steam_cleanup() -> Vec<Cleanup> {
    if !cfg!(windows) {
        return vec![];
    }
    let db = borrow_db_checked();
    db.steam_registrations
        .values()
        .filter(|reg| {
            let (key, _) = identity(&db, &reg.game_id);
            key != reg.key
                || !db
                    .applications
                    .installed_game_version
                    .contains_key(&reg.game_id)
        })
        .map(|reg| Cleanup {
            key: reg.key.clone(),
            name: reg.exported_name.clone(),
            profile: reg.profile_id.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn identity_is_source_scoped_and_local_cleanup_survives_uninstall() {
        let mut db = database::Database::new("games", None, "cache".into());
        db.base_url = "https://one.test/drop".into();
        let first = identity(&db, "game");
        db.base_url = "https://two.test/drop".into();
        assert_ne!(identity(&db, "game").0, first.0);
        db.base_url = "https://one.test/DROP".into();
        assert_ne!(identity(&db, "game").0, first.0);
        let local = identity(&db, "local-test");
        db.base_url = "https://another.test".into();
        assert_eq!(local, identity(&db, "local-test"));
    }
    #[tokio::test]
    #[cfg(not(windows))]
    async fn unsupported_hosts_never_discover_or_launch_steam() {
        assert!(!steam_context("game".into()).await.unwrap().supported);
        assert!(!steam_launch_preferred("game".into()).await.unwrap());
        assert!(steam_play("game".into()).await.is_err());
    }
}
