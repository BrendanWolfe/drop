use crate::models::data::DesktopUpdateHistory;

impl DesktopUpdateHistory {
    /// Only a changed running version confirms an update; attempts can fail.
    pub fn record_client_start(&mut self, version: &str, now: u64) {
        if self
            .last_seen_version
            .as_ref()
            .is_some_and(|previous| previous != version)
        {
            self.last_updated = Some(now);
        }
        self.last_seen_version = Some(version.to_owned());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::data::{Database, Settings};

    #[test]
    fn older_databases_default_to_empty_update_history() {
        let db = Database::new("games", None, "cache".into());
        let encoded = ron::to_string(&db).unwrap();
        let legacy = encoded.replace(
            "desktop_update_history:(lastChecked:None,lastUpdated:None,lastSeenVersion:None),",
            "",
        );
        assert_ne!(legacy, encoded);
        let decoded: Database = ron::from_str(&legacy).unwrap();
        assert_eq!(decoded.desktop_update_history.last_checked, None);
        assert_eq!(decoded.desktop_update_history.last_updated, None);
    }

    #[test]
    fn update_history_records_real_version_changes_not_first_launch_or_restarts() {
        let mut history = DesktopUpdateHistory::default();
        history.record_client_start("v1-build.1", 100);
        assert_eq!(history.last_updated, None);
        history.record_client_start("v1-build.1", 200);
        assert_eq!(history.last_updated, None);
        history.last_checked = Some(250);
        history.record_client_start("v1-build.2", 300);
        assert_eq!(history.last_updated, Some(300));
        let mut decoded: DesktopUpdateHistory =
            ron::from_str(&ron::to_string(&history).unwrap()).unwrap();
        decoded.record_client_start("v1-build.2", 400);
        assert_eq!(decoded.last_updated, Some(300));
        assert_eq!(decoded.last_checked, Some(250));
    }

    #[test]
    fn automatic_updates_preserve_legacy_defaults_and_explicit_opt_out() {
        let mut settings: Settings =
            ron::from_str("(autostart:false,maxDownloadThreads:4,forceOffline:false)").unwrap();
        assert!(settings.automatic_client_updates);
        settings.automatic_client_updates = false;
        let saved: Settings = ron::from_str(&ron::to_string(&settings).unwrap()).unwrap();
        assert!(!saved.automatic_client_updates);
    }
}
