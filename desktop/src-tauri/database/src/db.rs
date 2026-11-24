use std::{
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use keyring::Entry;
use log::info;

use crate::interface::DatabaseInterface;

pub static DB: LazyLock<DatabaseInterface> = LazyLock::new(DatabaseInterface::set_up_database);

const DATA_ROOT_PREFIX: &str = if cfg!(feature = "development-release") {
    "drop-dev"
} else if cfg!(debug_assertions) {
    "drop-debug"
} else {
    "drop"
};

pub static DATA_ROOT_DIR: LazyLock<Arc<PathBuf>> = LazyLock::new(|| {
    Arc::new(
        dirs::data_dir()
            .expect("Failed to get data dir")
            .join(DATA_ROOT_PREFIX),
    )
});

/*
pub(crate) static KEY_IV: LazyLock<([u8; 16], [u8; 16])> = LazyLock::new(|| {
    let entry = Entry::new("drop", "database_key").expect("failed to open keyring");
    let mut key = entry.get_secret().unwrap_or_else(|_| {
        let mut buffer = [0u8; 32];
        rand::fill(&mut buffer);
        entry.set_secret(&buffer).expect("failed to save key");
        info!("created new database key");
        buffer.to_vec()
    });
    let iv: Vec<u8> = key.split_off(16);
    (
        key[0..16].try_into().expect("key wrong length"),
        iv[0..16].try_into().expect("iv wrong length"),
    )
});
*/

// TODO: fix keyring
pub(crate) static KEY_IV: LazyLock<([u8; 16], [u8; 16])> = LazyLock::new(|| ([0; 16], [0; 16]));
