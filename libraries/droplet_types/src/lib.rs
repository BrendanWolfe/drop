use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const ARCHIVE_MANIFEST_VERSION: &str = "archive-v1";
pub const ARCHIVE_FILENAME: &str = ".drop-archive";
pub const ARCHIVE_CACHE_DIR: &str = ".drop-archive-cache";
pub const ARCHIVE_INVENTORY: &str = ".drop-archive-files";

/// Apply the same extraction path rules on the server and every client OS.
pub fn archive_relative_path(name: &str) -> std::io::Result<std::path::PathBuf> {
    use std::{
        io,
        path::{Component, Path, PathBuf},
    };
    let invalid = || io::Error::other("Archive contains an unsafe or reserved path");
    let name = name.replace('\\', "/");
    if name.contains(':') || name.contains('\0') || name.starts_with('/') {
        return Err(invalid());
    }
    let mut result = PathBuf::new();
    for component in Path::new(&name).components() {
        match component {
            Component::Normal(part) => {
                let text = part.to_string_lossy();
                if text.ends_with('.')
                    || text.ends_with(' ')
                    || [
                        ".dropdata",
                        ".dropdata.tmp",
                        ".drop-local-game",
                        ARCHIVE_FILENAME,
                        ARCHIVE_CACHE_DIR,
                        ARCHIVE_INVENTORY,
                        ".drop-archive-files.tmp",
                    ]
                    .iter()
                    .any(|reserved| text.eq_ignore_ascii_case(reserved))
                {
                    return Err(invalid());
                }
                result.push(part);
            }
            Component::CurDir => (),
            _ => return Err(invalid()),
        }
    }
    if result.as_os_str().is_empty() {
        return Err(invalid());
    }
    Ok(result)
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ArchiveMetadata {
    pub install_size: u64,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct FileEntry {
    pub filename: String,
    pub start: usize,
    pub length: usize, // TODO: Replace with u64 for 32 bit clients
    pub permissions: u32,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ChunkData {
    pub files: Vec<FileEntry>,
    pub checksum: String,
    pub iv: [u8; 16],
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Manifest {
    pub version: String,
    pub chunks: HashMap<String, ChunkData>,
    pub size: u64,
    pub key: [u8; 16],
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<ArchiveMetadata>,
}
