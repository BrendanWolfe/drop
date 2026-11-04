use std::{
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use http::{Extensions, HeaderMap, Method, StatusCode, header::*};
use reqwest::{Client, Request, Response, redirect::Policy};
use reqwest_middleware::{Middleware, Next, Result};
use reqwest_websocket::RequestBuilderExt;
use url::Url;

use crate::{AuthStore, ProxyAuthError, check_status, redact, target_origin};

/// The authenticated transport owns redirects so every destination is checked before sending.
/// Ordinary, unconfigured connections still use reqwest's built-in redirect handling.
pub fn protected_redirect_policy() -> Policy {
    Policy::none()
}

pub enum TransportError {
    Http(reqwest::Error),
    WebSocket(Box<reqwest_websocket::Error>),
    Proxy(ProxyAuthError),
}
impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Http(e) => e.to_string(),
            Self::WebSocket(e) => e.to_string(),
            Self::Proxy(e) => e.to_string(),
        };
        f.write_str(&redact(&message))
    }
}
impl fmt::Debug for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}
impl std::error::Error for TransportError {}
impl From<reqwest::Error> for TransportError {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e)
    }
}
impl From<reqwest_websocket::Error> for TransportError {
    fn from(e: reqwest_websocket::Error) -> Self {
        Self::WebSocket(Box::new(e))
    }
}
impl From<ProxyAuthError> for TransportError {
    fn from(e: ProxyAuthError) -> Self {
        Self::Proxy(e)
    }
}

/// Capture the original request budget once, not the timeout on a replayed request.
/// Supplying zero when exhausted lets reqwest retain its normal timeout error type.
struct RequestDeadline(Option<Instant>);

impl RequestDeadline {
    fn new(timeout: Option<Duration>) -> Self {
        Self(timeout.map(|timeout| Instant::now() + timeout))
    }

    fn remaining(&self) -> Option<Duration> {
        self.0
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }
}

fn drops_body(status: StatusCode) -> bool {
    matches!(status.as_u16(), 301..=303)
}

fn destination(
    status: StatusCode,
    headers: &HeaderMap,
    current: &Url,
    had_tokens: bool,
    hops: usize,
) -> std::result::Result<Option<Url>, ProxyAuthError> {
    if !matches!(status.as_u16(), 301 | 302 | 303 | 307 | 308) {
        return Ok(None);
    }
    let Some(location) = headers.get(LOCATION).and_then(|v| v.to_str().ok()) else {
        return Ok(None);
    };
    let Ok(url) = current.join(location) else {
        return Ok(None);
    };
    if hops >= 9 {
        return Err(ProxyAuthError::RedirectLimit);
    }
    if !matches!(url.scheme(), "http" | "https" | "ws" | "wss")
        || !url.username().is_empty()
        || url.password().is_some()
        || (had_tokens && target_origin(current) != target_origin(&url))
    {
        return Err(ProxyAuthError::UnsafeRedirect);
    }
    Ok(Some(url))
}

fn redirect_headers(
    headers: &mut HeaderMap,
    old: &Url,
    new: &Url,
    status: StatusCode,
    method: &mut Method,
) {
    if target_origin(old) != target_origin(new) {
        for name in [AUTHORIZATION, COOKIE, PROXY_AUTHORIZATION, HOST, REFERER] {
            headers.remove(name);
        }
    }
    if drops_body(status) {
        for name in [
            TRANSFER_ENCODING,
            CONTENT_ENCODING,
            CONTENT_TYPE,
            CONTENT_LENGTH,
        ] {
            headers.remove(name);
        }
        if *method != Method::GET && *method != Method::HEAD {
            *method = Method::GET;
        }
    }
}

pub struct ProxyAuthMiddleware {
    store: Arc<AuthStore>,
    protected: Client,
}

/// Drop must be allowed to observe and renew an expired Store Bearer token.
#[derive(Clone)]
pub struct AllowAuthenticationRejection;

impl ProxyAuthMiddleware {
    pub fn new(store: Arc<AuthStore>, protected: Client) -> Self {
        Self { store, protected }
    }
}

