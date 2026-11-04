use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use http::{Method, StatusCode};
use proxy_auth::transport::{
    AllowAuthenticationRejection, BlockingClient, DEFAULT_BLOCKING_TIMEOUT, ProxyAuthMiddleware,
    blocking_request, protected_redirect_policy, websocket_request,
};
use proxy_auth::{
    AuthStore, Credentials, HeaderInput, ProxyAuthError, SecretHeader, check_status,
    resolve_headers, server_origin, validate_headers,
};
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use url::Url;

fn secrets() -> Vec<SecretHeader> {
    vec![
        SecretHeader {
            name: "P-Access-Token-Id".into(),
            value: "private-id".into(),
        },
        SecretHeader {
            name: "P-Access-Token".into(),
            value: "private-token".into(),
        },
    ]
}

fn store(url: &Url) -> Arc<AuthStore> {
    let store = Arc::new(AuthStore::default());
    store
        .activate(url, Ok(Credentials::new(url, secrets()).unwrap()))
        .unwrap();
    store
}

fn client(store: Arc<AuthStore>) -> ClientWithMiddleware {
    let normal = reqwest::Client::builder().no_proxy().build().unwrap();
    let protected = reqwest::Client::builder()
        .no_proxy()
        .redirect(protected_redirect_policy())
        .build()
        .unwrap();
    ClientBuilder::new(normal)
        .with(ProxyAuthMiddleware::new(store, protected))
        .build()
}

fn blocking_client(protected: bool, timeout: Option<Duration>) -> BlockingClient {
    let mut builder = reqwest::blocking::Client::builder().no_proxy();
    if protected {
        builder = builder.redirect(protected_redirect_policy());
    }
    BlockingClient::new(builder, timeout).unwrap()
}

/// Tiny local HTTP server: bounded waits keep security regression failures from hanging tests.
fn server(responses: Vec<String>) -> (Url, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    server_with_delays(
        responses
            .into_iter()
            .map(|response| (Duration::ZERO, response))
            .collect(),
    )
}

fn server_with_delays(
    responses: Vec<(Duration, String)>,
) -> (Url, mpsc::Receiver<String>, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    listener.set_nonblocking(true).unwrap();
    let (sender, receiver) = mpsc::channel();
    let handle = thread::spawn(move || {
        for (delay, response) in responses {
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(_) => return,
                }
            };
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut data = Vec::new();
            let mut buf = [0; 1024];
            while !data.windows(4).any(|w| w == b"\r\n\r\n") {
                let read = stream.read(&mut buf).unwrap();
                if read == 0 {
                    break;
                }
                data.extend_from_slice(&buf[..read]);
            }
            sender.send(String::from_utf8(data).unwrap()).unwrap();
            thread::sleep(delay);
            // A timed-out client may close the socket before the delayed response is written.
            if let Err(error) = stream.write_all(response.as_bytes()) {
                assert!(matches!(
                    error.kind(),
                    std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                ));
            }
        }
    });
    (url, receiver, handle)
}

fn response(status: u16, extra: &str) -> String {
    format!("HTTP/1.1 {status} Test\r\nContent-Length: 0\r\nConnection: close\r\n{extra}\r\n")
}

fn delayed_redirect_responses() -> Vec<(Duration, String)> {
    // Each hop is below 500 ms, but together they exceed that total budget.
    vec![
        (
            Duration::from_millis(300),
            response(302, "Location: /final\r\n"),
        ),
        (Duration::from_millis(350), response(200, "")),
    ]
}

fn assert_both_hops_received_tokens(requests: mpsc::Receiver<String>) {
    let first = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    let second = requests.recv_timeout(Duration::from_secs(2)).unwrap();
    assert!(first.contains("p-access-token: private-token"));
    assert!(second.starts_with("GET /final "));
    assert!(second.contains("p-access-token: private-token"));
}

