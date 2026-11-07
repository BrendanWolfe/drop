use client::app_status::AppStatus;
use database::Database;

pub(super) fn can_check_game_updates(status: AppStatus, db: &Database) -> bool {
    status == AppStatus::SignedIn
        && db.auth.is_some()
        && !db.base_url.trim().is_empty()
        && !db.settings.force_offline
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::models::data::DatabaseAuth;

    fn configured_database() -> Database {
        let mut db = Database::new("games", None, "cache".into());
        db.base_url = "https://drop.example".into();
        db.auth = Some(DatabaseAuth::new(
            "private".into(),
            "certificate".into(),
            "client".into(),
            None,
        ));
        db
    }

    #[test]
    fn startup_update_checks_wait_for_a_signed_in_session() {
        let db = configured_database();
        for status in [
            AppStatus::NotConfigured,
            AppStatus::Offline,
            AppStatus::ServerError,
            AppStatus::SignedOut,
            AppStatus::SignedInNeedsReauth,
            AppStatus::ServerUnavailable,
        ] {
            assert!(!can_check_game_updates(status, &db));
        }
        assert!(can_check_game_updates(AppStatus::SignedIn, &db));
    }

    #[test]
    fn signed_in_state_alone_cannot_trigger_unauthenticated_requests() {
        let mut db = configured_database();
        db.auth = None;
        assert!(!can_check_game_updates(AppStatus::SignedIn, &db));
    }

    #[test]
    fn forced_offline_and_missing_server_configuration_skip_update_checks() {
        let mut db = configured_database();
        db.settings.force_offline = true;
        assert!(!can_check_game_updates(AppStatus::SignedIn, &db));
        db.settings.force_offline = false;
        db.base_url.clear();
        assert!(!can_check_game_updates(AppStatus::SignedIn, &db));
    }
}
