//! Drag-and-drop and keyboard reordering inside the shared column editor.

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

const DROP_BEFORE_CLASS: &str = "reprise-column-drop-before";
const DROP_AFTER_CLASS: &str = "reprise-column-drop-after";
/// Draggable reorder row (movable columns) — gets the accent hover surface.
const ROW_CLASS: &str = "reprise-column-row";
/// Drag handle icon — dim at rest, accentuated on hover/drag.
const HANDLE_CLASS: &str = "reprise-column-handle";
/// Resting opacity of the drag handle (quiet, not disabled-looking).
const HANDLE_REST_OPACITY: &str = "0.45";
/// Drag-handle opacity once the row is hovered.
const HANDLE_ACTIVE_OPACITY: &str = "0.85";
/// Opacity of the row itself while it is being dragged (a translucent ghost).
const DRAG_GHOST_OPACITY: f64 = 0.5;

pub(super) fn row_class() -> &'static str {
    ROW_CLASS
}

pub(super) fn handle_class() -> &'static str {
    HANDLE_CLASS
}

pub(super) fn keyboard_reorder_offset(
    key: gdk::Key,
    modifiers: gdk::ModifierType,
) -> Option<isize> {
    if !modifiers.contains(gdk::ModifierType::ALT_MASK) {
        return None;
    }
    match key {
        gdk::Key::Up => Some(-1),
        gdk::Key::Down => Some(1),
        _ => None,
    }
}

