//! Origin-scoped proxy credentials. Secrets never belong in the application database or URLs.
use std::{fmt, sync::RwLock};

use http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use serde::{Deserialize, Serialize};
use url::Url;

pub mod redaction;
pub mod transport;
pub mod vault;
pub use redaction::{Redactor, redact};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProxyAuthError {
    InvalidOrigin,
    InvalidHeaders,
    StorageUnavailable,
    MissingCredentials,
    Rejected,
    UnsafeRedirect,
    RedirectLimit,
}

impl fmt::Display for ProxyAuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidOrigin => "Use an HTTP or HTTPS server address without credentials, a query, or a fragment.",
            Self::InvalidHeaders => "Enter unique custom header names and non-empty values. Authentication, cookie, routing, and transport headers are reserved.",
            Self::StorageUnavailable => "OS credential storage is unavailable or locked. Unlock it, or turn off Remember tokens to use session-only authentication.",
            Self::MissingCredentials => "Saved proxy tokens could not be loaded. Re-enter them in Server settings.",
            Self::Rejected => "Server access was denied (HTTP 401/403). Check the reverse proxy tokens and your Drop account permissions.",
            Self::UnsafeRedirect => "Unsafe redirect blocked for a reverse proxy authenticated request.",
            Self::RedirectLimit => "Too many HTTP redirects.",
        })
    }
}
impl std::error::Error for ProxyAuthError {}

/// No Debug implementation: values must not appear in diagnostics.
#[derive(Clone, Serialize, Deserialize)]
pub struct SecretHeader {
    pub name: String,
    pub value: String,
}

/// A missing value means retain the saved value for this header, not an empty token.
#[derive(Deserialize)]
pub struct HeaderInput {
    pub name: String,
    pub value: Option<String>,
}

pub fn server_origin(url: &Url) -> Result<String, ProxyAuthError> {
    if !matches!(url.scheme(), "http" | "https")
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ProxyAuthError::InvalidOrigin);
    }
    Ok(url.origin().ascii_serialization())
}

fn target_origin(url: &Url) -> String {
    let mut url = url.clone();
    // WebSocket upgrades have the same security boundary as their HTTP origin.
    if url.scheme() == "ws" {
        let _ = url.set_scheme("http");
    } else if url.scheme() == "wss" {
        let _ = url.set_scheme("https");
    }
    url.origin().ascii_serialization()
}

fn reserved(name: &str) -> bool {
    matches!(
        name,
        "authorization"
            | "proxy-authorization"
            | "cookie"
            | "set-cookie"
            | "host"
            | "origin"
            | "referer"
            | "connection"
            | "upgrade"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "content-length"
            | "content-type"
            | "accept-encoding"
            | "user-agent"
            | "range"
            | "if-range"
            | "forwarded"
            | "proxy-connection"
    ) || name.starts_with("sec-")
        || name.starts_with("x-forwarded-")
}

pub fn validate_headers(headers: &[SecretHeader]) -> Result<HeaderMap, ProxyAuthError> {
    if headers.is_empty() || headers.len() > 16 {
        return Err(ProxyAuthError::InvalidHeaders);
    }
    let mut result = HeaderMap::new();
    for header in headers {
        let name = HeaderName::from_bytes(header.name.as_bytes())
            .map_err(|_| ProxyAuthError::InvalidHeaders)?;
        if reserved(name.as_str()) || result.contains_key(&name) || header.value.is_empty() {
            return Err(ProxyAuthError::InvalidHeaders);
        }
        let mut value =
            HeaderValue::from_str(&header.value).map_err(|_| ProxyAuthError::InvalidHeaders)?;
        value.set_sensitive(true);
        result.insert(name, value);
    }
    Ok(result)
}

pub fn resolve_headers(
    inputs: Vec<HeaderInput>,
    saved: &[SecretHeader],
) -> Result<Vec<SecretHeader>, ProxyAuthError> {
    let headers = inputs
        .into_iter()
        .map(|input| {
            let value = input
                .value
                .or_else(|| {
                    saved
                        .iter()
                        .find(|old| old.name.eq_ignore_ascii_case(&input.name))
                        .map(|old| old.value.clone())
                })
                .ok_or(ProxyAuthError::MissingCredentials)?;
            Ok(SecretHeader {
                name: input.name,
                value,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_headers(&headers)?;
    Ok(headers)
}

#[derive(Clone)]
pub struct Credentials {
    origin: String,
    headers: Vec<SecretHeader>,
    parsed: HeaderMap,
}

impl Credentials {
    pub fn new(url: &Url, headers: Vec<SecretHeader>) -> Result<Self, ProxyAuthError> {
        let origin = server_origin(url)?;
        let parsed = validate_headers(&headers)?;
        redaction::register(&headers);
        Ok(Self {
            origin,
            parsed,
            headers,
        })
    }
    pub fn names(&self) -> Vec<String> {
        self.headers.iter().map(|h| h.name.clone()).collect()
    }
    pub fn secrets(&self) -> &[SecretHeader] {
        &self.headers
    }
}

#[derive(Clone)]
struct ActiveAuth {
    origin: String,
    credentials: Result<Credentials, ProxyAuthError>,
}

/// Only the active connection can receive credentials. Failed loads fail closed for that origin.
#[derive(Default)]
pub struct AuthStore(RwLock<Option<ActiveAuth>>);

impl AuthStore {
    pub fn snapshot(&self) -> Self {
        Self(RwLock::new(self.0.read().unwrap().clone()))
    }
    pub fn is_configured(&self) -> bool {
        self.0.read().unwrap().is_some()
    }
    pub fn activate(
        &self,
        url: &Url,
        credentials: Result<Credentials, ProxyAuthError>,
    ) -> Result<(), ProxyAuthError> {
        let origin = server_origin(url)?;
        if credentials
            .as_ref()
            .is_ok_and(|credentials| credentials.origin != origin)
        {
            return Err(ProxyAuthError::InvalidOrigin);
        }
        *self.0.write().unwrap() = Some(ActiveAuth {
            origin,
            credentials,
        });
        Ok(())
    }
    pub fn clear(&self) {
        *self.0.write().unwrap() = None;
    }

    pub fn credentials_for(&self, url: &Url) -> Result<Option<Credentials>, ProxyAuthError> {
        let lock = self.0.read().unwrap();
        match lock
            .as_ref()
            .filter(|auth| auth.origin == target_origin(url))
        {
            Some(auth) => auth.credentials.clone().map(Some),
            None => Ok(None),
        }
    }
    pub fn headers_for(&self, url: &Url) -> Result<Option<HeaderMap>, ProxyAuthError> {
        Ok(self
            .credentials_for(url)?
            .map(|credentials| credentials.parsed))
    }
}

pub fn check_status(status: StatusCode) -> Result<(), ProxyAuthError> {
    if matches!(status, StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN) {
        return Err(ProxyAuthError::Rejected);
    }
    Ok(())
}
