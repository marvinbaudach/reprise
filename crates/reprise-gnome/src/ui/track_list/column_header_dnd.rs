//! Music-table binding for the shared header drag implementation.

pub(in crate::ui) fn css() -> String {
    crate::ui::table_columns::header_dnd::css()
}

pub(super) fn wire_header_drag(
    view: &gtk4::ColumnView,
    registry: &super::column_layout::ColumnRegistry,
) {
    let model: std::rc::Rc<dyn crate::ui::table_columns::EditorModel> = registry.clone();
    crate::ui::table_columns::header_dnd::install_header_drag(view, &model);
}
