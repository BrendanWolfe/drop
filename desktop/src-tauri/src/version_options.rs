use database::{models::data::UserConfiguration, platform::Platform};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiredContent {
    pub game_id: String,
    pub version_id: String,
    #[serde(default)]
    pub launch_ids: Vec<String>,
    #[serde(default)]
    version_name: Option<String>,
    #[serde(default)]
    pub installed_path: Option<String>,
    name: String,
    icon_object_id: String,
    short_description: String,
    size: GameSize,
}

#[derive(Default, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LaunchRuntime {
    #[default]
    Native,
    Proton,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionDownloadOption {
    pub game_id: String,
    pub version_id: String,
    display_name: Option<String>,
    #[serde(default)]
    version_index: Option<usize>,
    version_path: String,
    pub platform: Platform,
    #[serde(default)]
    installed_platform: Option<Platform>,
    #[serde(default)]
    proton_compatible: bool,
    #[serde(default, skip_serializing)]
    proton_required_content: Vec<RequiredContent>,
    #[serde(default)]
    pub runtime: LaunchRuntime,
    #[serde(default)]
    pub proton_setup_required: bool,
    size: GameSize,
    pub required_content: Vec<RequiredContent>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSize {
    install_size: usize,
    download_size: usize,
}

/// Eligibility does not depend on installed runtimes. Keep server version order
/// within each platform, preferring the installed platform, otherwise native.
pub fn prepare_options(
    options: Vec<VersionDownloadOption>,
    host: Platform,
    installed: Option<Platform>,
    proton_ready: bool,
) -> Vec<VersionDownloadOption> {
    let mut options: Vec<_> = options
        .into_iter()
        .filter(|option| option.platform.can_run_on(host, option.proton_compatible))
        .map(|mut option| {
            option.installed_platform = installed;
            if host == Platform::Linux && option.platform == Platform::Windows {
                option.runtime = LaunchRuntime::Proton;
                option.proton_setup_required = !proton_ready;
                option.required_content = std::mem::take(&mut option.proton_required_content);
            } else {
                option.runtime = LaunchRuntime::Native;
                option.proton_setup_required = false;
            }
            option
        })
        .collect();
    options.sort_by_key(|option| {
        (
            installed.is_some_and(|platform| option.platform != platform),
            option.platform != host,
        )
    });
    options
}

pub fn latest_for_platform(
    options: &[VersionDownloadOption],
    platform: Platform,
) -> Option<&VersionDownloadOption> {
    options.iter().find(|option| option.platform == platform)
}

pub fn contains_build(
    options: &[VersionDownloadOption],
    version_id: &str,
    platform: Platform,
) -> bool {
    options
        .iter()
        .any(|option| option.version_id == version_id && option.platform == platform)
}

pub fn has_update(
    options: &[VersionDownloadOption],
    platform: Platform,
    current_id: &str,
    current_index: usize,
) -> bool {
    latest_for_platform(options, platform).is_some_and(|latest| {
        latest.version_id != current_id
            && latest
                .version_index
                .is_none_or(|index| index > current_index)
    })
}

pub fn configuration_for_build(
    existing: Option<(Platform, &UserConfiguration)>,
    platform: Platform,
    enable_updates: bool,
) -> UserConfiguration {
    UserConfiguration {
        enable_updates,
        ..existing
            .filter(|(installed, _)| *installed == platform)
            .map(|(_, configuration)| configuration.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn option(id: &str, platform: Platform, proton: bool) -> VersionDownloadOption {
        serde_json::from_value(json!({
            "gameId": "game", "versionId": id, "versionPath": id,
            "platform": platform, "protonCompatible": proton,
            "size": { "installSize": 1, "downloadSize": 1 }, "requiredContent": [],
        }))
        .unwrap()
    }

    #[test]
    fn windows_needs_explicit_proton_permission_only_on_linux() {
        for host in [Platform::Windows, Platform::Linux, Platform::macOS] {
            let options = prepare_options(
                vec![
                    option("windows", Platform::Windows, false),
                    option("proton", Platform::Windows, true),
                    option("linux", Platform::Linux, true),
                    option("mac", Platform::macOS, false),
                ],
                host,
                None,
                true,
            );
            let ids: Vec<_> = options
                .iter()
                .map(|option| option.version_id.as_str())
                .collect();
            assert_eq!(
                ids,
                match host {
                    Platform::Windows => vec!["windows", "proton"],
                    Platform::Linux => vec!["linux", "proton"],
                    Platform::macOS => vec!["mac"],
                }
            );
        }
    }

    #[test]
    fn proton_setup_is_visible_and_never_changes_the_package_platform() {
        let options = prepare_options(
            vec![option("win", Platform::Windows, true)],
            Platform::Linux,
            None,
            false,
        );
        assert_eq!(options.len(), 1);
        assert_eq!(options[0].platform, Platform::Windows);
        assert_eq!(options[0].runtime, LaunchRuntime::Proton);
        assert!(options[0].proton_setup_required);
    }

    #[test]
    fn prefer_native_but_preserve_the_installed_platform_and_version_order() {
        for installed in [None, Some(Platform::Windows), Some(Platform::Linux)] {
            let options = prepare_options(
                vec![
                    option("win-new", Platform::Windows, true),
                    option("linux-new", Platform::Linux, false),
                    option("win-old", Platform::Windows, true),
                ],
                Platform::Linux,
                installed,
                true,
            );
            assert_eq!(options[0].platform, installed.unwrap_or(Platform::Linux));
            assert_eq!(
                latest_for_platform(&options, Platform::Windows)
                    .unwrap()
                    .version_id,
                "win-new"
            );
            assert_eq!(
                latest_for_platform(&options, Platform::Linux)
                    .unwrap()
                    .version_id,
                "linux-new"
            );
            assert!(latest_for_platform(&options, Platform::macOS).is_none());
        }
    }

    #[test]
    fn proton_uses_only_dependencies_of_proton_eligible_launches() {
        let mut build = option("win", Platform::Windows, true);
        let content = |id| {
            serde_json::from_value(json!({
                "gameId": id, "versionId": id, "name": id, "iconObjectId": "icon",
                "shortDescription": "", "size": { "installSize": 1, "downloadSize": 1 },
            }))
            .unwrap()
        };
        build.required_content = vec![content("not-eligible")];
        build.proton_required_content = vec![content("eligible")];
        let options = prepare_options(vec![build], Platform::Linux, None, true);
        assert_eq!(options[0].required_content[0].version_id, "eligible");
    }

    #[test]
    fn a_missing_installed_build_never_becomes_an_update_to_another_platform() {
        let options = prepare_options(
            vec![option("native", Platform::Linux, false)],
            Platform::Linux,
            Some(Platform::Windows),
            true,
        );
        assert!(latest_for_platform(&options, Platform::Windows).is_none());
        assert_eq!(options[0].installed_platform, Some(Platform::Windows));
    }

    #[test]
    fn legacy_download_options_do_not_imply_proton_permission() {
        let mut build = serde_json::to_value(option("win", Platform::Windows, false)).unwrap();
        build.as_object_mut().unwrap().remove("protonCompatible");
        let build = serde_json::from_value(build).unwrap();
        assert!(prepare_options(vec![build], Platform::Linux, None, true).is_empty());
    }

    #[test]
    fn old_installed_launches_keep_working_but_explicit_server_flags_win() {
        let mut launch = json!({
            "launchId": "launch", "name": "Play", "command": "game.exe", "platform": "Windows",
        });
        let cached: database::models::data::LaunchConfiguration =
            serde_json::from_value(launch.clone()).unwrap();
        assert!(cached.proton_compatible);
        launch["protonCompatible"] = json!(false);
        let remote: database::models::data::LaunchConfiguration =
            serde_json::from_value(launch).unwrap();
        assert!(!remote.proton_compatible);
    }

    #[test]
    fn updates_cannot_switch_platform_or_offer_an_older_proton_eligible_build() {
        let mut windows = option("win-old", Platform::Windows, true);
        windows.version_index = Some(1);
        let mut linux = option("linux-new", Platform::Linux, false);
        linux.version_index = Some(3);
        let options = vec![linux, windows];
        assert!(!has_update(&options, Platform::Windows, "win-current", 2));
        assert!(has_update(&options, Platform::Linux, "linux-old", 2));
        assert!(!has_update(&options, Platform::Linux, "linux-new", 3));
        assert!(!has_update(&options, Platform::macOS, "mac", 0));
    }

    #[test]
    fn download_preflight_requires_the_exact_eligible_version_and_platform() {
        let options = prepare_options(
            vec![
                option("allowed", Platform::Windows, true),
                option("denied", Platform::Windows, false),
                option("native", Platform::Linux, false),
            ],
            Platform::Linux,
            None,
            false,
        );
        assert!(contains_build(&options, "allowed", Platform::Windows));
        assert!(contains_build(&options, "native", Platform::Linux));
        assert!(!contains_build(&options, "denied", Platform::Windows));
        assert!(!contains_build(&options, "allowed", Platform::Linux));
        assert!(!contains_build(&options, "missing", Platform::Windows));
    }

    #[test]
    fn updates_preserve_runtime_overrides_but_explicit_build_switches_reset_them() {
        let configuration = UserConfiguration {
            launch_template: "wrapper {}".into(),
            override_handler: Some("proton-umu".into()),
            override_proton_path: Some("/tools/proton".into()),
            enable_updates: false,
        };
        let updated = configuration_for_build(
            Some((Platform::Windows, &configuration)),
            Platform::Windows,
            true,
        );
        assert_eq!(updated.override_handler, configuration.override_handler);
        assert_eq!(
            updated.override_proton_path,
            configuration.override_proton_path
        );
        assert_eq!(updated.launch_template, configuration.launch_template);
        assert!(updated.enable_updates);
        let switched = configuration_for_build(
            Some((Platform::Windows, &configuration)),
            Platform::Linux,
            false,
        );
        assert_eq!(switched, UserConfiguration::default());
    }
}
