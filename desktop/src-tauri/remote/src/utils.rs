use std::{
    fs::{self, File},
    io::Read,
    sync::{Arc, LazyLock},
    time::Duration,
};

use client::{app_state::AppState, app_status::AppStatus};
use database::db::DATA_ROOT_DIR;
use http::Extensions;
use log::{debug, info, warn};
use proxy_auth::{
    AuthStore,
    transport::{
        BlockingClient, DEFAULT_BLOCKING_TIMEOUT, ProxyAuthMiddleware, protected_redirect_policy,
    },
};
use reqwest::Certificate;
use reqwest_middleware::{
    ClientBuilder, ClientWithMiddleware, Error, Middleware, Next, Result,
    reqwest::{Request, Response},
};
use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, async_runtime::Mutex};
use url::Url;

use crate::{error::RemoteAccessError, proxy::PROXY_AUTH};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DropHealthcheck {
    app_name: String,
}
impl DropHealthcheck {
    pub fn app_name(&self) -> &String {
        &self.app_name
    }
}
static DROP_CERT_BUNDLE: LazyLock<Vec<Certificate>> = LazyLock::new(fetch_certificates);
static DROP_CLIENT_SYNC: LazyLock<BlockingClient> = LazyLock::new(|| get_client_sync(false));
static PROTECTED_CLIENT_SYNC: LazyLock<BlockingClient> = LazyLock::new(|| get_client_sync(true));
pub static DROP_CLIENT_ASYNC: LazyLock<ClientWithMiddleware> = LazyLock::new(get_client_async);
static DROP_CLIENT_WS_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| get_client_ws(false));
static PROTECTED_CLIENT_WS: LazyLock<reqwest::Client> = LazyLock::new(|| get_client_ws(true));

pub static DROP_APP_HANDLE: LazyLock<Mutex<Option<AppHandle>>> = LazyLock::new(|| Mutex::new(None));

struct AutoOfflineMiddleware;

#[async_trait::async_trait]
impl Middleware for AutoOfflineMiddleware {
    async fn handle(
        &self,
        req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        let url = req.url().clone();
        let res = next.run(req, extensions).await;
        match res {
            Ok(res) => {
                tauri::async_runtime::spawn(async move {
                    let lock = DROP_APP_HANDLE.lock().await;
                    if let Some(app_handle) = &*lock {
                        let state = app_handle.state::<std::sync::nonpoison::Mutex<AppState>>();
                        let state_lock = state.try_lock();
                        if let Ok(mut state_lock) = state_lock {
                            if state_lock.status == AppStatus::Offline {
                                state_lock.status = AppStatus::SignedIn;
                                app_handle
                                    .emit("update_state", &*state_lock)
                                    .expect("failed to emit state update");
                            }
                        } else {
                            warn!("failed to lock app state - {}", url.as_str());
                        }
                    };
                });

                Ok(res)
            }
            Err(err) => match err {
                Error::Middleware(error) => Err(Error::Middleware(error)),
                Error::Reqwest(error) => {
                    if error.is_connect() {
                        // Spawn to defer this action - the state will most likely be locked
                        tauri::async_runtime::spawn(async move {
                            let lock = DROP_APP_HANDLE.lock().await;
                            if let Some(app_handle) = &*lock {
                                let state =
                                    app_handle.state::<std::sync::nonpoison::Mutex<AppState>>();
                                let mut state_lock = state.lock();
                                state_lock.status = AppStatus::Offline;
                                app_handle
                                    .emit("update_state", &*state_lock)
                                    .expect("failed to emit state update");
                            };
                        });
                    };
                    Err(Error::Reqwest(error))
                }
            },
        }
    }
}

