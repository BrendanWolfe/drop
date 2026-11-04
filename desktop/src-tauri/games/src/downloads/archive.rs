use crate::{library::push_game_update, local, state::GameStatusManager};
use async_trait::async_trait;
use database::{
    ApplicationTransientStatus, DownloadableMetadata, borrow_db_checked, borrow_db_mut_checked,
    models::data::{ArchiveState, LocalGame},
};
use download_manager::{
    download_manager_frontend::{DownloadManagerSignal, DownloadStatus},
    downloadable::Downloadable,
    error::ApplicationDownloadError,
    util::{
        download_thread_control_flag::{DownloadThreadControl, DownloadThreadControlFlag},
        progress_object::{ProgressHandle, ProgressObject, ProgressType},
    },
};
use reqwest::{Client, StatusCode, header, redirect::Policy};
use std::{
    collections::HashSet,
    fmt,
    fs::{self, OpenOptions},
    io::Write,
    sync::{Arc, LazyLock, Mutex},
    time::Duration,
};
use tauri::AppHandle;
use tokio::sync::mpsc::Sender;
use url::Url;
use utils::{app_emit, lock};

static BUSY: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));

struct Busy(String);
impl Drop for Busy {
    fn drop(&mut self) {
        lock!(BUSY).remove(&self.0);
    }
}
pub fn is_busy(id: &str) -> bool {
    lock!(BUSY).contains(id)
}

pub fn parse_external_url(raw: &str) -> Result<Url, ApplicationDownloadError> {
    let url = Url::parse(raw).map_err(|_| fail("Enter a valid HTTP or HTTPS archive URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(fail(
            "Use an HTTP or HTTPS URL without embedded credentials",
        ));
    }
    Ok(url)
}

/// Deliberately NOT a Drop client: no authentication middleware, identity,
/// custom headers, cookies, certificates, or server online/offline side effects.
pub fn external_client() -> Result<Client, ApplicationDownloadError> {
    Client::builder()
        .user_agent("Drop Desktop Archive Downloader")
        .referer(false)
        .no_gzip()
        .no_brotli()
        .no_deflate()
        .no_zstd()
        .connect_timeout(Duration::from_secs(20))
        .redirect(Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 {
                return attempt.error("Too many redirects");
            }
            let url = attempt.url();
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return attempt.error("Unsupported archive redirect");
            }
            attempt.follow()
        }))
        .build()
        .map_err(network_error)
}

fn fail(message: impl Into<String>) -> ApplicationDownloadError {
    ApplicationDownloadError::Archive(message.into())
}
fn network_error(error: reqwest::Error) -> ApplicationDownloadError {
    fail(error.without_url().to_string())
}

fn validator(headers: &header::HeaderMap) -> Option<String> {
    headers
        .get(header::ETAG)
        .and_then(|v| v.to_str().ok())
        .filter(|v| !v.starts_with("W/"))
        .or_else(|| {
            headers
                .get(header::LAST_MODIFIED)
                .and_then(|v| v.to_str().ok())
        })
        .map(str::to_owned)
}

fn range_total(value: &str, offset: u64) -> Option<u64> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let start = start.parse::<u64>().ok()?;
    let end = end.parse::<u64>().ok()?;
    let total = total.parse::<u64>().ok()?;
    (start == offset && end >= start && end < total).then_some(total)
}

pub struct ArchiveDownloadAgent {
    game: LocalGame,
    meta: DownloadableMetadata,
    control: DownloadThreadControl,
    download: Arc<ProgressObject>,
    disk: Arc<ProgressObject>,
    status: Mutex<DownloadStatus>,
    sender: Sender<DownloadManagerSignal>,
}

impl fmt::Debug for ArchiveDownloadAgent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArchiveDownloadAgent")
            .field("id", &self.meta.id)
            .finish()
    }
}

impl ArchiveDownloadAgent {
    pub fn new(game: LocalGame, sender: Sender<DownloadManagerSignal>) -> Self {
        Self {
            meta: local::metadata(&game),
            game,
            control: DownloadThreadControl::new(DownloadThreadControlFlag::Stop),
            download: Arc::new(ProgressObject::new(
                0,
                1,
                sender.clone(),
                ProgressType::Download,
            )),
            disk: Arc::new(ProgressObject::new(
                0,
                1,
                sender.clone(),
                ProgressType::Disk,
            )),
            status: Mutex::new(DownloadStatus::Queued),
            sender,
        }
    }

