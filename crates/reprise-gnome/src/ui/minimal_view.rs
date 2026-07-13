use std::cell::Cell;
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::strings;

const FULL_MIN_WIDTH: i32 = 600;
const FULL_MIN_HEIGHT: i32 = 400;
const MINIMAL_WIDTH: i32 = 560;
const MINIMAL_HEIGHT: i32 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ViewMode {
    Full,
    Minimal,
}

fn next_mode(mode: ViewMode) -> ViewMode {
    match mode {
        ViewMode::Full => ViewMode::Minimal,
        ViewMode::Minimal => ViewMode::Full,
    }
}

pub(super) struct MinimalView {
    window: adw::ApplicationWindow,
    full_root: adw::NavigationSplitView,
    bottom_box: gtk4::Box,
    player_bar: Option<gtk4::ActionBar>,
    minimal_root: adw::ToolbarView,
    minimal_content: gtk4::Box,
    mode: Cell<ViewMode>,
    full_width: Cell<i32>,
    full_height: Cell<i32>,
    full_maximized: Cell<bool>,
    geometry_suppressed: Rc<Cell<bool>>,
}

impl MinimalView {
    pub(super) fn new(
        window: &adw::ApplicationWindow,
        full_root: &adw::NavigationSplitView,
        bottom_box: &gtk4::Box,
        player_bar: Option<&gtk4::ActionBar>,
    ) -> Rc<Self> {
        let restore = gtk4::Button::builder()
            .icon_name("view-restore-symbolic")
            .tooltip_text(strings::text(strings::RESTORE_FULL_VIEW))
            .build();
        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&adw::WindowTitle::new(
            &strings::text(strings::APP_NAME),
            &strings::text(strings::MINIMAL_VIEW),
        )));
        header.pack_end(&restore);
        let minimal_content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let minimal_root = adw::ToolbarView::new();
        minimal_root.add_top_bar(&header);
        minimal_root.set_content(Some(&minimal_content));
        let state = Rc::new(Self {
            window: window.clone(),
            full_root: full_root.clone(),
            bottom_box: bottom_box.clone(),
            player_bar: player_bar.cloned(),
            minimal_root,
            minimal_content,
            mode: Cell::new(ViewMode::Full),
            full_width: Cell::new(1200),
            full_height: Cell::new(800),
            full_maximized: Cell::new(false),
            geometry_suppressed: Rc::new(Cell::new(false)),
        });
        let weak = Rc::downgrade(&state);
        restore.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.set_mode(ViewMode::Full);
            }
        });
        let weak = Rc::downgrade(&state);
        window.connect_close_request(move |_| {
            if let Some(state) = weak.upgrade() {
                state.set_mode(ViewMode::Full);
            }
            glib::Propagation::Proceed
        });
        state
    }

    pub(super) fn geometry_guard(&self) -> Rc<Cell<bool>> {
        self.geometry_suppressed.clone()
    }

    pub(super) fn toggle(&self) {
        self.set_mode(next_mode(self.mode.get()));
    }

    fn set_mode(&self, mode: ViewMode) {
        if mode == self.mode.get() {
            return;
        }
        match mode {
            ViewMode::Minimal => self.enter_minimal(),
            ViewMode::Full => self.restore_full(),
        }
        self.mode.set(mode);
        tracing::info!(?mode, "window view mode changed");
    }

    fn enter_minimal(&self) {
        self.full_width.set(self.window.width().max(FULL_MIN_WIDTH));
        self.full_height
            .set(self.window.height().max(FULL_MIN_HEIGHT));
        self.full_maximized.set(self.window.is_maximized());
        self.geometry_suppressed.set(true);
        if self.window.is_maximized() {
            self.window.unmaximize();
        }
        if let Some(bar) = &self.player_bar {
            if bar.parent().is_some() {
                self.bottom_box.remove(bar);
            }
            self.minimal_content.append(bar);
        }
        self.window.set_width_request(MINIMAL_WIDTH);
        self.window.set_height_request(MINIMAL_HEIGHT);
        self.window.set_resizable(false);
        self.window.set_default_size(MINIMAL_WIDTH, MINIMAL_HEIGHT);
        self.window.set_content(Some(&self.minimal_root));
    }

    fn restore_full(&self) {
        if let Some(bar) = &self.player_bar {
            if bar.parent().is_some() {
                self.minimal_content.remove(bar);
            }
            self.bottom_box.append(bar);
        }
        self.window.set_width_request(FULL_MIN_WIDTH);
        self.window.set_height_request(FULL_MIN_HEIGHT);
        self.window.set_resizable(true);
        self.window
            .set_default_size(self.full_width.get(), self.full_height.get());
        self.window.set_content(Some(&self.full_root));
        if self.full_maximized.get() {
            self.window.maximize();
        }
        self.geometry_suppressed.set(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_alternates_full_and_minimal_modes() {
        assert_eq!(next_mode(ViewMode::Full), ViewMode::Minimal);
        assert_eq!(next_mode(ViewMode::Minimal), ViewMode::Full);
    }
}
