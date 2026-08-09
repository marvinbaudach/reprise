//! Shared table-column editing for every GTK table.

pub(in crate::ui) mod descriptor;

// Task 6 consumes this staged contract from the editor surface.
#[allow(unused_imports)]
pub(in crate::ui) use descriptor::{ColumnDescriptor, EditorModel};
