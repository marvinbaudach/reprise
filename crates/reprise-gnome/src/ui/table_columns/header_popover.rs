//! Right-click access to the shared editor from a table's header band.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use super::{editor, EditorModel};

/// True when a click at vertical offset `y` (relative to the ColumnView) landed
/// on the header row. The header is always the ColumnView's first child and sits
/// flush at the top, so its height defines the band.
fn is_header_click(y: f64, header_height: i32) -> bool {
    header_height > 0 && y <= f64::from(header_height)
}

fn build_header_popover(model: &Rc<dyn EditorModel>) -> (gtk4::Popover, gtk4::ListBox) {
    let surface = editor::build_surface(model, false);
    let content = gtk4::Frame::builder()
        .width_request(360)
        .height_request(440)
        .child(&surface.toolbar)
        .build();
    let popover = gtk4::Popover::builder()
        .autohide(true)
        .has_arrow(true)
        .child(&content)
        .build();
    popover.add_css_class("menu");
    popover.add_css_class("reprise-column-header-popover");
    (popover, surface.list)
}

/// Installs the right-click-on-header gesture that opens the editor popover.
pub(in crate::ui) fn install_header_popover(view: &gtk4::ColumnView, model: &Rc<dyn EditorModel>) {
    // input-parity: ACC-8 keyboard=column-editor
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    // Capture phase, claiming on press: GtkColumnViewTitle's own click
    // gesture claims EVERY press (any button) at the target, so a
    // bubble-phase ancestor gesture loses the sequence before its handler
    // can run — the exact claim race that also breaks GTK's native column
    // drag (see `column_header_dnd`'s module doc). At capture this gesture
    // runs first, and its claim below keeps the title's own gesture (and
    // GTK's plain visibility menu) from ever seeing header right-clicks.
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let model = Rc::downgrade(model);
    gesture.connect_pressed(glib::clone!(
        #[weak]
        view,
        move |gesture, _, x, y| {
            let Some(model) = model.upgrade() else {
                return;
            };
            let header_height = view.first_child().map_or(0, |header| header.height());
            if !is_header_click(y, header_height) {
                return;
            }
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            let (popover, initial_focus) = build_header_popover(&model);
            popover.set_parent(&view);
            popover.set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            let focus_guard = crate::ui::transient_focus::TransientFocusGuard::capture(&view);
            focus_guard.bind_popover(&popover, &initial_focus);
            crate::ui::popover_lifecycle::unparent_after_actions(&popover);
            popover.popup();
            tracing::debug!("column header popover opened");
        }
    ));
    view.add_controller(gesture);
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{install_header_popover, is_header_click};
    use crate::ui::table_columns::{header_dnd, ColumnDescriptor, EditorModel};

    struct FakeModel;

    impl EditorModel for FakeModel {
        fn title(&self) -> String {
            "Columns".to_owned()
        }

        fn columns(&self) -> Vec<ColumnDescriptor> {
            Vec::new()
        }

        fn is_visible(&self, _id: &str) -> bool {
            false
        }

        fn set_visible(&self, _id: &str, _visible: bool) {}

        fn move_column(&self, _id: &str, _target: &str, _after: bool) {}

        fn reset(&self) {}
    }

    #[test]
    fn column_layout_header_hit_test_matches_only_the_header_band() {
        assert!(is_header_click(0.0, 25));
        assert!(is_header_click(25.0, 25));
        assert!(!is_header_click(25.1, 25));
        assert!(!is_header_click(200.0, 25));
        assert!(!is_header_click(0.0, 0));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_gestures_do_not_keep_the_editor_model_alive() {
        gtk4::init().unwrap();
        let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
        let model: Rc<dyn EditorModel> = Rc::new(FakeModel);

        install_header_popover(&view, &model);
        header_dnd::install_header_drag(&view, &model);

        assert_eq!(
            Rc::strong_count(&model),
            1,
            "view-owned gesture closures retained the editor model"
        );
    }
}
