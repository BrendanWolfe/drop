//! Emulator packages are immutable builds, not a single mutable game install.
use crate::models::data::EmulatorInstallation;
use crate::platform::Platform;
use crate::{DatabaseApplications, DownloadType, DownloadableMetadata, GameDownloadStatus};

/// Unambiguous internal database key; not a user-facing folder name.
pub fn build_key(id: &str, version: &str, platform: Platform) -> String {
    fn hex(value: &str) -> String {
        value
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
    format!("{}-{}-{platform:?}", hex(id), hex(version))
}

impl DatabaseApplications {
    pub fn emulator_installation(
        &self,
        id: &str,
        version: &str,
        platform: Platform,
    ) -> Option<&EmulatorInstallation> {
        self.emulator_installations
            .get(&build_key(id, version, platform))
    }

    pub fn record_installation(&mut self, meta: &DownloadableMetadata, status: GameDownloadStatus) {
        if let GameDownloadStatus::Installed { install_dir, .. } = &status {
            self.bind_install_path(meta, std::path::Path::new(install_dir));
        }
        if meta.download_type == DownloadType::Emulator {
            self.emulator_installations.insert(
                build_key(&meta.id, &meta.version, meta.target_platform),
                EmulatorInstallation {
                    metadata: meta.clone(),
                    status,
                },
            );
        } else {
            self.installed_game_version
                .insert(meta.id.clone(), meta.clone());
            self.game_statuses.insert(meta.id.clone(), status);
        }
    }

    /// Move legacy records without moving/deleting any files. Idempotent.
    pub fn migrate_emulator(&mut self, id: &str) {
        let Some(mut meta) = self.installed_game_version.get(id).cloned() else {
            return;
        };
        let Some(status @ GameDownloadStatus::Installed { .. }) =
            self.game_statuses.get(id).cloned()
        else {
            return;
        };
        meta.download_type = DownloadType::Emulator;
        self.emulator_installations
            .entry(build_key(id, &meta.version, meta.target_platform))
            .or_insert(EmulatorInstallation {
                metadata: meta,
                status,
            });
        self.installed_game_version.remove(id);
        self.game_statuses.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Database, models::data::InstalledGameType};
    fn status(version: &str, path: &str) -> GameDownloadStatus {
        GameDownloadStatus::Installed {
            install_type: InstalledGameType::Installed,
            version_id: version.into(),
            install_dir: path.into(),
            update_available: false,
        }
    }
    #[test]
    fn migration_preserves_legacy_files_and_coexisting_builds_across_restart() {
        let mut db = Database::new("games", None, "cache".into());
        let old = DownloadableMetadata::new(
            "emu".into(),
            "v1".into(),
            Platform::Linux,
            DownloadType::Game,
        );
        db.applications
            .record_installation(&old, status("v1", "legacy/path"));
        db.applications.migrate_emulator("emu");
        db.applications.migrate_emulator("emu");
        for (version, platform) in [("v2", Platform::Linux), ("v1", Platform::Windows)] {
            let meta = DownloadableMetadata::new(
                "emu".into(),
                version.into(),
                platform,
                DownloadType::Emulator,
            );
            db.applications
                .record_installation(&meta, status(version, "new/path"));
        }
        let db: Database = ron::from_str(&ron::to_string(&db).unwrap()).unwrap();
        assert_eq!(db.applications.emulator_installations.len(), 3);
        assert!(db.applications.installed_game_version.is_empty());
        assert!(
            matches!(&db.applications.emulator_installation("emu", "v1", Platform::Linux).unwrap().status, GameDownloadStatus::Installed { install_dir, .. } if install_dir == "legacy/path")
        );
    }
    #[test]
    fn queue_identity_is_exact_for_emulators_but_still_single_install_for_games() {
        let first = DownloadableMetadata::new(
            "emu".into(),
            "v1".into(),
            Platform::Linux,
            DownloadType::Emulator,
        );
        let mut second = first.clone();
        second.version = "v2".into();
        let mut third = first.clone();
        third.target_platform = Platform::Windows;
        let keys = std::collections::HashSet::from([first.clone(), first, second, third]);
        assert_eq!(keys.len(), 3);
        let mut games = keys.into_iter().collect::<Vec<_>>();
        for game in &mut games {
            game.download_type = DownloadType::Game;
        }
        assert_eq!(
            games
                .into_iter()
                .collect::<std::collections::HashSet<_>>()
                .len(),
            1
        );
    }
    #[test]
    fn build_keys_are_unambiguous_and_safe_path_components() {
        assert_ne!(
            build_key("a-b", "c", Platform::Linux),
            build_key("a", "b-c", Platform::Linux)
        );
        let key = build_key("../emu", "../v1\\x:", Platform::Linux);
        assert!(!key.contains(['/', '\\', '.', ':']));
    }
}
