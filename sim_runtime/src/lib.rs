//! Shared runtime utilities for Shadow-Scale.
//!
//! This crate re-exports the data contracts from `sim_schema` and will gradually
//! accumulate helpers that operate on those types (validation, transforms,
//! command utilities) without depending on the full Bevy runtime in `core_sim`.

pub use sim_schema::*;