    fn update(
        &self,
        app: &AppHandle,
        state: ArchiveState,
        transient: Option<ApplicationTransientStatus>,
    ) {
        let mut db = borrow_db_mut_checked();
        if let Some(archive) = db
            .applications
            .local_games
            .get_mut(&self.meta.id)
            .and_then(|g| g.archive.as_mut())
        {
            archive.state = state;
            archive.error = None;
        }
        db.applications.transient_statuses.remove(&self.meta);
        if let Some(status) = transient {
            db.applications
                .transient_statuses
                .insert(self.meta.clone(), status);
        }
        let version = db
            .applications
            .game_versions
            .get(&self.meta.version)
            .cloned();
        push_game_update(
            app,
            &self.meta.id,
            version,
            GameStatusManager::fetch_state(&self.meta.id, &db),
        );
        app_emit!(app, "update_local_game", self.meta.id.clone());
        let _ = self.sender.try_send(DownloadManagerSignal::UpdateUIQueue);
    }

    async fn transfer(&self) -> Result<bool, ApplicationDownloadError> {
        let archive = {
            let db = borrow_db_checked();
            db.applications
                .local_games
                .get(&self.meta.id)
                .and_then(|g| g.archive.clone())
                .ok_or_else(|| fail("Archive job no longer exists"))?
        };
        if archive.downloaded && archive.archive_path.is_file() {
            let size = fs::metadata(&archive.archive_path)?.len() as usize;
            self.download.reset();
            self.download.set_max(size);
            ProgressHandle::new(self.download.get(0), self.download.clone()).skip(size);
            return Ok(true);
        }
        let client = external_client()?;
        let url = parse_external_url(&archive.url)?;
        let mut offset = if archive.validator.is_some() {
            fs::metadata(&archive.archive_path)
                .map(|v| v.len())
                .unwrap_or(0)
        } else {
            0
        };
        let mut request = client.get(url).header(header::ACCEPT_ENCODING, "identity");
        if offset > 0 {
            request = request
                .header(header::RANGE, format!("bytes={offset}-"))
                .header(
                    header::IF_RANGE,
                    archive.validator.as_deref().unwrap_or_default(),
                );
        }
        let mut response = tokio::select! {
            response = request.send() => response.map_err(network_error)?,
            _ = self.wait_for_stop() => return Ok(false),
        };
        // A fully received partial file may not have had its completion flag saved.
        // Restart rather than accepting an unvalidated 416 as a complete archive.
        if response.status() == StatusCode::RANGE_NOT_SATISFIABLE && offset > 0 {
            offset = 0;
            response = client
                .get(parse_external_url(&archive.url)?)
                .header(header::ACCEPT_ENCODING, "identity")
                .send()
                .await
                .map_err(network_error)?;
        }
        if !response.status().is_success() {
            return Err(fail(format!(
                "Archive host returned HTTP {}",
                response.status().as_u16()
            )));
        }
        let current_validator = validator(response.headers());
        let total = if response.status() == StatusCode::PARTIAL_CONTENT {
            if offset == 0 || current_validator != archive.validator {
                if let Some(a) = borrow_db_mut_checked()
                    .applications
                    .local_games
                    .get_mut(&self.meta.id)
                    .and_then(|g| g.archive.as_mut())
                {
                    a.validator = None;
                }
                return Err(fail(
                    "Archive changed while resuming. Retry the download to restart it.",
                ));
            }
            Some(
                response
                    .headers()
                    .get(header::CONTENT_RANGE)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| range_total(v, offset))
                    .ok_or_else(|| fail("Archive host returned an invalid byte range"))?,
            )
        } else {
            offset = 0; // The server ignored Range/If-Range. Never append a full response.
            response.content_length()
        };
        if let Some(parent) = archive.archive_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(offset == 0)
            .append(offset > 0)
            .open(&archive.archive_path)?;
        {
            let mut db = borrow_db_mut_checked();
            if let Some(a) = db
                .applications
                .local_games
                .get_mut(&self.meta.id)
                .and_then(|g| g.archive.as_mut())
            {
                a.validator = current_validator;
                a.downloaded = false;
            }
        }
        self.download.set_max(
            total
                .unwrap_or(0)
                .try_into()
                .map_err(|_| fail("Archive is too large"))?,
        );
        self.download.reset();
        let progress = ProgressHandle::new(self.download.get(0), self.download.clone());
        progress.skip(offset as usize);
        super::initialize_download_speed_limit_from_db();
        let mut received = offset;
        loop {
            let chunk = tokio::select! {
                chunk = response.chunk() => chunk.map_err(network_error)?,
                _ = self.wait_for_stop() => { file.sync_all()?; return Ok(false); },
            };
            let Some(chunk) = chunk else {
                break;
            };
            for part in chunk.chunks(64 * 1024) {
                if !super::limiter::throttle_bytes(part.len() as u64, &self.control).await {
                    file.sync_all()?;
                    return Ok(false);
                }
                file.write_all(part)?;
                received += part.len() as u64;
                progress.add(part.len());
            }
        }
        file.sync_all()?;
        if total.is_some_and(|total| received != total) {
            return Err(fail("Archive download was incomplete. Resume to retry."));
        }
        self.download.set_max(received as usize);
        if let Some(a) = borrow_db_mut_checked()
            .applications
            .local_games
            .get_mut(&self.meta.id)
            .and_then(|g| g.archive.as_mut())
        {
            a.downloaded = true;
        }
        Ok(true)
    }

    async fn wait_for_stop(&self) {
        while self.control.get() != DownloadThreadControlFlag::Stop {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}

#[async_trait]
impl Downloadable for ArchiveDownloadAgent {
    async fn download(&self, app: &AppHandle) -> Result<bool, ApplicationDownloadError> {
        if !lock!(BUSY).insert(self.meta.id.clone()) {
            return Err(fail("This archive is still stopping. Try again shortly."));
        }
        let busy = Arc::new(Busy(self.meta.id.clone()));
        *lock!(self.status) = DownloadStatus::Downloading;
        self.update(
            app,
            ArchiveState::Downloading,
            Some(ApplicationTransientStatus::Downloading {
                version_id: self.meta.version.clone(),
            }),
        );
        if !self.transfer().await? {
            return Ok(false);
        }
        if self.control.get() == DownloadThreadControlFlag::Stop {
            return Ok(false);
        }
        *lock!(self.status) = DownloadStatus::Extracting;
        self.update(
            app,
            ArchiveState::Extracting,
            Some(ApplicationTransientStatus::Extracting {
                version_id: self.meta.version.clone(),
            }),
        );
        let path = self
            .game
            .archive
            .as_ref()
            .ok_or_else(|| fail("Missing archive job"))?
            .archive_path
            .clone();
        let root = self.game.root.clone();
        let id = self.meta.id.clone();
        let control = self.control.clone();
        let disk = self.disk.clone();
        // Keep the busy lease until the blocking extractor has actually exited,
        // even if the manager force-aborts the async worker during shutdown.
        tokio::task::spawn_blocking(move || {
            let _busy = busy;
            super::extract::extract(&path, &root, &id, &control, &disk)
        })
        .await
        .map_err(|_| fail("Archive extraction worker failed"))?
        .map_err(Into::into)
    }
    fn validate(&self, _: &AppHandle) -> Result<bool, ApplicationDownloadError> {
        Ok(true)
    }
    fn dl_progress(&self) -> &Arc<ProgressObject> {
        &self.download
    }
    fn disk_progress(&self) -> &Arc<ProgressObject> {
        &self.disk
    }
    fn control_flag(&self) -> DownloadThreadControl {
        self.control.clone()
    }
    fn status(&self) -> DownloadStatus {
        lock!(self.status).clone()
    }
    fn metadata(&self) -> DownloadableMetadata {
        self.meta.clone()
    }
    fn on_queued(&self, app: &AppHandle) {
        *lock!(self.status) = DownloadStatus::Queued;
        self.update(
            app,
            ArchiveState::Incomplete,
            Some(ApplicationTransientStatus::Queued {
                version_id: self.meta.version.clone(),
            }),
        );
    }
    fn on_error(&self, app: &AppHandle, error: &ApplicationDownloadError) {
        *lock!(self.status) = DownloadStatus::Error;
        self.update(app, ArchiveState::Error, None);
        if let Some(a) = borrow_db_mut_checked()
            .applications
            .local_games
            .get_mut(&self.meta.id)
            .and_then(|g| g.archive.as_mut())
        {
            a.error = Some(error.to_string());
        }
        app_emit!(app, "update_local_game", self.meta.id.clone());
        app_emit!(app, "download_error", error.to_string());
    }
    async fn on_complete(&self, app: &AppHandle) -> Result<(), ApplicationDownloadError> {
        if let Some(a) = borrow_db_mut_checked()
            .applications
            .local_games
            .get_mut(&self.meta.id)
            .and_then(|g| g.archive.as_mut())
        {
            a.extracted = true;
        }
        self.update(app, ArchiveState::NeedsLauncher, None);
        // Failed/interrupted extractions retain their archive; successful ones
        // no longer need a second copy occupying the download cache.
        if let Some(archive) = &self.game.archive {
            let _ = fs::remove_file(&archive.archive_path);
        }
        app_emit!(app, "update_library", ());
        Ok(())
    }
    fn on_cancelled(&self, app: &AppHandle) {
        self.update(app, ArchiveState::Incomplete, None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_ranges_and_urls() {
        assert_eq!(range_total("bytes 100-199/200", 100), Some(200));
        assert_eq!(range_total("bytes 0-199/200", 100), None);
        assert_eq!(range_total("bytes 100-300/200", 100), None);
        for url in [
            "file:///etc/passwd",
            "ftp://host/game.zip",
            "https://user:password@host/game.rar",
        ] {
            assert!(parse_external_url(url).is_err());
        }
        assert!(parse_external_url("https://host/game.rar?signature=secret").is_ok());
    }

    #[tokio::test]
    async fn external_requests_and_redirects_never_use_drop_credentials() {
        use remote::proxy::{Credentials, PROXY_AUTH};
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        fn server(response: String) -> (String, std::thread::JoinHandle<String>) {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            listener.set_nonblocking(true).unwrap();
            let task = std::thread::spawn(move || {
                let start = std::time::Instant::now();
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && start.elapsed() < Duration::from_secs(10) =>
                        {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        Err(e) => panic!("test server did not receive a request: {e}"),
                    }
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut data = vec![];
                let mut byte = [0u8; 1];
                while !data.ends_with(b"\r\n\r\n") {
                    stream.read_exact(&mut byte).unwrap();
                    data.push(byte[0]);
                }
                stream.write_all(response.as_bytes()).unwrap();
                String::from_utf8(data).unwrap()
            });
            (url, task)
        }
        let (destination, received_destination) =
            server("HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK".into());
        let (origin, received_origin) = server(format!(
            "HTTP/1.1 302 Found\r\nLocation: {destination}/game.rar\r\nSet-Cookie: drop_session=secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        ));
        let origin = Url::parse(&origin).unwrap();
        let credentials = Credentials::new(
            &origin,
            serde_json::from_str(
                r#"[{"name":"X-Drop-Secret","value":"drop-token-must-not-leak"}]"#,
            )
            .unwrap(),
        )
        .unwrap();
        PROXY_AUTH.activate(&origin, Ok(credentials)).unwrap();
        let result = external_client()
            .unwrap()
            .get(origin)
            .timeout(Duration::from_secs(5))
            .send()
            .await;
        PROXY_AUTH.clear();
        assert_eq!(result.unwrap().text().await.unwrap(), "OK");
        for request in [
            received_origin.join().unwrap(),
            received_destination.join().unwrap(),
        ] {
            let request = request.to_ascii_lowercase();
            for forbidden in [
                "authorization:",
                "cookie:",
                "x-drop-secret:",
                "drop-token-must-not-leak",
            ] {
                assert!(
                    !request.contains(forbidden),
                    "Credential leaked: {forbidden}"
                );
            }
        }
    }
}
