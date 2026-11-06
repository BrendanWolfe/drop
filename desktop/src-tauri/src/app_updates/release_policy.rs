use semver::Version;
use serde::Deserialize;

#[derive(Deserialize)]
pub(super) struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub target_commitish: String,
    pub published_at: Option<String>,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize)]
pub(super) struct Asset {
    pub name: String,
    pub browser_download_url: String,
}

pub(super) fn newest_release(releases: &[Release]) -> Option<&Release> {
    releases
        .iter()
        .filter(|release| {
            !release.draft
                && !release.prerelease
                && release.target_commitish == "main"
                && release.published_at.is_some()
                && release
                    .assets
                    .iter()
                    .any(|asset| asset.name == "latest.json")
        })
        .max_by_key(|release| &release.published_at)
}

pub(super) fn should_offer_update(
    current: &Version,
    remote: &Version,
    tag: &str,
    current_tag: &str,
) -> bool {
    tag != current_tag && remote >= current
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, date: &str) -> Release {
        Release {
            tag_name: tag.into(),
            draft: false,
            prerelease: false,
            target_commitish: "main".into(),
            published_at: Some(date.into()),
            assets: vec![Asset {
                name: "latest.json".into(),
                browser_download_url: "https://github.com/test/latest.json".into(),
            }],
        }
    }

    #[test]
    fn selects_only_stable_main_desktop_releases() {
        let mut develop = release("dev", "2026-11-08T00:00:00Z");
        develop.prerelease = true;
        develop.target_commitish = "develop".into();
        let mut mislabeled = release("develop-without-prerelease-flag", "2026-11-09T00:00:00Z");
        mislabeled.target_commitish = "develop".into();
        let mut candidate = release("main-rc", "2026-11-10T00:00:00Z");
        candidate.prerelease = true;
        let mut draft = release("draft", "2026-11-11T00:00:00Z");
        draft.draft = true;
        let mut server = release("server-only", "2026-11-12T00:00:00Z");
        server.assets.clear();
        let releases = vec![
            develop,
            mislabeled,
            candidate,
            draft,
            server,
            release("v0.4.5+updater.1", "2026-11-06T00:00:00Z"),
            release("v0.4.5+build.41", "2026-11-05T00:00:00Z"),
        ];
        assert_eq!(
            newest_release(&releases).unwrap().tag_name,
            "v0.4.5+updater.1"
        );
    }

    #[test]
    fn ignores_legacy_sha_targets_and_unpublished_releases() {
        assert!(newest_release(&[]).is_none());
        let mut legacy = release("legacy", "2026-11-07T00:00:00Z");
        legacy.target_commitish = "5a43619d6cc3fa2fb6ac3637cd65e022ac66fcaa".into();
        assert!(newest_release(&[legacy]).is_none());
        let mut unpublished = release("unpublished", "2026-11-07T00:00:00Z");
        unpublished.published_at = None;
        assert!(newest_release(&[unpublished]).is_none());
    }

    #[test]
    fn legacy_client_accepts_same_version_bridge_then_new_client_does_not_repeat_it() {
        let current = Version::parse("0.4.5").unwrap();
        assert!(should_offer_update(
            &current,
            &current,
            "v0.4.5+updater.1",
            "v0.4.5-build.14"
        ));
        assert!(!should_offer_update(
            &current,
            &current,
            "v0.4.5+updater.1",
            "v0.4.5+updater.1"
        ));
        assert!(should_offer_update(
            &current,
            &Version::parse("0.4.6").unwrap(),
            "v0.4.6",
            "v0.4.5+updater.1"
        ));
        assert!(!should_offer_update(
            &current,
            &Version::parse("0.4.4").unwrap(),
            "v0.4.4",
            "v0.4.5+updater.1"
        ));
    }
}
