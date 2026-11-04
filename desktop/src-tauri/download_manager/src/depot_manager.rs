use std::{
    collections::HashMap,
    env,
    sync::RwLock,
    time::{Duration, Instant},
    usize,
};

use futures_util::StreamExt;
use log::{info, warn};
use remote::{
    error::RemoteAccessError, requests::make_authenticated_get, utils::DROP_CLIENT_ASYNC,
};
use serde::Deserialize;
use tauri::Url;

use crate::util::semaphore::{SyncSemaphore, SyncSemaphorePermit};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DepotManifestContent {
    version_id: String,
    //compression: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DepotManifest {
    content: HashMap<String, Vec<DepotManifestContent>>,
}

struct Depot {
    endpoint: String,
    manifest: Option<DepotManifest>,
    latest_speed: Option<usize>, // bytes per second
    current_downloads: SyncSemaphore,
    enabled: bool,
}

pub struct DepotManager {
    depots: RwLock<Vec<Depot>>,
}

#[derive(Deserialize)]
struct ServersideDepot {
    endpoint: String,
}

const SPEEDTEST_TIMEOUT: Duration = Duration::from_secs(4);

fn resolve_depot_endpoint(server_url: &Url, endpoint: &str) -> Result<Url, RemoteAccessError> {
    let mut endpoint = server_url.join(endpoint)?;
    if !matches!(endpoint.scheme(), "http" | "https")
        || endpoint.host_str().is_none()
        || !endpoint.username().is_empty()
        || endpoint.password().is_some()
        || endpoint.query().is_some()
        || endpoint.fragment().is_some()
    {
        return Err(RemoteAccessError::InvalidEndpoint);
    }
    // The endpoint is a directory, even when an external depot omits its trailing slash.
    if !endpoint.path().ends_with('/') {
        endpoint.set_path(&format!("{}/", endpoint.path()));
    }
    Ok(endpoint)
}

impl Default for DepotManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DepotManager {
    pub fn new() -> Self {
        Self {
            depots: RwLock::new(Vec::new()),
        }
    }

    async fn sync_depot(
        &self,
        depot: &mut Depot,
        server_url: &Url,
    ) -> Result<(), RemoteAccessError> {
        let endpoint = resolve_depot_endpoint(server_url, &depot.endpoint)?;
        // Persist the resolved URL so manifest probes, speed tests, and chunk downloads agree.
        depot.endpoint = endpoint.to_string();
        let manifest_url = endpoint.join("manifest.json")?;
        let manifest = DROP_CLIENT_ASYNC.get(manifest_url).send().await?;
        let manifest: DepotManifest = manifest.json().await?;
        depot.manifest.replace(manifest);

        let speedtest_url = endpoint.join("speedtest")?;
        let speedtest = DROP_CLIENT_ASYNC.get(speedtest_url).send().await?;

        let mut stream = speedtest.bytes_stream();
        let start = Instant::now();
        let mut total_length = 0;

        while let Some(chunk) = stream.next().await {
            let length = chunk?.len();
            total_length += length;
            if SPEEDTEST_TIMEOUT <= start.elapsed() {
                break;
            }
        }

        let elapsed = start.elapsed().as_millis() as usize;
        let speed = total_length.checked_div(elapsed).unwrap_or(usize::MAX);
        depot.latest_speed.replace(speed);

        Ok(())
    }

    pub async fn sync_depots(&self) -> Result<(), RemoteAccessError> {
        let server_url = database::DB.fetch_base_url();
        let depots = make_authenticated_get(server_url.join("/api/v1/client/depots")?).await?;
        let depots: Vec<ServersideDepot> = depots.json().await?;

        let mut new_depots = depots
            .into_iter()
            .map(|depot| Depot {
                endpoint: depot.endpoint,
                manifest: None,
                latest_speed: None,
                current_downloads: SyncSemaphore::new(),
                enabled: true,
            })
            .collect::<Vec<Depot>>();

        info!("syncing {} depots...", new_depots.len());

        for depot in &mut new_depots {
            if let Err(sync_error) = self.sync_depot(depot, &server_url).await {
                warn!("failed to sync depot {}: {:?}", depot.endpoint, sync_error);
                if env::var("FORCE_ENABLE_DEPOTS")
                    .map(|v| !v.is_empty())
                    .unwrap_or(false)
                {
                } else {
                    depot.enabled = false;
                }
            }
        }

        let enabled = new_depots.iter().filter(|v| v.enabled).count();
        if enabled == 0 {
            return Err(RemoteAccessError::NoDepots);
        }

        let mut depot_lock = self.depots.write().unwrap();
        *depot_lock = new_depots;

        Ok(())
    }

