pub mod data {
    use std::{hash::Hash, path::PathBuf};

    use serde::{Deserialize, Serialize};

    // NOTE: Within each version, you should NEVER use these types.
    // Declare it using the actual version that it is from, i.e. v1::Settings rather than just Settings from here

    pub type Database = v1::Database;
    pub type GameVersion = v1::GameVersion;
    pub type Settings = v1::Settings;
    pub type DesktopUpdateHistory = v1::DesktopUpdateHistory;
    pub type DatabaseAuth = v1::DatabaseAuth;

    pub type GameDownloadStatus = v1::GameDownloadStatus;
    pub type InstalledGameType = v1::InstalledGameType;
    pub type ApplicationTransientStatus = v1::ApplicationTransientStatus;
    /**
     * Need to be universally accessible by the ID, and the version is just a couple sprinkles on top
     */
    pub type DownloadableMetadata = v1::DownloadableMetadata;
    pub type DownloadType = v1::DownloadType;
    pub type DatabaseApplications = v1::DatabaseApplications;
    pub type UserConfiguration = v1::UserConfiguration;
    pub type LocalGame = v1::LocalGame;
    pub type ArchiveInstall = v1::ArchiveInstall;
    pub type ArchiveState = v1::ArchiveState;
    pub type LaunchConfiguration = v1::LaunchConfiguration;
    pub type LaunchConfigurationEmulator = v1::LaunchConfigurationEmulator;
    pub type EmulatorProfile = v1::EmulatorProfile;
    pub type EmulatorLaunchMode = v1::EmulatorLaunchMode;
    pub type EmulatorConfigFile = v1::EmulatorConfigFile;

    use std::collections::HashMap;