#[tokio::test]
async fn redirect_chain_shares_the_original_request_timeout() {
    let (url, requests, handle) = server_with_delays(delayed_redirect_responses());
    let error = client(store(&url))
        .get(url)
        .timeout(Duration::from_millis(500))
        .send()
        .await
        .unwrap_err();
    assert!(error.is_timeout(), "expected a timeout, got {error}");
    // This rules out a failure on the first hop. Resetting the timer would instead succeed.
    assert_both_hops_received_tokens(requests);
    handle.join().unwrap();
}

#[test]
fn blocking_redirect_chain_shares_the_original_request_timeout() {
    let normal = blocking_client(false, Some(DEFAULT_BLOCKING_TIMEOUT));
    let protected = blocking_client(true, Some(DEFAULT_BLOCKING_TIMEOUT));
    let (url, requests, handle) = server_with_delays(delayed_redirect_responses());
    let error = blocking_request(&normal, &protected, &store(&url), Method::GET, url)
        .unwrap()
        .timeout(Duration::from_millis(500))
        .send()
        .unwrap_err();
    assert!(
        matches!(error, proxy_auth::transport::TransportError::Http(ref error) if error.is_timeout()),
        "expected a timeout, got {error}"
    );
    assert_both_hops_received_tokens(requests);
    handle.join().unwrap();
}

#[test]
fn blocking_redirect_chain_shares_the_client_default_timeout() {
    let normal = blocking_client(false, Some(DEFAULT_BLOCKING_TIMEOUT));
    let protected = blocking_client(true, Some(Duration::from_millis(500)));
    let (url, requests, handle) = server_with_delays(delayed_redirect_responses());
    let result = blocking_request(&normal, &protected, &store(&url), Method::GET, url)
        .unwrap()
        .send();
    assert_both_hops_received_tokens(requests);
    handle.join().unwrap();
    let error = result.unwrap_err();
    assert!(
        matches!(error, proxy_auth::transport::TransportError::Http(ref error) if error.is_timeout()),
        "expected a timeout, got {error}"
    );
}

#[test]
fn blocking_request_timeout_overrides_the_client_default() {
    let normal = blocking_client(false, Some(DEFAULT_BLOCKING_TIMEOUT));
    let protected = blocking_client(true, Some(Duration::from_millis(200)));
    let (url, requests, handle) = server_with_delays(delayed_redirect_responses());
    let response = blocking_request(&normal, &protected, &store(&url), Method::GET, url)
        .unwrap()
        .timeout(Duration::from_secs(2))
        .send()
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_both_hops_received_tokens(requests);
    handle.join().unwrap();
}

#[test]
fn blocking_client_timeout_can_be_disabled() {
    let normal = blocking_client(false, Some(DEFAULT_BLOCKING_TIMEOUT));
    let protected = BlockingClient::new(
        reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_millis(1))
            .redirect(protected_redirect_policy()),
        None,
    )
    .unwrap();
    let (url, requests, handle) = server_with_delays(delayed_redirect_responses());
    let response = blocking_request(&normal, &protected, &store(&url), Method::GET, url)
        .unwrap()
        .send()
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_both_hops_received_tokens(requests);
    handle.join().unwrap();
}

#[test]
fn unconfigured_blocking_redirects_preserve_the_normal_client_timeout() {
    let normal = blocking_client(false, Some(Duration::from_millis(500)));
    let protected = blocking_client(true, Some(DEFAULT_BLOCKING_TIMEOUT));
    let (url, requests, handle) = server_with_delays(delayed_redirect_responses());
    let result = blocking_request(&normal, &protected, &AuthStore::default(), Method::GET, url)
        .unwrap()
        .send();
    for _ in 0..2 {
        assert!(
            !requests
                .recv_timeout(Duration::from_secs(2))
                .unwrap()
                .contains("p-access-token")
        );
    }
    handle.join().unwrap();
    let error = result.unwrap_err();
    assert!(
        matches!(error, proxy_auth::transport::TransportError::Http(ref error) if error.is_timeout()),
        "expected a timeout, got {error}"
    );
}

