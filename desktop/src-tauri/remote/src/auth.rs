use std::{
    collections::HashMap,
    env,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use client::{app_status::AppStatus, user::User};
use database::{DatabaseAuth, borrow_db_mut_checked, interface::borrow_db_checked};
use gethostname::gethostname;
use http::{HeaderValue, StatusCode};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use log::{error, warn};
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{
    error::{DropServerError, RemoteAccessError},
    requests::make_authenticated_get,
    utils::{DROP_CLIENT_ASYNC, DROP_CLIENT_SYNC},
};

use super::{
    cache::{cache_object, get_cached_object},
    requests::generate_url,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CapabilityConfiguration {}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InitiateRequestBody {
    name: String,
    platform: String,
    capabilities: HashMap<String, CapabilityConfiguration>,
    mode: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeRequestBody {
    client_id: String,
    token: String,
}

impl HandshakeRequestBody {
    pub fn new(client_id: String, token: String) -> Self {
        Self { client_id, token }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandshakeResponse {
    private: String,
    certificate: String,
    id: String,
}

impl From<HandshakeResponse> for DatabaseAuth {
    fn from(value: HandshakeResponse) -> Self {
        DatabaseAuth::new(value.private, value.certificate, value.id, None)
    }
}

#[derive(Serialize, Deserialize)]
struct Claims {
    exp: usize,
    nbf: usize,
}

pub fn generate_authorization_header() -> String {
    let certs = {
        let db = borrow_db_checked();
        db.auth.clone().expect("Authorisation not initialised")
    };

    sign_authorization_header(&certs)
}

fn sign_authorization_header(certs: &DatabaseAuth) -> String {
    let system_time: usize = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs() as usize;

    let claims = Claims {
        nbf: system_time,
        exp: system_time + 10,
    };

    let jwt = jsonwebtoken::encode(
        &Header::new(Algorithm::ES384),
        &claims,
        &EncodingKey::from_ec_pem(certs.private.as_bytes()).unwrap(),
    )
    .expect("failed to sign jwt");

    format!("JWT {} {}", certs.client_id, jwt)
}

pub async fn fetch_user() -> Result<User, RemoteAccessError> {
    let response = make_authenticated_get(generate_url(&["/api/v1/client/user"], &[])?).await?;
    if response.status() != 200 {
        let err: DropServerError = response.json().await?;
        warn!("{err:?}");

        if err.message == "Nonce expired" {
            return Err(RemoteAccessError::OutOfSync);
        }

        return Err(RemoteAccessError::InvalidResponse(err));
    }

    response
        .json::<User>()
        .await
        .map_err(std::convert::Into::into)
}

pub async fn ensure_web_token() -> Result<(), RemoteAccessError> {
    let (auth, base_url) = {
        let db = borrow_db_checked();
        let auth = db.auth.as_ref().ok_or_else(|| {
            RemoteAccessError::HandshakeFailed("Sign in to open the Store".into())
        })?;
        (auth.clone(), db.base_url.clone())
    };
    let server_url = Url::parse(&base_url)?;

    if let Some(token) = auth.web_token.as_deref().filter(|token| !token.is_empty()) {
        if let Ok(header) = HeaderValue::from_str(&format!("Bearer {token}")) {
            let response = DROP_CLIENT_ASYNC
                .get(server_url.join("/api/v1/user")?)
                .header("Authorization", header)
                .timeout(Duration::from_secs(15))
                .send()
                .await?;

            if web_token_is_valid(response).await? {
                return Ok(());
            }
        }
    }

    // Native client credentials remain valid when a separate Store token is missing or revoked.
    let response = DROP_CLIENT_ASYNC
        .post(server_url.join("/api/v1/client/user/webtoken")?)
        .header("Authorization", sign_authorization_header(&auth))
        .timeout(Duration::from_secs(15))
        .send()
        .await?;
    let token = parse_web_token(response).await?;

    let mut db = borrow_db_mut_checked();
    if db.base_url != base_url {
        return Err(RemoteAccessError::CorruptedState);
    }
    let current_auth = db
        .auth
        .as_mut()
        .filter(|current_auth| current_auth.client_id == auth.client_id)
        .ok_or(RemoteAccessError::CorruptedState)?;
    current_auth.web_token = Some(token);
    Ok(())
}

async fn web_token_is_valid(response: reqwest::Response) -> Result<bool, RemoteAccessError> {
    match response.status() {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Ok(false),
        _ => Ok(response
            .error_for_status()?
            .json::<Option<User>>()
            .await?
            .is_some()),
    }
}

async fn parse_web_token(response: reqwest::Response) -> Result<String, RemoteAccessError> {
    let token = response.error_for_status()?.text().await?;
    if token.is_empty()
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-._~+/=".contains(&byte))
    {
        return Err(RemoteAccessError::UnparseableResponse(
            "Invalid Store access token".into(),
        ));
    }
    Ok(token)
}

pub fn auth_initiate_logic(mode: String) -> Result<String, RemoteAccessError> {
    let base_url = {
        let db_lock = borrow_db_checked();
        Url::parse(&db_lock.base_url.clone())?
    };

    let hostname = gethostname();

    let endpoint = base_url.join("/api/v1/client/auth/initiate")?;
    let body = InitiateRequestBody {
        name: format!("{} (Desktop)", hostname.display()),
        platform: env::consts::OS.to_string(),
        capabilities: HashMap::from([
            ("peerAPI".to_owned(), CapabilityConfiguration {}),
            ("cloudSaves".to_owned(), CapabilityConfiguration {}),
            ("trackPlaytime".to_owned(), CapabilityConfiguration {}),
        ]),
        mode,
    };

    let client = DROP_CLIENT_SYNC.clone();
    let response = client.post(endpoint.to_string()).json(&body).send()?;

    if response.status() != 200 {
        let data: DropServerError = response.json()?;
        error!("could not start handshake: {:?}", data);

        return Err(RemoteAccessError::HandshakeFailed(data.message));
    }

    let response = response.text()?;

    Ok(response)
}

pub async fn setup() -> (AppStatus, Option<User>) {
    let auth = {
        let data = borrow_db_checked();
        data.auth.clone()
    };

    if auth.is_some() {
        let user_result = match fetch_user().await {
            Ok(data) => data,
            Err(RemoteAccessError::FetchError(_)) => {
                let user = get_cached_object::<User>("user").ok();
                return (AppStatus::Offline, user);
            }
            Err(_) => return (AppStatus::SignedInNeedsReauth, None),
        };
        if let Err(e) = cache_object("user", &user_result) {
            warn!("Could not cache user object with error {e}");
        }
        return (AppStatus::SignedIn, Some(user_result));
    }

    (AppStatus::SignedOut, None)
}

#[cfg(test)]
mod tests {
    use super::{parse_web_token, web_token_is_valid};

    #[test]
    fn accepts_web_token_with_authenticated_user() {
        let response = http::Response::builder()
            .status(200)
            .body(r#"{"id":"user-id","username":"user","admin":false,"displayName":"User","profilePictureObjectId":"picture-id"}"#)
            .unwrap();
        assert!(tauri::async_runtime::block_on(web_token_is_valid(response.into())).unwrap());
    }

    #[test]
    fn marks_missing_user_and_rejected_web_tokens_invalid() {
        // The user endpoint returns 200 null for an invalid bearer token.
        for status in [200, 401, 403] {
            let response = http::Response::builder()
                .status(status)
                .body("null")
                .unwrap();
            assert!(!tauri::async_runtime::block_on(web_token_is_valid(response.into())).unwrap());
        }
    }

    #[test]
    fn surfaces_validation_errors_without_renewing_web_token() {
        for (status, body) in [(500, "server-error"), (200, "<html>Error</html>")] {
            let response = http::Response::builder().status(status).body(body).unwrap();
            assert!(tauri::async_runtime::block_on(web_token_is_valid(response.into())).is_err());
        }
    }

    #[test]
    fn accepts_successful_web_token_response() {
        let token = "83ed70d9-51ad-4bef-80bf-8f8891dcfb5f";
        let response = http::Response::builder().status(200).body(token).unwrap();
        let parsed = tauri::async_runtime::block_on(parse_web_token(response.into())).unwrap();
        assert_eq!(parsed, token);
    }

    #[test]
    fn rejects_failed_web_token_responses() {
        for status in [401, 403, 500] {
            let response = http::Response::builder()
                .status(status)
                .body("server-error")
                .unwrap();
            assert!(tauri::async_runtime::block_on(parse_web_token(response.into())).is_err());
        }
    }

    #[test]
    fn rejects_empty_or_malformed_web_tokens() {
        for body in [
            "",
            " ",
            "token\n",
            "<html>Error</html>",
            "{\"statusCode\":403}",
        ] {
            let response = http::Response::builder().status(200).body(body).unwrap();
            assert!(tauri::async_runtime::block_on(parse_web_token(response.into())).is_err());
        }
    }
}
