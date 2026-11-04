//! Extract directly into a Drop-owned directory. Never change the process cwd.
use crate::local::OWNER_FILE;
use download_manager::util::{
    download_thread_control_flag::{DownloadThreadControl, DownloadThreadControlFlag},
    progress_object::{ProgressHandle, ProgressObject},
};
use libarchive_drop::{
    archive::{Entry, FileType, ReadFilter, ReadFormat},
    reader::{Builder, FileReader, Reader},
};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

fn invalid(message: impl ToString) -> io::Error {
    io::Error::other(message.to_string())
}

pub fn safe_relative(name: &str) -> io::Result<PathBuf> {
    // Treat Windows separators and drive/ADS paths as such on every host.
    let name = name.replace('\\', "/");
    if name.contains(':') || name.contains('\0') || name.starts_with('/') {
        return Err(invalid("Archive contains an absolute or invalid path"));
    }
    let mut result = PathBuf::new();
    for component in Path::new(&name).components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_string_lossy();
                if text.ends_with('.')
                    || text.ends_with(' ')
                    || text.eq_ignore_ascii_case(OWNER_FILE)
                {
                    return Err(invalid("Archive contains a reserved path"));
                }
                result.push(part);
            }
            Component::CurDir => (),
            _ => return Err(invalid("Archive path escapes the game folder")),
        }
    }
    if result.as_os_str().is_empty() {
        return Err(invalid("Empty archive path"));
    }
    Ok(result)
}

fn reader(path: &Path) -> io::Result<FileReader> {
    let mut builder = Builder::new();
    builder.support_filter(ReadFilter::All).map_err(invalid)?;
    // All enables both RAR4 and RAR5 on supported libarchive releases.
    // Explicit magic validation below prevents the raw/mtree handlers accepting HTML.
    builder.support_format(ReadFormat::All).map_err(invalid)?;
    builder.open_file(path).map_err(invalid)
}

fn check_magic(path: &Path) -> io::Result<()> {
    use std::io::Read;
    let mut bytes = [0u8; 512];
    let length = fs::File::open(path)?.read(&mut bytes)?;
    let data = &bytes[..length];
    if data.starts_with(b"PK\x03\x04")
        || data.starts_with(b"PK\x05\x06")
        || data.starts_with(b"7z\xbc\xaf\x27\x1c")
        || data.starts_with(b"Rar!\x1a\x07")
        || data.starts_with(b"\x1f\x8b")
        || data.starts_with(b"BZh")
        || data.starts_with(b"\xfd7zXZ\0")
        || data.get(257..262) == Some(b"ustar")
    {
        Ok(())
    } else {
        Err(invalid(
            "Not a supported archive. Use a direct ZIP, 7z, RAR, or tar download link.",
        ))
    }
}

fn destination(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(invalid("Extraction path contains a symbolic link"));
            }
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error),
        }
    }
    Ok(current)
}

