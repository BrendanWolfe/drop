use serde::Serialize;
use std::{
    collections::HashMap,
    fs,
    io::{Error, Result},
    path::{Path, PathBuf},
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub steam_root: PathBuf,
}
pub struct Discovery {
    pub profiles: Vec<Profile>,
    pub running: bool,
    pub active_account: Option<String>,
    pub active_root: Option<PathBuf>,
}

#[cfg(windows)]
fn registry_string(key: &str, name: &str, machine: bool) -> Option<String> {
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
    };
    RegKey::predef(if machine {
        HKEY_LOCAL_MACHINE
    } else {
        HKEY_CURRENT_USER
    })
    .open_subkey(key)
    .ok()?
    .get_value(name)
    .ok()
}
#[cfg(windows)]
fn roots() -> Vec<PathBuf> {
    let mut paths = vec![];
    for (key, name, machine) in [
        (r"Software\Valve\Steam", "SteamPath", false),
        (r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath", true),
        (r"SOFTWARE\Valve\Steam", "InstallPath", true),
    ] {
        if let Some(path) = registry_string(key, name, machine) {
            paths.push(PathBuf::from(path));
        }
    }
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(path) = std::env::var_os(variable) {
            paths.push(PathBuf::from(path).join("Steam"));
        }
    }
    paths
        .into_iter()
        .filter(|path| path.join("steam.exe").is_file())
        .filter_map(|path| dunce::canonicalize(path).ok())
        .fold(vec![], |mut paths, path| {
            if !paths.iter().any(|p: &PathBuf| {
                p.to_string_lossy()
                    .eq_ignore_ascii_case(&path.to_string_lossy())
            }) {
                paths.push(path);
            }
            paths
        })
}
#[cfg(not(windows))]
fn roots() -> Vec<PathBuf> {
    vec![]
}

pub fn running() -> bool {
    let system = sysinfo::System::new_with_specifics(
        sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::nothing()),
    );
    system.processes().values().any(|p| {
        p.name().to_string_lossy().eq_ignore_ascii_case("steam.exe")
            || p.name()
                .to_string_lossy()
                .eq_ignore_ascii_case("steamwebhelper.exe")
    })
}
pub fn require_closed() -> Result<()> {
    if running() {
        return Err(Error::other(
            "Close Steam completely before changing shortcuts. Drop will preserve your existing shortcuts. Exit Steam from its tray menu, then retry.",
        ));
    }
    Ok(())
}

fn login_users(root: &Path) -> Result<HashMap<String, (String, bool)>> {
    let text = match fs::read_to_string(root.join("config/loginusers.vdf")) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
        Err(e) => return Err(e),
    };
    let parsed = keyvalues_parser::parse(&text)
        .map_err(|_| Error::other("Unable to parse Steam account information"))?;
    let users = parsed
        .value
        .get_obj()
        .ok_or_else(|| Error::other("Invalid Steam account information"))?;
    let mut result = HashMap::new();
    for (id, values) in users.iter() {
        let Some(user) = values.first().and_then(|v| v.get_obj()) else {
            continue;
        };
        let field = |name: &str| {
            user.get(name)
                .and_then(|v| v.first())
                .and_then(|v| v.get_str())
        };
        let Some(id) = id.parse::<u64>().ok().filter(|id| *id >= 76561197960265728) else {
            continue;
        };
        let account_id = (id & 0xffff_ffff).to_string();
        result.insert(
            account_id,
            (
                field("PersonaName")
                    .or_else(|| field("AccountName"))
                    .unwrap_or("Steam user")
                    .to_string(),
                field("MostRecent") == Some("1"),
            ),
        );
    }
    Ok(result)
}
pub fn discover() -> Result<Discovery> {
    let running = running();
    let system =
        sysinfo::System::new_with_specifics(sysinfo::RefreshKind::nothing().with_processes(
            sysinfo::ProcessRefreshKind::nothing().with_exe(sysinfo::UpdateKind::Always),
        ));
    let active_root = system
        .processes()
        .values()
        .filter(|p| p.name().to_string_lossy().eq_ignore_ascii_case("steam.exe"))
        .find_map(|p| {
            p.exe()
                .and_then(|p| p.parent())
                .and_then(|p| dunce::canonicalize(p).ok())
        });
    let mut profiles = vec![];
    let mut recent = vec![];
    for root in roots() {
        let users = login_users(&root)?;
        let userdata = root.join("userdata");
        if !userdata.exists() {
            continue;
        }
        for entry in fs::read_dir(userdata)? {
            let entry = entry?;
            let account_id = entry.file_name().to_string_lossy().into_owned();
            if account_id == "0"
                || !entry.file_type()?.is_dir()
                || !account_id.bytes().all(|b| b.is_ascii_digit())
            {
                continue;
            }
            let (name, last) = users
                .get(&account_id)
                .cloned()
                .unwrap_or((format!("Steam account {account_id}"), false));
            let id = format!(
                "{}:{account_id}",
                crate::steam::digest(&root.to_string_lossy())
            );
            if last {
                recent.push(account_id.clone());
            }
            profiles.push(Profile {
                id,
                account_id,
                name,
                steam_root: root.clone(),
            });
        }
    }
    #[cfg(windows)]
    let active = {
        use winreg::{RegKey, enums::HKEY_CURRENT_USER};
        RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey(r"Software\Valve\Steam\ActiveProcess")
            .ok()
            .and_then(|key| key.get_value::<u32, _>("ActiveUser").ok())
            .filter(|id| *id != 0)
            .map(|id| id.to_string())
    };
    #[cfg(not(windows))]
    let active: Option<String> = None;
    // When stopped, Steam's most recent login is the intended account. Never
    // infer an account while running if Steam reports no active user.
    let active_account = if running {
        active
    } else if recent.len() == 1 {
        recent.pop()
    } else {
        None
    };
    profiles.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(Discovery {
        profiles,
        running,
        active_account,
        active_root,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_profile_names_and_most_recent_without_shortcuts() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("config")).unwrap();
        fs::write(
            root.path().join("config/loginusers.vdf"),
            r#""users" { "76561197960265770" { "PersonaName" "Player One" "MostRecent" "1" } }"#,
        )
        .unwrap();
        assert_eq!(
            login_users(root.path()).unwrap()["42"],
            ("Player One".into(), true)
        );
    }
}
