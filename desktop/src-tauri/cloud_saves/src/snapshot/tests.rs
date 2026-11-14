use super::*;

fn root(path: &Path) -> Root {
    Root {
        id: "saves".into(),
        path: path.to_owned(),
        kind: Kind::Directory,
        exclude: vec!["Cache/**".into(), "**/*.lock".into()],
    }
}

#[test]
fn directories_round_trip_and_preserve_excluded_files() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let source = temp.path().join("source");
    fs::create_dir_all(source.join("profile/world"))?;
    fs::create_dir_all(source.join("empty"))?;
    fs::create_dir_all(source.join("Cache"))?;
    fs::write(source.join("profile/world/chunk.dat"), b"world")?;
    fs::write(source.join("player.dat"), b"player")?;
    fs::write(source.join("Cache/ignored"), b"cache")?;
    fs::write(source.join("profile/write.lock"), b"locked")?;
    let archive = temp.path().join("snapshot.tar.zst");
    let manifest = capture(&[root(&source)], "game", "Linux", 1, &archive, MAX_BYTES)?;
    assert_eq!(manifest.contents.files.len(), 2);
    let destination = temp.path().join("destination");
    fs::create_dir_all(destination.join("Cache"))?;
    fs::write(destination.join("Cache/local"), b"preserve me")?;
    fs::write(destination.join("deleted-slot.dat"), b"old")?;
    let roots = [root(&destination)];
    let before = scan(&roots)?;
    let prepared = prepare_restore(&archive, &roots, "game", "Linux", 1)?;
    apply_restore(&prepared, &roots, &before)?;
    assert_eq!(
        fs::read(destination.join("profile/world/chunk.dat"))?,
        b"world"
    );
    assert!(destination.join("empty").is_dir());
    assert!(!destination.join("deleted-slot.dat").exists());
    assert_eq!(fs::read(destination.join("Cache/local"))?, b"preserve me");
    assert_eq!(
        scan(&roots)?.fingerprint()?,
        manifest.contents.fingerprint()?
    );
    Ok(())
}

#[test]
fn standalone_file_and_multiple_roots() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let mut file = root(&temp.path().join("progress.dat"));
    file.kind = Kind::File;
    file.id = "progress".into();
    fs::write(&file.path, b"progress")?;
    let directory = root(&temp.path().join("profiles"));
    fs::create_dir(&directory.path)?;
    let roots = [directory, file];
    let archive = temp.path().join("save.zst");
    capture(&roots, "game", "Windows", 3, &archive, MAX_BYTES)?;
    let prepared = prepare_restore(&archive, &roots, "game", "Windows", 3)?;
    assert_eq!(prepared.manifest.contents.files.len(), 1);
    assert!(prepare_restore(&archive, &roots, "other-game", "Windows", 3).is_err());
    assert!(prepare_restore(&archive, &roots, "game", "Linux", 3).is_err());
    assert!(prepare_restore(&archive, &roots, "game", "Windows", 4).is_err());
    Ok(())
}

#[test]
fn fingerprints_detect_same_size_edits_and_deletions() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let roots = [root(temp.path())];
    fs::write(temp.path().join("save"), b"one")?;
    let first = scan(&roots)?.fingerprint()?;
    fs::write(temp.path().join("save"), b"two")?;
    assert_ne!(first, scan(&roots)?.fingerprint()?);
    fs::remove_file(temp.path().join("save"))?;
    assert!(scan(&roots)?.files.is_empty());
    Ok(())
}

#[test]
fn refuses_overlaps_and_unsafe_manifest_paths() -> Result<()> {
    let temp = tempfile::tempdir()?;
    assert!(
        scan(&[
            root(temp.path()),
            Root {
                id: "nested".into(),
                ..root(&temp.path().join("nested"))
            }
        ])
        .is_err()
    );
    let roots = [root(temp.path())];
    let mut contents = scan(&roots)?;
    for path in [
        "../escape",
        "/absolute",
        "C:/escape",
        "a\\b",
        "CON",
        "file.",
    ] {
        contents.files = vec![FileEntry {
            root: "saves".into(),
            path: path.into(),
            bytes: 0,
            checksum: "a".repeat(64),
        }];
        assert!(validate_contents(&contents, &roots).is_err(), "{path}");
    }
    Ok(())
}

#[test]
fn restore_refuses_files_modified_since_confirmation() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let source = temp.path().join("saves");
    fs::create_dir(&source)?;
    fs::write(source.join("slot"), b"old")?;
    let roots = [root(&source)];
    let archive = temp.path().join("save.zst");
    capture(&roots, "game", "Linux", 1, &archive, MAX_BYTES)?;
    let before = scan(&roots)?;
    let prepared = prepare_restore(&archive, &roots, "game", "Linux", 1)?;
    fs::write(source.join("slot"), b"new")?;
    assert!(apply_restore(&prepared, &roots, &before).is_err());
    assert_eq!(fs::read(source.join("slot"))?, b"new");
    Ok(())
}

