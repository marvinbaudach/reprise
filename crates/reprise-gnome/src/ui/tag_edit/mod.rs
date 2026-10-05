pub(in crate::ui) mod autocomplete_entry;
pub(crate) mod tag_edit_flow;
pub(crate) mod tag_editor;
pub(in crate::ui) mod tag_editor_dirty;
pub(in crate::ui) mod tag_editor_failures;
pub(in crate::ui) mod tag_editor_form;
pub(in crate::ui) mod tag_editor_save;
pub(in crate::ui) mod tag_editor_state;
pub(in crate::ui) mod tag_editor_style;
pub(in crate::ui) mod tag_editor_widgets;
pub(in crate::ui) mod tag_reload_anchor;
mod tag_save_refresh;
mod tag_write_admission;
#[expect(
    unused_imports,
    reason = "child modules share the parent UI vocabulary through this import"
)]
use super::*;
