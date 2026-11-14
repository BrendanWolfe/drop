//! Versioned, bounded snapshots. The legacy prototype modules are deliberately
//! not exported: their unchecked extraction must never be used for cloud data.
pub mod policy;
pub mod snapshot;
