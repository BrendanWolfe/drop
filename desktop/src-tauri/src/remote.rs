use std::{
    sync::{Arc, nonpoison::Mutex},
    time::Duration,
};

use client::app_status::AppStatus;
use database::{borrow_db_checked, borrow_db_mut_checked};
use futures_lite::StreamExt;
use log::{debug, warn};
use remote::{
    auth::{auth_initiate_logic, ensure_web_token, generate_authorization_header},
    cache::{cache_object, get_cached_object},
    error::RemoteAccessError,
    proxy::{
        self, AuthStore, CONNECTION_UPDATE_LOCK, PreparedAuth, ProxyAuthInput, ProxyAuthStatus,
    },
    requests::generate_url,
    setup,
    utils::{DropHealthcheck, blocking_request, get_client_async_for, websocket_request},
};
use reqwest_websocket::Message;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use url::Url;
use utils::{app_emit, webbrowser_open::webbrowser_open};

use crate::{AppState, recieve_handshake};

#[tauri::command]
pub async fn prepare_store() -> Result<(), RemoteAccessError> {
    ensure_web_token().await
}

#[tauri::command]
pub async fn use_remote(
    url: String,
    proxy_auth: Option<ProxyAuthInput>,
    state: tauri::State<'_, Mutex<AppState>>,
) -> Result<(), RemoteAccessError> {
    let _update = CONNECTION_UPDATE_LOCK.lock().await;
    let base_url = Url::parse(&url)?;
    // Validate before logging or sending: credentials must never be embedded in a URL.
    proxy::server_origin(&base_url)?;
    let prepared = proxy::prepare(&base_url, proxy_auth)?;
    test_connection(&base_url, &prepared).await?;
    proxy::commit(&base_url, prepared)?;

    let mut db_state = borrow_db_mut_checked();
    if Url::parse(&db_state.base_url).ok().map(|u| u.origin()) != Some(base_url.origin()) {
        db_state.auth = None;
    }
    db_state.base_url = base_url.to_string();
    drop(db_state);

    let mut app_state = state.lock();
    app_state.status = AppStatus::SignedOut;
    app_state.user = None;
    Ok(())
}