#[async_trait::async_trait]
impl Middleware for ProxyAuthMiddleware {
    async fn handle(
        &self,
        mut req: Request,
        extensions: &mut Extensions,
        next: Next<'_>,
    ) -> Result<Response> {
        let deadline = RequestDeadline::new(req.timeout().copied());
        // A snapshot prevents a connection switch in flight from changing the trust boundary.
        let store = self.store.snapshot();
        if !store.is_configured() {
            return next.run(req, extensions).await;
        }
        for hops in 0..10 {
            let url = req.url().clone();
            let headers = store
                .headers_for(&url)
                .map_err(reqwest_middleware::Error::middleware)?;
            let had_tokens = headers.is_some();
            if let Some(headers) = headers {
                req.headers_mut().extend(headers);
            }
            let replay = req.try_clone();
            let mut bodyless = Request::new(req.method().clone(), url.clone());
            *bodyless.headers_mut() = req.headers().clone();
            // Each execute starts a new reqwest timer; only give it the unspent budget.
            // That timer also bounds reading the final response body.
            *req.timeout_mut() = deadline.remaining();
            let response = self.protected.execute(req).await?;
            let status = response.status();
            let redirect = destination(status, response.headers(), &url, had_tokens, hops)
                .map_err(reqwest_middleware::Error::middleware)?;
            if let Some(target) = redirect {
                req = match replay {
                    Some(req) => req,
                    None if drops_body(status) => bodyless,
                    None => return Ok(response),
                };
                let mut method = req.method().clone();
                redirect_headers(req.headers_mut(), &url, &target, status, &mut method);
                *req.method_mut() = method;
                if drops_body(status) {
                    *req.body_mut() = None;
                }
                *req.url_mut() = target;
                continue;
            }
            if had_tokens && extensions.get::<AllowAuthenticationRejection>().is_none() {
                check_status(status).map_err(reqwest_middleware::Error::middleware)?;
            }
            return Ok(response);
        }
        Err(reqwest_middleware::Error::middleware(
            ProxyAuthError::RedirectLimit,
        ))
    }
}

/// Match reqwest's blocking default without leaving the redirect handler unaware of it.
pub const DEFAULT_BLOCKING_TIMEOUT: Duration = Duration::from_secs(30);

/// Keep the effective client timeout alongside reqwest, which does not expose it after building.
#[derive(Clone)]
pub struct BlockingClient {
    inner: reqwest::blocking::Client,
    timeout: Option<Duration>,
}

impl BlockingClient {
    /// Apply the same default to reqwest and redirect deadlines. `None` disables the timeout.
    pub fn new(
        builder: reqwest::blocking::ClientBuilder,
        timeout: Option<Duration>,
    ) -> std::result::Result<Self, reqwest::Error> {
        Ok(Self {
            inner: builder.timeout(timeout).build()?,
            timeout,
        })
    }
}

pub struct BlockingRequestBuilder {
    builder: reqwest::blocking::RequestBuilder,
    client: BlockingClient,
    store: AuthStore,
}
impl BlockingRequestBuilder {
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.builder = self.builder.timeout(timeout);
        self
    }
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.builder = self.builder.header(name.as_ref(), value.as_ref());
        self
    }
    pub fn json<T: serde::Serialize + ?Sized>(mut self, body: &T) -> Self {
        self.builder = self.builder.json(body);
        self
    }
    pub fn build(self) -> std::result::Result<reqwest::blocking::Request, reqwest::Error> {
        self.builder.build()
    }
    pub fn send(self) -> std::result::Result<reqwest::blocking::Response, TransportError> {
        if !self.store.is_configured() {
            return Ok(self.builder.send()?);
        }
        let mut req = self.builder.build()?;
        let deadline = RequestDeadline::new(req.timeout().copied().or(self.client.timeout));
        for hops in 0..10 {
            let url = req.url().clone();
            let headers = self.store.headers_for(&url)?;
            let had_tokens = headers.is_some();
            if let Some(headers) = headers {
                req.headers_mut().extend(headers);
            }
            let replay = req.try_clone();
            let mut bodyless = reqwest::blocking::Request::new(req.method().clone(), url.clone());
            *bodyless.headers_mut() = req.headers().clone();
            *req.timeout_mut() = deadline.remaining();
            let response = self.client.inner.execute(req)?;
            let status = response.status();
            if let Some(target) = destination(status, response.headers(), &url, had_tokens, hops)? {
                req = match replay {
                    Some(req) => req,
                    None if drops_body(status) => bodyless,
                    None => return Ok(response),
                };
                let mut method = req.method().clone();
                redirect_headers(req.headers_mut(), &url, &target, status, &mut method);
                *req.method_mut() = method;
                if drops_body(status) {
                    *req.body_mut() = None;
                }
                *req.url_mut() = target;
                continue;
            }
            if had_tokens {
                check_status(status)?;
            }
            return Ok(response);
        }
        Err(ProxyAuthError::RedirectLimit.into())
    }
}