fn fetch_certificates() -> Vec<Certificate> {
    let certificate_dir = DATA_ROOT_DIR.join("certificates");

    let mut certs = Vec::new();
    match fs::read_dir(certificate_dir) {
        Ok(c) => {
            for entry in c {
                match entry {
                    Ok(c) => {
                        let mut buf = Vec::new();
                        match File::open(c.path()) {
                            Ok(f) => f,
                            Err(e) => {
                                warn!(
                                    "Failed to open file at {} with error {}",
                                    c.path().display(),
                                    e
                                );
                                continue;
                            }
                        }
                        .read_to_end(&mut buf)
                        .unwrap_or_else(|e| {
                            panic!(
                                "Failed to read to end of certificate file {} with error {}",
                                c.path().display(),
                                e
                            )
                        });

                        match Certificate::from_pem_bundle(&buf) {
                            Ok(certificates) => {
                                for cert in certificates {
                                    certs.push(cert);
                                }
                                info!(
                                    "added {} certificate(s) from {}",
                                    certs.len(),
                                    c.file_name().display()
                                );
                            }
                            Err(e) => warn!(
                                "Invalid certificate file {} with error {}",
                                c.path().display(),
                                e
                            ),
                        }
                    }
                    Err(_) => todo!(),
                }
            }
        }
        Err(e) => {
            debug!("not loading certificates due to error: {e}");
        }
    };
    certs
}

fn get_client_sync(protected: bool) -> BlockingClient {
    let mut client = reqwest::blocking::ClientBuilder::new();
    if protected {
        client = client.redirect(protected_redirect_policy());
    }

    for cert in DROP_CERT_BUNDLE.iter() {
        client = client.add_root_certificate(cert.clone());
    }
    BlockingClient::new(
        client
            .use_rustls_tls()
            .user_agent("Drop Desktop Client")
            .connect_timeout(Duration::from_millis(1500)),
        Some(DEFAULT_BLOCKING_TIMEOUT),
    )
    .expect("Failed to build synchronous client")
}
pub fn get_client_async() -> ClientWithMiddleware {
    build_client_async(PROXY_AUTH.clone(), true)
}

pub fn get_client_async_for(store: Arc<AuthStore>) -> ClientWithMiddleware {
    // Connection probes must not change the active connection's online/offline state.
    build_client_async(store, false)
}

fn build_client_async(store: Arc<AuthStore>, auto_offline: bool) -> ClientWithMiddleware {
    let mut client = reqwest::ClientBuilder::new();

    for cert in DROP_CERT_BUNDLE.iter() {
        client = client.add_root_certificate(cert.clone());
    }
    let normal_client = client
        .use_rustls_tls()
        .user_agent("Drop Desktop Client")
        .build()
        .expect("Failed to build asynchronous client");

    let mut protected = reqwest::ClientBuilder::new();
    for cert in DROP_CERT_BUNDLE.iter() {
        protected = protected.add_root_certificate(cert.clone());
    }
    let protected = protected
        .use_rustls_tls()
        .user_agent("Drop Desktop Client")
        .redirect(protected_redirect_policy())
        .build()
        .expect("Failed to build protected asynchronous client");

    let mut builder = ClientBuilder::new(normal_client);
    if auto_offline {
        builder = builder.with(AutoOfflineMiddleware);
    }
    builder
        .with(ProxyAuthMiddleware::new(store, protected))
        .build()
}
fn get_client_ws(protected: bool) -> reqwest::Client {
    let mut client = reqwest::ClientBuilder::new();
    if protected {
        client = client.redirect(protected_redirect_policy());
    }

    for cert in DROP_CERT_BUNDLE.iter() {
        client = client.add_root_certificate(cert.clone());
    }
    client
        .use_rustls_tls()
        .user_agent("Drop Desktop Client")
        .http1_only()
        .build()
        .expect("Failed to build websocket client")
}

pub fn blocking_request(
    method: http::Method,
    url: Url,
) -> std::result::Result<proxy_auth::transport::BlockingRequestBuilder, RemoteAccessError> {
    Ok(proxy_auth::transport::blocking_request(
        &DROP_CLIENT_SYNC,
        &PROTECTED_CLIENT_SYNC,
        &PROXY_AUTH,
        method,
        url,
    )?)
}

pub fn websocket_request(
    url: Url,
) -> std::result::Result<proxy_auth::transport::WebsocketRequestBuilder, RemoteAccessError> {
    Ok(proxy_auth::transport::websocket_request(
        &DROP_CLIENT_WS_CLIENT,
        &PROTECTED_CLIENT_WS,
        &PROXY_AUTH,
        url,
    )?)
}
