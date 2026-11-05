//! Desktop updates are independent of the Drop server and game-update scheduler.
use std::{
    io::Write,
    sync::nonpoison::Mutex,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use semver::Version;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use tempfile::NamedTempFile;

const RELEASES_API: &str = "https://api.github.com/repos/BrendanWolfe/drop/releases?per_page=100";
const PUBLIC_KEY: Option<&str> = option_env!("DROP_UPDATER_PUBLIC_KEY");
const RELEASE_TAG: Option<&str> = option_env!("DROP_RELEASE_TAG");
const EVENT: &str = "desktop-update";

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    phase: Phase,
    tag: Option<String>,
    downloaded: u64,
    total: Option<u64>,
    error: Option<String>,
}

#[derive(Clone, Copy, Default, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    #[default]
    Idle,
    Checking,
    Downloading,
    Ready,
    Installing,
    Error,
    Disabled,
}

struct PreparedUpdate {
    update: Update,
    // Keep the verified payload on disk, not in RAM while waiting for the user.
    payload: NamedTempFile,
}

#[derive(Default)]
pub struct DesktopUpdater {
    status: Mutex<UpdateStatus>,
    // Serializes automatic checks, manual retries and installs.
    operation: tokio::sync::Mutex<Option<PreparedUpdate>>,
}

impl DesktopUpdater {
    fn publish(&self, app: &AppHandle, status: UpdateStatus) {
        *self.status.lock() = status.clone();
        let _ = app.emit(EVENT, status);
    }
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    published_at: Option<String>,
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

fn newest_release(releases: &[Release]) -> Option<&Release> {
    releases
        .iter()
        .filter(|release| {
            !release.draft
                && release.published_at.is_some()
                && release
                    .assets
                    .iter()
                    .any(|asset| asset.name == "latest.json")
        })
        .max_by_key(|release| &release.published_at)
}

fn should_offer_update(current: &Version, remote: &Version, tag: &str, current_tag: &str) -> bool {
    // This fork also publishes rebuilds without bumping the application version.
    // The embedded release tag prevents repeatedly offering the installed rebuild.
    tag != current_tag && remote >= current
}

fn disabled_reason() -> Option<&'static str> {
    if PUBLIC_KEY.is_none_or(|key| key.trim().is_empty()) {
        return Some("This build has no update signing public key configured.");
    }
    if cfg!(debug_assertions) {
        return Some("Desktop updates are disabled in development builds.");
    }
    #[cfg(target_os = "linux")]
    if tauri::utils::platform::bundle_type().is_none() {
        return Some("Desktop updates require a packaged installation.");
    }
    None
}

pub fn start(app: &AppHandle) {
    if let Some(reason) = disabled_reason() {
        log::info!("desktop updater disabled: {reason}");
        app.state::<DesktopUpdater>().publish(
            app,
            UpdateStatus {
                phase: Phase::Disabled,
                ..Default::default()
            },
        );
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            let result = check_desktop_update(app.clone()).await;
            let delay = if result.is_err() { 5 * 60 } else { 30 * 60 };
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
    });
}

#[tauri::command]
pub fn desktop_update_status(state: State<'_, DesktopUpdater>) -> UpdateStatus {
    state.status.lock().clone()
}

#[tauri::command]
pub async fn check_desktop_update(app: AppHandle) -> Result<(), String> {
    if let Some(reason) = disabled_reason() {
        return Err(reason.into());
    }
    let state = app.state::<DesktopUpdater>();
    let Ok(mut prepared) = state.operation.try_lock() else {
        return Ok(());
    };
    if prepared.is_some() {
        return Ok(());
    }
    state.publish(
        &app,
        UpdateStatus {
            phase: Phase::Checking,
            ..Default::default()
        },
    );
    match download_latest(&app, &state).await {
        Ok(Some(update)) => {
            *prepared = Some(update);
            let mut status = state.status.lock().clone();
            status.phase = Phase::Ready;
            state.publish(&app, status);
            Ok(())
        }
        Ok(None) => {
            state.publish(&app, UpdateStatus::default());
            Ok(())
        }
        Err(error) => {
            log::warn!("desktop update check/download failed: {error:#}");
            let mut status = state.status.lock().clone();
            status.phase = Phase::Error;
            status.error = Some(format!("{error:#}"));
            state.publish(&app, status);
            Err(format!("{error:#}"))
        }
    }
}

