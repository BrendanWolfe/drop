#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::{io::SeekFrom, path::PathBuf};

use anyhow::anyhow;
use async_trait::async_trait;
use tokio::{
    fs::File,
    io::{AsyncReadExt as _, AsyncSeekExt as _},
};

#[derive(Clone)]
pub struct PathVersionBackend {
    pub base_dir: PathBuf,
    pub only_file: Option<String>,
}

impl PathVersionBackend {
    fn payload(&self, name: &str) -> anyhow::Result<PathBuf> {
        let relative = name.strip_prefix("./").unwrap_or(name);
        anyhow::ensure!(
            self.only_file
                .as_deref()
                .is_none_or(|file| file == relative),
            "File does not belong to this version"
        );
        confined_path(&self.base_dir, relative, false)
    }
}

pub fn confined_path(
    base: &std::path::Path,
    relative: &str,
    allow_root: bool,
) -> anyhow::Result<PathBuf> {
    use std::path::Component;
    let mut current = base.canonicalize()?;
    let relative = std::path::Path::new(relative);
    anyhow::ensure!(
        allow_root || !relative.as_os_str().is_empty(),
        "Missing source path"
    );
    for part in relative.components() {
        let Component::Normal(part) = part else {
            return Err(anyhow!("Invalid source path"));
        };
        current.push(part);
        anyhow::ensure!(
            !current.symlink_metadata()?.is_symlink(),
            "Linked source files are not supported"
        );
    }
    if !allow_root {
        anyhow::ensure!(
            current.symlink_metadata()?.is_file(),
            "Expected a regular source file"
        );
    }
    Ok(current)
}

use crate::versions::{
    _list_files,
    types::{MinimumFileObject, VersionBackend, VersionFile},
};

#[async_trait]
impl VersionBackend for PathVersionBackend {
    async fn list_files(&self) -> anyhow::Result<Vec<VersionFile>> {
        if let Some(file) = &self.only_file {
            return Ok(vec![self.peek_file(file.clone()).await?]);
        }
        let mut vec = Vec::new();
        _list_files(&mut vec, &self.base_dir)?;

        let mut results = Vec::new();

        for pathbuf in vec.iter() {
            let relative = pathbuf.strip_prefix(self.base_dir.clone())?;

            results.push(
                self.peek_file(
                    relative
                        .to_str()
                        .ok_or(anyhow!(
                            "Could not parse path: {}",
                            relative.to_string_lossy()
                        ))?
                        .to_owned(),
                )
                .await?,
            );
        }

        Ok(results)
    }

    async fn reader(
        &self,
        file: &VersionFile,
        start: u64,
        end: u64,
    ) -> anyhow::Result<Box<dyn MinimumFileObject>> {
        let mut file = File::open(self.payload(&file.relative_filename)?).await?;

        if start != 0 {
            file.seek(SeekFrom::Start(start)).await?;
        }

        anyhow::ensure!(start <= end, "Invalid source file range");
        Ok(Box::new(file.take(end - start)))
    }

    async fn peek_file(&self, sub_path: String) -> anyhow::Result<VersionFile> {
        let pathbuf = self.payload(&sub_path)?;
        if !pathbuf.exists() {
            return Err(anyhow!("Path doesn't exist: {}", pathbuf.to_string_lossy()));
        };

        let file = File::open(pathbuf.clone()).await?;
        let metadata = file.try_clone().await?.metadata().await?;
        let permission_object = metadata.permissions();
        let permissions = {
            let perm: u32;
            #[cfg(target_family = "unix")]
            {
                perm = permission_object.mode();
            }
            #[cfg(not(target_family = "unix"))]
            {
                perm = 0
            }
            perm
        };

        Ok(VersionFile {
            relative_filename: sub_path,
            permission: permissions,
            size: metadata.len(),
        })
    }

    fn require_whole_files(&self) -> bool {
        false
    }
}