#[tokio::test]
async fn websocket_redirect_chain_shares_the_original_request_timeout() {
    let normal = reqwest::Client::builder().no_proxy().build().unwrap();
    let protected = reqwest::Client::builder()
        .no_proxy()
        .redirect(protected_redirect_policy())
        .build()
        .unwrap();
    let (url, requests, handle) = server_with_delays(delayed_redirect_responses());
    let error = websocket_request(&normal, &protected, &store(&url), url)
        .unwrap()
        .timeout(Duration::from_millis(500))
        .upgrade()
        .send()
        .await
        .err()
        .expect("expected a timeout");
    assert!(
        matches!(error, proxy_auth::transport::TransportError::WebSocket(ref error)
        if matches!(&**error, reqwest_websocket::Error::Reqwest(error) if error.is_timeout())),
        "expected a timeout, got {error}"
    );
    assert_both_hops_received_tokens(requests);
    handle.join().unwrap();
}

#[tokio::test]
async fn redirect_chain_can_finish_within_its_total_timeout() {
    let (url, requests, handle) = server_with_delays(vec![
        (
            Duration::from_millis(50),
            response(302, "Location: /final\r\n"),
        ),
        (Duration::from_millis(50), response(200, "")),
    ]);
    let response = client(store(&url))
        .get(url)
        .timeout(Duration::from_millis(500))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_both_hops_received_tokens(requests);
    handle.join().unwrap();
}

#[test]
fn validates_headers_and_redacts_values() {
    let headers = validate_headers(&secrets()).unwrap();
    assert!(headers["p-access-token"].is_sensitive());
    assert!(!format!("{headers:?}").contains("private-token"));
    for name in [
        "Authorization",
        "aUtHoRiZaTiOn",
        "Cookie",
        "Host",
        "Connection",
        "Proxy-Authorization",
        "Sec-WebSocket-Key",
        "X-Forwarded-Host",
        "invalid name",
    ] {
        assert!(
            validate_headers(&[SecretHeader {
                name: name.into(),
                value: "secret".into()
            }])
            .is_err()
        );
    }
    for value in ["", "token\r\nX-Injected: true", "token\n"] {
        assert!(
            validate_headers(&[SecretHeader {
                name: "X-Token".into(),
                value: value.into()
            }])
            .is_err()
        );
    }
    let mut duplicate = secrets();
    duplicate.push(SecretHeader {
        name: "p-access-token".into(),
        value: "other".into(),
    });
    assert!(validate_headers(&duplicate).is_err());
}

#[test]
fn isolates_origin_port_scheme_and_connection_switches() {
    let origin = Url::parse("https://drop.example:8443/base").unwrap();
    let store = store(&origin);
    for url in [
        "https://drop.example:8443/api/v1?q=1",
        "wss://drop.example:8443/ws",
    ] {
        assert!(
            store
                .headers_for(&Url::parse(url).unwrap())
                .unwrap()
                .is_some()
        );
    }
    for url in [
        "https://drop.example/api",
        "http://drop.example:8443/api",
        "https://cdn.example:8443/game",
        "https://drop.example.attacker.test:8443/",
    ] {
        assert!(
            store
                .headers_for(&Url::parse(url).unwrap())
                .unwrap()
                .is_none()
        );
    }
    let other = Url::parse("https://other.example").unwrap();
    assert!(
        store
            .activate(&other, Ok(Credentials::new(&origin, secrets()).unwrap()))
            .is_err()
    );
    store
        .activate(&other, Ok(Credentials::new(&other, secrets()).unwrap()))
        .unwrap();
    assert!(store.headers_for(&origin).unwrap().is_none());
    store.clear();
    assert!(store.headers_for(&other).unwrap().is_none());
}