pub fn blocking_request(
    normal: &BlockingClient,
    protected: &BlockingClient,
    store: &AuthStore,
    method: Method,
    url: Url,
) -> std::result::Result<BlockingRequestBuilder, ProxyAuthError> {
    let store = store.snapshot();
    let client = if store.is_configured() {
        protected
    } else {
        normal
    }
    .clone();
    let mut builder = client.inner.request(method, url.clone());
    if let Some(headers) = store.headers_for(&url)? {
        builder = builder.headers(headers);
    }
    Ok(BlockingRequestBuilder {
        builder,
        client,
        store,
    })
}

pub struct WebsocketRequestBuilder {
    builder: reqwest::RequestBuilder,
    client: Client,
    store: AuthStore,
}
impl WebsocketRequestBuilder {
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.builder = self.builder.timeout(timeout);
        self
    }
    pub fn header(mut self, name: impl AsRef<str>, value: impl AsRef<str>) -> Self {
        self.builder = self.builder.header(name.as_ref(), value.as_ref());
        self
    }
    pub fn build(self) -> std::result::Result<Request, reqwest::Error> {
        self.builder.build()
    }
    pub fn upgrade(self) -> Self {
        self
    }
    pub async fn send(
        self,
    ) -> std::result::Result<reqwest_websocket::UpgradeResponse, TransportError> {
        if !self.store.is_configured() {
            return Ok(self.builder.upgrade().send().await?);
        }
        let mut req = self.builder.build()?;
        let deadline = RequestDeadline::new(req.timeout().copied());
        for hops in 0..10 {
            let url = req.url().clone();
            let headers = self.store.headers_for(&url)?;
            let had_tokens = headers.is_some();
            if let Some(headers) = headers {
                req.headers_mut().extend(headers);
            }
            let replay = req.try_clone();
            *req.timeout_mut() = deadline.remaining();
            let response = reqwest::RequestBuilder::from_parts(self.client.clone(), req)
                .upgrade()
                .send()
                .await?;
            let status = response.status();
            if let Some(target) = destination(status, response.headers(), &url, had_tokens, hops)? {
                let Some(mut next) = replay else {
                    return Ok(response);
                };
                let mut method = next.method().clone();
                redirect_headers(next.headers_mut(), &url, &target, status, &mut method);
                *next.method_mut() = method;
                *next.url_mut() = target;
                req = next;
                continue;
            }
            if had_tokens {
                check_status(status)?;
            }
            return Ok(response);
        }
        Err(ProxyAuthError::RedirectLimit.into())
    }
}

pub fn websocket_request(
    normal: &Client,
    protected: &Client,
    store: &AuthStore,
    url: Url,
) -> std::result::Result<WebsocketRequestBuilder, ProxyAuthError> {
    let store = store.snapshot();
    let client = if store.is_configured() {
        protected
    } else {
        normal
    }
    .clone();
    let mut builder = client.get(url.clone());
    if let Some(headers) = store.headers_for(&url)? {
        builder = builder.headers(headers);
    }
    Ok(WebsocketRequestBuilder {
        builder,
        client,
        store,
    })
}
