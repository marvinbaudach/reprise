//! Shared table-column editing for every GTK table.

pub(in crate::ui) mod descriptor;
pub(in crate::ui) mod editor;
pub(in crate::ui) mod editor_dnd;

pub(in crate::ui) use descriptor::{ColumnDescriptor, EditorModel};
