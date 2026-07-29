//! The row interaction surface shared by the source tables (podcasts, radio).
//!
//! A `ColumnView` cell is not the widget its factory builds. Adwaita styles
//! `columnview > listview > row > cell` with `padding: 8px 6px`, so a gesture
//! attached to the factory's own child only ever covers the padded content
//! box. Measured on the podcasts table at 640x50 per row, that leaves 48% of
//! the row inert: every point in the cell padding picks the private
//! `GtkColumnViewCellWidget`, which carries no gesture, and the row's context
//! menu simply does not open there.
//!
//! [`wrap`] and [`css`] are therefore one contract. The table hands its cell
//! padding to the surface, and the surface re-applies exactly the same
//! padding itself: the wrapped child keeps the identical content box, while
//! the surface — and with it the secondary-click gesture — spans the whole
//! cell. Neither half works alone, so a table opts in by adding
//! [`TABLE_CSS_CLASS`] to its `ColumnView` and building every cell child
//! through [`wrap`].
//!
//! Keyboard parity (ACC-4a) does not belong on a cell: cells are recycled and
//! deliberately not focusable, so the Menu/Shift+F10 handler goes on the
//! `ColumnView` and reads the current selection. [`context_keys`] builds that
//! controller.

use gtk4::prelude::*;

/// Opt-in class for a `ColumnView` whose cells hand their padding to the
/// surface. A table that sets this class must build *every* cell child
/// through [`wrap`], or those cells lose their padding without gaining a
/// surface.
pub(in crate::ui) const TABLE_CSS_CLASS: &str = "reprise-source-context-table";

const SURFACE_CSS_CLASS: &str = "reprise-source-context-surface";

/// Adwaita's own `columnview` cell padding, moved verbatim onto the surface.
/// `acc_1_wrapped_cell_keeps_the_plain_cell_content_geometry` pins it against
/// a plain cell, so a future Adwaita change fails that test instead of
/// silently shifting every source row by a few pixels.
const CELL_PADDING: &str = "8px 6px";

pub(in crate::ui) fn css() -> String {
    format!(
        ".{TABLE_CSS_CLASS} > listview > row > cell {{ padding: 0; }}\n\
         .{SURFACE_CSS_CLASS} {{ padding: {CELL_PADDING}; }}"
    )
}

/// Wraps a cell child in the full-cell interaction surface that carries the
/// row's [`secondary_click`] gesture.
pub(in crate::ui) fn wrap(child: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let surface = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    surface.add_css_class(SURFACE_CSS_CLASS);
    surface.set_hexpand(true);
    surface.set_halign(gtk4::Align::Fill);
    surface.set_can_target(true);
    // A pointer target, never a focus stop: the ColumnView row stays the
    // single Tab stop (ACC-3) and keeps owning arrow navigation.
    surface.set_focusable(false);
    // A cell allocates its child the whole content box; a Box only stretches
    // children that expand. Without this the wrapped pill and button cells
    // would shrink to their natural width inside the surface.
    child.as_ref().set_hexpand(true);
    surface.append(child);
    surface
}

/// The secondary-click gesture for a wrapped cell.
///
/// Capture phase, not bubble: the surface may wrap an interactive child (the
/// podcast unsubscribe button) and the row must answer the secondary button
/// before that child sees it.
pub(in crate::ui) fn secondary_click() -> gtk4::GestureClick {
    // input-parity: ACC-8 keyboard=menu-shift-f10
    let gesture = gtk4::GestureClick::new();
    gesture.set_button(gtk4::gdk::BUTTON_SECONDARY);
    gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    gesture
}

/// The Menu/Shift+F10 controller for a source table's `ColumnView`.
///
/// Capture phase so the shortcut reaches the table before a focused cell
/// child (again the unsubscribe button) can swallow it.
pub(in crate::ui) fn context_keys() -> gtk4::EventControllerKey {
    let keys = gtk4::EventControllerKey::new();
    keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
    keys
}

