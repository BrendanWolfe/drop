use std::sync::{Arc, LazyLock};

use database::{borrow_db_checked, borrow_db_mut_checked};
pub use proxy_auth::redact;
pub use proxy_auth::{AuthStore, Credentials, HeaderInput, ProxyAuthError, server_origin};
use proxy_auth::{
    resolve_headers,
    vault::{CredentialVault, OsCredentialVault, PersistenceState, commit_credentials},
};
use serde::{Deserialize, Serialize};
use url::Url;

pub static PROXY_AUTH: LazyLock<Arc<AuthStore>> = LazyLock::new(|| Arc::new(AuthStore::default()));
pub static CONNECTION_UPDATE_LOCK: LazyLock<tauri::async_runtime::Mutex<()>> =
    LazyLock::new(|| tauri::async_runtime::Mutex::new(()));

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyAuthInput {
    pub enabled: bool,
    pub remember: bool,
    pub headers: Vec<HeaderInput>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProxyAuthStatus {
    pub enabled: bool,
    pub remember: bool,
    pub headers: Vec<HeaderStatus>,
    pub cleanup_pending: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeaderStatus {
    name: String,
    has_value: bool,
}

pub struct PreparedAuth {
    pub credentials: Option<Credentials>,
    remember: bool,
}

/// Called once before any startup/background network work. Never silently downgrade a failed load.
pub fn initialize() {
    // Tombstones are separate from enabled settings: old credentials must never be reactivated.
    let pending = borrow_db_checked().proxy_auth_cleanup.clone();
    for origin in pending {
        if let Ok(url) = Url::parse(&origin) {
            if OsCredentialVault.delete(&url).is_ok() {
                borrow_db_mut_checked().proxy_auth_cleanup.remove(&origin);
            }
        }
    }
    let (url, configured) = {
        let db = borrow_db_checked();
        let Ok(url) = Url::parse(&db.base_url) else {
            return;
        };
        let Ok(origin) = server_origin(&url) else {
            return;
        };
        let configured = db.proxy_auth.contains_key(&origin);
        (url, configured)
    };
    if configured {
        let _ = PROXY_AUTH.activate(&url, OsCredentialVault.load(&url));
    }
}

pub fn status(url: &Url) -> Result<ProxyAuthStatus, ProxyAuthError> {
    let origin = server_origin(url)?;
    let saved_names = borrow_db_checked().proxy_auth.get(&origin).cloned();
    let credentials = PROXY_AUTH.credentials_for(url).ok().flatten();
    let enabled = saved_names.is_some() || credentials.is_some();
    let remember = saved_names.is_some() || !enabled;
    let names = credentials
        .as_ref()
        .map(Credentials::names)
        .or(saved_names)
        .unwrap_or_else(|| vec!["P-Access-Token-Id".into(), "P-Access-Token".into()]);
    Ok(ProxyAuthStatus {
        enabled,
        remember,
        cleanup_pending: borrow_db_checked().proxy_auth_cleanup.contains(&origin),
        headers: names
            .into_iter()
            .map(|name| HeaderStatus {
                has_value: credentials.as_ref().is_some_and(|c| {
                    c.secrets()
                        .iter()
                        .any(|h| h.name.eq_ignore_ascii_case(&name))
                }),
                name,
            })
            .collect(),
    })
}

pub fn prepare(url: &Url, input: Option<ProxyAuthInput>) -> Result<PreparedAuth, ProxyAuthError> {
    server_origin(url)?;
    let Some(input) = input.filter(|input| input.enabled) else {
        return Ok(PreparedAuth {
            credentials: None,
            remember: false,
        });
    };
    let needs_saved = input.headers.iter().any(|h| h.value.is_none());
    let saved = if needs_saved {
        PROXY_AUTH.credentials_for(url)?.or_else(|| {
            let remembered = borrow_db_checked()
                .proxy_auth
                .contains_key(&server_origin(url).ok()?);
            remembered
                .then(|| OsCredentialVault.load(url).ok())
                .flatten()
        })
    } else {
        None
    };
    let headers = resolve_headers(
        input.headers,
        saved.as_ref().map(Credentials::secrets).unwrap_or(&[]),
    )?;
    Ok(PreparedAuth {
        credentials: Some(Credentials::new(url, headers)?),
        remember: input.remember,
    })
}

/// Commit only after a successful healthcheck. No token values are serialized to the database.
pub fn commit(url: &Url, prepared: PreparedAuth) -> Result<(), ProxyAuthError> {
    let origin = server_origin(url)?;
    let mut persistence = {
        let db = borrow_db_checked();
        PersistenceState {
            remembered_names: db.proxy_auth.get(&origin).cloned(),
            cleanup_pending: db.proxy_auth_cleanup.contains(&origin),
        }
    };
    commit_credentials(
        &OsCredentialVault,
        &PROXY_AUTH,
        url,
        prepared.credentials,
        prepared.remember,
        &mut persistence,
    )?;
    {
        let mut db = borrow_db_mut_checked();
        if let Some(names) = persistence.remembered_names {
            db.proxy_auth.insert(origin.clone(), names);
        } else {
            db.proxy_auth.remove(&origin);
        }
        if persistence.cleanup_pending {
            db.proxy_auth_cleanup.insert(origin);
        } else {
            db.proxy_auth_cleanup.remove(&origin);
        }
    }
    Ok(())
}