    pub fn next_depot(
        &self,
        game_id: &str,
        version_id: &str,
    ) -> Result<(String, SyncSemaphorePermit), RemoteAccessError> {
        let lock = self.depots.read().unwrap();
        let best_depot = lock
            .iter()
            .filter(|v| {
                let manifest = match &v.manifest {
                    Some(v) => v,
                    None => return false,
                };

                let versions = match manifest.content.get(game_id) {
                    Some(v) => v,
                    None => return false,
                };

                let _version = match versions.iter().find(|v| v.version_id == version_id) {
                    Some(v) => v,
                    None => return false,
                };

                true
            })
            .max_by(|x, y| {
                let x_speed = x.latest_speed.unwrap_or(0) / x.current_downloads.permits();
                let y_speed = y.latest_speed.unwrap_or(0) / y.current_downloads.permits();
                x_speed.cmp(&y_speed)
            })
            .ok_or(RemoteAccessError::NoDepots)?;

        Ok((
            best_depot.endpoint.clone(),
            best_depot.current_downloads.acquire(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_depot_uses_the_configured_public_or_lan_server() {
        for server in [
            "https://drop.example.com/",
            "http://192.168.1.50:3000/",
            "http://[::1]:3000/",
        ] {
            let server = Url::parse(server).unwrap();
            let endpoint = resolve_depot_endpoint(&server, "/api/v1/depot/").unwrap();
            assert_eq!(endpoint.origin(), server.origin());
            for path in ["manifest.json", "speedtest", "content/game/version/chunk"] {
                let request = endpoint.join(path).unwrap();
                assert_eq!(request.origin(), server.origin());
                assert_eq!(request.path(), format!("/api/v1/depot/{path}"));
            }
        }
    }

    #[test]
    fn root_relative_depot_does_not_inherit_the_server_path() {
        let server = Url::parse("https://drop.example.com/base/").unwrap();
        let endpoint = resolve_depot_endpoint(&server, "/api/v1/depot").unwrap();
        assert_eq!(endpoint.as_str(), "https://drop.example.com/api/v1/depot/");
    }

    #[test]
    fn path_relative_depot_uses_the_configured_base_path() {
        let server = Url::parse("https://drop.example.com/base/").unwrap();
        let endpoint = resolve_depot_endpoint(&server, "depot/").unwrap();
        assert_eq!(endpoint.as_str(), "https://drop.example.com/base/depot/");
    }

    #[test]
    fn absolute_external_depots_keep_their_own_origin_and_path() {
        let server = Url::parse("https://drop.example.com/").unwrap();
        for address in [
            "https://cdn.example.com:8443/games",
            "https://cdn.example.com:8443/games/",
        ] {
            let endpoint = resolve_depot_endpoint(&server, address).unwrap();
            assert_eq!(endpoint.as_str(), "https://cdn.example.com:8443/games/");
            assert_ne!(endpoint.origin(), server.origin());
            assert_eq!(
                endpoint
                    .join("content/game/version/chunk")
                    .unwrap()
                    .as_str(),
                "https://cdn.example.com:8443/games/content/game/version/chunk"
            );
        }
    }

    #[test]
    fn resolving_encoded_depot_paths_does_not_double_encode_them() {
        let server = Url::parse("https://drop.example.com/").unwrap();
        let endpoint = resolve_depot_endpoint(&server, "/depot%20files").unwrap();
        assert_eq!(endpoint.as_str(), "https://drop.example.com/depot%20files/");
    }

    #[test]
    fn invalid_depot_urls_are_rejected_before_network_requests() {
        let server = Url::parse("https://drop.example.com/").unwrap();
        for address in [
            "file:///tmp/depot/",
            "ftp://cdn.example.com/",
            "https://user:secret@cdn.example.com/",
            "/api/v1/depot/?token=secret",
            "/api/v1/depot/#secret",
        ] {
            assert!(resolve_depot_endpoint(&server, address).is_err());
        }
    }
}
