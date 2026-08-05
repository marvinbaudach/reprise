//! The 64px media column: artwork, playing marker, selection checkbox.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;

use super::skeleton::{MEDIA_HEIGHT, MEDIA_WIDTH};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum MediaShape {
    /// 16:9 — YouTube thumbnails.
    Wide,
    /// 1:1 — podcast artwork and station logos.
    Square,
}

pub(in crate::ui) fn media_size(shape: MediaShape) -> (i32, i32) {
    match shape {
        MediaShape::Wide => (MEDIA_WIDTH, 36),
        MediaShape::Square => (36, 36),
    }
}

#[derive(Default)]
struct OverlayState {
    loaded: Cell<bool>,
    selection_mode: Cell<bool>,
    selected: Cell<bool>,
    hovered: Cell<bool>,
    /// The row holds the focus. Kept apart from `focused_self` because either
    /// alone is reason enough to show the checkbox, and one shared flag would
    /// let the row losing focus *to the checkbox* hide it in the same breath.
    focused_row: Cell<bool>,
    /// The checkbox itself holds the focus.
    focused_self: Cell<bool>,
}

impl OverlayState {
    fn shows_checkbox(&self) -> bool {
        self.selection_mode.get()
            && (self.selected.get()
                || self.hovered.get()
                || self.focused_row.get()
                || self.focused_self.get())
    }
}

pub(in crate::ui) struct MediaColumn {
    root: gtk4::Overlay,
    checkbox: gtk4::CheckButton,
    state_overlay: Rc<RefCell<Option<gtk4::Widget>>>,
    state: Rc<OverlayState>,
    toggle_handler: RefCell<Option<gtk4::glib::SignalHandlerId>>,
}

