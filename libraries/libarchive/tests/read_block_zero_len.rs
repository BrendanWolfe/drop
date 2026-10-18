#![cfg(target_os = "linux")]

extern crate libarchive3_sys;
extern crate libarchive_drop;

use std::ffi::c_void;
use std::sync::Mutex;

use libarchive_drop::archive::Handle;
use libarchive_drop::reader::{Builder, Reader, ReaderEntry};

/// A zero-length block with a non-NULL buffer is not end-of-entry.
///
/// libarchive can return `ARCHIVE_OK` with `size == 0` and a non-NULL `buff`
/// while decompressing large entries (for example solid RAR archives at
/// internal 64 MiB boundaries). `Reader::read_block` must skip that block and
/// continue reading rather than treating it as `ARCHIVE_EOF`.
#[derive(Clone, Copy)]
enum MockBlock {
    /// `ARCHIVE_OK`, NULL buffer, zero length.
    NullEmpty,
    /// `ARCHIVE_OK`, non-NULL buffer, zero length.
    NonNullEmpty,
    /// `ARCHIVE_OK` with real data.
    Data(&'static [u8]),
    /// End of entry.
    Eof,
}

static MOCK_BLOCKS: Mutex<Vec<MockBlock>> = Mutex::new(Vec::new());
static EMPTY_BUFFER: [u8; 1] = [0];

/// Interposes on libarchive's `archive_read_data_block` for this test binary.
///
/// The executable's symbol takes precedence over the shared libarchive symbol,
/// which lets the test drive the exact sequence libarchive produces without a
/// proprietary RAR file or a large fixture.
#[no_mangle]
pub unsafe extern "C" fn archive_read_data_block(
    _archive: *mut libarchive3_sys::ffi::Struct_archive,
    buff: *mut *const c_void,
    size: *mut usize,
    offset: *mut i64,
) -> i32 {
    let block = {
        let mut blocks = MOCK_BLOCKS.lock().unwrap();
        blocks.remove(0)
    };

    match block {
        MockBlock::NullEmpty => {
            *buff = std::ptr::null();
            *size = 0;
            *offset = 0;
            libarchive3_sys::ffi::ARCHIVE_OK
        }
        MockBlock::NonNullEmpty => {
            *buff = EMPTY_BUFFER.as_ptr().cast::<c_void>();
            *size = 0;
            *offset = 0;
            libarchive3_sys::ffi::ARCHIVE_OK
        }
        MockBlock::Data(data) => {
            *buff = data.as_ptr().cast::<c_void>();
            *size = data.len();
            *offset = 0;
            libarchive3_sys::ffi::ARCHIVE_OK
        }
        MockBlock::Eof => libarchive3_sys::ffi::ARCHIVE_EOF,
    }
}

struct MockReader {
    archive: Builder,
    entry: ReaderEntry,
}

impl MockReader {
    fn new() -> Self {
        Self {
            archive: Builder::new(),
            entry: ReaderEntry::default(),
        }
    }
}

impl Handle for MockReader {
    unsafe fn handle(&self) -> *mut libarchive3_sys::ffi::Struct_archive {
        self.archive.handle()
    }
}

impl Reader for MockReader {
    fn entry(&mut self) -> &mut ReaderEntry {
        &mut self.entry
    }
}

#[test]
fn read_block_skips_zero_length_blocks() {
    {
        let mut blocks = MOCK_BLOCKS.lock().unwrap();
        blocks.clear();
        blocks.extend_from_slice(&[
            MockBlock::NullEmpty,
            MockBlock::NonNullEmpty,
            MockBlock::Data(b"payload"),
            MockBlock::Eof,
        ]);
    }

    let reader = MockReader::new();

    // The old implementation returned `Some(&[])` for NonNullEmpty, which
    // callers interpret as EOF. It must skip both empty blocks and return data.
    assert_eq!(reader.read_block().unwrap(), Some(&b"payload"[..]));
    assert_eq!(reader.read_block().unwrap(), None);
}
