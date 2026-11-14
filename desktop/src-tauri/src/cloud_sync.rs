//! Conservative cloud saves: stable background recovery snapshots, explicit
//! restores. Launcher exit is never treated as proof a game has stopped.
use anyhow::{Context, Result, anyhow, bail};
use cloud_saves::snapshot::{self, Contents, Kind, Manifest, Root};
use database::{
    ApplicationTransientStatus, DatabaseAuth, borrow_db_checked, borrow_db_mut_checked,
    db::DATA_ROOT_DIR, platform::Platform,
};
use remote::{auth::authorization_header_for, utils::DROP_CLIENT_ASYNC};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter};
use tokio::sync::{Mutex, RwLock};
use tokio_util::io::ReaderStream;

// Serializes capture, restore, settings changes, and Drop-managed launch gates.
static OPERATIONS: Mutex<()> = Mutex::const_new(());
static RESTORE_LAUNCH: RwLock<()> = RwLock::const_new(());
static REGISTERED: Mutex<Option<(String, String)>> = Mutex::const_new(None);

#[derive(Clone)]
struct Session {
    server: String,
    auth: DatabaseAuth,
}
impl Session {
    fn current() -> Result<Self> {
        let db = borrow_db_checked();
        if !db.settings.cloud_saves_enabled {
            bail!("Enable cloud saves in Settings → General first.");
        }
        Ok(Self {
            server: db.base_url.clone(),
            auth: db.auth.clone().context("Sign in to use cloud saves.")?,
        })
    }
    fn check(&self) -> Result<()> {
        let db = borrow_db_checked();
        if !db.settings.cloud_saves_enabled
            || db.base_url != self.server
            || db.auth.as_ref().map(|a| &a.client_id) != Some(&self.auth.client_id)
        {
            bail!("Cloud saves stopped because the account, server or preference changed.");
        }
        Ok(())
    }
    fn url(&self, id: &str, namespace: &str, snapshot: Option<&str>) -> Result<url::Url> {
        self.check()?;
        if borrow_db_checked().settings.force_offline {
            bail!("Cloud saves are pending while offline.");
        }
        let mut url = url::Url::parse(&self.server)?.join("/api/v1/client/cloud-saves/")?;
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| anyhow!("Invalid server URL."))?;
            path.pop_if_empty().push(id);
            if let Some(snapshot) = snapshot {
                path.push(snapshot);
            }
        }
        url.query_pairs_mut().append_pair("namespace", namespace);
        Ok(url)
    }
    fn directory(&self, id: &str, namespace: &str) -> Result<PathBuf> {
        if id.is_empty() || !id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-') {
            bail!("Invalid game ID.");
        }
        let owner = format!(
            "{:x}",
            Sha256::digest(format!("{}\n{}", self.server, self.auth.client_id).as_bytes())
        );
        let path = DATA_ROOT_DIR
            .join("cloud-saves")
            .join(owner)
            .join(id)
            .join(namespace);
        fs::create_dir_all(&path)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        }
        Ok(path)
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Location {
    id: String,
    platform: String,
    path: String,
    r#type: Kind,
    #[serde(default)]
    exclude: Vec<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotSummary {
    id: String,
    revision: u32,
    definition_revision: u32,
    client_id: String,
    kind: String,
    checksum: String,
    content_hash: String,
    bytes: u64,
    created_at: String,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Limits {
    archive_bytes: u64,
    history: u32,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteState {
    definition_revision: u32,
    locations: Vec<Location>,
    revision: u32,
    snapshots: Vec<SnapshotSummary>,
    limits: Limits,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct Pending {
    id: String,
    expected_revision: u32,
    kind: String,
    manifest: Manifest,
    checksum: String,
}

#[derive(Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct LocalState {
    #[serde(default)]
    revision: Option<u32>,
    #[serde(default)]
    definition_revision: Option<u32>,
    #[serde(default)]
    baseline: Option<Contents>,
    #[serde(default)]
    remote: Option<RemoteState>,
    #[serde(default)]
    observed_hash: Option<String>,
    #[serde(default)]
    observed_at: u64,
    #[serde(default)]
    pending: Option<Pending>,
    #[serde(default)]
    message: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreJournal {
    archive: PathBuf,
    recovery: Option<PathBuf>,
    roots: Vec<Root>,
    game_id: String,
    namespace: String,
    definition_revision: u32,
    applied_definition_revision: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStatus {
    enabled: bool,
    phase: String,
    message: String,
    namespace: String,
    revision: u32,
    definition_revision: u32,
    snapshots: Vec<SnapshotSummary>,
    locations: Vec<String>,
    recovery_directory: Option<String>,
    interrupted_restore: bool,
    recoveries: Vec<String>,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn load(path: &Path) -> Result<LocalState> {
    match fs::read(path.join("state.json")) {
        Ok(data) => Ok(serde_json::from_slice(&data)
            .context("Cloud save state is damaged; automatic sync paused.")?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LocalState::default()),
        Err(error) => Err(error.into()),
    }
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path.parent().context("Invalid state path.")?;
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|e| e.error)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

fn save(path: &Path, state: &LocalState) -> Result<()> {
    write_json(&path.join("state.json"), state)
}

fn installed(id: &str) -> Result<(String, PathBuf)> {
    let db = borrow_db_checked();
    if db.applications.local_games.contains_key(id) {
        bail!("Device-local games do not support server cloud saves.");
    }
    let meta = db
        .applications
        .installed_game_version
        .get(id)
        .context("Install the game before syncing its saves.")?;
    Ok((
        format!("{:?}", meta.target_platform),
        db.applications
            .resolve_install_path(meta)
            .map_err(|e| anyhow!(e))?,
    ))
}

fn running(id: &str) -> bool {
    let db = borrow_db_checked();
    db.applications
        .transient_statuses
        .iter()
        .any(|(meta, status)| {
            meta.id == id && matches!(status, ApplicationTransientStatus::Running {})
        })
}

fn resolve_roots(id: &str, namespace: &str, locations: &[Location]) -> Result<Vec<Root>> {
    let (_, install) = installed(id)?;
    if !install.is_dir() {
        bail!("The game installation is unavailable. No saves were changed.");
    }
    let home = dirs::home_dir().context("User home directory unavailable.")?;
    let mut roots = Vec::new();
    for location in locations {
        if location.platform != namespace {
            bail!("Invalid save location platform.");
        }
        let (placeholder, relative) = location
            .path
            .split_once('/')
            .context("Invalid save path.")?;
        if relative.is_empty()
            || relative.split('/').any(|p| {
                p.is_empty()
                    || p == "."
                    || p == ".."
                    || p.chars().any(|c| c.is_control() || "\\:<>*?".contains(c))
            })
        {
            bail!("Unsafe save definition.");
        }
        let base = match placeholder {
            "<base>" => install.clone(),
            "<home>" => home.clone(),
            "<xdgData>" if namespace == "Linux" && Platform::HOST == Platform::Linux => {
                dirs::data_dir().context("Data directory unavailable.")?
            }
            "<xdgConfig>" if namespace == "Linux" && Platform::HOST == Platform::Linux => {
                dirs::config_dir().context("Config directory unavailable.")?
            }
            token if token.starts_with("<win") && namespace == "Windows" => {
                if Platform::HOST == Platform::Windows {
                    match token {
                        "<winAppData>" => dirs::config_dir(),
                        "<winLocalAppData>" => dirs::data_local_dir(),
                        "<winLocalAppDataLow>" => Some(home.join("AppData/LocalLow")),
                        "<winDocuments>" => dirs::document_dir(),
                        _ => None,
                    }
                    .context("Unsupported Windows save root.")?
                } else if Platform::HOST == Platform::Linux {
                    // Drop's UMU launchers use this exact per-game WINEPREFIX.
                    // Never guess an arbitrary Steam prefix or a prefix username.
                    let users = DATA_ROOT_DIR.join("pfx").join(id).join("drive_c/users");
                    let candidates = [users.join("steamuser"), users.join(whoami::username())];
                    let user = candidates.iter().find(|p| p.is_dir()).context("Drop Proton prefix not found. External Steam prefixes are not automatically inferred.")?;
                    user.join(match token {
                        "<winAppData>" => "AppData/Roaming",
                        "<winLocalAppData>" => "AppData/Local",
                        "<winLocalAppDataLow>" => "AppData/LocalLow",
                        "<winDocuments>" => "Documents",
                        _ => bail!("Unsupported Windows save root."),
                    })
                } else {
                    bail!("Unsupported save platform.");
                }
            }
            _ => bail!("Unsupported save root for this platform."),
        };
        let path = base.join(relative);
        // Keep our own state/recovery folders out of a game's save definition.
        if path.starts_with(DATA_ROOT_DIR.join("cloud-saves"))
            || DATA_ROOT_DIR.join("cloud-saves").starts_with(&path)
        {
            bail!("Save locations cannot include Drop's cloud-save storage.");
        }
        roots.push(Root {
            id: location.id.clone(),
            path,
            kind: location.r#type.clone(),
            exclude: location.exclude.clone(),
        });
    }
    roots.sort_by(|a, b| a.id.cmp(&b.id));
    snapshot::validate_roots(&roots).map_err(|e| anyhow!(e.to_string()))?;
    Ok(roots)
}

async fn ensure_capability(session: &Session) -> Result<()> {
    let mut registered = REGISTERED.lock().await;
    let identity = (session.server.clone(), session.auth.client_id.clone());
    if registered.as_ref() != Some(&identity) {
        let mut url = session.url("capability", "Linux", None)?;
        url.set_path("/api/v1/client/capability");
        url.set_query(None);
        let response = DROP_CLIENT_ASYNC
            .post(url)
            .header(
                "Authorization",
                authorization_header_for(Some(&session.auth))?,
            )
            .json(&serde_json::json!({ "capability": "cloudSaves", "configuration": {} }))
            .timeout(Duration::from_secs(15))
            .send()
            .await?;
        let _ = check_response(response).await?;
        *registered = Some(identity);
    }
    drop(registered);
    Ok(())
}

async fn fetch_enabled_games(session: &Session) -> Result<Vec<String>> {
    ensure_capability(session).await?;
    let mut url = session.url("list", "Linux", None)?;
    url.set_path("/api/v1/client/cloud-saves");
    url.set_query(None);
    let response = DROP_CLIENT_ASYNC
        .get(url)
        .header(
            "Authorization",
            authorization_header_for(Some(&session.auth))?,
        )
        .timeout(Duration::from_secs(15))
        .send()
        .await?;
    #[derive(Deserialize)]
    struct EnabledGame {
        id: String,
    }
    Ok(check_response(response)
        .await?
        .json::<Vec<EnabledGame>>()
        .await?
        .into_iter()
        .map(|game| game.id)
        .collect())
}

async fn fetch_remote(session: &Session, id: &str, namespace: &str) -> Result<RemoteState> {
    ensure_capability(session).await?;
    let response = DROP_CLIENT_ASYNC
        .get(session.url(id, namespace, None)?)
        .header(
            "Authorization",
            authorization_header_for(Some(&session.auth))?,
        )
        .timeout(Duration::from_secs(15))
        .send()
        .await?;
    check_response(response)
        .await?
        .json()
        .await
        .map_err(Into::into)
}

async fn check_response(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status();
    let value = response
        .json::<serde_json::Value>()
        .await
        .unwrap_or_default();
    bail!(
        "{}",
        value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or(if status.as_u16() == 409 {
                "Cloud save conflict. Check cloud saves before continuing."
            } else {
                "Cloud saves unavailable. Check the connection and the game's admin configuration."
            })
    );
}

async fn scan(roots: Vec<Root>) -> Result<Contents> {
    tauri::async_runtime::spawn_blocking(move || {
        snapshot::scan(&roots).map_err(|e| anyhow!(e.to_string()))
    })
    .await?
}

fn pending_path(directory: &Path, pending: &Pending) -> PathBuf {
    directory.join(format!("pending-{}.tar.zst", pending.id))
}

fn baseline(state: &mut LocalState, remote: &RemoteState, contents: Contents) {
    state.revision = Some(remote.revision);
    state.definition_revision = Some(remote.definition_revision);
    state.baseline = Some(contents);
    state.observed_hash = None;
    state.message = None;
}

fn needs_choice(state: &LocalState, remote: &RemoteState, local_hash: &str) -> bool {
    cloud_saves::policy::requires_choice(
        state.revision,
        state.definition_revision,
        remote.revision,
        remote.definition_revision,
        local_hash,
        remote
            .snapshots
            .first()
            .map(|s| (s.content_hash.as_str(), s.definition_revision)),
    )
}

async fn capture_pending(
    session: &Session,
    id: &str,
    namespace: &str,
    directory: &Path,
    remote: &RemoteState,
    kind: &str,
) -> Result<Pending> {
    session.check()?;
    let roots = resolve_roots(id, namespace, &remote.locations)?;
    let snapshot_id = uuid::Uuid::new_v4().to_string();
    let output = directory.join(format!("pending-{snapshot_id}.tar.zst"));
    let game = id.to_owned();
    let platform = namespace.to_owned();
    let revision = remote.definition_revision;
    let limit = remote.limits.archive_bytes.min(512 * 1024 * 1024);
    let archive = output.clone();
    let captured = tauri::async_runtime::spawn_blocking(move || -> Result<_> {
        let manifest = snapshot::capture(&roots, &game, &platform, revision, &archive, limit)
            .map_err(|e| anyhow!(e.to_string()))?;
        let checksum =
            snapshot::hash_reader(File::open(&archive)?).map_err(|e| anyhow!(e.to_string()))?;
        Ok((manifest, checksum))
    })
    .await?;
    let (manifest, checksum) = match captured {
        Ok(captured) => captured,
        Err(error) => {
            let _ = fs::remove_file(&output);
            return Err(error);
        }
    };
    Ok(Pending {
        id: snapshot_id,
        expected_revision: remote.revision,
        kind: kind.into(),
        manifest,
        checksum,
    })
}

async fn upload_pending(
    session: &Session,
    id: &str,
    namespace: &str,
    directory: &Path,
    state: &mut LocalState,
    remote: &RemoteState,
) -> Result<()> {
    let pending = state.pending.clone().context("No pending snapshot.")?;
    let accepted = remote
        .snapshots
        .iter()
        .find(|s| s.id == pending.id && s.checksum == pending.checksum);
    let summary = if let Some(accepted) = accepted {
        accepted.clone()
    } else {
        if remote.revision != pending.expected_revision
            || remote.definition_revision != pending.manifest.definition_revision
        {
            bail!(
                "Cloud save conflict or configuration change. Your pending snapshot is preserved. Choose local or cloud."
            );
        }
        let file = tokio::fs::File::open(pending_path(directory, &pending)).await?;
        let bytes = file.metadata().await?.len();
        let response = DROP_CLIENT_ASYNC
            .post(session.url(id, namespace, None)?)
            .header(
                "Authorization",
                authorization_header_for(Some(&session.auth))?,
            )
            .header("Content-Type", "application/octet-stream")
            .header("Content-Length", bytes)
            .header("X-Snapshot-Id", &pending.id)
            .header("X-Expected-Revision", pending.expected_revision)
            .header(
                "X-Definition-Revision",
                pending.manifest.definition_revision,
            )
            .header("X-Archive-Sha256", &pending.checksum)
            .header(
                "X-Content-Sha256",
                pending
                    .manifest
                    .contents
                    .fingerprint()
                    .map_err(|e| anyhow!(e.to_string()))?,
            )
            .header("X-Capture-Kind", &pending.kind)
            .body(reqwest::Body::wrap_stream(ReaderStream::new(file)))
            .timeout(Duration::from_secs(120))
            .send()
            .await?;
        check_response(response)
            .await?
            .json::<SnapshotSummary>()
            .await?
    };
    state.revision = Some(summary.revision);
    state.definition_revision = Some(summary.definition_revision);
    state.baseline = Some(pending.manifest.contents.clone());
    state.pending = None;
    state.message = None;
    save(directory, state)?;
    fs::remove_file(pending_path(directory, &pending))?;
    Ok(())
}

fn preserve_pending(directory: &Path, state: &mut LocalState) -> Result<()> {
    if let Some(pending) = &state.pending {
        let recovery = directory.join("recovery");
        fs::create_dir_all(&recovery)?;
        if fs::read_dir(&recovery)?.count() >= 10 {
            bail!(
                "Recovery folder is full. Move older recovery archives elsewhere before continuing."
            );
        }
        let path = pending_path(directory, pending);
        if path.exists() {
            fs::rename(
                path,
                recovery.join(format!("pending-{}.tar.zst", pending.id)),
            )?;
            #[cfg(unix)]
            File::open(&recovery)?.sync_all()?;
        }
        state.pending = None;
        save(directory, state)?;
    }
    Ok(())
}

fn recovery_files(directory: &Path) -> Result<Vec<String>> {
    let folder = directory.join("recovery");
    if !folder.exists() {
        return Ok(vec![]);
    }
    let mut files = fs::read_dir(folder)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| name.ends_with(".tar.zst"))
        .collect::<Vec<_>>();
    files.sort();
    Ok(files)
}

async fn automatic_game(session: &Session, id: &str, connected: bool) -> Result<()> {
    let (namespace, _) = installed(id)?;
    let directory = session.directory(id, &namespace)?;
    let mut state = load(&directory)?;
    if directory.join("restore.json").exists() {
        return Ok(());
    }
    let fresh = if connected {
        fetch_remote(session, id, &namespace).await
    } else {
        Err(anyhow!(
            "Offline: local snapshots will remain queued until the server is available."
        ))
    };
    let online = fresh.is_ok();
    // Never upload without a fresh server-side opt-in check. Cached definitions
    // may only be used to queue a local snapshot during a connection failure.
    let remote = match fresh {
        Ok(remote) => remote,
        Err(error) => {
            state.message = Some(error.to_string());
            save(&directory, &state)?;
            state
                .remote
                .clone()
                .context("No cached save configuration for offline capture.")?
        }
    };
    state.remote = Some(remote.clone());
    if online {
        state.message = None;
    }
    let result: Result<()> = async {
        if state.pending.is_some() {
            if online { upload_pending(session, id, &namespace, &directory, &mut state, &remote).await?; }
            return Ok(());
        }
        let contents = scan(resolve_roots(id, &namespace, &remote.locations)?).await?;
        let hash = contents.fingerprint().map_err(|e| anyhow!(e.to_string()))?;
        if needs_choice(&state, &remote, &hash) { bail!("Cloud saves or their configuration changed. Choose local or cloud before syncing."); }
        if online && remote.snapshots.first().is_some_and(|s| s.content_hash == hash && s.definition_revision == remote.definition_revision) {
            baseline(&mut state, &remote, contents);
            return Ok(());
        }
        if state.baseline.as_ref().is_some_and(|b| b.fingerprint().ok().as_deref() == Some(&hash)) { return Ok(()); }
        if !contents.has_saves() { return Ok(()); }
        if state.baseline.as_ref().is_some_and(|b| b.roots.iter().any(|old| old.present && contents.roots.iter().any(|r| r.id == old.id && !r.present))) {
            bail!("A save location disappeared. Automatic capture paused to avoid backing up a missing drive or folder.");
        }
        if state.observed_hash.as_deref() != Some(&hash) {
            state.observed_hash = Some(hash);
            state.observed_at = now();
            return Ok(());
        }
        if now().saturating_sub(state.observed_at) < 60 { return Ok(()); }
        let pending = capture_pending(session, id, &namespace, &directory, &remote, "background").await?;
        state.pending = Some(pending);
        save(&directory, &state)?;
        if online { upload_pending(session, id, &namespace, &directory, &mut state, &remote).await?; }
        Ok(())
    }.await;
    if let Err(error) = &result {
        state.message = Some(error.to_string());
    }
    save(&directory, &state)?;
    result
}

pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            let Ok(session) = Session::current() else {
                continue;
            };
            let games = {
                let db = borrow_db_checked();
                db.applications
                    .installed_game_version
                    .keys()
                    .filter(|id| !db.applications.local_games.contains_key(*id))
                    .cloned()
                    .collect::<Vec<_>>()
            };
            if games.is_empty() {
                continue;
            }
            let eligible = fetch_enabled_games(&session).await;
            for id in games {
                let Ok(_guard) = OPERATIONS.try_lock() else {
                    break;
                };
                if session.check().is_err() {
                    break;
                }
                match &eligible {
                    Ok(enabled) if !enabled.contains(&id) => continue,
                    Err(_) => {
                        let cached = installed(&id)
                            .and_then(|(namespace, _)| session.directory(&id, &namespace))
                            .and_then(|directory| load(&directory));
                        if !cached.is_ok_and(|state| state.remote.is_some()) {
                            continue;
                        }
                    }
                    _ => (),
                }
                // Status is persisted per game; failures must not stop other games.
                let _ = automatic_game(&session, &id, eligible.is_ok()).await;
                let _ = app.emit("cloud-saves-changed", &id);
            }
        }
    });
}

#[tauri::command]
pub async fn set_cloud_saves_enabled(enabled: bool) -> Result<(), String> {
    let _guard = OPERATIONS.lock().await;
    borrow_db_mut_checked().settings.cloud_saves_enabled = enabled;
    Ok(())
}

#[tauri::command]
pub fn open_cloud_save_recovery(id: String, app: AppHandle) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    let session = Session::current().map_err(|e| e.to_string())?;
    let (namespace, _) = installed(&id).map_err(|e| e.to_string())?;
    let directory = session
        .directory(&id, &namespace)
        .map_err(|e| e.to_string())?
        .join("recovery");
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(directory.to_string_lossy(), None::<&str>)
        .map_err(|e| e.to_string())
}

