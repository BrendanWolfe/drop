use std::{path::PathBuf, task::Poll};

use anyhow::anyhow;
use async_trait::async_trait;
use libarchive_drop::{
    archive::{Entry, FileType, ReadCompression, ReadFormat},
    reader::{Builder, FileReader, Reader},
};
use tokio::io::{AsyncRead, AsyncReadExt};

use crate::versions::types::{MinimumFileObject, VersionBackend, VersionFile};

pub struct ZipVersionBackend {
    path: PathBuf,
    strict: bool,
}
impl ZipVersionBackend {
    pub fn new(path: PathBuf) -> anyhow::Result<Self> {
        Ok(Self {
            path,
            strict: false,
        })
    }

    pub fn strict_zip(path: PathBuf) -> Self {
        Self { path, strict: true }
    }

    fn open_archive(&self) -> Result<FileReader, anyhow::Error> {
        let mut archive = Builder::new();
        if self.strict {
            archive.support_format(ReadFormat::Zip)?;
        } else {
            archive.support_format(ReadFormat::All)?;
            archive.support_compression(ReadCompression::All)?;
        }
        let archive = archive.open_file(&self.path)?;

        Ok(archive)
    }
}

struct ArchiveReader {
    archive: FileReader,
    prev_block: Option<Vec<u8>>,
}

impl AsyncRead for ArchiveReader {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        if buf.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }

        if let Some(block) = &mut self.prev_block {
            let to_read = buf.remaining().min(block.len());
            buf.put_slice(&block[..to_read]);
            block.drain(..to_read);

            if block.is_empty() {
                self.prev_block = None;
            }

            return Poll::Ready(Ok(()));
        }

        let block = match self.archive.read_block() {
            Ok(Some(v)) => v,
            Ok(None) => return Poll::Ready(Ok(())),
            Err(err) => return Poll::Ready(Err(std::io::Error::other(err.to_string()))),
        };

        let write_amount = buf.remaining().min(block.len());
        let (prefix, suffix) = block.split_at(write_amount);
        buf.put_slice(prefix);

        if !suffix.is_empty() {
            self.prev_block = Some(suffix.to_vec());
        }

        Poll::Ready(Ok(()))
    }
}

#[async_trait]
impl VersionBackend for ZipVersionBackend {
    async fn list_files(&self) -> anyhow::Result<Vec<VersionFile>> {
        let mut archive = self.open_archive()?;
        let mut results = Vec::new();

        while let Some(header) = if self.strict {
            archive.next_header_checked()?
        } else {
            archive.next_header()
        } {
            if self.strict {
                anyhow::ensure!(
                    header.hardlink().is_none(),
                    "ZIP companions cannot contain hard links"
                );
                let name = header.pathname().trim_end_matches('/');
                if name == "." && matches!(header.filetype(), FileType::Directory) {
                    continue;
                }
                droplet_types::archive_relative_path(name)?;
            }
            match header.filetype() {
                FileType::RegularFile => (),
                FileType::Directory => continue,
                _ => {
                    anyhow::ensure!(
                        !self.strict,
                        "ZIP companions cannot contain links or special entries"
                    );
                    continue;
                }
            }
            results.push(VersionFile {
                relative_filename: header.pathname().to_string(),
                permission: 0o744,
                size: header.size().try_into()?,
            });
        }

        Ok(results)
    }

    async fn reader(
        &self,
        file: &VersionFile,
        start: u64,
        end: u64,
    ) -> anyhow::Result<Box<dyn MinimumFileObject>> {
        let mut archive = self.open_archive()?;

        // Find entry in archive
        loop {
            let entry = match archive.next_header() {
                Some(v) => v,
                None => return Err(anyhow!("entry not found:{}", file.relative_filename)),
            };
            if entry.pathname() == file.relative_filename
                && matches!(entry.filetype(), FileType::RegularFile)
            {
                break;
            }
        }

        anyhow::ensure!(
            start == 0,
            "ZIP entries must be streamed from the beginning"
        );
        Ok(Box::new(
            ArchiveReader {
                archive,
                prev_block: None,
            }
            .take(end),
        ))
    }

    async fn peek_file(&self, sub_path: String) -> anyhow::Result<VersionFile> {
        let files = self.list_files().await?;
        let file = files
            .iter()
            .find(|v| v.relative_filename == sub_path)
            .expect("file not found");

        Ok(file.clone())
    }

    fn require_whole_files(&self) -> bool {
        true
    }
}
