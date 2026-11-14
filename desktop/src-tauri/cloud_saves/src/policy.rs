//! Comparing content, not timestamps, prevents clock skew from losing saves.
pub fn requires_choice(
    known_revision: Option<u32>,
    known_definition: Option<u32>,
    remote_revision: u32,
    remote_definition: u32,
    local_hash: &str,
    remote_hash: Option<(&str, u32)>,
) -> bool {
    if known_definition.is_some_and(|v| v != remote_definition) {
        return true;
    }
    if remote_hash
        .is_some_and(|(hash, definition)| hash == local_hash && definition == remote_definition)
    {
        return false;
    }
    match known_revision {
        Some(revision) => revision != remote_revision,
        None => remote_revision != 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_sync_never_overwrites_an_existing_different_cloud_save() {
        assert!(requires_choice(
            None,
            None,
            4,
            1,
            "local",
            Some(("cloud", 1))
        ));
        assert!(!requires_choice(
            None,
            None,
            4,
            1,
            "same",
            Some(("same", 1))
        ));
        assert!(!requires_choice(None, None, 0, 1, "local", None));
    }
    #[test]
    fn remote_changes_require_review_even_when_local_files_have_not_changed() {
        assert!(requires_choice(
            Some(3),
            Some(1),
            4,
            1,
            "local",
            Some(("cloud", 1))
        ));
        assert!(!requires_choice(
            Some(3),
            Some(1),
            3,
            1,
            "changed-local",
            Some(("previous", 1))
        ));
    }
    #[test]
    fn definition_changes_are_not_silently_adopted() {
        assert!(requires_choice(
            Some(3),
            Some(1),
            3,
            2,
            "same",
            Some(("same", 2))
        ));
    }
}