pub fn extract(
    path: &Path,
    root: &Path,
    id: &str,
    control: &DownloadThreadControl,
    progress: &Arc<ProgressObject>,
) -> io::Result<bool> {
    if fs::read_to_string(root.join(OWNER_FILE))? != id {
        return Err(invalid(
            "This folder is not owned by this archive installation",
        ));
    }
    check_magic(path)?;
    let mut archive = reader(path)?;
    let mut total = 0usize;
    let mut count = 0usize;
    while let Some(entry) = archive.next_header_checked().map_err(invalid)? {
        if control.get() == DownloadThreadControlFlag::Stop {
            return Ok(false);
        }
        let name = entry.pathname_checked().map_err(invalid)?;
        // Root directory entries such as ./ carry no content.
        if matches!(name, "." | "./") && matches!(entry.filetype(), FileType::Directory) {
            continue;
        }
        safe_relative(name)?;
        if entry.has_hardlink()
            || !matches!(
                entry.filetype(),
                FileType::Directory | FileType::RegularFile
            )
        {
            return Err(invalid("Archive links and special files are not supported"));
        }
        total = total
            .checked_add(usize::try_from(entry.size().max(0)).map_err(invalid)?)
            .ok_or_else(|| invalid("Archive is too large"))?;
        count += 1;
        if count > 1_000_000 {
            return Err(invalid("Archive contains too many entries"));
        }
    }
    if count == 0 {
        return Err(invalid("Archive contains no files"));
    }
    progress.set_size(1);
    progress.set_max(total);
    progress.reset();
    let handle = ProgressHandle::new(progress.get(0), progress.clone());
    let mut archive = reader(path)?;
    while let Some(entry) = archive.next_header_checked().map_err(invalid)? {
        if control.get() == DownloadThreadControlFlag::Stop {
            return Ok(false);
        }
        let name = entry.pathname_checked().map_err(invalid)?;
        if matches!(name, "." | "./") && matches!(entry.filetype(), FileType::Directory) {
            continue;
        }
        let target = destination(root, &safe_relative(name)?)?;
        let directory = matches!(entry.filetype(), FileType::Directory);
        let permissions = entry.permissions();
        if directory {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&target)?;
        let mut buffer = vec![0u8; 128 * 1024];
        loop {
            if control.get() == DownloadThreadControlFlag::Stop {
                return Ok(false);
            }
            let length = archive.read_data(&mut buffer).map_err(invalid)?;
            if length == 0 {
                break;
            }
            file.write_all(&buffer[..length])?;
            handle.add(length);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(
                &target,
                fs::Permissions::from_mode((permissions & 0o777) | 0o600),
            )?;
        }
        #[cfg(not(unix))]
        let _ = permissions;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use download_manager::util::progress_object::ProgressType;

    fn extract_fixture(path: &Path) {
        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(OWNER_FILE), "test-game").unwrap();
        let (sender, _receiver) = tokio::sync::mpsc::channel(1500);
        let progress = Arc::new(ProgressObject::new(0, 1, sender, ProgressType::Disk));
        let control = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
        assert!(extract(path, directory.path(), "test-game", &control, &progress).unwrap());
        assert!(progress.get_max() > 0);
        assert_eq!(progress.sum(), progress.get_max());
    }

    #[test]
    fn extracts_rar4_rar5_solid_zip_and_tar() {
        for filename in [
            "test_read_format_rar_windows.rar",
            "test_read_format_rar5_compressed.rar",
            "test_read_format_rar5_solid.rar",
            "game.zip",
            "game.tar.gz",
        ] {
            extract_fixture(
                &Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures")
                    .join(filename),
            );
        }
    }

    #[test]
    fn extracts_seven_zip() {
        extract_fixture(
            &Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../libraries/droplet/testfiles/bare_files/standard_archive.7z"),
        );
    }

    #[test]
    fn refuses_unowned_folders_and_html_downloads() {
        let directory = tempfile::tempdir().unwrap();
        let archive = directory.path().join("not-an-archive.zip");
        fs::write(&archive, b"<html>Sign in to download</html>").unwrap();
        assert!(check_magic(&archive).is_err());
        let (sender, _) = tokio::sync::mpsc::channel(10);
        let progress = Arc::new(ProgressObject::new(0, 1, sender, ProgressType::Disk));
        let control = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
        assert!(extract(&archive, directory.path(), "test-game", &control, &progress).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn refuses_existing_symlinks_inside_the_destination() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("linked")).unwrap();
        assert!(destination(root.path(), Path::new("linked/file")).is_err());
    }

    #[test]
    fn damaged_archives_do_not_complete_successfully() {
        let directory = tempfile::tempdir().unwrap();
        let data = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/game.zip"))
            .unwrap();
        let path = directory.path().join("broken.zip");
        fs::write(&path, &data[..35]).unwrap();
        fs::write(directory.path().join(OWNER_FILE), "test-game").unwrap();
        let (sender, _) = tokio::sync::mpsc::channel(10);
        let progress = Arc::new(ProgressObject::new(0, 1, sender, ProgressType::Disk));
        let control = DownloadThreadControl::new(DownloadThreadControlFlag::Go);
        assert!(extract(&path, directory.path(), "test-game", &control, &progress).is_err());
    }
    #[test]
    fn rejects_escape_paths_on_every_platform() {
        for path in [
            "../outside",
            "game/../../outside",
            "/absolute",
            "C:\\outside",
            "\\\\server\\share",
            "file:stream",
            ".drop-local-game",
            "folder/../outside",
        ] {
            assert!(safe_relative(path).is_err(), "{path}");
        }
        assert_eq!(
            safe_relative("Game\\bin\\play.exe").unwrap(),
            PathBuf::from("Game/bin/play.exe")
        );
    }
}