#[test]
fn refuses_credentials_in_server_urls() {
    for address in [
        "https://user:secret@drop.example",
        "file:///tmp/server",
        "https://drop.example?token=secret",
        "https://drop.example#secret",
    ] {
        assert_eq!(
            server_origin(&Url::parse(address).unwrap()),
            Err(ProxyAuthError::InvalidOrigin)
        );
    }
    assert_eq!(
        server_origin(&Url::parse("https://DROP.example:443/path").unwrap()).unwrap(),
        "https://drop.example"
    );
}

#[test]
fn keeps_saved_values_only_for_matching_header_names() {
    let kept = resolve_headers(
        vec![HeaderInput {
            name: "p-access-token".into(),
            value: None,
        }],
        &secrets(),
    )
    .unwrap();
    assert_eq!(kept[0].value, "private-token");
    assert!(
        resolve_headers(
            vec![HeaderInput {
                name: "X-New-Name".into(),
                value: None
            }],
            &secrets()
        )
        .is_err()
    );
    assert!(
        resolve_headers(
            vec![HeaderInput {
                name: "p-access-token".into(),
                value: None
            }],
            &[]
        )
        .is_err()
    );
}

#[tokio::test]
async fn injects_into_api_store_metadata_and_download_requests_without_changing_drop_auth() {
    let (url, requests, handle) = server(vec![response(200, ""); 6]);
    let client = client(store(&url));
    for (path, authorization) in [
        ("/api/v1", None),
        ("/api/v1/client/auth/handshake", None),
        ("/api/v1/client/user", Some("JWT client-id signed-token")),
        ("/store", Some("Bearer store-token")),
        ("/manifest.json", Some("JWT client-id signed-token")),
        ("/download/game", Some("JWT client-id signed-token")),
    ] {
        let mut request = client.get(url.join(path).unwrap());
        if let Some(auth) = authorization {
            request = request.header("Authorization", auth);
        }
        assert_eq!(request.send().await.unwrap().status(), 200);
        let captured = requests
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .to_lowercase();
        assert!(captured.contains("p-access-token-id: private-id\r\n"));
        assert!(captured.contains("p-access-token: private-token\r\n"));
        if let Some(auth) = authorization {
            assert!(captured.contains(&format!("authorization: {}\r\n", auth.to_lowercase())));
        } else {
            assert!(!captured.contains("authorization:"));
        }
    }
    handle.join().unwrap();
}

#[tokio::test]
async fn external_depots_never_receive_proxy_credentials() {
    let (url, requests, handle) = server(vec![response(200, "")]);
    let client = client(store(&Url::parse("https://drop.example").unwrap()));
    client.get(url).send().await.unwrap();
    assert!(
        !requests
            .recv()
            .unwrap()
            .to_lowercase()
            .contains("p-access-token")
    );
    handle.join().unwrap();
}

#[tokio::test]
async fn external_redirect_into_server_attaches_tokens_only_at_the_protected_hop() {
    let (target, target_requests, target_handle) = server(vec![response(200, "")]);
    let (external, external_requests, external_handle) =
        server(vec![response(302, &format!("Location: {target}\r\n"))]);
    let client = client(store(&target));
    assert_eq!(
        client
            .get(external)
            .header("Authorization", "Bearer external-token")
            .send()
            .await
            .unwrap()
            .status(),
        200
    );
    let first = external_requests.recv().unwrap();
    assert!(!first.contains("p-access-token"));
    let protected = target_requests.recv().unwrap();
    assert!(protected.contains("p-access-token: private-token"));
    assert!(!protected.contains("authorization:"));
    external_handle.join().unwrap();
    target_handle.join().unwrap();
}

#[tokio::test]
async fn redirected_request_cannot_leave_the_server_after_receiving_tokens() {
    let destination = TcpListener::bind("127.0.0.1:0").unwrap();
    destination.set_nonblocking(true).unwrap();
    let (target, target_requests, target_handle) = server(vec![response(
        302,
        &format!(
            "Location: http://{}/\r\n",
            destination.local_addr().unwrap()
        ),
    )]);
    let (external, external_requests, external_handle) =
        server(vec![response(302, &format!("Location: {target}\r\n"))]);
    assert!(client(store(&target)).get(external).send().await.is_err());
    assert!(!external_requests.recv().unwrap().contains("p-access-token"));
    assert!(
        target_requests
            .recv()
            .unwrap()
            .contains("p-access-token: private-token")
    );
    assert!(destination.accept().is_err());
    external_handle.join().unwrap();
    target_handle.join().unwrap();
}

