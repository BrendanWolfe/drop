//! Managed paths on disk are relative to stable library IDs. `install_dir` is a
//! resolved runtime/IPC cache; only legacy and external installs persist it.
use std::{
    collections::HashSet,
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{DatabaseApplications, DownloadType, DownloadableMetadata, GameDownloadStatus};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InstallReference {
    pub location_id: String,
    pub relative_path: PathBuf,
}

fn key(meta: &DownloadableMetadata) -> String {
    format!(
        "{:?}-{}",
        meta.download_type,
        crate::emulator_installations::build_key(&meta.id, &meta.version, meta.target_platform)
    )
}

pub fn valid_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
        && !path.to_string_lossy().contains(['\\', ':'])
}

impl DatabaseApplications {
    pub fn register_install_locations(&mut self) {
        let mut roots = self.install_dirs.clone();
        roots.extend(self.emulator_install_dirs.clone().unwrap_or_default());
        for root in roots {
            if !self
                .install_location_roots
                .values()
                .any(|value| value == &root)
            {
                self.install_location_roots
                    .insert(format!("{:032x}", rand::random::<u128>()), root);
            }
        }
    }

    pub fn bind_install_path(&mut self, meta: &DownloadableMetadata, path: &Path) {
        if self.local_games.contains_key(&meta.id) {
            return;
        }
        self.register_install_locations();
        let reference = self
            .install_location_roots
            .iter()
            // Discovery can find a package under either kind of registered root.
            // Defaults/kinds constrain new downloads, not adoption of existing files.
            .filter(|(_, root)| {
                self.install_dirs.contains(root)
                    || self
                        .install_directories(
                            crate::install_locations::InstallLocationKind::Emulator,
                        )
                        .contains(root)
            })
            .filter_map(|(id, root)| {
                let relative = path.strip_prefix(root).ok()?;
                valid_relative(relative).then(|| {
                    (
                        root.components().count(),
                        InstallReference {
                            location_id: id.clone(),
                            relative_path: relative.to_path_buf(),
                        },
                    )
                })
            })
            .max_by_key(|(depth, _)| *depth)
            .map(|(_, reference)| reference);
        if let Some(reference) = reference {
            self.install_references.insert(key(meta), reference);
        } else {
            self.install_references.remove(&key(meta));
        }
    }

    pub fn resolve_install_path(&self, meta: &DownloadableMetadata) -> Result<PathBuf, String> {
        if let Some(reference) = self.install_references.get(&key(meta)) {
            if !valid_relative(&reference.relative_path) {
                return Err("Invalid relative installation path.".into());
            }
            let root = self
                .install_location_roots
                .get(&reference.location_id)
                .ok_or("The installation location is unavailable.")?;
            return Ok(root.join(&reference.relative_path));
        }
        let status = if meta.download_type == DownloadType::Emulator {
            self.emulator_installation(&meta.id, &meta.version, meta.target_platform)
                .map(|item| &item.status)
        } else {
            self.game_statuses.get(&meta.id)
        };
        match status {
            Some(GameDownloadStatus::Installed { install_dir, .. }) if !install_dir.is_empty() => {
                Ok(install_dir.into())
            }
            _ => Err("The installation directory is unavailable.".into()),
        }
    }

    fn installation_metadata(&self) -> Vec<DownloadableMetadata> {
        self.installed_game_version
            .values()
            .cloned()
            .chain(
                self.emulator_installations
                    .values()
                    .map(|item| item.metadata.clone()),
            )
            .collect()
    }

    fn installation_status_mut(
        &mut self,
        meta: &DownloadableMetadata,
    ) -> Option<&mut GameDownloadStatus> {
        if meta.download_type == DownloadType::Emulator {
            self.emulator_installations
                .get_mut(&crate::emulator_installations::build_key(
                    &meta.id,
                    &meta.version,
                    meta.target_platform,
                ))
                .map(|item| &mut item.status)
        } else {
            self.game_statuses.get_mut(&meta.id)
        }
    }

