//! GDScript-facing `GodotClass` types. Everything the engine can call lives here;
//! the decode work itself lives in [`crate::snapshot`] and [`crate::dict`].

pub(crate) mod command;
pub(crate) mod decoder;
pub(crate) mod script_host;
pub(crate) mod variant;

pub use command::CommandBridge;
pub use decoder::SnapshotDecoder;
pub use script_host::ScriptHostBridge;