async fn test_connection(base_url: &Url, prepared: &PreparedAuth) -> Result<(), RemoteAccessError> {
    let candidate = Arc::new(AuthStore::default());
    if let Some(credentials) = &prepared.credentials {
        candidate.activate(base_url, Ok(credentials.clone()))?;
    }
    // Test Drop url
    let test_endpoint = base_url.join("/api/v1")?;
    let client = get_client_async_for(candidate);
    let response = client
        .get(test_endpoint.to_string())
        .timeout(Duration::from_secs(3))
        .send()
        .await?;

    if matches!(response.status().as_u16(), 401 | 403) {
        return Err(RemoteAccessError::ConnectionDenied);
    }
    let response = response.error_for_status()?;

    let result: DropHealthcheck = response.json().await?;

    if result.app_name() != "Drop" {
        warn!("user entered drop endpoint that connected, but wasn't identified as Drop");
        return Err(RemoteAccessError::InvalidEndpoint);
    }

    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionSettings {
    url: String,
    proxy_auth: Option<ProxyAuthStatus>,
}

#[tauri::command]
pub fn fetch_connection_settings() -> Result<ConnectionSettings, RemoteAccessError> {
    let url = borrow_db_checked().base_url.clone();
    let proxy_auth = Url::parse(&url)
        .ok()
        .map(|url| proxy::status(&url))
        .transpose()?;
    Ok(ConnectionSettings { url, proxy_auth })
}

#[tauri::command]
pub async fn update_proxy_auth(proxy_auth: ProxyAuthInput) -> Result<(), RemoteAccessError> {
    let _update = CONNECTION_UPDATE_LOCK.lock().await;
    let url = Url::parse(&borrow_db_checked().base_url)?;
    let prepared = proxy::prepare(&url, Some(proxy_auth))?;
    // Turning authentication off must remain possible even if the proxy/server is unavailable.
    if prepared.credentials.is_some() {
        test_connection(&url, &prepared).await?;
    }
    proxy::commit(&url, prepared)?;
    Ok(())
}

#[tauri::command]
pub fn gen_drop_url(path: String) -> Result<String, RemoteAccessError> {
    let base_url = {
        let handle = borrow_db_checked();

        Url::parse(&handle.base_url).map_err(RemoteAccessError::ParsingError)?
    };

    let url = base_url.join(&path)?;

    Ok(url.to_string())
}

#[tauri::command]
pub fn fetch_drop_object(path: String) -> Result<Vec<u8>, RemoteAccessError> {
    let _drop_url = gen_drop_url(path.clone())?;
    let req = generate_url(&[&path], &[])?;
    let req = blocking_request(http::Method::GET, req)?
        .header("Authorization", generate_authorization_header()?)
        .send();

    match req {
        Ok(data) => {
            let data = data.bytes()?.to_vec();
            cache_object(&path, &data)?;
            Ok(data)
        }
        Err(e) => {
            let e = RemoteAccessError::from(e);
            if matches!(&e, RemoteAccessError::ProxyAuth(_)) {
                return Err(e);
            }
            debug!("{e}");
            get_cached_object::<Vec<u8>>(&path)
        }
    }
}
#[tauri::command]
pub fn sign_out(app: AppHandle) {
    // Clear auth from database
    {
        let mut handle = borrow_db_mut_checked();
        handle.auth = None;
    }

    // Update app state
    {
        let state = app.state::<Mutex<AppState>>();
        let mut app_state_handle = state.lock();
        app_state_handle.status = AppStatus::SignedOut;
        app_state_handle.user = None;
    }

    // Emit event for frontend
    app_emit!(&app, "auth/signedout", ());
}

#[tauri::command]
pub async fn retry_connect(state: tauri::State<'_, Mutex<AppState>>) -> Result<(), ()> {
    let (app_status, user) = setup().await;

    let mut guard = state.lock();
    guard.status = app_status;
    guard.user = user;
    drop(guard);

    Ok(())
}

#[tauri::command]
pub fn auth_initiate() -> Result<(), RemoteAccessError> {
    let base_url = {
        let db_lock = borrow_db_checked();
        Url::parse(&db_lock.base_url.clone())?
    };

    let redir_url = auth_initiate_logic("callback".to_string())?;
    let complete_redir_url = base_url.join(&redir_url)?;

    debug!("opening web browser to continue authentication");
    webbrowser_open(complete_redir_url.as_ref());
    Ok(())
}

#[derive(Deserialize)]
struct CodeWebsocketResponse {
    #[serde(rename = "type")]
    response_type: String,
    value: String,
}

#[tauri::command]
pub fn auth_initiate_code(app: AppHandle) -> Result<String, RemoteAccessError> {
    let base_url = {
        let db_lock = borrow_db_checked();
        Url::parse(&db_lock.base_url.clone())?.clone()
    };

    let code = auth_initiate_logic("code".to_string())?;
    let header_code = code.clone();

    println!("using code: {code} to sign in");

    tauri::async_runtime::spawn(async move {
        let load = async || -> Result<(), RemoteAccessError> {
            let ws_url = base_url.join("/api/v1/client/auth/code/ws")?;
            let response = websocket_request(ws_url)?
                .header("Authorization", header_code)
                .upgrade()
                .send()
                .await?;

            let mut websocket = response.into_websocket().await?;

            while let Some(token) = websocket.try_next().await? {
                if let Message::Text(response) = token {
                    let response = serde_json::from_str::<CodeWebsocketResponse>(&response)
                        .map_err(|e| RemoteAccessError::UnparseableResponse(e.to_string()))?;
                    match response.response_type.as_str() {
                        "token" => {
                            let recieve_app = app.clone();
                            manual_recieve_handshake(recieve_app, response.value).await;
                            return Ok(());
                        }
                        _ => return Err(RemoteAccessError::HandshakeFailed(response.value)),
                    }
                }
            }
            Err(RemoteAccessError::HandshakeFailed(
                "Failed to connect to websocket".to_string(),
            ))
        };

        let result = load().await;
        if let Err(err) = result {
            warn!("{err}");
            app_emit!(&app, "auth/failed", err.to_string());
        }
    });

    Ok(code)
}

#[tauri::command]
pub async fn manual_recieve_handshake(app: AppHandle, token: String) {
    recieve_handshake(app, format!("handshake/{token}")).await;
}
