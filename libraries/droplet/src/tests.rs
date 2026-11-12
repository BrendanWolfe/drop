#![cfg(test)]
extern crate test_generator;

use std::path::Path;

use test_generator::test_resources;
use tokio::io::SimplexStream;

use crate::manifest::{ManifestWriterFactory, generate_manifest_rusty};

#[test_resources("testfiles/**/*.7z")]
fn manifest_gen(resource: &str) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("failed to create tokio runtime");

    runtime.block_on(async move {
        let filepath = Path::new(resource);
        let manifest = generate_manifest_rusty(
            filepath,
            |_| {},
            |message| {
                println!("({}) {}", filepath.display(), message);
            },
            None::<&dyn ManifestWriterFactory<Writer = SimplexStream>>, // Dummy type signature, not actually used
            None,
        )
        .await
        .unwrap_or_else(|err| {
            panic!(
                "failed to generate manifest for {}: {:?}",
                filepath.display(),
                err
            )
        });

        let first_chunk = manifest
            .chunks
            .values()
            .next()
            .expect("no chunks generated");
        let first_chunk_length = first_chunk.files.len();
        if first_chunk_length == 0 {
            panic!("{} has no files in manifest", filepath.display());
        }
    });
}

#[cfg(test)]
mod archive_reader {
    use std::future::poll_fn;
    use std::path::{Path, PathBuf};
    use std::pin::Pin;

    use tokio::io::{AsyncRead, AsyncReadExt};

    use crate::versions::{
        archive_backend::ZipVersionBackend,
        types::{VersionBackend, VersionFile},
    };

    const BLOCK_SIZE: usize = 10240;
    const PAYLOAD_LEN: usize = BLOCK_SIZE * 2 + 123;