#[test]
fn rejects_extra_entries_and_file_checksum_corruption() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let roots = [root(&temp.path().join("saves"))];
    fs::create_dir(&roots[0].path)?;
    fs::write(roots[0].path.join("slot"), b"valid")?;
    let contents = scan(&roots)?;
    let manifest = Manifest {
        format: 1,
        game_id: "game".into(),
        namespace: "Linux".into(),
        definition_revision: 1,
        contents,
    };
    let path = temp.path().join("bad.zst");
    let mut archive = tar::Builder::new(zstd::Encoder::new(File::create(&path)?, 1)?);
    for (name, data) in [
        ("manifest.json", serde_json::to_vec(&manifest)?),
        ("files/0", b"wrong".to_vec()),
    ] {
        let mut header = tar::Header::new_gnu();
        header.set_size(data.len() as u64);
        header.set_mode(0o600);
        header.set_cksum();
        archive.append_data(&mut header, name, data.as_slice())?;
    }
    archive.into_inner()?.finish()?;
    assert!(prepare_restore(&path, &roots, "game", "Linux", 1).is_err());
    Ok(())
}

#[test]
fn recovery_removes_only_unchanged_files_introduced_into_previously_missing_roots() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let first = root(&temp.path().join("first"));
    fs::create_dir(&first.path)?;
    fs::write(first.path.join("save"), b"original")?;
    let mut second = root(&temp.path().join("second"));
    second.id = "second".into();
    let roots = [first, second];
    let local_archive = temp.path().join("local.zst");
    capture(&roots, "game", "Linux", 1, &local_archive, MAX_BYTES)?;
    fs::create_dir(&roots[1].path)?;
    fs::write(roots[1].path.join("world"), b"cloud")?;
    fs::write(roots[0].path.join("save"), b"cloud")?;
    let applied = scan(&roots)?;
    let prepared = prepare_restore(&local_archive, &roots, "game", "Linux", 1)?;
    assert!(apply_restore(&prepared, &roots, &applied).is_err());
    fs::write(roots[1].path.join("new-game-progress"), b"do not delete")?;
    assert!(apply_recovery(&prepared, &applied, &roots, &scan(&roots)?).is_err());
    assert_eq!(
        fs::read(roots[1].path.join("new-game-progress"))?,
        b"do not delete"
    );
    fs::remove_file(roots[1].path.join("new-game-progress"))?;
    apply_recovery(&prepared, &applied, &roots, &scan(&roots)?)?;
    assert_eq!(fs::read(roots[0].path.join("save"))?, b"original");
    assert!(!roots[1].path.join("world").exists());
    Ok(())
}

#[test]
fn enforces_expanded_file_count_and_archive_limits() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let roots = [root(temp.path())];
    let mut contents = scan(&roots)?;
    let file = FileEntry {
        root: "saves".into(),
        path: "large".into(),
        bytes: MAX_BYTES + 1,
        checksum: "a".repeat(64),
    };
    contents.files = vec![file.clone()];
    assert!(validate_contents(&contents, &roots).is_err());
    contents.files = vec![FileEntry { bytes: 0, ..file }; MAX_FILES + 1];
    assert!(validate_contents(&contents, &roots).is_err());
    let source = temp.path().join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("slot"), b"save")?;
    assert!(
        capture(
            &[root(&source)],
            "game",
            "Linux",
            1,
            &temp.path().join("tiny.zst"),
            1
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn rejects_link_entries_even_with_an_otherwise_valid_manifest() -> Result<()> {
    let temp = tempfile::tempdir()?;
    let source = temp.path().join("source");
    fs::create_dir(&source)?;
    fs::write(source.join("slot"), b"save")?;
    let roots = [root(&source)];
    let manifest = Manifest {
        format: 1,
        game_id: "game".into(),
        namespace: "Linux".into(),
        definition_revision: 1,
        contents: scan(&roots)?,
    };
    let archive_path = temp.path().join("link.zst");
    let mut archive = tar::Builder::new(zstd::Encoder::new(File::create(&archive_path)?, 1)?);
    let data = serde_json::to_vec(&manifest)?;
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    archive.append_data(&mut header, "manifest.json", data.as_slice())?;
    let mut link = tar::Header::new_gnu();
    link.set_entry_type(tar::EntryType::Symlink);
    link.set_size(0);
    link.set_mode(0o600);
    link.set_link_name("/outside")?;
    link.set_cksum();
    archive.append_data(&mut link, "files/0", io::empty())?;
    archive.into_inner()?.finish()?;
    assert!(prepare_restore(&archive_path, &roots, "game", "Linux", 1).is_err());
    Ok(())
}

#[cfg(unix)]
#[test]
fn rejects_symlinks_in_roots_and_children() -> Result<()> {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir()?;
    let real = temp.path().join("real");
    fs::create_dir(&real)?;
    let link = temp.path().join("link");
    symlink(&real, &link)?;
    assert!(scan(&[root(&link)]).is_err());
    symlink(temp.path().join("outside"), real.join("escape"))?;
    assert!(scan(&[root(&real)]).is_err());
    Ok(())
}
