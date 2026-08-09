//! Toolkit-independent column identity, layout and persistence.
//!
//! One table's columns are one enum implementing [`ColumnKey`]; everything
//! that operates on a layout — ordering, normalization, the persisted string —
//! is written once, generic over that trait. The GTK, Tauri and Compose
//! surfaces read the same stored value, and two implementations of one format
//! drift.

pub mod key;
pub mod track;

pub use key::{ColumnKey, Pin};
pub use track::ColumnId;