async fn status(id: &str) -> Result<SaveStatus> {
    let enabled = borrow_db_checked().settings.cloud_saves_enabled;
    let (namespace, _) = installed(id)?;
    if !enabled {
        return Ok(SaveStatus {
            enabled,
            phase: "disabled".into(),
            message: "Enable cloud saves globally in Settings → General.".into(),
            namespace,
            revision: 0,
            definition_revision: 0,
            snapshots: vec![],
            locations: vec![],
            recovery_directory: None,
            interrupted_restore: false,
            recoveries: vec![],
        });
    }
    let session = Session::current()?;
    let directory = session.directory(id, &namespace)?;
    let mut state = load(&directory)?;
    // Recovery must remain available offline or after an administrator changes
    // the definition. The journal records the originally approved destinations.
    if directory.join("restore.json").exists() {
        let journal: RestoreJournal =
            serde_json::from_slice(&fs::read(directory.join("restore.json"))?)?;
        return Ok(SaveStatus {
            enabled, phase: "recovery".into(),
            message: "An interrupted restore was detected. Automatic sync is paused. Recover the previous local backup or retry the cloud restore after reconnecting.".into(),
            namespace, revision: state.remote.as_ref().map_or(0, |r| r.revision),
            definition_revision: state.remote.as_ref().map_or(0, |r| r.definition_revision),
            snapshots: state.remote.as_ref().map_or_else(Vec::new, |r| r.snapshots.clone()),
            locations: journal.roots.iter().map(|r| r.path.to_string_lossy().into_owned()).collect(),
            recovery_directory: Some(directory.join("recovery").to_string_lossy().into_owned()),
            interrupted_restore: true, recoveries: recovery_files(&directory)?,
        });
    }
    let fresh = fetch_remote(&session, id, &namespace).await;
    let offline_message = fresh.as_ref().err().map(ToString::to_string);
    let remote = match fresh {
        Ok(remote) => {
            state.remote = Some(remote.clone());
            save(&directory, &state)?;
            remote
        }
        Err(error) => state.remote.clone().ok_or(error)?,
    };
    let roots = resolve_roots(id, &namespace, &remote.locations)?;
    let contents = scan(roots.clone()).await?;
    let hash = contents.fingerprint().map_err(|e| anyhow!(e.to_string()))?;
    let interrupted = directory.join("restore.json").exists();
    let (phase, message) = if interrupted {
        ("recovery", "An interrupted restore was detected. Automatic sync is paused. Retry the restore or recover the previous local backup.".into())
    } else if let Some(message) = offline_message {
        ("offline", message)
    } else if needs_choice(&state, &remote, &hash) {
        ("conflict", "Cloud saves or their configuration differ from this device. Choose a complete local or cloud snapshot; files are never merged automatically.".into())
    } else if state.pending.is_some() {
        (
            "pending",
            state
                .message
                .clone()
                .unwrap_or("A snapshot is queued for upload.".into()),
        )
    } else if let Some(message) = &state.message {
        ("error", message.clone())
    } else if remote
        .snapshots
        .first()
        .is_some_and(|s| s.content_hash == hash)
    {
        (
            "current",
            "Local files match the latest cloud snapshot.".into(),
        )
    } else if !contents.has_saves() {
        ("empty", "No local saves found yet.".into())
    } else {
        (
            "pending",
            "Local changes detected. A recovery snapshot will be captured after two quiet scans."
                .into(),
        )
    };
    Ok(SaveStatus {
        enabled,
        phase: phase.into(),
        message,
        namespace,
        revision: remote.revision,
        definition_revision: remote.definition_revision,
        snapshots: remote.snapshots,
        locations: roots
            .iter()
            .map(|r| r.path.to_string_lossy().into_owned())
            .collect(),
        recovery_directory: Some(directory.join("recovery").to_string_lossy().into_owned()),
        interrupted_restore: interrupted,
        recoveries: recovery_files(&directory)?,
    })
}

