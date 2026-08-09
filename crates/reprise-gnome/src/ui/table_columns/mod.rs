//! Shared table-column editing for every GTK table.

pub(in crate::ui) mod descriptor;
pub(in crate::ui) mod editor;
pub(in crate::ui) mod editor_dnd;
pub(in crate::ui) mod header_dnd;
pub(in crate::ui) mod header_popover;
pub(in crate::ui) mod registry;
pub(in crate::ui) mod width_persistence;

pub(in crate::ui) use descriptor::{ColumnDescriptor, EditorModel};
