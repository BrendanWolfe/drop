#![feature(nonpoison_rwlock)]

pub mod db;
pub mod client_updates;
pub mod debug;
pub mod interface;
pub mod install_locations;
pub mod emulator_installations;
pub mod models;
pub mod platform;

pub use db::DB;
pub use interface::{borrow_db_checked, borrow_db_mut_checked};
pub use models::data::{
    ApplicationTransientStatus, Database, DatabaseApplications, DatabaseAuth, DownloadType,
    DownloadableMetadata, GameDownloadStatus, GameVersion, Settings,
};