#[tauri::command]
pub async fn fetch_cloud_save_status(id: String) -> Result<SaveStatus, String> {
    let _guard = OPERATIONS.lock().await;
    status(&id).await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn upload_cloud_save(
    id: String,
    confirmed_closed: bool,
    use_local: bool,
    expected_revision: u32,
    definition_revision: u32,
    app: AppHandle,
) -> Result<(), String> {
    let _guard = OPERATIONS.lock().await;
    let _launch_guard = RESTORE_LAUNCH.write().await;
    let result: Result<()> = async {
        if !confirmed_closed || running(&id) { bail!("Close the game and its launcher before uploading a confirmed save."); }
        let session = Session::current()?;
        let (namespace, _) = installed(&id)?;
        let directory = session.directory(&id, &namespace)?;
        if directory.join("restore.json").exists() { bail!("Recover or retry the interrupted restore first."); }
        let remote = fetch_remote(&session, &id, &namespace).await?;
        if remote.revision != expected_revision || remote.definition_revision != definition_revision { bail!("Cloud saves changed since you reviewed them. Check again before choosing local saves."); }
        let mut state = load(&directory)?;
        let contents = scan(resolve_roots(&id, &namespace, &remote.locations)?).await?;
        let hash = contents.fingerprint().map_err(|e| anyhow!(e.to_string()))?;
        if needs_choice(&state, &remote, &hash) && !use_local { bail!("Cloud save conflict. Explicitly choose Use local to publish this device's saves."); }
        preserve_pending(&directory, &mut state)?;
        state.pending = Some(capture_pending(&session, &id, &namespace, &directory, &remote, "manual").await?);
        state.remote = Some(remote.clone());
        save(&directory, &state)?;
        upload_pending(&session, &id, &namespace, &directory, &mut state, &remote).await
    }.await;
    let _ = app.emit("cloud-saves-changed", &id);
    result.map_err(|e| e.to_string())
}

async fn download(
    session: &Session,
    id: &str,
    namespace: &str,
    summary: &SnapshotSummary,
    path: &Path,
    limit: u64,
) -> Result<()> {
    let response = DROP_CLIENT_ASYNC
        .get(session.url(id, namespace, Some(&summary.id))?)
        .header(
            "Authorization",
            authorization_header_for(Some(&session.auth))?,
        )
        .timeout(Duration::from_secs(120))
        .send()
        .await?;
    let mut response = check_response(response).await?;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    let mut bytes = 0u64;
    let mut hash = Sha256::new();
    use tokio::io::AsyncWriteExt;
    while let Some(chunk) = response.chunk().await? {
        bytes += chunk.len() as u64;
        if bytes > limit.min(512 * 1024 * 1024) || bytes > summary.bytes {
            bail!("Snapshot exceeds the download limit.");
        }
        hash.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.sync_all().await?;
    if bytes != summary.bytes || format!("{:x}", hash.finalize()) != summary.checksum {
        bail!("Downloaded snapshot checksum mismatch.");
    }
    Ok(())
}

#[tauri::command]
pub async fn restore_cloud_save(
    id: String,
    snapshot_id: String,
    confirmed_closed: bool,
    app: AppHandle,
) -> Result<(), String> {
    let _guard = OPERATIONS.lock().await;
    let _launch_guard = RESTORE_LAUNCH.write().await;
    let result: Result<()> = async {
        if !confirmed_closed || running(&id) { bail!("Close the game and its launcher before restoring saves."); }
        let session = Session::current()?;
        let (namespace, _) = installed(&id)?;
        let directory = session.directory(&id, &namespace)?;
        if directory.join("restore.json").exists() { bail!("Recover or retry the interrupted restore before selecting another snapshot."); }
        let remote = fetch_remote(&session, &id, &namespace).await?;
        let summary = remote.snapshots.iter().find(|s| s.id == snapshot_id).context("Snapshot is no longer retained on the server.")?.clone();
        let roots = resolve_roots(&id, &namespace, &remote.locations)?;
        let before = scan(roots.clone()).await?;
        let archive = directory.join(format!("restore-{}.tar.zst", uuid::Uuid::new_v4()));
        let mut archive_guard = ArchiveGuard { path: archive.clone(), retained: false };
        if let Err(error) = download(&session, &id, &namespace, &summary, &archive, remote.limits.archive_bytes).await { let _ = fs::remove_file(&archive); return Err(error); }
        // Validate before allocating a recovery copy or changing live files.
        let game = id.clone(); let platform = namespace.clone(); let definition = summary.definition_revision;
        let prepared_roots = roots.clone(); let prepared_archive = archive.clone();
        let prepared = tauri::async_runtime::spawn_blocking(move || snapshot::prepare_restore(&prepared_archive, &prepared_roots, &game, &platform, definition).map_err(|e| anyhow!(e.to_string()))).await??;
        if prepared.manifest.contents.fingerprint().map_err(|e| anyhow!(e.to_string()))? != summary.content_hash { bail!("Snapshot contents do not match server metadata."); }
        let existing_journal = directory.join("restore.json");
        let mut state = load(&directory)?;
        preserve_pending(&directory, &mut state)?;
        let recovery = backup_recovery_attempt(&directory, &roots, &id, &namespace, remote.definition_revision, &before).await?;
        let journal = RestoreJournal { archive: archive.clone(), recovery, roots: roots.clone(), game_id: id.clone(), namespace: namespace.clone(), definition_revision: remote.definition_revision, applied_definition_revision: summary.definition_revision };
        session.check()?;
        write_json(&existing_journal, &journal)?;
        archive_guard.retained = true;
        let restored = prepared.manifest.contents.clone();
        tauri::async_runtime::spawn_blocking(move || snapshot::apply_restore(&prepared, &roots, &before).map_err(|e| anyhow!(e.to_string()))).await??;
        baseline(&mut state, &remote, restored);
        if summary.revision != remote.revision {
            state.revision = None;
            state.message = Some("An older snapshot was restored locally. Explicitly upload it to make it the current cloud save.".into());
        }
        state.remote = Some(remote);
        save(&directory, &state)?;
        fs::remove_file(existing_journal)?;
        fs::remove_file(archive)?;
        Ok(())
    }.await;
    let _ = app.emit("cloud-saves-changed", &id);
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn recover_cloud_save(
    id: String,
    confirmed_closed: bool,
    app: AppHandle,
) -> Result<(), String> {
    let _guard = OPERATIONS.lock().await;
    let _launch_guard = RESTORE_LAUNCH.write().await;
    let result: Result<()> = async {
        if !confirmed_closed || running(&id) {
            bail!("Close the game before recovering saves.");
        }
        let session = Session::current()?;
        let (namespace, _) = installed(&id)?;
        let directory = session.directory(&id, &namespace)?;
        let journal_path = directory.join("restore.json");
        let journal: RestoreJournal = serde_json::from_slice(&fs::read(&journal_path)?)?;
        let recovery = journal
            .recovery
            .context("No previous local saves existed. Retry the cloud restore to complete it.")?;
        let before = scan(journal.roots.clone()).await?;
        let _ = backup_recovery_attempt(
            &directory,
            &journal.roots,
            &id,
            &namespace,
            journal.definition_revision,
            &before,
        )
        .await?;
        let old_archive = journal.archive.clone();
        session.check()?;
        tauri::async_runtime::spawn_blocking(move || {
            let prepared = snapshot::prepare_restore(
                &recovery,
                &journal.roots,
                &journal.game_id,
                &journal.namespace,
                journal.definition_revision,
            )
            .map_err(|e| anyhow!(e.to_string()))?;
            let applied = snapshot::prepare_restore(
                &journal.archive,
                &journal.roots,
                &journal.game_id,
                &journal.namespace,
                journal.applied_definition_revision,
            )
            .map_err(|e| anyhow!(e.to_string()))?;
            snapshot::apply_recovery(
                &prepared,
                &applied.manifest.contents,
                &journal.roots,
                &before,
            )
            .map_err(|e| anyhow!(e.to_string()))
        })
        .await??;
        // Require a fresh local/cloud choice after rollback, never auto-publish.
        let mut state = load(&directory)?;
        state.revision = None;
        state.baseline = None;
        state.message = Some(
            "Previous local backup recovered. Review local and cloud saves before syncing.".into(),
        );
        save(&directory, &state)?;
        fs::remove_file(journal_path)?;
        if old_archive.parent() == Some(directory.as_path()) {
            fs::remove_file(old_archive)?;
        }
        Ok(())
    }
    .await;
    let _ = app.emit("cloud-saves-changed", &id);
    result.map_err(|e| e.to_string())
}

struct ArchiveGuard {
    path: PathBuf,
    retained: bool,
}

async fn backup_recovery_attempt(
    directory: &Path,
    roots: &[Root],
    id: &str,
    namespace: &str,
    definition: u32,
    contents: &Contents,
) -> Result<Option<PathBuf>> {
    if !contents.has_saves() {
        return Ok(None);
    }
    if recovery_files(directory)?.len() >= 10 {
        bail!("Recovery folder is full. Move older archives elsewhere before continuing.");
    }
    let folder = directory.join("recovery");
    fs::create_dir_all(&folder)?;
    let output = folder.join(format!(
        "before-recovery-{}-{}.tar.zst",
        now(),
        uuid::Uuid::new_v4()
    ));
    let mut guard = ArchiveGuard {
        path: output.clone(),
        retained: false,
    };
    let roots = roots.to_vec();
    let game = id.to_owned();
    let platform = namespace.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        snapshot::capture(
            &roots,
            &game,
            &platform,
            definition,
            &output,
            512 * 1024 * 1024,
        )
        .map_err(|e| anyhow!(e.to_string()))
    })
    .await??;
    #[cfg(unix)]
    File::open(folder)?.sync_all()?;
    guard.retained = true;
    Ok(Some(guard.path.clone()))
}

#[tauri::command]
pub async fn retry_cloud_save_restore(
    id: String,
    confirmed_closed: bool,
    app: AppHandle,
) -> Result<(), String> {
    let _guard = OPERATIONS.lock().await;
    let _launch_guard = RESTORE_LAUNCH.write().await;
    let result: Result<()> = async {
        if !confirmed_closed || running(&id) {
            bail!("Close the game before retrying the interrupted restore.");
        }
        let session = Session::current()?;
        let (namespace, _) = installed(&id)?;
        let directory = session.directory(&id, &namespace)?;
        let journal: RestoreJournal =
            serde_json::from_slice(&fs::read(directory.join("restore.json"))?)?;
        if journal.game_id != id || journal.namespace != namespace {
            bail!("Restore journal identity mismatch.");
        }
        let roots = journal.roots.clone();
        let archive = journal.archive.clone();
        let game = id.clone();
        let platform = namespace.clone();
        let definition = journal.applied_definition_revision;
        let prepared = tauri::async_runtime::spawn_blocking(move || {
            snapshot::prepare_restore(&archive, &roots, &game, &platform, definition)
                .map_err(|e| anyhow!(e.to_string()))
        })
        .await??;
        let before = scan(journal.roots.clone()).await?;
        let _ = backup_recovery_attempt(
            &directory,
            &journal.roots,
            &id,
            &namespace,
            journal.definition_revision,
            &before,
        )
        .await?;
        session.check()?;
        let old_archive = journal.archive.clone();
        tauri::async_runtime::spawn_blocking(move || {
            snapshot::apply_restore(&prepared, &journal.roots, &before)
                .map_err(|e| anyhow!(e.to_string()))
        })
        .await??;
        let mut state = load(&directory)?;
        state.revision = None;
        state.baseline = None;
        state.message =
            Some("Interrupted restore completed locally. Check cloud saves before syncing.".into());
        save(&directory, &state)?;
        fs::remove_file(directory.join("restore.json"))?;
        if old_archive.parent() == Some(directory.as_path()) {
            fs::remove_file(old_archive)?;
        }
        Ok(())
    }
    .await;
    let _ = app.emit("cloud-saves-changed", &id);
    result.map_err(|e| e.to_string())
}
impl Drop for ArchiveGuard {
    fn drop(&mut self) {
        if !self.retained {
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[tauri::command]
pub async fn restore_local_save_backup(
    id: String,
    name: String,
    confirmed_closed: bool,
    app: AppHandle,
) -> Result<(), String> {
    let _guard = OPERATIONS.lock().await;
    let _launch_guard = RESTORE_LAUNCH.write().await;
    let result: Result<()> = async {
        if !confirmed_closed || running(&id) {
            bail!("Close the game before recovering a local backup.");
        }
        let session = Session::current()?;
        let (namespace, _) = installed(&id)?;
        let directory = session.directory(&id, &namespace)?;
        if directory.join("restore.json").exists() {
            bail!("Recover the interrupted restore before selecting a different local backup.");
        }
        if !recovery_files(&directory)?.contains(&name) || name.contains(['/', '\\']) {
            bail!("Invalid local backup name.");
        }
        let archive = directory.join("recovery").join(name);
        snapshot::safe_path(&archive).map_err(|e| anyhow!(e.to_string()))?;
        let mut state = load(&directory)?;
        let remote = state
            .remote
            .clone()
            .context("No cached save definition. Check cloud saves first.")?;
        let roots = resolve_roots(&id, &namespace, &remote.locations)?;
        let before = scan(roots.clone()).await?;
        let game = id.clone();
        let platform = namespace.clone();
        let prepare_path = archive.clone();
        let prepare_roots = roots.clone();
        let prepared = tauri::async_runtime::spawn_blocking(move || {
            let manifest =
                snapshot::inspect_manifest(&prepare_path).map_err(|e| anyhow!(e.to_string()))?;
            snapshot::prepare_restore(
                &prepare_path,
                &prepare_roots,
                &game,
                &platform,
                manifest.definition_revision,
            )
            .map_err(|e| anyhow!(e.to_string()))
        })
        .await??;
        preserve_pending(&directory, &mut state)?;
        let recovery = backup_recovery_attempt(
            &directory,
            &roots,
            &id,
            &namespace,
            remote.definition_revision,
            &before,
        )
        .await?;
        let journal = RestoreJournal {
            archive: archive.clone(),
            recovery,
            roots: roots.clone(),
            game_id: id.clone(),
            namespace,
            definition_revision: remote.definition_revision,
            applied_definition_revision: prepared.manifest.definition_revision,
        };
        session.check()?;
        write_json(&directory.join("restore.json"), &journal)?;
        tauri::async_runtime::spawn_blocking(move || {
            snapshot::apply_restore(&prepared, &roots, &before).map_err(|e| anyhow!(e.to_string()))
        })
        .await??;
        state.revision = None;
        state.baseline = None;
        state.message =
            Some("Local backup restored. Choose local or cloud before syncing again.".into());
        save(&directory, &state)?;
        fs::remove_file(directory.join("restore.json"))?;
        Ok(())
    }
    .await;
    let _ = app.emit("cloud-saves-changed", &id);
    result.map_err(|e| e.to_string())
}

/// Prevent a Drop-managed launch from racing a restore. External launchers
/// remain outside our control; the user must explicitly confirm they are closed.
pub async fn launch_gate() -> tokio::sync::RwLockReadGuard<'static, ()> {
    RESTORE_LAUNCH.read().await
}