#[tokio::test]
async fn redirect_into_locked_credentials_fails_closed_before_sending() {
    let protected = TcpListener::bind("127.0.0.1:0").unwrap();
    protected.set_nonblocking(true).unwrap();
    let target = Url::parse(&format!("http://{}/", protected.local_addr().unwrap())).unwrap();
    let (external, external_requests, external_handle) =
        server(vec![response(302, &format!("Location: {target}\r\n"))]);
    let store = Arc::new(AuthStore::default());
    store
        .activate(&target, Err(ProxyAuthError::StorageUnavailable))
        .unwrap();
    assert!(client(store).get(external).send().await.is_err());
    assert!(!external_requests.recv().unwrap().contains("p-access-token"));
    assert!(protected.accept().is_err());
    external_handle.join().unwrap();
}

#[tokio::test]
async fn http_400_reflections_are_redacted_without_discarding_diagnostics() {
    let body = r#"{"message":"proxy rejected private-token", "P-Access-Token-Id":"private-id"}"#;
    let (url, _requests, handle) = server(vec![format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )]);
    let response = client(store(&url)).get(url).send().await.unwrap();
    assert_eq!(response.status(), 400);
    let formatted = proxy_auth::redact(&response.text().await.unwrap());
    assert!(formatted.contains("proxy rejected"));
    assert!(!formatted.contains("private-token"));
    assert!(!formatted.contains("private-id"));
    handle.join().unwrap();
}

#[tokio::test]
async fn follows_same_origin_redirect_with_tokens() {
    let (url, requests, handle) = server(vec![
        response(302, "Location: /final\r\n"),
        response(200, ""),
    ]);
    let client = client(store(&url));
    assert_eq!(client.get(url).send().await.unwrap().status(), 200);
    for _ in 0..2 {
        assert!(
            requests
                .recv()
                .unwrap()
                .contains("p-access-token: private-token")
        );
    }
    handle.join().unwrap();
}

#[tokio::test]
async fn blocks_cross_origin_redirects_before_sending_any_secrets() {
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let location = format!(
        "Location: http://{}/stolen\r\n",
        target.local_addr().unwrap()
    );
    let (url, requests, handle) = server(vec![response(302, &location)]);
    let client = client(store(&url));
    let error = client.get(url).send().await.unwrap_err();
    assert!(!error.to_string().contains("private-token"));
    assert!(
        requests
            .recv()
            .unwrap()
            .contains("p-access-token: private-token")
    );
    assert!(target.accept().is_err());
    handle.join().unwrap();
}

#[tokio::test]
async fn no_auth_preserves_cross_origin_redirects_and_http_errors() {
    let (target, target_requests, target_handle) = server(vec![response(403, "")]);
    let (url, requests, handle) = server(vec![response(302, &format!("Location: {target}\r\n"))]);
    let client = client(Arc::new(AuthStore::default()));
    assert_eq!(client.get(url).send().await.unwrap().status(), 403);
    assert!(!requests.recv().unwrap().contains("p-access-token"));
    assert!(!target_requests.recv().unwrap().contains("p-access-token"));
    handle.join().unwrap();
    target_handle.join().unwrap();
}

#[tokio::test]
async fn auth_rejections_are_actionable_and_do_not_expose_response_bodies() {
    for status in [401, 403] {
        let raw = format!(
            "HTTP/1.1 {status} Rejected\r\nContent-Length: 13\r\nConnection: close\r\n\r\nprivate-token"
        );
        let (url, _requests, handle) = server(vec![raw]);
        let client = client(store(&url));
        let error = client.get(url).send().await.unwrap_err();
        assert!(error.to_string().contains("401/403"));
        assert!(!format!("{error:?}").contains("private-token"));
        handle.join().unwrap();
    }
    assert!(check_status(StatusCode::UNAUTHORIZED).is_err());
    assert!(check_status(StatusCode::FORBIDDEN).is_err());
}

