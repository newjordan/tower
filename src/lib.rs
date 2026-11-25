//! Agent tower simulation.
//!
//! The deterministic 100-agent world lives in [`sim`]. The binary crate root
//! serves the observer API on top of that world. Wall-clock timestamps and
//! random ids are observational; seeded decisions, economy, and floor
//! progression are covered by [`sim::state_fingerprint`].

pub mod assets;
pub mod ops;
pub mod public_assets;
pub mod sim;
pub mod storage;