fn parse_drag_payload(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

fn set_drop_indicator(widget: &impl IsA<gtk4::Widget>, after: Option<bool>) {
    widget.remove_css_class(DROP_BEFORE_CLASS);
    widget.remove_css_class(DROP_AFTER_CLASS);
    match after {
        Some(true) => widget.add_css_class(DROP_AFTER_CLASS),
        Some(false) => widget.add_css_class(DROP_BEFORE_CLASS),
        None => {}
    }
}

fn is_after_half(widget: &impl IsA<gtk4::Widget>, y: f64) -> bool {
    y >= f64::from(widget.height()) / 2.0
}

pub(super) fn wire_row_drag_and_drop(
    widget: &impl IsA<gtk4::Widget>,
    id: String,
    on_drop: impl Fn(String, bool) + 'static,
) {
    // input-parity: ACC-8 keyboard=alt-arrows
    let source = gtk4::DragSource::new();
    source.set_actions(gdk::DragAction::MOVE);
    // Observe pointer movement before ActionRow or one of its controls claims it.
    // Clicks still propagate normally when the gesture does not become a drag.
    source.set_propagation_phase(gtk4::PropagationPhase::Capture);
    source.connect_prepare(move |_, _, _| Some(gdk::ContentProvider::for_value(&id.to_value())));
    // Fade the row to a ghost while it is being dragged, restoring it on end.
    {
        let ghost = widget.upcast_ref::<gtk4::Widget>().clone();
        source.connect_drag_begin(move |_, _| ghost.set_opacity(DRAG_GHOST_OPACITY));
    }
    {
        let ghost = widget.upcast_ref::<gtk4::Widget>().clone();
        source.connect_drag_end(move |_, _, _| ghost.set_opacity(1.0));
    }
    widget.add_controller(source);

    // input-parity: ACC-8 keyboard=alt-arrows
    let target = gtk4::DropTarget::new(glib::Type::STRING, gdk::DragAction::MOVE);
    {
        let widget = widget.upcast_ref::<gtk4::Widget>().clone();
        target.connect_motion(move |_, _, y| {
            set_drop_indicator(&widget, Some(is_after_half(&widget, y)));
            gdk::DragAction::MOVE
        });
    }
    {
        let widget = widget.upcast_ref::<gtk4::Widget>().clone();
        target.connect_leave(move |_| set_drop_indicator(&widget, None));
    }
    let drop_widget = widget.upcast_ref::<gtk4::Widget>().clone();
    target.connect_drop(move |_, value, _, y| {
        set_drop_indicator(&drop_widget, None);
        let Ok(value) = value.get::<String>() else {
            return false;
        };
        let Some(source) = parse_drag_payload(&value) else {
            return false;
        };
        on_drop(source, is_after_half(&drop_widget, y));
        true
    });
    widget.add_controller(target);
}

/// Redesign chrome for the column-layout editor; installed app-wide by
/// [`crate::ui::style`].
pub(in crate::ui) fn css() -> String {
    use crate::ui::style::tokens::{DROP_INDICATOR_THICKNESS, HOVER_BG_ALPHA, TRANSITION};
    format!(
        ".{ROW_CLASS} {{ transition: background-color {TRANSITION}; }}\n\
         .{ROW_CLASS}:hover {{ background-color: alpha(@accent_bg_color, {HOVER_BG_ALPHA}); }}\n\
         .{HANDLE_CLASS} {{ opacity: {HANDLE_REST_OPACITY}; \
           transition: opacity {TRANSITION}, color {TRANSITION}; }}\n\
         .{ROW_CLASS}:hover .{HANDLE_CLASS} {{ opacity: {HANDLE_ACTIVE_OPACITY}; color: @reprise_accent_text_color; }}\n\
         .{ROW_CLASS}:drop(active) .{HANDLE_CLASS} {{ opacity: 1; color: @reprise_accent_text_color; }}\n\
         .{DROP_BEFORE_CLASS}:drop(active) {{ box-shadow: inset 0 {DROP_INDICATOR_THICKNESS} @accent_color; }}\n\
         .{DROP_AFTER_CLASS}:drop(active) {{ box-shadow: inset 0 -{DROP_INDICATOR_THICKNESS} @accent_color; }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use libadwaita as adw;
    use libadwaita::prelude::*;

    #[test]
    fn alt_arrow_keys_reorder_without_stealing_plain_navigation() {
        let alt = gdk::ModifierType::ALT_MASK;
        assert_eq!(keyboard_reorder_offset(gdk::Key::Up, alt), Some(-1));
        assert_eq!(keyboard_reorder_offset(gdk::Key::Down, alt), Some(1));
        assert_eq!(
            keyboard_reorder_offset(gdk::Key::Up, gdk::ModifierType::empty()),
            None
        );
        assert_eq!(keyboard_reorder_offset(gdk::Key::Return, alt), None);
    }

    #[test]
    fn css_styles_reorder_rows_and_drag_handle() {
        let css = super::css();
        assert!(css.contains(".reprise-column-row:hover"));
        assert!(css.contains("@accent_bg_color"));
        assert!(css.contains(".reprise-column-handle"));
        assert!(css.contains(".reprise-column-drop-before"));
    }

    #[test]
    fn drag_payload_accepts_table_column_ids_and_rejects_empty_values() {
        assert_eq!(parse_drag_payload("artist").as_deref(), Some("artist"));
        assert_eq!(parse_drag_payload("cover").as_deref(), Some("cover"));
        assert_eq!(parse_drag_payload("title").as_deref(), Some("title"));
        assert_eq!(parse_drag_payload("  "), None);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn movable_row_captures_drag_before_child_controls_and_accepts_drops() {
        if gtk4::init().is_err() {
            return;
        }
        let row = adw::ActionRow::builder().title("Artist").build();
        let toggle = gtk4::Switch::new();
        row.add_suffix(&toggle);
        row.set_activatable_widget(Some(&toggle));
        wire_row_drag_and_drop(&row, "artist".to_owned(), |_, _| {});
        let controllers = row.observe_controllers();
        let mut drag_phase = None;
        let mut has_drop = false;
        for index in 0..controllers.n_items() {
            let controller = controllers.item(index).unwrap();
            if let Ok(source) = controller.clone().downcast::<gtk4::DragSource>() {
                drag_phase = Some(source.propagation_phase());
            }
            has_drop |= controller.is::<gtk4::DropTarget>();
        }
        assert_eq!(drag_phase, Some(gtk4::PropagationPhase::Capture));
        assert!(has_drop);
    }
}
