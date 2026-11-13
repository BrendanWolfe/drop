use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::DatabaseApplications;

#[derive(Clone, Copy, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum InstallLocationKind {
    #[default]
    Game,
    Emulator,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallLocationList {
    directories: Vec<PathBuf>,
    default_index: Option<usize>,
}

#[derive(Serialize)]
pub struct InstallLocations {
    game: InstallLocationList,
    emulator: InstallLocationList,
}

impl DatabaseApplications {
    pub fn install_directories(&self, kind: InstallLocationKind) -> &[PathBuf] {
        match kind {
            InstallLocationKind::Game => &self.install_dirs,
            InstallLocationKind::Emulator => self
                .emulator_install_dirs
                .as_deref()
                .unwrap_or(&self.install_dirs),
        }
    }

    pub fn default_install_index(&self, kind: InstallLocationKind) -> Option<usize> {
        let directories = self.install_directories(kind);
        let configured = match kind {
            InstallLocationKind::Game => &self.default_install_dir,
            InstallLocationKind::Emulator => &self.default_emulator_install_dir,
        };
        configured
            .as_ref()
            .and_then(|path| directories.iter().position(|dir| dir == path))
            .or_else(|| (!directories.is_empty()).then_some(0))
    }

    pub fn install_locations(&self) -> InstallLocations {
        let list = |kind| InstallLocationList {
            directories: self.install_directories(kind).to_vec(),
            default_index: self.default_install_index(kind),
        };
        InstallLocations {
            game: list(InstallLocationKind::Game),
            emulator: list(InstallLocationKind::Emulator),
        }
    }

    fn install_directories_mut(&mut self, kind: InstallLocationKind) -> &mut Vec<PathBuf> {
        // Snapshot legacy emulator locations before editing either list.
        let emulators = self
            .emulator_install_dirs
            .get_or_insert_with(|| self.install_dirs.clone());
        match kind {
            InstallLocationKind::Game => &mut self.install_dirs,
            InstallLocationKind::Emulator => emulators,
        }
    }

    pub fn add_install_directory(
        &mut self,
        kind: InstallLocationKind,
        path: PathBuf,
    ) -> Result<(), String> {
        if self.install_directories(kind).contains(&path) {
            return Err("This location is already in the list.".into());
        }
        self.install_directories_mut(kind).push(path);
        self.register_install_locations();
        Ok(())
    }

    pub fn set_default_install_directory(
        &mut self,
        kind: InstallLocationKind,
        index: usize,
    ) -> Result<(), String> {
        let path = self
            .install_directories(kind)
            .get(index)
            .cloned()
            .ok_or("This location no longer exists. Refresh the list and try again.")?;
        match kind {
            InstallLocationKind::Game => self.default_install_dir = Some(path),
            InstallLocationKind::Emulator => self.default_emulator_install_dir = Some(path),
        }
        Ok(())
    }

    pub fn remove_install_directory(
        &mut self,
        kind: InstallLocationKind,
        index: usize,
    ) -> Result<(), String> {
        let directories = self.install_directories(kind);
        if index >= directories.len() {
            return Err("This location no longer exists. Refresh the list and try again.".into());
        }
        if directories.len() <= 1 {
            return Err("Keep at least one installation location.".into());
        }
        let root = &directories[index];
        let in_use = self
            .installed_game_version
            .values()
            .chain(
                self.emulator_installations
                    .values()
                    .map(|item| &item.metadata),
            )
            .any(|meta| {
                self.resolve_install_path(meta)
                    .is_ok_and(|path| path.starts_with(root))
            });
        if in_use {
            return Err("This location still has registered installations. Move or uninstall them before removing it.".into());
        }
        let default = directories[self.default_install_index(kind).unwrap()].clone();
        let directories = self.install_directories_mut(kind);
        let removed = directories.remove(index);
        let default = if removed == default {
            directories[0].clone()
        } else {
            default
        };
        match kind {
            InstallLocationKind::Game => self.default_install_dir = Some(default),
            InstallLocationKind::Emulator => self.default_emulator_install_dir = Some(default),
        }
        Ok(())
    }

    pub fn resolve_install_directory(
        &self,
        kind: InstallLocationKind,
        index: Option<usize>,
        path: Option<&Path>,
    ) -> Result<PathBuf, String> {
        let directories = self.install_directories(kind);
        if let Some(path) = path {
            return directories
                .iter()
                .find(|dir| dir.as_path() == path)
                .cloned()
                .ok_or(
                    "The selected location was removed. Choose an installation location again."
                        .into(),
                );
        }
        index
            .or_else(|| self.default_install_index(kind))
            .and_then(|index| directories.get(index))
            .cloned()
            .ok_or("Choose a configured installation location.".into())
    }