    /// Called after decoding both old and new databases. No filesystem/network IO.
    pub fn migrate_install_records(&mut self) {
        self.register_install_locations();
        for meta in self.installation_metadata() {
            if !self.install_references.contains_key(&key(&meta)) {
                if let Ok(path) = self.resolve_install_path(&meta) {
                    self.bind_install_path(&meta, &path);
                }
            }
            let path = self.resolve_install_path(&meta).ok();
            if let Some(GameDownloadStatus::Installed { install_dir, .. }) =
                self.installation_status_mut(&meta)
            {
                *install_dir = path
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
            }
        }
    }

    /// Operates on the serializer's snapshot, never on live runtime paths.
    pub fn prepare_install_records_for_save(&mut self) {
        self.migrate_install_records();
        let mut live = HashSet::new();
        for meta in self.installation_metadata() {
            if self.local_games.contains_key(&meta.id) {
                continue;
            }
            if self.install_references.contains_key(&key(&meta)) {
                if let Some(GameDownloadStatus::Installed { install_dir, .. }) =
                    self.installation_status_mut(&meta)
                {
                    install_dir.clear();
                    live.insert(key(&meta));
                }
            }
        }
        self.install_references.retain(|key, _| live.contains(key));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        Database,
        models::data::{DatabaseVersionSerializable, InstalledGameType},
        platform::Platform,
    };

    #[test]
    fn managed_records_persist_relative_paths_and_resolve_against_location_ids() {
        let mut db = Database::new("/old/library", None, "cache".into());
        let meta = DownloadableMetadata::new(
            "game".into(),
            "v1".into(),
            Platform::Linux,
            DownloadType::Game,
        );
        db.applications.record_installation(
            &meta,
            GameDownloadStatus::Installed {
                install_type: InstalledGameType::Installed,
                version_id: "v1".into(),
                install_dir: "/old/library/renamed-game".into(),
                update_available: false,
            },
        );
        let encoded = ron::to_string(&DatabaseVersionSerializable(db)).unwrap();
        assert!(!encoded.contains("/old/library/renamed-game"));
        assert!(encoded.contains("relativePath:\"renamed-game\""));
        let mut decoded: DatabaseVersionSerializable = ron::from_str(&encoded).unwrap();
        assert_eq!(
            decoded.0.applications.resolve_install_path(&meta).unwrap(),
            Path::new("/old/library/renamed-game")
        );
        for root in decoded.0.applications.install_location_roots.values_mut() {
            *root = "/new/library".into();
        }
        decoded.0.applications.migrate_install_records();
        assert_eq!(
            decoded.0.applications.resolve_install_path(&meta).unwrap(),
            Path::new("/new/library/renamed-game")
        );
    }

    #[test]
    fn unsafe_relative_paths_are_rejected() {
        for path in [
            "",
            "/",
            "../game",
            "game/../other",
            "C:\\game",
            "game\\other",
        ] {
            assert!(!valid_relative(Path::new(path)), "{path}");
        }
        assert!(valid_relative(Path::new(".drop-emulators/build")));
    }

    #[test]
    fn old_absolute_records_migrate_without_losing_unregistered_legacy_paths() {
        for path in ["/library/game", "/external/game"] {
            let mut db = Database::new("/library", None, "cache".into());
            let meta = DownloadableMetadata::new(
                "game".into(),
                "v1".into(),
                Platform::Linux,
                DownloadType::Game,
            );
            db.applications
                .installed_game_version
                .insert(meta.id.clone(), meta.clone());
            db.applications.game_statuses.insert(
                meta.id.clone(),
                GameDownloadStatus::Installed {
                    install_type: InstalledGameType::Installed,
                    version_id: "v1".into(),
                    install_dir: path.into(),
                    update_available: true,
                },
            );
            let old = format!(
                "V0_4_0(database:{})",
                ron::to_string(&db)
                    .unwrap()
                    .replace("installLocationRoots:{},", "")
                    .replace("installReferences:{},", "")
            );
            let decoded: DatabaseVersionSerializable = ron::from_str(&old).unwrap();
            assert_eq!(
                decoded.0.applications.resolve_install_path(&meta).unwrap(),
                Path::new(path)
            );
            let encoded = ron::to_string(&decoded).unwrap();
            assert_eq!(encoded.contains(path), path.starts_with("/external"));
        }
    }
}
