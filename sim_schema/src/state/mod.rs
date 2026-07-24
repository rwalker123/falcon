//! World-state types, partitioned along the nine domain sections of `snapshot.fbs`.
//!
//! Each module owns one section's structs and enums; append a new snapshot field to the module
//! that owns its section (and to the matching `crate::codec` module).

pub mod campaign;
pub mod culture;
pub mod economy;
pub mod governance;
pub mod knowledge;
pub mod map;
pub mod population;
pub mod subsistence;

pub use campaign::*;
pub use culture::*;
pub use economy::*;
pub use governance::*;
pub use knowledge::*;
pub use map::*;
pub use population::*;
pub use subsistence::*;