    pub fn configured_install_index(
        &self,
        kind: InstallLocationKind,
        index: Option<usize>,
        path: Option<&Path>,
    ) -> Result<usize, String> {
        let directories = self.install_directories(kind);
        match path {
            Some(path) => directories
                .iter()
                .position(|directory| directory.as_path() == path),
            None => index.filter(|index| *index < directories.len()),
        }
        .ok_or("The selected location was removed. Refresh the list and try again.".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Database;

    #[test]
    fn locations_and_defaults_are_independent_and_persist() {
        let mut db = Database::new("games", None, "cache".into());
        let apps = &mut db.applications;
        apps.add_install_directory(InstallLocationKind::Game, "other-games".into())
            .unwrap();
        apps.add_install_directory(InstallLocationKind::Emulator, "emulators".into())
            .unwrap();
        apps.set_default_install_directory(InstallLocationKind::Game, 1)
            .unwrap();
        apps.set_default_install_directory(InstallLocationKind::Emulator, 1)
            .unwrap();
        let decoded: Database = ron::from_str(&ron::to_string(&db).unwrap()).unwrap();
        assert_eq!(
            decoded
                .applications
                .resolve_install_directory(InstallLocationKind::Game, None, None)
                .unwrap(),
            PathBuf::from("other-games")
        );
        assert_eq!(
            decoded
                .applications
                .resolve_install_directory(InstallLocationKind::Emulator, None, None)
                .unwrap(),
            PathBuf::from("emulators")
        );
    }

    #[test]
    fn removing_locations_keeps_or_replaces_the_default_safely() {
        let mut apps = Database::new("one", None, "cache".into()).applications;
        for path in ["two", "three"] {
            apps.add_install_directory(InstallLocationKind::Game, path.into())
                .unwrap();
        }
        apps.set_default_install_directory(InstallLocationKind::Game, 2)
            .unwrap();
        apps.remove_install_directory(InstallLocationKind::Game, 0)
            .unwrap();
        assert_eq!(
            apps.default_install_index(InstallLocationKind::Game),
            Some(1)
        );
        apps.remove_install_directory(InstallLocationKind::Game, 1)
            .unwrap();
        assert_eq!(
            apps.resolve_install_directory(InstallLocationKind::Game, None, None)
                .unwrap(),
            PathBuf::from("two")
        );
        assert!(
            apps.remove_install_directory(InstallLocationKind::Game, 0)
                .is_err()
        );
        assert!(
            apps.remove_install_directory(InstallLocationKind::Game, 99)
                .is_err()
        );
        assert!(
            apps.set_default_install_directory(InstallLocationKind::Game, 99)
                .is_err()
        );
    }

    #[test]
    fn legacy_locations_are_preserved_without_coupling_later_edits() {
        let mut apps: DatabaseApplications = ron::from_str(
            r#"(installDirs:["legacy"],gameStatuses:{},gameVersions:{},installedGameVersion:{},additionalProtonPaths:[],defaultProtonPath:None)"#,
        ).unwrap();
        assert_eq!(
            apps.install_directories(InstallLocationKind::Emulator),
            &[PathBuf::from("legacy")]
        );
        apps.add_install_directory(InstallLocationKind::Game, "new-games".into())
            .unwrap();
        assert_eq!(
            apps.install_directories(InstallLocationKind::Emulator),
            &[PathBuf::from("legacy")]
        );
        assert!(
            apps.add_install_directory(InstallLocationKind::Emulator, "legacy".into())
                .is_err()
        );
    }

    #[test]
    fn stable_paths_are_validated_against_the_correct_list() {
        let mut apps = Database::new("games", None, "cache".into()).applications;
        apps.add_install_directory(InstallLocationKind::Emulator, "emulators".into())
            .unwrap();
        assert_eq!(
            apps.configured_install_index(
                InstallLocationKind::Emulator,
                None,
                Some(Path::new("emulators"))
            )
            .unwrap(),
            1
        );
        assert!(
            apps.configured_install_index(
                InstallLocationKind::Game,
                None,
                Some(Path::new("emulators"))
            )
            .is_err()
        );
        assert!(
            apps.configured_install_index(InstallLocationKind::Game, None, None)
                .is_err()
        );
        assert!(
            apps.resolve_install_directory(
                InstallLocationKind::Game,
                None,
                Some(Path::new("emulators"))
            )
            .is_err()
        );
        assert_eq!(
            apps.resolve_install_directory(
                InstallLocationKind::Emulator,
                None,
                Some(Path::new("emulators"))
            )
            .unwrap(),
            PathBuf::from("emulators")
        );
        assert!(
            apps.resolve_install_directory(
                InstallLocationKind::Emulator,
                None,
                Some(Path::new("removed"))
            )
            .is_err()
        );
    }
}
