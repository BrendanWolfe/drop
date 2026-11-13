//! Bounded, best-effort outbox. Reporting never blocks a download worker.
//! Entries are tied to the originating server/device, never sent after an account switch.
use database::{borrow_db_checked, db::DATA_ROOT_DIR};
use download_manager::frontend_updates::DownloadActivityEvent;
use remote::{auth::generate_authorization_header, proxy::PROXY_AUTH, utils::get_client_async_for};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Emitter, Listener};

const MAX_PENDING: usize = 256;

#[derive(Clone, Serialize, Deserialize)]
struct Pending {
    server: String,
    client_id: String,
    endpoint: String,
    body: Value,
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn clean(text: &str, limit: usize) -> String {
    // Do not upload credentials, signed URL queries, or local home paths.
    let text = remote::proxy::redact(text);
    let urls = regex::Regex::new(r"(https?://[^\s?#]+)[?#][^\s]*").unwrap();
    let credentials =
        regex::Regex::new(r"(?i)\b(password|token|secret|authorization|cookie)\s*[:=]\s*[^\s,;]+")
            .unwrap();
    let headers =
        regex::Regex::new(r"(?i)\b(Bearer|JWT|Basic)\s+[^\s,;]+(?:\s+[^\s,;]+)?").unwrap();
    let userinfo = regex::Regex::new(r"(https?://)[^\s/@]+:[^\s/@]+@").unwrap();
    let mut text = headers.replace_all(&text, "$1 [redacted]").into_owned();
    text = credentials.replace_all(&text, "$1=[redacted]").into_owned();
    text = userinfo.replace_all(&text, "$1[redacted]@").into_owned();
    text = urls.replace_all(&text, "$1?[redacted]").into_owned();
    if let Some(home) = dirs::home_dir() {
        text = text.replace(home.to_string_lossy().as_ref(), "[home]");
    }
    text.chars().take(limit).collect()
}

#[tauri::command]
pub fn report_client_error(
    app: AppHandle,
    source: String,
    message: String,
    details: Option<String>,
    game_id: Option<String>,
) {
    let source = match source.as_str() {
        "download" | "vue" | "javascript" | "unhandled-rejection" => source,
        _ => "client".to_owned(),
    };
    // Emit locally, then return immediately; transport failures are never UI errors.
    let mut body = json!({
        "reportId": uuid::Uuid::new_v4().to_string(), "source": source,
        "message": clean(&message, 1000), "details": clean(details.as_deref().unwrap_or(""), 6000),
        "clientVersion": env!("CARGO_PKG_VERSION"), "occurredAt": now(),
    });
    if let Some(game_id) = game_id {
        body["gameId"] = json!(game_id);
    }
    let _ = app.emit("client_error_report", body);
}

fn pending(endpoint: &str, body: Value) -> Option<Pending> {
    let db = borrow_db_checked();
    let auth = db.auth.as_ref()?;
    if db.base_url.is_empty() {
        return None;
    }
    Some(Pending {
        server: db.base_url.clone(),
        client_id: auth.client_id.clone(),
        endpoint: endpoint.to_owned(),
        body,
    })
}

pub fn start(app: &AppHandle) {
    let (sender, mut receiver) = tokio::sync::mpsc::channel::<Pending>(MAX_PENDING);
    let activity_sender = sender.clone();
    let origins = Mutex::new(HashMap::<String, (String, String)>::new());
    app.listen("download_activity", move |event| {
        let Ok(activity) = serde_json::from_str::<DownloadActivityEvent>(event.payload()) else {
            return;
        };
        // External archives and local-only games are not server downloads.
        if borrow_db_checked()
            .applications
            .local_games
            .contains_key(&activity.game_id)
        {
            return;
        }
        let mut body = serde_json::to_value(&activity).unwrap_or(Value::Null);
        if let Some(body) = body.as_object_mut() {
            body.remove("error");
        }
        // Attribute the whole attempt to the account that queued it, even if
        // someone signs out while the worker is still running.
        let (server, client_id) = {
            let mut origins = origins.lock().unwrap_or_else(|error| error.into_inner());
            if activity.state == "Queued" {
                let Some(origin) = pending("/api/v1/client/downloads/history", Value::Null) else {
                    return;
                };
                origins.insert(
                    activity.session_id.clone(),
                    (origin.server, origin.client_id),
                );
            }
            let Some(origin) = origins.get(&activity.session_id).cloned() else {
                return;
            };
            if matches!(
                activity.state.as_str(),
                "Completed" | "Cancelled" | "Failed"
            ) {
                origins.remove(&activity.session_id);
            }
            origin
        };
        let item = |endpoint: &str, body| Pending {
            server: server.clone(),
            client_id: client_id.clone(),
            endpoint: endpoint.to_owned(),
            body,
        };
        let _ = activity_sender.try_send(item("/api/v1/client/downloads/history", body));
        if let Some(error) = activity.error {
            let body = json!({
                "reportId": format!("{}-error", activity.session_id), "source": "download",
                "message": clean(&error, 1000), "details": "", "gameId": activity.game_id,
                "clientVersion": env!("CARGO_PKG_VERSION"), "occurredAt": activity.occurred_at,
            });
            let _ = activity_sender.try_send(item("/api/v1/client/errors", body));
        }
    });
    app.listen("client_error_report", move |event| {
        if let Ok(body) = serde_json::from_str::<Value>(event.payload())
            && let Some(item) = pending("/api/v1/client/errors", body)
        {
            let _ = sender.try_send(item);
        }
    });
    tauri::async_runtime::spawn(async move {
        let path = DATA_ROOT_DIR.join("activity-outbox.json");
        let mut queue: VecDeque<Pending> = std::fs::read(&path)
            .ok()
            .filter(|data| data.len() <= 8_000_000)
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        queue.truncate(MAX_PENDING);
        let client = get_client_async_for(PROXY_AUTH.clone());
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        loop {
            tokio::select! {
                item = receiver.recv() => {
                    let Some(item) = item else { return; };
                    if queue.len() < MAX_PENDING { queue.push_back(item); }
                }
                _ = interval.tick() => {}
            }
            while let Ok(item) = receiver.try_recv() {
                if queue.len() < MAX_PENDING {
                    queue.push_back(item);
                }
            }
            // Persist before sending, so shutdown/offline periods do not lose terminal events.
            save(&path, &queue);
            while let Some(item) = queue.front() {
                let ready = {
                    let db = borrow_db_checked();
                    if db
                        .auth
                        .as_ref()
                        .is_none_or(|auth| auth.client_id != item.client_id)
                        || db.base_url != item.server
                    {
                        None
                    } else {
                        Some(!db.settings.force_offline)
                    }
                };
                match ready {
                    None => {
                        queue.pop_front();
                        continue;
                    }
                    Some(false) => break,
                    Some(true) => {}
                }
                let (Ok(url), Ok(header)) = (
                    url::Url::parse(&item.server).and_then(|base| base.join(&item.endpoint)),
                    generate_authorization_header(),
                ) else {
                    break;
                };
                // Configuration can change between the checks and signing.
                // Pin the destination and ensure this JWT belongs to the origin device.
                if header.split_whitespace().nth(1) != Some(item.client_id.as_str()) {
                    break;
                }
                let response = client
                    .post(url)
                    .header("Authorization", header)
                    .timeout(Duration::from_secs(4))
                    .json(&item.body)
                    .send()
                    .await;
                match response {
                    Ok(response)
                        if response.status().is_success()
                            || matches!(response.status().as_u16(), 400 | 404 | 405 | 413) =>
                    {
                        queue.pop_front();
                    }
                    _ => break,
                }
            }
            save(&path, &queue);
        }
    });
}

fn save(path: &std::path::Path, queue: &VecDeque<Pending>) {
    if let Ok(data) = serde_json::to_vec(queue) {
        let temporary = path.with_extension("tmp");
        if std::fs::write(&temporary, data).is_ok() {
            let _ = std::fs::rename(temporary, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reports_remove_secrets_and_bound_text() {
        let text = clean(
            "Authorization: Bearer topsecret https://user:pass@host/game?token=secret password=hunter2",
            2000,
        );
        for secret in ["topsecret", "user:pass", "token=secret", "hunter2"] {
            assert!(!text.contains(secret), "{text}");
        }
        assert_eq!(clean(&"é".repeat(3000), 2000).chars().count(), 2000);
    }
}
