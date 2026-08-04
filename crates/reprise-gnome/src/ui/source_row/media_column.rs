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
    focused: Cell<bool>,
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
    pub(in crate::ui) fn connect_toggled(&self, callback: impl Fn(&gtk4::CheckButton) + 'static) {
        let handler = self.checkbox().connect_toggled(callback);
        self.toggle_handler.replace(Some(handler));
    }

    pub(in crate::ui) fn set_hovered(&self, hovered: bool) {
        self.state.hovered.set(hovered);
        self.recompute();
    }

    pub(in crate::ui) fn set_focused(&self, focused: bool) {
        self.state.focused.set(focused);
        self.recompute();
    }

    /// Exactly one thing sits above the artwork. A later hover-play glyph
    /// belongs in a branch here, between the checkbox and playing marker, so
    /// the complete precedence remains visible in one place.
    fn recompute(&self) {
        let show_checkbox = self.state.selection_mode.get()
            && (self.state.selected.get() || self.state.hovered.get() || self.state.focused.get());
        self.checkbox
            .set_opacity(if show_checkbox { 1.0 } else { 0.0 });
        self.checkbox.set_can_target(show_checkbox);
        let state_overlay = self.state_overlay.borrow().clone();
        if let Some(overlay) = state_overlay {
            overlay.set_visible(self.state.loaded.get() && !show_checkbox);
        }
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