#[tokio::test]
async fn unavailable_credentials_fail_closed_but_do_not_block_external_requests() {
    let (url, requests, handle) = server(vec![response(200, "")]);
    let store = Arc::new(AuthStore::default());
    store
        .activate(&url, Err(ProxyAuthError::StorageUnavailable))
        .unwrap();
    let client = client(store.clone());
    assert!(
        client
            .get(url.clone())
            .send()
            .await
            .unwrap_err()
            .to_string()
            .contains("credential storage")
    );
    assert!(requests.try_recv().is_err());
    store.clear();
    assert_eq!(client.get(url).send().await.unwrap().status(), 200);
    handle.join().unwrap();
}

#[tokio::test]
async fn connection_failures_do_not_expose_credentials() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
    drop(listener);
    let error = client(store(&url)).get(url).send().await.unwrap_err();
    assert!(!format!("{error:?}").contains("private-token"));
}

#[test]
fn blocking_sign_in_and_object_requests_are_scoped_and_preserve_auth() {
    let normal = blocking_client(false, Some(DEFAULT_BLOCKING_TIMEOUT));
    let protected = blocking_client(true, Some(DEFAULT_BLOCKING_TIMEOUT));
    let (url, requests, handle) = server(vec![response(200, "")]);
    let store = store(&url);
    blocking_request(&normal, &protected, &store, Method::GET, url.clone())
        .unwrap()
        .header("Authorization", "JWT client signed-token")
        .send()
        .unwrap();
    let captured = requests.recv().unwrap();
    assert!(captured.contains("authorization: JWT client signed-token"));
    assert!(captured.contains("p-access-token: private-token"));
    let external = blocking_request(
        &normal,
        &protected,
        &store,
        Method::GET,
        Url::parse("https://external.example").unwrap(),
    )
    .unwrap()
    .build()
    .unwrap();
    assert!(!external.headers().contains_key("p-access-token"));
    handle.join().unwrap();
}

#[test]
fn blocking_redirects_cannot_escape_origin() {
    let normal = blocking_client(false, Some(DEFAULT_BLOCKING_TIMEOUT));
    let protected = blocking_client(true, Some(DEFAULT_BLOCKING_TIMEOUT));
    let target = TcpListener::bind("127.0.0.1:0").unwrap();
    target.set_nonblocking(true).unwrap();
    let (url, _requests, handle) = server(vec![response(
        302,
        &format!("Location: http://{}/\r\n", target.local_addr().unwrap()),
    )]);
    let store = store(&url);
    assert!(
        blocking_request(&normal, &protected, &store, Method::GET, url)
            .unwrap()
            .send()
            .is_err()
    );
    assert!(target.accept().is_err());
    handle.join().unwrap();
}

#[test]
fn blocking_redirect_into_server_is_authenticated_and_does_not_leak_to_external_hop() {
    let normal = blocking_client(false, Some(DEFAULT_BLOCKING_TIMEOUT));
    let protected = blocking_client(true, Some(DEFAULT_BLOCKING_TIMEOUT));
    let (target, target_requests, target_handle) = server(vec![response(200, "")]);
    let (external, external_requests, external_handle) =
        server(vec![response(302, &format!("Location: {target}\r\n"))]);
    blocking_request(&normal, &protected, &store(&target), Method::GET, external)
        .unwrap()
        .send()
        .unwrap();
    assert!(!external_requests.recv().unwrap().contains("p-access-token"));
    assert!(
        target_requests
            .recv()
            .unwrap()
            .contains("p-access-token: private-token")
    );
    external_handle.join().unwrap();
    target_handle.join().unwrap();
}