    impl PartialEq for DownloadableMetadata {
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id && self.download_type == other.download_type
        }
    }
    impl Hash for DownloadableMetadata {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.id.hash(state);
            self.download_type.hash(state);
        }
    }

    #[derive(Serialize, Deserialize)]
    enum DatabaseVersionEnum {
        V0_4_0 { database: v1::Database },
        V0_4_4 { database: v1::Database },
    }

    pub struct DatabaseVersionSerializable(pub(crate) Database);

    impl Serialize for DatabaseVersionSerializable {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            // Older clients automatically uninstall entries whose server cache
            // is missing. Do not let them interpret local folder registrations
            // as server installations: use a new format tag when locals exist.
            let version = if self.0.applications.local_games.is_empty() {
                DatabaseVersionEnum::V0_4_0 { database: self.0.clone() }
            } else {
                DatabaseVersionEnum::V0_4_4 { database: self.0.clone() }
            };
            version.serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for DatabaseVersionSerializable {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            Ok(match DatabaseVersionEnum::deserialize(deserializer)? {
                DatabaseVersionEnum::V0_4_0 { database } | DatabaseVersionEnum::V0_4_4 { database } => DatabaseVersionSerializable(database),
            })
        }
    }

    mod v1 {
        use serde_with::serde_as;
        use std::{collections::HashMap, path::PathBuf};

        use crate::platform::Platform;

        use super::{Deserialize, Serialize};

        fn default_template() -> UserConfiguration {
            UserConfiguration {
                launch_template: "{}".to_owned(),
                override_proton_path: None,
                override_handler: None,
                enable_updates: false,
            }
        }

        // Preserve launches already installed by clients predating eligibility.
        // Updated servers always send an explicit value; new downloads opt in.
        fn legacy_proton_compatible() -> bool {
            true
        }

        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
        #[serde(rename_all = "camelCase")]
        pub struct UserConfiguration {
            pub launch_template: String,
            pub override_proton_path: Option<String>,
            #[serde(default)]
            pub override_handler: Option<String>,
            pub enable_updates: bool,
        }

        impl Default for UserConfiguration {
            fn default() -> Self {
                default_template()
            }
        }

        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
        #[serde(rename_all = "camelCase")]
        pub struct GameVersion {
            pub game_id: String,
            pub version_id: String,

            pub display_name: Option<String>,
            pub version_path: String,

            pub only_setup: bool,

            pub version_index: usize,
            pub delta: bool,

            #[serde(default = "default_template")]
            pub user_configuration: UserConfiguration,

            pub launches: Vec<LaunchConfiguration>,
            pub setups: Vec<SetupConfiguration>,
        }

        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
        #[serde(rename_all = "camelCase")]
        pub struct LaunchConfiguration {
            pub launch_id: String,

            pub name: String,
            pub command: String,
            pub platform: Platform,
            #[serde(default = "legacy_proton_compatible")]
            pub proton_compatible: bool,
            pub umu_id_override: Option<String>,

            pub emulator: Option<LaunchConfigurationEmulator>,
            #[serde(default)]
            pub emulator_profile: Option<EmulatorProfile>,
        }

        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
        #[serde(rename_all = "camelCase")]
        pub struct EmulatorProfile {
            pub mode: EmulatorLaunchMode,
            pub config_files: Vec<EmulatorConfigFile>,
        }

        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
        #[serde(rename_all = "lowercase")]
        pub enum EmulatorLaunchMode {
            Cli,
            Ui,
        }

        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
        pub struct EmulatorConfigFile {
            pub path: String,
            pub content: String,
        }

        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
        #[serde(rename_all = "camelCase")]
        /**
         * This is intended to be used to look up the actual launch configuration that we store elsewhere
         */
        pub struct LaunchConfigurationEmulator {
            pub launch_id: String,
            pub game_id: String,
            pub version_id: String,
        }

        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
        #[serde(rename_all = "camelCase")]
        pub struct SetupConfiguration {
            pub command: String,
            pub platform: Platform,
            #[serde(default = "legacy_proton_compatible")]
            pub proton_compatible: bool,
        }

        /// Device-local entries are authoritative here, never in the server cache.
        #[derive(Serialize, Deserialize, Clone, Debug)]
        #[serde(rename_all = "camelCase")]
        pub struct LocalGame {
            pub id: String,
            pub name: String,
            pub root: PathBuf,
            pub launcher: Option<PathBuf>,
            pub arguments: String,
            pub working_directory: PathBuf,
            pub platform: Platform,
            pub playtime_seconds: u64,
            pub archive: Option<ArchiveInstall>,
        }

        #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
        pub enum ArchiveState {
            Incomplete,
            Downloading,
            Extracting,
            NeedsLauncher,
            Ready,
            Error,
        }

        #[derive(Serialize, Deserialize, Clone)]
        #[serde(rename_all = "camelCase")]
        pub struct ArchiveInstall {
            pub url: String,
            pub archive_path: PathBuf,
            pub validator: Option<String>,
            pub downloaded: bool,
            pub extracted: bool,
            pub state: ArchiveState,
            pub error: Option<String>,
        }

        impl std::fmt::Debug for ArchiveInstall {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct("ArchiveInstall").field("state", &self.state).finish_non_exhaustive()
            }
        }

        #[derive(Serialize, Deserialize, Clone, Debug)]
        #[serde(rename_all = "camelCase")]
        pub struct Settings {
            pub autostart: bool,
            pub max_download_threads: usize,
            /// Aggregate download speed limit in bytes per second. 0 means unlimited.
            #[serde(default)]
            pub max_download_speed: u64,
            #[serde(default = "automatic_updates_enabled")]
            pub automatic_client_updates: bool,
            pub force_offline: bool, // ... other settings ...
        }
        impl Default for Settings {
            fn default() -> Self {
                Self {
                    autostart: false,
                    max_download_threads: 4,
                    max_download_speed: 0,
                    automatic_client_updates: true,
                    force_offline: false,
                }
            }
        }

        fn automatic_updates_enabled() -> bool {
            true
        }

        #[derive(Serialize, Deserialize, Clone, Debug, Default)]
        #[serde(rename_all = "camelCase")]
        pub struct DesktopUpdateHistory {
            pub last_checked: Option<u64>,
            pub last_updated: Option<u64>,
            pub last_seen_version: Option<String>,
        }

        #[derive(Serialize, Clone, Deserialize, Debug)]
        #[serde(tag = "type")]
        pub enum InstalledGameType {
            SetupRequired,
            Installed,
            PartiallyInstalled {
                #[serde(default)]
                configuration: UserConfiguration,
            },
        }

        #[derive(Serialize, Clone, Deserialize, Debug)]
        #[serde(tag = "type")]
        pub enum GameDownloadStatus {
            Remote {},
            Installed {
                install_type: InstalledGameType,
                version_id: String,
                install_dir: String,
                update_available: bool,
            },
        }
        // Stuff that shouldn't be synced to disk
        #[derive(Clone, Serialize, Deserialize, Debug)]
        #[serde(tag = "type")]
        pub enum ApplicationTransientStatus {
            Queued { version_id: String },
            Downloading { version_id: String },
            Uninstalling {},
            Updating { version_id: String },
            Validating { version_id: String },
            Extracting { version_id: String },
            Running {},
        }

        #[derive(serde::Serialize, Clone, Deserialize)]
        pub struct DatabaseAuth {
            pub private: String,
            pub cert: String,
            pub client_id: String,
            pub web_token: Option<String>,
        }

        #[derive(
            Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Clone, Copy,
        )]
        pub enum DownloadType {
            Game,
            Tool,
            Dlc,
            Mod,
        }

        #[derive(Debug, Eq, PartialOrd, Ord, Serialize, Deserialize, Clone)]
        #[serde(rename_all = "camelCase")]
        pub struct DownloadableMetadata {
            pub id: String,
            pub version: String,
            pub target_platform: Platform,
            pub download_type: DownloadType,
        }
        impl DownloadableMetadata {
            pub fn new(
                id: String,
                version: String,
                target_platform: Platform,
                download_type: DownloadType,
            ) -> Self {
                Self {
                    id,
                    version,
                    target_platform,
                    download_type,
                }
            }
        }

        #[serde_as]
        #[derive(Serialize, Clone, Deserialize, Default)]
        #[serde(rename_all = "camelCase")]
        pub struct DatabaseApplications {
            pub install_dirs: Vec<PathBuf>,
            // Legacy databases inherit their existing locations until first edited.
            #[serde(default)]
            pub emulator_install_dirs: Option<Vec<PathBuf>>,
            #[serde(default)]
            pub default_install_dir: Option<PathBuf>,
            #[serde(default)]
            pub default_emulator_install_dir: Option<PathBuf>,
            #[serde(default)]
            pub local_games: HashMap<String, LocalGame>,
            // Guaranteed to exist if the game also exists in the app state map
            pub game_statuses: HashMap<String, GameDownloadStatus>,

            pub game_versions: HashMap<String, GameVersion>,
            pub installed_game_version: HashMap<String, DownloadableMetadata>,

            pub additional_proton_paths: Vec<String>,
            pub default_proton_path: Option<String>,

            #[serde(skip)]
            pub transient_statuses: HashMap<DownloadableMetadata, ApplicationTransientStatus>,
        }

        #[derive(Serialize, Deserialize, Clone, Default)]
        pub struct Database {
            #[serde(default)]
            pub desktop_update_history: DesktopUpdateHistory,
            #[serde(default)]
            pub steam_registrations: HashMap<String, steam_shortcuts::Registration>,
            #[serde(default)]
            pub steam_profile: Option<String>,
            /// Header names only. Values live in the OS credential store, never this database.
            #[serde(default)]
            pub proxy_auth: HashMap<String, Vec<String>>,
            /// Origins whose dormant OS credential records must be deleted when the vault unlocks.
            #[serde(default)]
            pub proxy_auth_cleanup: std::collections::HashSet<String>,
            #[serde(default)]
            pub settings: Settings,
            pub auth: Option<DatabaseAuth>,
            pub base_url: String,
            pub applications: DatabaseApplications,
            pub cache_dir: PathBuf,

            #[serde(skip)]
            pub prev_database: Option<PathBuf>,
        }
    }

    impl Database {
        pub fn new<T: Into<PathBuf>>(
            games_base_dir: T,
            prev_database: Option<PathBuf>,
            cache_dir: PathBuf,
        ) -> Self {
            let games_base_dir = games_base_dir.into();
            Self {
                desktop_update_history: Default::default(),
                proxy_auth: HashMap::new(),
                steam_registrations: HashMap::new(),
                steam_profile: None,
                proxy_auth_cleanup: Default::default(),
                applications: DatabaseApplications {
                    install_dirs: vec![games_base_dir.clone()],
                    emulator_install_dirs: Some(vec![games_base_dir]),
                    default_install_dir: None,
                    default_emulator_install_dir: None,
                    local_games: HashMap::new(),
                    game_statuses: HashMap::new(),
                    game_versions: HashMap::new(),
                    installed_game_version: HashMap::new(),
                    transient_statuses: HashMap::new(),
                    additional_proton_paths: Vec::new(),
                    default_proton_path: None,
                },
                prev_database,
                base_url: String::new(),
                auth: None,
                settings: Settings::default(),
                cache_dir,
            }
        }
    }
    impl DatabaseAuth {
        pub fn new(
            private: String,
            cert: String,
            client_id: String,
            web_token: Option<String>,
        ) -> Self {
            Self {
                private,
                cert,
                client_id,
                web_token,
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::{Database, InstalledGameType, Settings, UserConfiguration};

        #[test]
        fn legacy_launches_default_to_no_emulator_profile() {
            let launch: super::LaunchConfiguration = ron::from_str(
                r#"(launchId:"launch",name:"Play",command:"emulator '{rom}'",platform:Linux,umuIdOverride:None,emulator:None)"#,
            )
            .unwrap();
            assert!(launch.emulator_profile.is_none());
            let mut launch = launch;
            launch.emulator_profile = Some(super::EmulatorProfile {
                mode: super::EmulatorLaunchMode::Ui,
                config_files: vec![super::EmulatorConfigFile {
                    path: "settings.json".into(),
                    content: "{gameDir:json}".into(),
                }],
            });
            let encoded = ron::to_string(&launch).unwrap();
            let decoded: super::LaunchConfiguration = ron::from_str(&encoded).unwrap();
            assert_eq!(decoded, launch);
        }

        #[test]
        fn legacy_database_defaults_to_no_proxy_authentication() {
            let database = Database::new("games", None, "cache".into());
            let encoded = ron::to_string(&database).unwrap();
            let legacy = encoded.replace("proxy_auth:{},", "").replace("proxy_auth_cleanup:[],", "");
            assert_ne!(legacy, encoded);
            let decoded: Database = ron::from_str(&legacy).unwrap();
            assert!(decoded.proxy_auth.is_empty());
            assert!(decoded.proxy_auth_cleanup.is_empty());
        }

        #[test]
        fn deferred_cleanup_survives_restart_without_enabling_old_credentials() {
            let mut database = Database::new("games", None, "cache".into());
            database.proxy_auth_cleanup.insert("https://drop.example".into());
            let encoded = ron::to_string(&database).unwrap();
            let decoded: Database = ron::from_str(&encoded).unwrap();
            assert!(decoded.proxy_auth.is_empty());
            assert!(decoded.proxy_auth_cleanup.contains("https://drop.example"));
        }

        #[test]
        fn settings_default_speed_is_unlimited() {
            assert_eq!(Settings::default().max_download_speed, 0);
        }

        #[test]
        fn steam_registrations_persist_and_legacy_databases_default_to_none() {
            let mut database=Database::new("games",None,"cache".into());
            let encoded=ron::to_string(&database).unwrap();
            let legacy=encoded.replace("steam_registrations:{},", "").replace("steam_profile:None,", "");
            assert_ne!(encoded,legacy); let decoded:Database=ron::from_str(&legacy).unwrap(); assert!(decoded.steam_registrations.is_empty()); assert!(decoded.steam_profile.is_none());
            let reg=steam_shortcuts::Registration { key:"key".into(),game_id:"game".into(),source:"instance".into(),steam_root:"steam".into(),profile_id:"42".into(),app_id:0xdeadbeef,game_id64:steam_shortcuts::game_id(0xdeadbeef),ownership_token:"token".into(),exported:steam_shortcuts::ExportedLaunch { executable:"play.exe".into(),arguments:vec!["Player One".into()],working_directory:"game".into(),launch_id:"play".into(),configuration:"{}".into() },exported_name:"Game".into(),prefer_steam:true };
            database.steam_registrations.insert("key".into(),reg); database.steam_profile=Some("profile".into());
            let decoded:Database=ron::from_str(&ron::to_string(&database).unwrap()).unwrap();
            assert!(decoded.steam_registrations["key"].prefer_steam); assert_eq!(decoded.steam_registrations["key"].game_id64,steam_shortcuts::game_id(0xdeadbeef));
        }

        #[test]
        fn older_databases_default_to_an_empty_local_library() {
            let database = Database::new("games", None, "cache".into());
            let encoded = ron::to_string(&database).unwrap();
            let legacy = encoded.replace("localGames:{},", "");
            assert_ne!(legacy, encoded);
            let decoded: Database = ron::from_str(&legacy).unwrap();
            assert!(decoded.applications.local_games.is_empty());
        }

        #[test]
        fn local_folders_use_a_format_older_clients_cannot_uninstall() {
            use super::{DatabaseVersionSerializable, LocalGame};
            use crate::platform::Platform;
            let mut database = Database::new("games", None, "cache".into());
            let legacy = ron::to_string(&DatabaseVersionSerializable(database.clone())).unwrap();
            assert!(legacy.starts_with("V0_4_0("));
            let _: DatabaseVersionSerializable = ron::from_str(&legacy).unwrap();
            database.applications.local_games.insert("local-test".into(), LocalGame {
                id: "local-test".into(), name: "My game".into(), root: "games/test".into(),
                launcher: Some("games/test/play.exe".into()), arguments: String::new(),
                working_directory: "games/test".into(), platform: Platform::Windows,
                playtime_seconds: 123, archive: None,
            });
            let encoded = ron::to_string(&DatabaseVersionSerializable(database)).unwrap();
            assert!(encoded.starts_with("V0_4_4("));
            let decoded: DatabaseVersionSerializable = ron::from_str(&encoded).unwrap();
            assert_eq!(decoded.0.applications.local_games["local-test"].playtime_seconds, 123);
        }

        #[test]
        fn settings_without_speed_deserialize_as_unlimited() {
            let settings: Settings =
                ron::from_str("(autostart:false,maxDownloadThreads:4,forceOffline:false)")
                    .expect("old settings should still deserialize");

            assert!(!settings.autostart);
            assert_eq!(settings.max_download_threads, 4);
            assert!(!settings.force_offline);
            assert_eq!(settings.max_download_speed, 0);
        }

        #[test]
        fn settings_speed_uses_camel_case_and_round_trips() {
            let settings: Settings = ron::from_str(
                "(autostart:true,maxDownloadThreads:8,maxDownloadSpeed:1048576,forceOffline:true)",
            )
            .expect("settings with speed should deserialize");

            assert_eq!(settings.max_download_speed, 1_048_576);

            let encoded = ron::to_string(&settings).expect("settings should serialize");
            assert!(encoded.contains("maxDownloadSpeed"));

            let round_trip: Settings = ron::from_str(&encoded).expect("settings should round trip");
            assert_eq!(round_trip.max_download_speed, 1_048_576);
        }

        #[test]
        fn partially_installed_configuration_round_trips() {
            let configuration = UserConfiguration {
                launch_template: "proton run {}".to_owned(),
                override_proton_path: Some("/custom/proton".to_owned()),
                override_handler: Some("custom-handler".to_owned()),
                enable_updates: true,
            };
            let install_type = InstalledGameType::PartiallyInstalled {
                configuration: configuration.clone(),
            };

            let encoded =
                ron::to_string(&install_type).expect("partially installed should serialize");
            assert!(encoded.contains("configuration"));

            let round_trip: InstalledGameType =
                ron::from_str(&encoded).expect("partially installed should round trip");
            match round_trip {
                InstalledGameType::PartiallyInstalled {
                    configuration: decoded,
                } => {
                    assert_eq!(decoded, configuration);
                }
                other => panic!("expected partially installed, got {other:?}"),
            }
        }

        #[test]
        fn partially_installed_without_configuration_uses_default() {
            let decoded: InstalledGameType = ron::from_str("(type:\"PartiallyInstalled\")").expect(
                "legacy partially installed entries without configuration should deserialize",
            );

            match decoded {
                InstalledGameType::PartiallyInstalled { configuration } => {
                    assert_eq!(configuration, UserConfiguration::default());
                }
                other => panic!("expected partially installed, got {other:?}"),
            }
        }
    }
}
