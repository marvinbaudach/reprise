//! Toolkit-independent column identity, layout and persistence.
//!
//! One table's columns are one enum implementing [`ColumnKey`]; everything
//! that operates on a layout — ordering, normalization, the persisted string —
//! is written once, generic over that trait. The GTK, Tauri and Compose
//! surfaces read the same stored value, and two implementations of one format
//! drift.

pub mod key;
pub mod layout;
pub mod track;

#[cfg(test)]
pub(crate) mod probe;

pub use key::{ColumnKey, Pin};
pub use layout::Layout;
pub use track::ColumnId;