#[tokio::test]
async fn websocket_redirect_into_server_is_authenticated_before_upgrading() {
    let normal = reqwest::Client::builder().no_proxy().build().unwrap();
    let protected = reqwest::Client::builder()
        .no_proxy()
        .redirect(protected_redirect_policy())
        .build()
        .unwrap();
    let (target, target_requests, target_handle) = server(vec![response(401, "")]);
    let (external, external_requests, external_handle) =
        server(vec![response(302, &format!("Location: {target}\r\n"))]);
    let result = websocket_request(&normal, &protected, &store(&target), external)
        .unwrap()
        .upgrade()
        .send()
        .await;
    assert!(result.is_err());
    assert!(!external_requests.recv().unwrap().contains("p-access-token"));
    let captured = target_requests.recv().unwrap();
    assert!(captured.contains("p-access-token: private-token"));
    assert!(captured.contains("upgrade: websocket"));
    external_handle.join().unwrap();
    target_handle.join().unwrap();
}

#[tokio::test]
async fn websocket_handshake_builder_preserves_drop_code_and_scopes_proxy_tokens() {
    let normal = reqwest::Client::new();
    let protected = reqwest::Client::builder()
        .redirect(protected_redirect_policy())
        .build()
        .unwrap();
    let url = Url::parse("https://drop.example/api/v1/client/auth/code/ws").unwrap();
    let store = store(&url);
    let request = websocket_request(&normal, &protected, &store, url)
        .unwrap()
        .header("Authorization", "drop-login-code")
        .build()
        .unwrap();
    assert_eq!(request.headers()["authorization"], "drop-login-code");
    assert_eq!(request.headers()["p-access-token"], "private-token");
    let external = websocket_request(
        &normal,
        &protected,
        &store,
        Url::parse("https://external.example/ws").unwrap(),
    )
    .unwrap()
    .build()
    .unwrap();
    assert!(!external.headers().contains_key("p-access-token"));
}

#[tokio::test]
async fn websocket_upgrade_sends_proxy_tokens_alongside_the_login_code() {
    let normal = reqwest::Client::builder().no_proxy().build().unwrap();
    let protected = reqwest::Client::builder()
        .no_proxy()
        .redirect(protected_redirect_policy())
        .build()
        .unwrap();
    let (url, requests, handle) = server(vec![response(401, "")]);
    let store = store(&url);
    // The real upgrade path should preserve both authentication layers, including on rejection.
    let result = websocket_request(&normal, &protected, &store, url)
        .unwrap()
        .header("Authorization", "drop-login-code")
        .upgrade()
        .send()
        .await;
    let captured = requests.recv().unwrap().to_lowercase();
    assert!(captured.contains("upgrade: websocket"));
    assert!(captured.contains("authorization: drop-login-code"));
    assert!(captured.contains("p-access-token: private-token"));
    if let Err(error) = result {
        assert!(!format!("{error:?}").contains("private-token"));
    }
    handle.join().unwrap();
}

#[tokio::test]
async fn drop_bearer_validation_can_observe_rejection_and_renew_without_losing_proxy_auth() {
    let (url, requests, handle) = server(vec![response(401, ""), response(200, "")]);
    let client = client(store(&url));
    let rejected = client
        .get(url.join("/api/v1/user").unwrap())
        .header("Authorization", "Bearer expired-token")
        .with_extension(AllowAuthenticationRejection)
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), 401);
    let renewed = client
        .post(url.join("/api/v1/client/user/webtoken").unwrap())
        .header("Authorization", "JWT client signed-token")
        .send()
        .await
        .unwrap();
    assert_eq!(renewed.status(), 200);
    let validation = requests.recv().unwrap();
    assert!(validation.contains("authorization: Bearer expired-token"));
    assert!(validation.contains("p-access-token: private-token"));
    let renewal = requests.recv().unwrap();
    assert!(renewal.contains("authorization: JWT client signed-token"));
    assert!(renewal.contains("p-access-token: private-token"));
    handle.join().unwrap();
}