impl MediaColumn {
    pub(in crate::ui) fn new(child: &impl IsA<gtk4::Widget>, shape: MediaShape) -> Self {
        let (width, height) = media_size(shape);
        child.as_ref().set_size_request(width, height);
        child.as_ref().set_halign(gtk4::Align::Center);
        child.as_ref().set_valign(gtk4::Align::Center);

        let root = gtk4::Overlay::new();
        root.add_css_class("reprise-source-row-media");
        root.set_size_request(MEDIA_WIDTH, MEDIA_HEIGHT);
        root.set_child(Some(child.as_ref()));

        let checkbox = gtk4::CheckButton::new();
        checkbox.set_halign(gtk4::Align::Center);
        checkbox.set_valign(gtk4::Align::Center);
        checkbox.set_opacity(0.0);
        checkbox.set_can_target(false);
        root.add_overlay(&checkbox);

        let column = Self {
            root,
            checkbox,
            state_overlay: Rc::new(RefCell::new(None)),
            state: Rc::new(OverlayState::default()),
            toggle_handler: RefCell::new(None),
        };
        // The checkbox watches its own focus here rather than leaving it to
        // the caller: a row's `has-focus` is false while the focus sits on one
        // of its children, so a caller that only forwards the row's focus
        // hands a keyboard user an invisible checkbox. Wiring it where the
        // widget is created is what stops the next caller from forgetting.
        let focus_state = column.state.clone();
        let focus_overlay = column.state_overlay.clone();
        let focus_checkbox = column.checkbox.clone();
        column.checkbox.connect_has_focus_notify(move |checkbox| {
            focus_state.focused_self.set(checkbox.has_focus());
            recompute_with(&focus_state, &focus_overlay, &focus_checkbox);
        });
        column.recompute();
        column
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    /// The playing marker, owned by the caller because only it knows what
    /// "loaded" means in its view.
    pub(in crate::ui) fn set_state_overlay(&self, widget: &impl IsA<gtk4::Widget>) {
        self.root.add_overlay(widget.as_ref());
        self.state_overlay.replace(Some(widget.as_ref().clone()));
        self.recompute();
    }

    pub(in crate::ui) fn checkbox(&self) -> &gtk4::CheckButton {
        &self.checkbox
    }

    pub(in crate::ui) fn set_loaded(&self, loaded: bool) {
        self.state.loaded.set(loaded);
        self.recompute();
    }

    pub(in crate::ui) fn set_selection_mode(&self, active: bool) {
        self.state.selection_mode.set(active);
        self.recompute();
    }

    pub(in crate::ui) fn set_selected(&self, selected: bool) {
        self.state.selected.set(selected);
        let handler = self.toggle_handler.borrow_mut().take();
        if let Some(handler) = handler.as_ref() {
            self.checkbox.block_signal(handler);
        }
        self.checkbox.set_active(selected);
        if let Some(handler) = handler.as_ref() {
            self.checkbox.unblock_signal(handler);
        }
        self.toggle_handler.replace(handler);
        self.recompute();
    }

    /// Installs the view-owned selection action while keeping state updates
    /// from feeding back into that action through `set_active`.
    ///
    /// Replaces any previous handler outright. Only the newest id is blocked
    /// during `set_active`, so a forgotten predecessor would keep firing —
    /// with a stale episode id — through exactly the feedback loop the
    /// blocking exists to close.
    pub(in crate::ui) fn connect_toggled(&self, callback: impl Fn(&gtk4::CheckButton) + 'static) {
        if let Some(previous) = self.toggle_handler.borrow_mut().take() {
            self.checkbox.disconnect(previous);
        }
        let handler = self.checkbox().connect_toggled(callback);
        self.toggle_handler.replace(Some(handler));
    }

    pub(in crate::ui) fn set_hovered(&self, hovered: bool) {
        self.state.hovered.set(hovered);
        self.recompute();
    }

    /// The *row's* focus. The checkbox's own focus is wired in `new` and needs
    /// no caller.
    pub(in crate::ui) fn set_focused(&self, focused: bool) {
        self.state.focused_row.set(focused);
        self.recompute();
    }

    fn recompute(&self) {
        recompute_with(&self.state, &self.state_overlay, &self.checkbox);
    }
}

/// Exactly one thing sits above the artwork. A later hover-play glyph belongs
/// in a branch here, between the checkbox and the playing marker, so the
/// complete precedence remains visible in one place.
///
/// Free-standing rather than a method because the checkbox's own focus handler
/// cannot hold a borrow of the column that owns it.
fn recompute_with(
    state: &Rc<OverlayState>,
    state_overlay: &Rc<RefCell<Option<gtk4::Widget>>>,
    checkbox: &gtk4::CheckButton,
) {
    let show_checkbox = state.shows_checkbox();
    checkbox.set_opacity(if show_checkbox { 1.0 } else { 0.0 });
    checkbox.set_can_target(show_checkbox);
    let state_overlay = state_overlay.borrow().clone();
    if let Some(overlay) = state_overlay {
        overlay.set_visible(state.loaded.get() && !show_checkbox);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `SRC-16`: 16:9 and square artwork occupy the same column, which is what
    /// puts the title at the same x position in both views.
    #[test]
    fn src_16_both_shapes_fit_the_same_column() {
        assert_eq!(media_size(MediaShape::Wide), (64, 36));
        assert_eq!(media_size(MediaShape::Square), (36, 36));
        for shape in [MediaShape::Wide, MediaShape::Square] {
            let (width, height) = media_size(shape);
            assert!(width <= MEDIA_WIDTH, "{shape:?} is wider than the column");
            assert!(
                height <= MEDIA_HEIGHT,
                "{shape:?} is taller than the column"
            );
        }
    }

    /// `SRC-12a`: the checkbox is not a permanent column. It appears only once
    /// a selection exists, and then only on the rows the user is pointing at
    /// or has already picked.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_12a_the_checkbox_appears_only_while_a_selection_is_active() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let column = MediaColumn::new(&gtk4::Image::new(), MediaShape::Square);
        assert_eq!(column.checkbox().opacity(), 0.0, "idle row");

        column.set_hovered(true);
        assert_eq!(
            column.checkbox().opacity(),
            0.0,
            "hover without a selection"
        );

        column.set_selection_mode(true);
        assert_eq!(column.checkbox().opacity(), 1.0, "hover during a selection");

        column.set_hovered(false);
        assert_eq!(
            column.checkbox().opacity(),
            0.0,
            "unhovered, unselected row"
        );

        column.set_selected(true);
        assert_eq!(
            column.checkbox().opacity(),
            1.0,
            "selected row stays marked"
        );
    }

    /// One thing above the artwork at a time: an equalizer behind a checkbox
    /// is two states competing for 64×40 pixels.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn src_16_the_checkbox_replaces_the_playing_marker_rather_than_covering_it() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        let column = MediaColumn::new(&gtk4::Image::new(), MediaShape::Wide);
        let marker = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        column.set_state_overlay(&marker);
        column.set_loaded(true);
        assert!(marker.is_visible(), "a loaded row shows its marker");

        column.set_selection_mode(true);
        column.set_selected(true);
        assert!(!marker.is_visible(), "the checkbox takes the slot");

        column.set_selection_mode(false);
        assert!(marker.is_visible(), "and gives it back");
    }
}
