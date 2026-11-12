//! Serve the compressed archive itself, never individual entries within it.
use std::{io::SeekFrom, path::PathBuf};

use anyhow::{bail, ensure};
use async_trait::async_trait;
use droplet_types::ARCHIVE_FILENAME;
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncSeekExt},
};

use super::types::{MinimumFileObject, VersionBackend, VersionFile};

pub struct RawArchiveBackend {
    path: PathBuf,
}

impl RawArchiveBackend {
    pub fn new(path: PathBuf) -> anyhow::Result<Self> {
        ensure!(
            path.is_file(),
            "Archive download mode requires a single archive file"
        );
        Ok(Self { path })
    }
}

#[async_trait]
impl VersionBackend for RawArchiveBackend {
    fn require_whole_files(&self) -> bool {
        false
    }

    async fn list_files(&self) -> anyhow::Result<Vec<VersionFile>> {
        Ok(vec![self.peek_file(ARCHIVE_FILENAME.to_string()).await?])
    }

    async fn peek_file(&self, sub_path: String) -> anyhow::Result<VersionFile> {
        ensure!(
            sub_path == ARCHIVE_FILENAME,
            "Invalid archive payload filename"
        );
        Ok(VersionFile {
            relative_filename: sub_path,
            permission: 0o600,
            size: tokio::fs::metadata(&self.path).await?.len(),
        })
    }

    async fn reader(
        &self,
        file: &VersionFile,
        start: u64,
        end: u64,
    ) -> anyhow::Result<Box<dyn MinimumFileObject>> {
        ensure!(
            file.relative_filename == ARCHIVE_FILENAME,
            "Invalid archive payload filename"
        );
        let mut reader = File::open(&self.path).await?;
        if start > end || end > reader.metadata().await?.len() {
            bail!("Invalid archive byte range");
        }
        reader.seek(SeekFrom::Start(start)).await?;
        Ok(Box::new(reader.take(end - start)))
    }
}
