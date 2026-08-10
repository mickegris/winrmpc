//! Snapcast multiroom control — a sibling module to `mpd`, not bolted onto
//! `MpdClient`, since it's a fully independent connection/protocol. See
//! docs/plans/snapcast-control.md.

pub mod client;
pub mod error;
pub mod protocol;
pub mod types;

pub use client::SnapcastClient;
pub use types::{SnapClient, SnapGroup, SnapStream};