/// ACC-4a's context-menu shortcut, shared with the track list so the source
/// tables cannot drift into their own key combination.
pub(in crate::ui) fn is_context_menu_shortcut(
    key: gtk4::gdk::Key,
    modifiers: gtk4::gdk::ModifierType,
) -> bool {
    crate::ui::track_list::track_list_context_keys::is_context_menu_shortcut(key, modifiers)
}

/// Test-only: every sampled point of the table's first row that does *not*
/// resolve to a context surface, in view coordinates. An empty result is the
/// ACC-1 contract — the whole row answers the secondary button.
#[cfg(test)]
pub(in crate::ui) fn row_points_without_a_surface(view: &gtk4::ColumnView) -> Vec<(i32, i32)> {
    const SAMPLE_STEP: usize = 3;

    let surface = first_surface(view.upcast_ref()).expect("a realized cell surface");
    let row = surface
        .parent()
        .and_then(|cell| cell.parent())
        .expect("the surface sits in a cell inside a row");
    let bounds = row
        .compute_bounds(view)
        .expect("the row has bounds in view space");

    let left = bounds.x() as i32;
    let top = bounds.y() as i32;
    let right = left + bounds.width() as i32;
    let bottom = top + bounds.height() as i32;

    let mut uncovered = Vec::new();
    for x in (left..right).step_by(SAMPLE_STEP) {
        for y in (top..bottom).step_by(SAMPLE_STEP) {
            if !picks_a_surface(view, x, y) {
                uncovered.push((x, y));
            }
        }
    }
    uncovered
}

#[cfg(test)]
fn picks_a_surface(view: &gtk4::ColumnView, x: i32, y: i32) -> bool {
    let mut node = view.pick(f64::from(x), f64::from(y), gtk4::PickFlags::DEFAULT);
    while let Some(widget) = node {
        if widget.has_css_class(SURFACE_CSS_CLASS) {
            return true;
        }
        node = widget.parent();
    }
    false
}

