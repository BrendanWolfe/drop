use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{self, Read},
    path::{Component, Path, PathBuf},
};
use walkdir::WalkDir;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;
pub const MAX_FILES: usize = 20_000;
pub const MAX_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_MANIFEST: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    File,
    Directory,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Root {
    pub id: String,
    pub path: PathBuf,
    pub kind: Kind,
    pub exclude: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RootEntry {
    pub id: String,
    pub kind: Kind,
    pub present: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileEntry {
    pub root: String,
    pub path: String,
    pub bytes: u64,
    pub checksum: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DirectoryEntry {
    pub root: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Contents {
    pub roots: Vec<RootEntry>,
    pub files: Vec<FileEntry>,
    pub directories: Vec<DirectoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub format: u32,
    pub game_id: String,
    pub namespace: String,
    pub definition_revision: u32,
    pub contents: Contents,
}

fn fail(message: &str) -> Box<dyn std::error::Error + Send + Sync> {
    io::Error::other(message).into()
}

pub fn hash_reader(mut reader: impl Read) -> Result<String> {
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

impl Contents {
    pub fn fingerprint(&self) -> Result<String> {
        hash_reader(serde_json::to_vec(self)?.as_slice())
    }
    pub fn has_saves(&self) -> bool {
        self.roots.iter().any(|r| r.present)
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 40
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}

fn relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && path.split('/').all(|p| {
            !p.is_empty()
                && p != "."
                && p != ".."
                && !p.ends_with(['.', ' '])
                && !p
                    .chars()
                    .any(|c| c.is_control() || "\\:<>\"|?*".contains(c))
                && ![
                    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6",
                    "COM7", "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7",
                    "LPT8", "LPT9",
                ]
                .contains(
                    &p.split('.')
                        .next()
                        .unwrap_or("")
                        .to_ascii_uppercase()
                        .as_str(),
                )
        })
}

/// Reject links in every existing ancestor, not just the last path component.
/// This protects against static symlink escapes. A hostile local process racing
/// filesystem operations is outside the client trust boundary.
pub fn safe_path(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err(fail("Save locations must be absolute."));
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir | Component::CurDir) {
            return Err(fail("Unsafe save path."));
        }
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(fail("Symbolic links are not supported in save locations."));
            }
            Ok(_) => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn exclusions(root: &Root) -> Result<GlobSet> {
    let mut set = GlobSetBuilder::new();
    for pattern in &root.exclude {
        if pattern.is_empty()
            || pattern.starts_with('/')
            || pattern.split('/').any(|p| p == "..")
            || pattern.contains('\\')
        {
            return Err(fail("Unsafe exclusion pattern."));
        }
        set.add(GlobBuilder::new(pattern).literal_separator(true).build()?);
    }
    Ok(set.build()?)
}

pub fn validate_roots(roots: &[Root]) -> Result<()> {
    if roots.is_empty() || roots.len() > 16 {
        return Err(fail(
            "No save locations for this platform, or too many locations.",
        ));
    }
    let mut ids = BTreeSet::new();
    for (index, root) in roots.iter().enumerate() {
        if !valid_id(&root.id) || !ids.insert(root.id.clone()) {
            return Err(fail("Invalid or duplicate save location ID."));
        }
        safe_path(&root.path)?;
        exclusions(root)?;
        let root_key = root.path.to_string_lossy().to_lowercase();
        for other in &roots[..index] {
            let other_key = other.path.to_string_lossy().to_lowercase();
            let separator = std::path::MAIN_SEPARATOR;
            if root_key == other_key
                || root_key.starts_with(&format!("{other_key}{separator}"))
                || other_key.starts_with(&format!("{root_key}{separator}"))
            {
                return Err(fail("Save locations must not overlap."));
            }
        }
    }
    Ok(())
}

fn source(root: &Root, path: &str) -> PathBuf {
    if root.kind == Kind::File {
        root.path.clone()
    } else {
        root.path.join(path)
    }
}

pub fn scan(roots: &[Root]) -> Result<Contents> {
    validate_roots(roots)?;
    let mut contents = Contents {
        roots: vec![],
        files: vec![],
        directories: vec![],
    };
    let mut bytes: u64 = 0;
    for root in roots {
        let meta = match fs::symlink_metadata(&root.path) {
            Ok(meta) => Some(meta),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        contents.roots.push(RootEntry {
            id: root.id.clone(),
            kind: root.kind.clone(),
            present: meta.is_some(),
        });
        let Some(meta) = meta else {
            continue;
        };
        if (root.kind == Kind::File && !meta.is_file())
            || (root.kind == Kind::Directory && !meta.is_dir())
        {
            return Err(fail(
                "Save location is not the configured file/directory type.",
            ));
        }
        let exclude = exclusions(root)?;
        let walker = WalkDir::new(&root.path)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|entry| {
                if entry.depth() == 0 {
                    return true;
                }
                let relative = entry
                    .path()
                    .strip_prefix(&root.path)
                    .unwrap_or(entry.path());
                !exclude.is_match(relative)
                    && !exclude.is_match(format!("{}/", relative.to_string_lossy()))
            });
        for entry in walker {
            let entry = entry?;
            if entry.depth() > 64 {
                return Err(fail("Save directory nesting is too deep."));
            }
            let path = if root.kind == Kind::File {
                "data".to_owned()
            } else {
                entry
                    .path()
                    .strip_prefix(&root.path)?
                    .to_str()
                    .ok_or_else(|| fail("Save filenames must be valid UTF-8."))?
                    .replace(std::path::MAIN_SEPARATOR, "/")
            };
            if entry.depth() == 0 && root.kind == Kind::Directory {
                continue;
            }
            if !relative(&path) {
                return Err(fail("Unsafe or non-portable save filename."));
            }
            if entry.file_type().is_dir() {
                contents.directories.push(DirectoryEntry {
                    root: root.id.clone(),
                    path,
                });
            } else if entry.file_type().is_file() {
                let meta = entry.metadata()?;
                bytes = bytes
                    .checked_add(meta.len())
                    .ok_or_else(|| fail("Save size overflow."))?;
                if bytes > MAX_BYTES {
                    return Err(fail("Expanded saves exceed 1 GiB."));
                }
                // Limit the read as well: a game may append while we hash it.
                let file = File::open(entry.path())?;
                let checksum = hash_reader(file.take(meta.len() + 1))?;
                if fs::metadata(entry.path())?.len() != meta.len() {
                    return Err(fail(
                        "Save files changed during capture. Try again after closing the game.",
                    ));
                }
                contents.files.push(FileEntry {
                    root: root.id.clone(),
                    path,
                    bytes: meta.len(),
                    checksum,
                });
            } else {
                return Err(fail("Links and special files cannot be included in saves."));
            }
            if contents.files.len() + contents.directories.len() > MAX_FILES {
                return Err(fail("Too many save files or directories."));
            }
        }
    }
    validate_contents(&contents, roots)?;
    Ok(contents)
}

fn validate_contents(contents: &Contents, roots: &[Root]) -> Result<()> {
    if contents.roots.len() != roots.len()
        || contents.files.len() + contents.directories.len() > MAX_FILES
    {
        return Err(fail("Invalid snapshot contents."));
    }
    for (entry, root) in contents.roots.iter().zip(roots) {
        if entry.id != root.id || entry.kind != root.kind {
            return Err(fail(
                "Snapshot locations do not match the current configuration.",
            ));
        }
    }
    let mut paths = BTreeMap::<(String, String), bool>::new();
    let mut bytes = 0u64;
    let filters: BTreeMap<_, _> = roots
        .iter()
        .map(|r| Ok((r.id.as_str(), exclusions(r)?)))
        .collect::<Result<_>>()?;
    let mut add = |root_id: &str, path: &str, is_file: bool| -> Result<()> {
        let root = contents
            .roots
            .iter()
            .find(|r| r.id == root_id && r.present)
            .ok_or_else(|| fail("Unknown or missing snapshot root."))?;
        if !relative(path) || (root.kind == Kind::File && (path != "data" || !is_file)) {
            return Err(fail("Invalid snapshot path."));
        }
        if root.kind == Kind::Directory {
            let filter = &filters[root_id];
            let mut parent = Some(Path::new(path));
            while let Some(p) = parent {
                if filter.is_match(p) || filter.is_match(format!("{}/", p.to_string_lossy())) {
                    return Err(fail("Snapshot contains excluded files."));
                }
                parent = p.parent().filter(|p| !p.as_os_str().is_empty());
            }
        }
        if paths
            .insert((root_id.to_owned(), path.to_lowercase()), is_file)
            .is_some()
        {
            return Err(fail("Duplicate or case-colliding snapshot path."));
        }
        Ok(())
    };
    for directory in &contents.directories {
        add(&directory.root, &directory.path, false)?;
    }
    for file in &contents.files {
        if file.checksum.len() != 64
            || !file
                .checksum
                .bytes()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
        {
            return Err(fail("Invalid file checksum."));
        }
        bytes = bytes
            .checked_add(file.bytes)
            .ok_or_else(|| fail("Save size overflow."))?;
        if bytes > MAX_BYTES {
            return Err(fail("Expanded saves exceed 1 GiB."));
        }
        add(&file.root, &file.path, true)?;
    }
    for (root, path) in paths.keys() {
        let mut parent = Path::new(path).parent();
        while let Some(p) = parent.filter(|p| !p.as_os_str().is_empty()) {
            if paths.get(&(root.clone(), p.to_string_lossy().into_owned())) != Some(&false) {
                return Err(fail("Missing directory or file/directory path collision."));
            }
            parent = p.parent();
        }
    }
    for root in &contents.roots {
        if root.present
            && root.kind == Kind::File
            && !contents.files.iter().any(|f| f.root == root.id)
        {
            return Err(fail("Snapshot file is missing."));
        }
    }
    Ok(())
}

/// Copies into private staging, then rescans live files before publishing.
pub fn capture(
    roots: &[Root],
    game_id: &str,
    namespace: &str,
    definition_revision: u32,
    output: &Path,
    archive_limit: u64,
) -> Result<Manifest> {
    let before = scan(roots)?;
    if !before.has_saves() {
        return Err(fail("No local saves found yet."));
    }
    let staging = tempfile::tempdir()?;
    for (index, entry) in before.files.iter().enumerate() {
        let root = roots
            .iter()
            .find(|r| r.id == entry.root)
            .ok_or_else(|| fail("Unknown root."))?;
        let src = source(root, &entry.path);
        safe_path(&src)?;
        let mut input = File::open(src)?.take(entry.bytes + 1);
        let path = staging.path().join(index.to_string());
        let mut dest = File::create(&path)?;
        if io::copy(&mut input, &mut dest)? != entry.bytes
            || hash_reader(File::open(path)?)? != entry.checksum
        {
            return Err(fail(
                "Save files changed during capture. Try again after closing the game.",
            ));
        }
    }
    if before != scan(roots)? {
        return Err(fail(
            "Save files changed during capture. Try again after closing the game.",
        ));
    }
    let manifest = Manifest {
        format: 1,
        game_id: game_id.into(),
        namespace: namespace.into(),
        definition_revision,
        contents: before,
    };
    let output_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    let mut archive = tar::Builder::new(zstd::Encoder::new(output_file, 3)?);
    let data = serde_json::to_vec(&manifest)?;
    if data.len() as u64 > MAX_MANIFEST {
        return Err(fail("Snapshot manifest too large."));
    }
    let mut header = tar::Header::new_gnu();
    header.set_size(data.len() as u64);
    header.set_mode(0o600);
    header.set_cksum();
    archive.append_data(&mut header, "manifest.json", data.as_slice())?;
    for index in 0..manifest.contents.files.len() {
        archive.append_file(
            format!("files/{index}"),
            &mut File::open(staging.path().join(index.to_string()))?,
        )?;
    }
    archive.into_inner()?.finish()?.sync_all()?;
    if fs::metadata(output)?.len() > archive_limit {
        return Err(fail("Cloud save exceeds the server archive size limit."));
    }
    Ok(manifest)
}

pub struct PreparedRestore {
    staging: tempfile::TempDir,
    pub manifest: Manifest,
}

pub fn inspect_manifest(archive: &Path) -> Result<Manifest> {
    let mut decoder = zstd::Decoder::new(File::open(archive)?)?;
    decoder.window_log_max(27)?;
    let mut tar = tar::Archive::new(decoder.take(MAX_MANIFEST + 1024));
    let mut entries = tar.entries()?.raw(true);
    let mut entry = entries
        .next()
        .ok_or_else(|| fail("Empty snapshot archive."))??;
    if !entry.header().entry_type().is_file()
        || entry.path()?.as_ref() != Path::new("manifest.json")
        || entry.size() > MAX_MANIFEST
    {
        return Err(fail("Invalid snapshot manifest entry."));
    }
    let mut data = Vec::new();
    entry.read_to_end(&mut data)?;
    Ok(serde_json::from_slice(&data)?)
}

/// Strict reader: never let tar choose destination paths or create links.
pub fn prepare_restore(
    archive: &Path,
    roots: &[Root],
    game_id: &str,
    namespace: &str,
    definition_revision: u32,
) -> Result<PreparedRestore> {
    validate_roots(roots)?;
    let mut decoder = zstd::Decoder::new(File::open(archive)?)?;
    decoder.window_log_max(27)?;
    let mut tar =
        tar::Archive::new(decoder.take(MAX_BYTES + MAX_MANIFEST + (MAX_FILES as u64 * 2048)));
    let mut entries = tar.entries()?.raw(true);
    let mut first = entries
        .next()
        .ok_or_else(|| fail("Empty snapshot archive."))??;
    if !first.header().entry_type().is_file()
        || first.path()?.as_ref() != Path::new("manifest.json")
        || first.size() > MAX_MANIFEST
    {
        return Err(fail("Invalid snapshot manifest entry."));
    }
    let mut data = Vec::new();
    first.read_to_end(&mut data)?;
    drop(first);
    let manifest: Manifest = serde_json::from_slice(&data)?;
    if manifest.format != 1
        || manifest.game_id != game_id
        || manifest.namespace != namespace
        || manifest.definition_revision != definition_revision
    {
        return Err(fail(
            "Snapshot identity or configuration does not match. Restore requires its original save definition.",
        ));
    }
    validate_contents(&manifest.contents, roots)?;
    let staging = tempfile::tempdir()?;
    for (index, file) in manifest.contents.files.iter().enumerate() {
        let mut entry = entries
            .next()
            .ok_or_else(|| fail("Snapshot file missing."))??;
        if !entry.header().entry_type().is_file()
            || entry.path()?.as_ref() != Path::new(&format!("files/{index}"))
            || entry.size() != file.bytes
        {
            return Err(fail("Invalid snapshot file entry."));
        }
        let path = staging.path().join(index.to_string());
        let mut output = File::create(&path)?;
        if io::copy(&mut entry, &mut output)? != file.bytes
            || hash_reader(File::open(path)?)? != file.checksum
        {
            return Err(fail("Snapshot file checksum mismatch."));
        }
    }
    if entries.next().is_some() {
        return Err(fail("Unexpected archive entry."));
    }
    Ok(PreparedRestore { staging, manifest })
}

/// Restore only configured files. Exclusions and unrelated files are preserved.
/// The caller must durably record a recovery archive before invoking this.
pub fn apply_restore(
    prepared: &PreparedRestore,
    roots: &[Root],
    expected_local: &Contents,
) -> Result<()> {
    if &scan(roots)? != expected_local {
        return Err(fail(
            "Local saves changed while preparing restore. Nothing was replaced.",
        ));
    }
    for root in &prepared.manifest.contents.roots {
        if !root.present && expected_local.files.iter().any(|f| f.root == root.id) {
            return Err(fail(
                "The cloud snapshot is missing a save location that contains local saves. Refusing to merge incomplete save sets.",
            ));
        }
    }
    // Reject shape conflicts before modifying anything. These need explicit
    // intervention rather than recursively deleting a directory to make room.
    for entry in &prepared.manifest.contents.files {
        let root = roots
            .iter()
            .find(|r| r.id == entry.root)
            .ok_or_else(|| fail("Unknown restore root."))?;
        let dest = source(root, &entry.path);
        safe_path(&dest)?;
        if dest.is_dir() {
            return Err(fail("A restore file would replace an existing directory."));
        }
    }
    for entry in &prepared.manifest.contents.directories {
        let root = roots
            .iter()
            .find(|r| r.id == entry.root)
            .ok_or_else(|| fail("Unknown restore root."))?;
        let dest = root.path.join(&entry.path);
        safe_path(&dest)?;
        if dest.is_file() {
            return Err(fail("A restore directory would replace an existing file."));
        }
    }
    for entry in &prepared.manifest.contents.roots {
        if entry.present && entry.kind == Kind::Directory {
            let root = roots
                .iter()
                .find(|r| r.id == entry.id)
                .ok_or_else(|| fail("Unknown restore root."))?;
            safe_path(&root.path)?;
            fs::create_dir_all(&root.path)?;
        }
    }
    for directory in &prepared.manifest.contents.directories {
        let root = roots
            .iter()
            .find(|r| r.id == directory.root)
            .ok_or_else(|| fail("Unknown restore root."))?;
        let dest = root.path.join(&directory.path);
        safe_path(&dest)?;
        fs::create_dir_all(dest)?;
    }
    for (index, entry) in prepared.manifest.contents.files.iter().enumerate() {
        let root = roots
            .iter()
            .find(|r| r.id == entry.root)
            .ok_or_else(|| fail("Unknown restore root."))?;
        let dest = source(root, &entry.path);
        safe_path(&dest)?;
        let parent = dest
            .parent()
            .ok_or_else(|| fail("Invalid restore destination."))?;
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        io::copy(
            &mut File::open(prepared.staging.path().join(index.to_string()))?,
            &mut temp,
        )?;
        temp.as_file().sync_all()?;
        temp.persist(&dest).map_err(|e| e.error)?;
    }
    // Only remove files enumerated in the pre-restore scan, not the whole tree.
    // A missing root in an archive is not a request to wipe its local folder.
    for old in &expected_local.files {
        let present = prepared
            .manifest
            .contents
            .roots
            .iter()
            .any(|r| r.id == old.root && r.present);
        if present
            && !prepared
                .manifest
                .contents
                .files
                .iter()
                .any(|f| f.root == old.root && f.path == old.path)
        {
            let root = roots
                .iter()
                .find(|r| r.id == old.root)
                .ok_or_else(|| fail("Unknown root."))?;
            let dest = source(root, &old.path);
            safe_path(&dest)?;
            // Do not remove a file that the game has rewritten mid-restore.
            if hash_reader(File::open(&dest)?)? != old.checksum {
                return Err(fail(
                    "A save changed during restore. Recovery backup has been preserved.",
                ));
            }
            fs::remove_file(dest)?;
        }
    }
    let mut old_directories = expected_local.directories.clone();
    old_directories.sort_by_key(|d| std::cmp::Reverse(d.path.len()));
    for old in old_directories {
        let present = prepared
            .manifest
            .contents
            .roots
            .iter()
            .any(|r| r.id == old.root && r.present);
        if present
            && !prepared
                .manifest
                .contents
                .directories
                .iter()
                .any(|d| d.root == old.root && d.path == old.path)
        {
            let root = roots
                .iter()
                .find(|r| r.id == old.root)
                .ok_or_else(|| fail("Unknown root."))?;
            let dest = root.path.join(old.path);
            safe_path(&dest)?;
            match fs::remove_dir(dest) {
                Ok(()) => (),
                Err(error)
                    if error.kind() == io::ErrorKind::DirectoryNotEmpty
                        || error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}

/// Roll back an interrupted restore. For roots that originally did not exist,
/// remove only unchanged files known to have come from that interrupted restore.
pub fn apply_recovery(
    prepared: &PreparedRestore,
    applied: &Contents,
    roots: &[Root],
    expected_local: &Contents,
) -> Result<()> {
    if &scan(roots)? != expected_local {
        return Err(fail("Saves changed before recovery."));
    }
    let absent: BTreeSet<_> = prepared
        .manifest
        .contents
        .roots
        .iter()
        .filter(|r| !r.present)
        .map(|r| r.id.as_str())
        .collect();
    for file in expected_local
        .files
        .iter()
        .filter(|f| absent.contains(f.root.as_str()))
    {
        if !applied
            .files
            .iter()
            .any(|f| f.root == file.root && f.path == file.path && f.checksum == file.checksum)
        {
            return Err(fail(
                "Recovery would delete saves not introduced by the interrupted restore. Files were preserved.",
            ));
        }
    }
    for file in expected_local
        .files
        .iter()
        .filter(|f| absent.contains(f.root.as_str()))
    {
        let root = roots
            .iter()
            .find(|r| r.id == file.root)
            .ok_or_else(|| fail("Unknown root."))?;
        let dest = source(root, &file.path);
        safe_path(&dest)?;
        if hash_reader(File::open(&dest)?)? != file.checksum {
            return Err(fail("Save changed during recovery."));
        }
        fs::remove_file(dest)?;
    }
    apply_restore(prepared, roots, &scan(roots)?)
}

#[cfg(test)]
mod tests;