    fn payload() -> Vec<u8> {
        (0..PAYLOAD_LEN)
            .map(|i| (i.wrapping_mul(31).wrapping_add(i / 7) & 0xff) as u8)
            .collect()
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut crc = 0xffff_ffffu32;
        for &byte in data {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                let mask = (crc & 1).wrapping_neg();
                crc = (crc >> 1) ^ (0xedb8_8320 & mask);
            }
        }
        !crc
    }

    fn push_u16(out: &mut Vec<u8>, value: u16) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn push_u32(out: &mut Vec<u8>, value: u32) {
        out.extend_from_slice(&value.to_le_bytes());
    }

    fn zip_stored(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();

        for (name, data) in entries {
            let offset = out.len() as u32;
            let name = name.as_bytes();
            let size = data.len() as u32;
            let crc = crc32(data);

            push_u32(&mut out, 0x0403_4b50);
            push_u16(&mut out, 20);
            push_u16(&mut out, 0);
            push_u16(&mut out, 0);
            push_u16(&mut out, 0);
            push_u16(&mut out, 0x0021);
            push_u32(&mut out, crc);
            push_u32(&mut out, size);
            push_u32(&mut out, size);
            push_u16(&mut out, name.len() as u16);
            push_u16(&mut out, 0);
            out.extend_from_slice(name);
            out.extend_from_slice(data);

            push_u32(&mut central, 0x0201_4b50);
            push_u16(&mut central, 20);
            push_u16(&mut central, 20);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0x0021);
            push_u32(&mut central, crc);
            push_u32(&mut central, size);
            push_u32(&mut central, size);
            push_u16(&mut central, name.len() as u16);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u32(&mut central, 0o100644 << 16);
            push_u32(&mut central, offset);
            central.extend_from_slice(name);
        }

        let central_offset = out.len() as u32;
        let central_size = central.len() as u32;
        out.extend_from_slice(&central);

        if entries.len() >= u16::MAX as usize {
            let zip64_offset = out.len() as u64;
            push_u32(&mut out, 0x0606_4b50);
            out.extend_from_slice(&44u64.to_le_bytes());
            push_u16(&mut out, 45);
            push_u16(&mut out, 45);
            push_u32(&mut out, 0);
            push_u32(&mut out, 0);
            out.extend_from_slice(&(entries.len() as u64).to_le_bytes());
            out.extend_from_slice(&(entries.len() as u64).to_le_bytes());
            out.extend_from_slice(&(central_size as u64).to_le_bytes());
            out.extend_from_slice(&(central_offset as u64).to_le_bytes());
            push_u32(&mut out, 0x0706_4b50);
            push_u32(&mut out, 0);
            out.extend_from_slice(&zip64_offset.to_le_bytes());
            push_u32(&mut out, 1);
        }

        push_u32(&mut out, 0x0605_4b50);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, entries.len().min(u16::MAX as usize) as u16);
        push_u16(&mut out, entries.len().min(u16::MAX as usize) as u16);
        push_u32(&mut out, central_size);
        push_u32(&mut out, central_offset);
        push_u16(&mut out, 0);

        out
    }

    struct TempZip(PathBuf);

    impl TempZip {
        fn new(bytes: &[u8]) -> Self {
            let path = std::env::temp_dir().join(format!(
                "droplet-archive-reader-{}.zip",
                uuid::Uuid::new_v4()
            ));
            std::fs::write(&path, bytes).expect("write zip fixture");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempZip {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    async fn read_all<R: AsyncRead + Unpin + ?Sized>(reader: &mut R, buffer_len: usize) -> Vec<u8> {
        let mut result = Vec::new();
        let mut buffer = vec![0u8; buffer_len];
        loop {
            let read = reader.read(&mut buffer).await.expect("read from archive");
            if read == 0 {
                break;
            }
            result.extend_from_slice(&buffer[..read]);
        }
        result
    }

    async fn read_with_buffer(
        backend: &ZipVersionBackend,
        file: &VersionFile,
        buffer_len: usize,
    ) -> Vec<u8> {
        let mut reader = backend
            .reader(file, 0, file.size)
            .await
            .expect("create reader");
        read_all(&mut *reader, buffer_len).await
    }

    fn find_file<'a>(files: &'a [VersionFile], name: &str) -> &'a VersionFile {
        files
            .iter()
            .find(|file| file.relative_filename == name)
            .unwrap_or_else(|| panic!("missing archive entry: {name}"))
    }

    #[tokio::test]
    async fn zip_reader_preserves_bytes_across_small_and_block_sized_buffers() {
        let payload = payload();
        let first = b"first".as_slice();
        let last = b"last entry, should not leak into the payload".as_slice();
        let zip = zip_stored(&[
            ("first.txt", first),
            ("nested/payload.bin", &payload),
            ("last.txt", last),
        ]);
        let temp = TempZip::new(&zip);
        let backend = ZipVersionBackend::new(temp.path().to_path_buf()).expect("open backend");

        let files = backend.list_files().await.expect("list files");
        assert_eq!(files.len(), 3);
        let payload_file = find_file(&files, "nested/payload.bin");
        assert_eq!(payload_file.size, payload.len() as u64);

        for buffer_len in [
            1,
            2,
            3,
            7,
            31,
            512,
            4095,
            4096,
            4097,
            BLOCK_SIZE,
            BLOCK_SIZE + 1,
            payload.len(),
        ] {
            let actual = read_with_buffer(&backend, payload_file, buffer_len).await;
            assert_eq!(actual, payload, "buffer size {buffer_len}");
        }

        assert_eq!(
            read_with_buffer(&backend, find_file(&files, "first.txt"), 4096).await,
            first
        );
        assert_eq!(
            read_with_buffer(&backend, find_file(&files, "last.txt"), 4096).await,
            last
        );
    }

    #[tokio::test]
    async fn zero_capacity_read_does_not_consume_a_block() {
        let payload = payload();
        let zip = zip_stored(&[("payload.bin", &payload)]);
        let temp = TempZip::new(&zip);
        let backend = ZipVersionBackend::new(temp.path().to_path_buf()).expect("open backend");
        let file = backend
            .peek_file("payload.bin".to_string())
            .await
            .expect("peek file");
        let mut reader = backend
            .reader(&file, 0, file.size)
            .await
            .expect("create reader");

        for _ in 0..3 {
            let mut empty: [u8; 0] = [];
            poll_fn(|cx| {
                let mut read_buf = tokio::io::ReadBuf::new(&mut empty);
                AsyncRead::poll_read(Pin::new(&mut *reader), cx, &mut read_buf)
            })
            .await
            .expect("zero capacity read");
        }

        assert_eq!(read_all(&mut *reader, 4096).await, payload);
    }

    #[tokio::test]
    async fn archive_download_hashes_the_original_archive_and_serves_exact_ranges() {
        use crate::{
            manifest::generate_archive_manifest, versions::raw_archive_backend::RawArchiveBackend,
        };
        use droplet_types::{ARCHIVE_FILENAME, ARCHIVE_MANIFEST_VERSION};
        use sha2::{Digest, Sha256};
        let bytes = zip_stored(&[("game/play.exe", b"game"), ("game/assets.bin", b"assets")]);
        let temp = TempZip::new(&bytes);
        let manifest = generate_archive_manifest(temp.path(), |_| {}, |_| {})
            .await
            .unwrap();
        assert_eq!(manifest.version, ARCHIVE_MANIFEST_VERSION);
        assert_eq!(manifest.size, bytes.len() as u64);
        assert_eq!(manifest.archive.unwrap().install_size, 10);
        assert_eq!(manifest.chunks.len(), 1);
        let chunk = manifest.chunks.values().next().unwrap();
        assert_eq!(chunk.checksum, hex::encode(Sha256::digest(&bytes)));
        assert_eq!(chunk.files[0].filename, ARCHIVE_FILENAME);
        let backend = RawArchiveBackend::new(temp.path().to_path_buf()).unwrap();
        let file = backend
            .peek_file(ARCHIVE_FILENAME.to_string())
            .await
            .unwrap();
        let mut reader = backend.reader(&file, 7, 37).await.unwrap();
        assert_eq!(read_all(&mut *reader, 5).await, bytes[7..37]);
        assert!(
            backend
                .reader(&file, 3, bytes.len() as u64 + 1)
                .await
                .is_err()
        );
        assert!(backend.peek_file("game/play.exe".into()).await.is_err());
    }

    #[tokio::test]
    async fn archive_download_supports_zip64_with_187260_files() {
        let names: Vec<_> = (0..187_260)
            .map(|index| format!("assets/{index}.bin"))
            .collect();
        let entries: Vec<_> = names
            .iter()
            .map(|name| (name.as_str(), b"x".as_slice()))
            .collect();
        let bytes = zip_stored(&entries);
        let temp = TempZip::new(&bytes);
        let manifest = crate::manifest::generate_archive_manifest(temp.path(), |_| {}, |_| {})
            .await
            .unwrap();
        assert_eq!(manifest.archive.unwrap().install_size, 187_260);
        assert_eq!(
            manifest.chunks.len(),
            1,
            "files inside the ZIP must not become transfer chunks"
        );
        assert_eq!(manifest.size, bytes.len() as u64);
    }

    #[tokio::test]
    async fn archive_download_supports_rar4_rar5_and_solid_rar() {
        for name in [
            "test_read_format_rar_windows.rar",
            "test_read_format_rar5_compressed.rar",
            "test_read_format_rar5_solid.rar",
        ] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../desktop/src-tauri/games/tests/fixtures")
                .join(name);
            let manifest = crate::manifest::generate_archive_manifest(&path, |_| {}, |_| {})
                .await
                .unwrap();
            assert_eq!(manifest.size, std::fs::metadata(&path).unwrap().len());
            assert!(manifest.archive.unwrap().install_size > 0);
        }
    }

    #[tokio::test]
    async fn archive_download_rejects_reserved_and_traversal_paths() {
        for name in [
            "../escape",
            ".dropdata",
            ".drop-archive-cache/payload",
            "C:\\escape",
        ] {
            let temp = TempZip::new(&zip_stored(&[(name, b"bad")]));
            assert!(
                crate::manifest::generate_archive_manifest(temp.path(), |_| {}, |_| {})
                    .await
                    .is_err(),
                "{name}"
            );
        }
    }

    #[tokio::test]
    async fn raw_archive_byte_ranges_support_offsets_above_four_gibibytes() {
        use crate::versions::raw_archive_backend::RawArchiveBackend;
        use std::io::{Seek, SeekFrom, Write};
        let temp = TempZip::new(b"header");
        let offset = 5 * 1024 * 1024 * 1024u64;
        let mut handle = std::fs::OpenOptions::new()
            .write(true)
            .open(temp.path())
            .unwrap();
        handle.seek(SeekFrom::Start(offset)).unwrap();
        handle.write_all(b"late bytes").unwrap();
        drop(handle);
        let backend = RawArchiveBackend::new(temp.path().to_path_buf()).unwrap();
        let file = backend
            .peek_file(droplet_types::ARCHIVE_FILENAME.into())
            .await
            .unwrap();
        let mut reader = backend.reader(&file, offset, offset + 10).await.unwrap();
        assert_eq!(read_all(&mut *reader, 3).await, b"late bytes");
    }

    #[tokio::test]
    async fn archive_download_rejects_invalid_encrypted_and_duplicate_entries() {
        for bytes in [
            b"<html>not an archive</html>".to_vec(),
            zip_stored(&[("game.exe", b"a"), ("game.exe", b"b")]),
        ] {
            let temp = TempZip::new(&bytes);
            assert!(
                crate::manifest::generate_archive_manifest(temp.path(), |_| {}, |_| {})
                    .await
                    .is_err()
            );
        }
        let mut bytes = zip_stored(&[("encrypted.exe", b"secret")]);
        bytes[6] = 1;
        let central = bytes
            .windows(4)
            .position(|bytes| bytes == b"PK\x01\x02")
            .unwrap();
        bytes[central + 8] = 1;
        let temp = TempZip::new(&bytes);
        assert!(
            crate::manifest::generate_archive_manifest(temp.path(), |_| {}, |_| {})
                .await
                .is_err()
        );
    }
}