async fn download_latest(
    app: &AppHandle,
    state: &DesktopUpdater,
) -> Result<Option<PreparedUpdate>> {
    // Use a separate unauthenticated client: never forward Drop server credentials to GitHub.
    let releases: Vec<Release> = reqwest::Client::builder()
        .user_agent(concat!("Drop-Desktop/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(30))
        .build()?
        .get(RELEASES_API)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let Some(release) = newest_release(&releases) else {
        return Ok(None);
    };
    let current_tag = RELEASE_TAG
        .map(str::to_owned)
        .unwrap_or_else(|| format!("v{}", app.package_info().version));
    if release.tag_name == current_tag {
        return Ok(None);
    }
    let manifest = release
        .assets
        .iter()
        .find(|asset| asset.name == "latest.json")
        .context("Release has no updater manifest")?;
    let tag = release.tag_name.clone();
    let exit_app = app.clone();
    let update = app
        .updater_builder()
        .pubkey(PUBLIC_KEY.context("Missing updater public key")?.trim())
        .endpoints(vec![manifest.browser_download_url.parse()?])?
        .timeout(Duration::from_secs(30))
        .version_comparator(move |current, remote| {
            should_offer_update(&current, &remote.version, &tag, &current_tag)
        })
        .on_before_exit(move || {
            // The Windows installer exits the process itself, bypassing quit().
            tauri::async_runtime::block_on(finish_downloads());
            exit_app.cleanup_before_exit();
        })
        .build()?
        .check()
        .await?;
    let Some(mut update) = update else {
        return Ok(None);
    };
    // Large installers need a longer timeout than the small metadata requests.
    update.timeout = Some(Duration::from_secs(30 * 60));
    let mut status = UpdateStatus {
        phase: Phase::Downloading,
        tag: Some(release.tag_name.clone()),
        ..Default::default()
    };
    state.publish(app, status.clone());
    let mut last_progress = Instant::now();
    let bytes = update
        .download(
            |chunk, total| {
                status.downloaded += chunk as u64;
                status.total = total;
                if last_progress.elapsed() >= Duration::from_millis(250) {
                    state.publish(app, status.clone());
                    last_progress = Instant::now();
                }
            },
            || {},
        )
        .await
        .context("Could not download or verify desktop update")?;
    let mut payload = NamedTempFile::new().context("Could not create update cache")?;
    payload
        .write_all(&bytes)
        .context("Could not cache desktop update")?;
    status.downloaded = bytes.len() as u64;
    state.publish(app, status);
    Ok(Some(PreparedUpdate { update, payload }))
}

#[tauri::command]
pub async fn install_desktop_update(app: AppHandle) -> Result<(), String> {
    let state = app.state::<DesktopUpdater>();
    let mut prepared = state
        .operation
        .try_lock()
        .map_err(|_| "An update operation is already running")?;
    let ready = prepared
        .as_ref()
        .ok_or("No verified desktop update is ready")?;
    let mut status = state.status.lock().clone();
    status.phase = Phase::Installing;
    status.error = None;
    state.publish(&app, status.clone());
    let update = ready.update.clone();
    let path = ready.payload.path().to_owned();
    // Installer work can block (including a package-manager authorization prompt).
    // Tauri's updater verifies the signature again before installing.
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<()> {
        let bytes = std::fs::read(path).context("Could not read cached update")?;
        update
            .install(bytes)
            .context("Could not install desktop update")?;
        Ok(())
    })
    .await;
    match result {
        Ok(Ok(())) => {
            // Windows exits through the updater and its installer relaunches Drop.
            // On Linux/macOS we restart only after installation succeeds.
            finish_downloads().await;
            let _ = prepared.take();
            app.restart();
        }
        result => {
            let error = match result {
                Ok(Err(error)) => format!("{error:#}"),
                Err(error) => error.to_string(),
                Ok(Ok(())) => unreachable!(),
            };
            log::warn!("desktop update installation failed: {error}");
            status.phase = Phase::Ready;
            status.error = Some(error.clone());
            state.publish(&app, status);
            Err(error)
        }
    }
}

async fn finish_downloads() {
    if let Err(error) = ::download_manager::DOWNLOAD_MANAGER
        .ensure_terminated()
        .await
    {
        log::warn!("could not finish downloads before desktop update restart: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, date: &str, draft: bool, manifest: bool) -> Release {
        Release {
            tag_name: tag.into(),
            draft,
            published_at: Some(date.into()),
            assets: if manifest {
                vec![Asset {
                    name: "latest.json".into(),
                    browser_download_url: "https://github.com/test/latest.json".into(),
                }]
            } else {
                vec![]
            },
        }
    }

    #[test]
    fn selects_newest_published_desktop_release_not_server_or_draft() {
        let releases = vec![
            release("server", "2026-11-05T00:00:00Z", false, false),
            release("draft", "2026-11-06T00:00:00Z", true, true),
            release("v0.4.5-build.9", "2026-11-04T00:00:00Z", false, true),
            release("v0.4.6", "2026-11-03T00:00:00Z", false, true),
        ];
        assert_eq!(
            newest_release(&releases).unwrap().tag_name,
            "v0.4.5-build.9"
        );
        assert!(newest_release(&[]).is_none());
        let mut unpublished = release("unpublished", "2026-11-07T00:00:00Z", false, true);
        unpublished.published_at = None;
        assert!(newest_release(&[unpublished]).is_none());
    }

    #[test]
    fn offers_new_versions_and_rebuilds_but_not_installed_tags_or_downgrades() {
        let current = Version::parse("0.4.5").unwrap();
        assert!(should_offer_update(
            &current,
            &Version::parse("0.4.6").unwrap(),
            "v0.4.6",
            "v0.4.5"
        ));
        assert!(should_offer_update(
            &current,
            &current,
            "v0.4.5-build.2",
            "v0.4.5-build.1"
        ));
        assert!(!should_offer_update(
            &current,
            &current,
            "v0.4.5-build.2",
            "v0.4.5-build.2"
        ));
        assert!(!should_offer_update(
            &current,
            &Version::parse("0.4.4").unwrap(),
            "v0.4.4",
            "v0.4.5"
        ));
    }

    #[test]
    fn status_wire_format_matches_the_header_widget() {
        let status = UpdateStatus {
            phase: Phase::Ready,
            tag: Some("v0.4.5-build.2".into()),
            downloaded: 123,
            total: Some(123),
            error: None,
        };
        assert_eq!(
            serde_json::to_value(status).unwrap(),
            serde_json::json!({
                "phase": "ready", "tag": "v0.4.5-build.2",
                "downloaded": 123, "total": 123, "error": null
            })
        );
    }
}