#[cfg(test)]
fn first_surface(widget: &gtk4::Widget) -> Option<gtk4::Widget> {
    if widget.has_css_class(SURFACE_CSS_CLASS) {
        return Some(widget.clone());
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = first_surface(&current) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

/// Test-only: runs a real layout cycle so allocations and pick results are
/// settled before a display test measures them.
#[cfg(test)]
pub(in crate::ui) fn settle_layout() {
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(200));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_label_column(view: &gtk4::ColumnView, wrapped: bool) {
        let factory = gtk4::SignalListItemFactory::new();
        factory.connect_setup(move |_, object| {
            let Some(item) = object.downcast_ref::<gtk4::ListItem>() else {
                return;
            };
            let label = gtk4::Label::builder()
                .xalign(0.0)
                .hexpand(true)
                .label("Episode")
                .build();
            if wrapped {
                item.set_child(Some(&wrap(&label)));
            } else {
                item.set_child(Some(&label));
            }
        });
        let column = gtk4::ColumnViewColumn::builder()
            .title("Title")
            .factory(&factory)
            .expand(true)
            .build();
        view.append_column(&column);
    }

    fn table(wrapped: bool) -> gtk4::ColumnView {
        let store = gtk4::gio::ListStore::new::<gtk4::gio::MenuItem>();
        store.append(&gtk4::gio::MenuItem::new(Some("row"), None));
        let selection = gtk4::SingleSelection::new(Some(store));
        let view = gtk4::ColumnView::new(Some(selection));
        if wrapped {
            view.add_css_class(TABLE_CSS_CLASS);
        }
        single_label_column(&view, wrapped);
        view
    }

    /// `(x, y, width, height)` of a widget's border box in another widget's
    /// coordinate space.
    type Rect = (f32, f32, f32, f32);

    fn rect(widget: &gtk4::Widget, target: &impl IsA<gtk4::Widget>) -> Rect {
        let bounds = widget
            .compute_bounds(target)
            .expect("both widgets share a realized hierarchy");
        (bounds.x(), bounds.y(), bounds.width(), bounds.height())
    }

    /// The label's rectangle inside its row, plus the row's own size.
    ///
    /// Measured against the *row*, not the cell: `WidgetExt::width` reports
    /// the content box, so a padded cell and an unpadded one hosting a padded
    /// surface report the same numbers for different geometry. Row-relative
    /// bounds are the border-box truth both cases must agree on.
    fn row_relative_label_bounds(view: &gtk4::ColumnView, wrapped: bool) -> (Rect, Rect) {
        let label = first_cell_label(view.upcast_ref()).expect("a realized cell label");
        let cell = if wrapped {
            label.parent().and_then(|surface| surface.parent())
        } else {
            label.parent()
        }
        .expect("the label sits in a cell");
        let row = cell.parent().expect("the cell sits in a row");
        (rect(&label, &row), rect(&row, view))
    }

    /// The header's labels live in `GtkColumnViewTitle` widgets, which are
    /// siblings of the list view rather than descendants of a cell.
    fn first_cell_label(widget: &gtk4::Widget) -> Option<gtk4::Widget> {
        if widget.type_().name() == "GtkColumnViewTitle" {
            return None;
        }
        if widget.is::<gtk4::Label>() {
            return Some(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            if let Some(found) = first_cell_label(&current) {
                return Some(found);
            }
            child = current.next_sibling();
        }
        None
    }

    fn present(children: &[&gtk4::ColumnView]) -> gtk4::Window {
        let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        for child in children {
            column.append(*child);
        }
        let window = gtk4::Window::new();
        window.set_default_size(600, 400);
        window.set_child(Some(&column));
        window.present();
        settle_layout();
        window
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_1_wrapped_cell_keeps_the_plain_cell_content_geometry() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        crate::ui::style::install_css_string_for_test(&css());

        let plain = table(false);
        let wrapped = table(true);
        let _window = present(&[&plain, &wrapped]);

        assert_eq!(
            row_relative_label_bounds(&wrapped, true),
            row_relative_label_bounds(&plain, false),
            "the surface must re-apply exactly the cell padding it removed"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_1_the_surface_spans_the_whole_cell_instead_of_the_content_box() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        crate::ui::style::install_css_string_for_test(&css());

        let view = table(true);
        let _window = present(&[&view]);

        let surface = first_surface(view.upcast_ref()).expect("a realized cell surface");
        let cell = surface.parent().expect("the surface sits in a cell");
        assert_eq!(rect(&surface, &view), rect(&cell, &view));

        let uncovered = row_points_without_a_surface(&view);
        assert!(
            uncovered.is_empty(),
            "row points without a context surface: {uncovered:?}"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_3_the_surface_is_a_pointer_target_without_becoming_a_focus_stop() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let surface = wrap(&gtk4::Label::new(Some("Episode")));

        assert!(surface.hexpands());
        assert_eq!(surface.halign(), gtk4::Align::Fill);
        assert!(surface.can_target());
        assert!(!surface.is_focusable());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_4a_context_controllers_run_before_the_cell_children() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let gesture = secondary_click();
        assert_eq!(gesture.button(), gtk4::gdk::BUTTON_SECONDARY);
        assert_eq!(gesture.propagation_phase(), gtk4::PropagationPhase::Capture);
        assert_eq!(
            context_keys().propagation_phase(),
            gtk4::PropagationPhase::Capture
        );
    }

    #[test]
    fn menu_key_and_shift_f10_share_the_application_context_shortcut() {
        assert!(is_context_menu_shortcut(
            gtk4::gdk::Key::Menu,
            gtk4::gdk::ModifierType::empty()
        ));
        assert!(is_context_menu_shortcut(
            gtk4::gdk::Key::F10,
            gtk4::gdk::ModifierType::SHIFT_MASK
        ));
        assert!(!is_context_menu_shortcut(
            gtk4::gdk::Key::F10,
            gtk4::gdk::ModifierType::empty()
        ));
    }
}
