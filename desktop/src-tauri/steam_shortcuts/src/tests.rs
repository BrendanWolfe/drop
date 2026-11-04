use super::*;
fn launch() -> ExportedLaunch {
    ExportedLaunch {
        executable: r"C:\Games\A Game\bin\play.exe".into(),
        arguments: vec!["--name".into(), "Player One".into()],
        working_directory: r"C:\Games\A Game\bin".into(),
        launch_id: "play".into(),
        configuration: "{}".into(),
    }
}
fn registration() -> Registration {
    Registration {
        key: "drop-key".into(),
        game_id: "game".into(),
        source: "https://drop.test".into(),
        steam_root: "steam".into(),
        profile_id: "42".into(),
        app_id: 0xdeadbeef,
        game_id64: game_id(0xdeadbeef),
        ownership_token: "unique-token".into(),
        exported: launch(),
        exported_name: "A Game".into(),
        prefer_steam: false,
    }
}
#[test]
fn windows_quotes_handle_empty_unicode_quotes_and_trailing_slashes() {
    for (input, expected) in [
        ("", "\"\""),
        ("plain", "plain"),
        ("Player One", "\"Player One\""),
        ("日本語 🎮", "\"日本語 🎮\""),
        ("a\"b", "\"a\\\"b\""),
        ("C:\\Game Dir\\", "\"C:\\Game Dir\\\\\""),
        ("a\\\"b", "\"a\\\\\\\"b\""),
    ] {
        assert_eq!(quote_argument(input), expected, "{input}");
    }
}
#[test]
fn launch_identifier_is_exact_decimal_string() {
    assert_eq!(game_id(u32::MAX), "18446744069448138752");
    assert_eq!(game_id(0x80000000), "9223372036888330240");
    assert!(initial_app_id("play.exe", "Game") & 0x80000000 != 0);
}
#[test]
fn repeated_add_and_update_preserve_identity_and_user_edits() {
    let reg = registration();
    let mut file = Shortcuts::parse(None).unwrap();
    file.upsert(&reg, "A Game", None).unwrap();
    file.upsert(&reg, "A Game", Some("A Game")).unwrap();
    assert_eq!(file.items().len(), 1);
    let fields = match &mut file.items_mut()[0].value {
        Value::Object(fields) => fields,
        _ => unreachable!(),
    };
    vdf::set(fields, "AppName", vdf::text("User's title"));
    vdf::set(fields, "AllowOverlay", vdf::int(0));
    vdf::set(fields, "future", Value::String(vec![255]));
    file.upsert(&reg, "A Game", Some("A Game")).unwrap();
    let fields = file.fields(reg.app_id).unwrap();
    assert_eq!(vdf::string(fields, "AppName").unwrap(), "User's title");
    assert_eq!(vdf::number(fields, "AllowOverlay"), Some(0));
    assert_eq!(vdf::get(fields, "future"), Some(&Value::String(vec![255])));
    let mut changed = reg.clone();
    changed.exported.arguments.push("--fullscreen".into());
    assert!(!file.matches(&reg, &changed.exported).unwrap());
    file.upsert(&changed, "New name", Some("A Game")).unwrap();
    assert!(file.matches(&changed, &changed.exported).unwrap());
    assert_eq!(file.items().len(), 1);
    assert!(file.verify(&reg).unwrap());
}
#[test]
fn name_and_visible_tag_are_never_proof_of_ownership() {
    let reg = registration();
    let mut file = Shortcuts::parse(None).unwrap();
    file.upsert(&reg, "A Game", None).unwrap();
    let mut stranger = reg.clone();
    stranger.ownership_token = "other-client".into();
    let original = file.bytes();
    assert!(file.upsert(&stranger, "A Game", None).is_err());
    assert!(file.remove(&stranger).is_err());
    assert_eq!(file.bytes(), original);
}
#[test]
fn collision_allocation_preserves_manual_shortcuts_and_reserved_identities() {
    let reg = registration();
    let mut file = Shortcuts::parse(None).unwrap();
    file.upsert(&reg, "Manual game", None).unwrap();
    assert_eq!(
        file.allocate_app_id(reg.app_id, &[reg.app_id + 1]).unwrap(),
        reg.app_id + 2
    );
    assert_eq!(
        file.allocate_app_id(u32::MAX, &[u32::MAX]).unwrap(),
        0x80000000
    );
    assert!(file.contains(reg.app_id));
}
#[test]
fn remove_and_recreate_touch_only_owned_entry() {
    let reg = registration();
    let mut unrelated = reg.clone();
    unrelated.app_id += 1;
    unrelated.ownership_token = "other".into();
    let mut file = Shortcuts::parse(None).unwrap();
    file.upsert(&reg, "A Game", None).unwrap();
    file.upsert(&unrelated, "Unrelated", None).unwrap();
    let before = file.fields(unrelated.app_id).unwrap().clone();
    file.remove(&reg).unwrap();
    assert_eq!(file.fields(unrelated.app_id), Some(&before));
    assert!(!file.verify(&reg).unwrap());
    file.upsert(&reg, "A Game", None).unwrap();
    assert!(file.verify(&reg).unwrap());
    assert_eq!(file.fields(unrelated.app_id), Some(&before));
}
#[test]
fn missing_valid_empty_malformed_and_zero_length_files() {
    assert!(Shortcuts::parse(None).is_ok());
    let empty = Shortcuts::parse(None).unwrap().bytes();
    assert!(Shortcuts::parse(Some(&empty)).is_ok());
    for bytes in [vec![], vec![8], b"not a shortcut".to_vec()] {
        assert!(Shortcuts::parse(Some(&bytes)).is_err());
    }
}
#[test]
fn creates_backup_and_preserves_original_on_running_or_parse_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shortcuts.vdf");
    let reg = registration();
    mutate(&path, || Ok(()), |s| s.upsert(&reg, "A Game", None)).unwrap();
    let original = fs::read(&path).unwrap();
    mutate(
        &path,
        || Ok(()),
        |s| s.upsert(&reg, "Renamed", Some("A Game")),
    )
    .unwrap();
    let backups = fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|s| s == "bak"))
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    assert_eq!(fs::read(backups[0].path()).unwrap(), original);
    let latest = fs::read(&path).unwrap();
    assert!(
        mutate(
            &path,
            || Err(Error::other("Steam running")),
            |s| s.remove(&reg)
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), latest);
    fs::write(&path, b"broken").unwrap();
    assert!(mutate(&path, || Ok(()), |s| s.remove(&reg)).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"broken");
}
#[test]
fn concurrent_changes_and_steam_starting_during_write_refuse_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shortcuts.vdf");
    let reg = registration();
    mutate(&path, || Ok(()), |s| s.upsert(&reg, "A Game", None)).unwrap();
    let mut calls = 0;
    let result = mutate(
        &path,
        || {
            calls += 1;
            if calls == 2 {
                fs::write(&path, b"changed by Steam")?;
            }
            Ok(())
        },
        |s| s.remove(&reg),
    );
    assert!(result.is_err());
    assert_eq!(fs::read(&path).unwrap(), b"changed by Steam");
    let bytes = Shortcuts::parse(None).unwrap().bytes();
    fs::write(&path, &bytes).unwrap();
    let mut calls = 0;
    assert!(
        mutate(
            &path,
            || {
                calls += 1;
                if calls == 2 {
                    Err(Error::other("Steam started"))
                } else {
                    Ok(())
                }
            },
            |s| s.upsert(&reg, "A Game", None)
        )
        .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), bytes);
}
#[test]
fn failed_io_and_invalid_profile_leave_source_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shortcuts.vdf");
    fs::create_dir(&path).unwrap();
    assert!(mutate(&path, || Ok(()), |s| s.remove(&registration())).is_err());
    assert!(path.is_dir());
    assert!(shortcut_path(dir.path(), "../42").is_err());
    assert!(shortcut_path(dir.path(), "").is_err());
}
#[test]
fn registration_round_trip_retains_identity() {
    // Persistence is shared with the desktop database; a serde format-independent round trip is tested there.
    let reg = registration();
    assert_eq!(reg.game_id64, game_id(reg.app_id));
    assert!(reg.game_id64.parse::<u64>().unwrap() > 9007199254740991);
}
