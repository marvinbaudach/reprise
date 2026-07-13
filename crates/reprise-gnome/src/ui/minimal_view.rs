//! Persistent Library/Compact root switching and geometry isolation.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::library::settings::{self, CompactLayout, WindowViewMode};
use rusqlite::Connection;

use super::compact_player::{CompactPlayer, CompactPlayerHandle};
use super::compact_player_layouts::{self, LayoutMetrics};
use super::first_run::FirstRunDecision;
use super::strings;

const FULL_MIN_WIDTH: i32 = 600;
const FULL_MIN_HEIGHT: i32 = 400;
const FULL_DEFAULT_WIDTH: i32 = 1200;
const FULL_DEFAULT_HEIGHT: i32 = 800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ViewTransition {
    pub(super) mode: WindowViewMode,
    pub(super) layout: CompactLayout,
}

pub(super) fn startup_transition(
    persisted_mode: WindowViewMode,
    persisted_layout: CompactLayout,
    first_run: FirstRunDecision,
) -> ViewTransition {
    let mode = if first_run == FirstRunDecision::ShowWizard {
        WindowViewMode::Library
    } else {
        persisted_mode
    };
    ViewTransition {
        mode,
        layout: persisted_layout,
    }
}

fn toggled_transition(current: ViewTransition) -> ViewTransition {
    let mode = match current.mode {
        WindowViewMode::Library => WindowViewMode::Compact,
        WindowViewMode::Compact => WindowViewMode::Library,
    };
    ViewTransition { mode, ..current }
}

fn persisted_mode_transition(current: ViewTransition, persisted: bool) -> (ViewTransition, bool) {
    if persisted {
        (toggled_transition(current), true)
    } else {
        (current, false)
    }
}

fn selected_layout_transition(
    current: ViewTransition,
    layout: CompactLayout,
    persisted: bool,
) -> (ViewTransition, bool) {
    if persisted {
        (ViewTransition { layout, ..current }, true)
    } else {
        (current, false)
    }
}

fn layout_metrics(layout: CompactLayout) -> LayoutMetrics {
    compact_player_layouts::metrics(layout)
}

fn full_dimension(value: i32, minimum: i32, fallback: i32) -> i32 {
    if value > 0 {
        value.max(minimum)
    } else {
        fallback
    }
}

fn updated_full_geometry(current: (i32, i32), live: (i32, i32), maximized: bool) -> (i32, i32) {
    if maximized {
        current
    } else {
        (
            full_dimension(live.0, FULL_MIN_WIDTH, current.0),
            full_dimension(live.1, FULL_MIN_HEIGHT, current.1),
        )
    }
}

pub(super) struct MinimalView {
    window: adw::ApplicationWindow,
    full_root: adw::NavigationSplitView,
    compact: Option<CompactPlayerHandle>,
    compact_root: Option<adw::ToastOverlay>,
    conn: Rc<RefCell<Connection>>,
    transition: Cell<ViewTransition>,
    full_width: Cell<i32>,
    full_height: Cell<i32>,
    full_maximized: Cell<bool>,
    geometry_suppressed: Rc<Cell<bool>>,
    toast: Rc<dyn Fn(&str)>,
}

impl MinimalView {
    pub(super) fn new(
        window: &adw::ApplicationWindow,
        full_root: &adw::NavigationSplitView,
        compact: Option<&CompactPlayer>,
        conn: Rc<RefCell<Connection>>,
        initial: ViewTransition,
        toast: Rc<dyn Fn(&str)>,
    ) -> Rc<Self> {
        let initial = if compact.is_none() && initial.mode == WindowViewMode::Compact {
            tracing::warn!("compact mode unavailable without a playback controller; using Library");
            ViewTransition {
                mode: WindowViewMode::Library,
                ..initial
            }
        } else {
            initial
        };
        let compact = compact.map(CompactPlayer::handle);
        let compact_root = compact.as_ref().map(|compact| {
            let overlay = adw::ToastOverlay::new();
            overlay.set_child(Some(compact.widget()));
            overlay
        });
        let state = Rc::new(Self {
            window: window.clone(),
            full_root: full_root.clone(),
            compact,
            compact_root,
            conn,
            transition: Cell::new(initial),
            full_width: Cell::new(full_dimension(
                window.default_width(),
                FULL_MIN_WIDTH,
                FULL_DEFAULT_WIDTH,
            )),
            full_height: Cell::new(full_dimension(
                window.default_height(),
                FULL_MIN_HEIGHT,
                FULL_DEFAULT_HEIGHT,
            )),
            full_maximized: Cell::new(window.is_maximized()),
            geometry_suppressed: Rc::new(Cell::new(false)),
            toast,
        });
        state.wire_full_geometry_tracking();
        state
    }

    pub(super) fn geometry_guard(&self) -> Rc<Cell<bool>> {
        self.geometry_suppressed.clone()
    }

    pub(super) fn toggle(&self) {
        let current = self.transition.get();
        let desired = toggled_transition(current);
        if desired.mode == WindowViewMode::Compact && self.compact.is_none() {
            self.show_toast(strings::COMPACT_PLAYER_UNAVAILABLE);
            return;
        }
        let persisted = {
            let conn = self.conn.borrow();
            settings::set_window_view_mode(&conn, desired.mode)
        };
        if let Err(error) = persisted {
            tracing::warn!(%error, ?desired, "could not persist window view mode");
            self.show_toast(strings::VIEW_MODE_SAVE_FAILED);
            debug_assert_eq!(persisted_mode_transition(current, false), (current, false));
            return;
        }
        match desired.mode {
            WindowViewMode::Library => self.restore_library(),
            WindowViewMode::Compact => self.enter_compact(true),
        }
        self.transition.set(desired);
        tracing::info!(mode = ?desired.mode, layout = ?desired.layout, "window view mode changed");
    }

    pub(super) fn select_layout(&self, layout: CompactLayout) {
        let current = self.transition.get();
        if layout == current.layout {
            return;
        }
        let Some(compact) = &self.compact else {
            self.show_toast(strings::COMPACT_PLAYER_UNAVAILABLE);
            return;
        };
        let persisted = {
            let conn = self.conn.borrow();
            settings::set_compact_layout(&conn, layout)
        };
        if let Err(error) = persisted {
            tracing::warn!(%error, ?layout, "could not persist compact layout");
            compact.set_layout(current.layout);
            if current.mode == WindowViewMode::Compact {
                self.apply_compact_metrics(current.layout);
            }
            self.show_toast(strings::COMPACT_LAYOUT_SAVE_FAILED);
            return;
        }
        let (next, committed) = selected_layout_transition(current, layout, true);
        debug_assert!(committed);
        compact.set_layout(layout);
        self.transition.set(next);
        if next.mode == WindowViewMode::Compact {
            self.apply_compact_metrics(layout);
        }
        tracing::info!(?layout, "compact layout changed");
    }

    pub(super) fn apply_initial(&self) {
        let initial = self.transition.get();
        if let Some(compact) = &self.compact {
            compact.set_layout(initial.layout);
        }
        match initial.mode {
            WindowViewMode::Library => self.restore_library(),
            WindowViewMode::Compact => self.enter_compact(false),
        }
        tracing::info!(mode = ?initial.mode, layout = ?initial.layout, "initial window view applied");
    }

    fn enter_compact(&self, capture_full_geometry: bool) {
        let (Some(compact), Some(compact_root)) = (&self.compact, &self.compact_root) else {
            return;
        };
        if capture_full_geometry {
            let maximized = self.window.is_maximized();
            let geometry = updated_full_geometry(
                (self.full_width.get(), self.full_height.get()),
                (self.window.width(), self.window.height()),
                maximized,
            );
            self.full_width.set(geometry.0);
            self.full_height.set(geometry.1);
            self.full_maximized.set(maximized);
        }
        self.geometry_suppressed.set(true);
        if self.window.is_maximized() {
            self.window.unmaximize();
        }
        let layout = self.transition.get().layout;
        compact.set_layout(layout);
        // Drop the Library root and its much larger minimum before making the
        // toplevel non-resizable. Otherwise GTK/WM can freeze the old Library
        // allocation and leave a small compact child floating in a large
        // window after a maximized or otherwise wide Library session.
        self.window.set_resizable(true);
        self.window.set_width_request(-1);
        self.window.set_height_request(-1);
        self.window.set_content(Some(compact_root));
        self.apply_compact_metrics(layout);
    }

    fn restore_library(&self) {
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

    fn apply_compact_metrics(&self, layout: CompactLayout) {
        let metrics = layout_metrics(layout);
        self.window.set_resizable(true);
        self.window.set_width_request(metrics.width);
        self.window.set_height_request(metrics.height);
        self.window.set_default_size(metrics.width, metrics.height);
        self.window.set_resizable(false);
    }

    fn show_toast(&self, message: &str) {
        let message = strings::text(message);
        if self.transition.get().mode == WindowViewMode::Compact {
            if let Some(overlay) = &self.compact_root {
                overlay.add_toast(adw::Toast::new(&message));
                return;
            }
        }
        (self.toast)(&message);
    }

    fn wire_full_geometry_tracking(self: &Rc<Self>) {
        for property in ["width", "height", "maximized"] {
            let state = Rc::downgrade(self);
            self.window
                .connect_notify_local(Some(property), move |window, _| {
                    let Some(state) = state.upgrade() else {
                        return;
                    };
                    if state.geometry_suppressed.get() {
                        return;
                    }
                    let maximized = window.is_maximized();
                    let geometry = updated_full_geometry(
                        (state.full_width.get(), state.full_height.get()),
                        (window.width(), window.height()),
                        maximized,
                    );
                    state.full_width.set(geometry.0);
                    state.full_height.set(geometry.1);
                    state.full_maximized.set(maximized);
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_alternates_library_and_compact_modes() {
        let library = ViewTransition {
            mode: WindowViewMode::Library,
            layout: CompactLayout::Bar,
        };
        assert_eq!(toggled_transition(library).mode, WindowViewMode::Compact);
        assert_eq!(toggled_transition(toggled_transition(library)), library);
    }

    #[test]
    fn library_compact_toggle_retains_the_selected_layout() {
        let current = ViewTransition {
            mode: WindowViewMode::Library,
            layout: CompactLayout::Card,
        };
        let compact = toggled_transition(current);
        assert_eq!(compact.mode, WindowViewMode::Compact);
        assert_eq!(compact.layout, CompactLayout::Card);
        assert_eq!(toggled_transition(compact), current);
        assert_eq!(full_dimension(900, FULL_MIN_WIDTH, FULL_DEFAULT_WIDTH), 900);
        assert_eq!(full_dimension(0, FULL_MIN_WIDTH, FULL_DEFAULT_WIDTH), 1200);
        assert_eq!(
            updated_full_geometry((900, 600), (1920, 1080), true),
            (900, 600)
        );
        assert_eq!(
            updated_full_geometry((900, 600), (820, 540), false),
            (820, 540)
        );
    }

    #[test]
    fn first_run_always_forces_the_library() {
        let transition = startup_transition(
            WindowViewMode::Compact,
            CompactLayout::Pill,
            FirstRunDecision::ShowWizard,
        );
        assert_eq!(transition.mode, WindowViewMode::Library);
        assert_eq!(transition.layout, CompactLayout::Pill);
    }

    #[test]
    fn completed_or_existing_library_restores_compact() {
        for decision in [
            FirstRunDecision::AlreadyCompleted,
            FirstRunDecision::ExistingLibrary,
        ] {
            assert_eq!(
                startup_transition(WindowViewMode::Compact, CompactLayout::Cover, decision),
                ViewTransition {
                    mode: WindowViewMode::Compact,
                    layout: CompactLayout::Cover,
                }
            );
        }
    }

    #[test]
    fn selecting_a_layout_keeps_compact_mode() {
        let current = ViewTransition {
            mode: WindowViewMode::Compact,
            layout: CompactLayout::Bar,
        };
        assert_eq!(
            selected_layout_transition(current, CompactLayout::Card, true).0,
            ViewTransition {
                mode: WindowViewMode::Compact,
                layout: CompactLayout::Card,
            }
        );
    }

    #[test]
    fn failed_mode_persistence_keeps_the_root_and_state() {
        let current = ViewTransition {
            mode: WindowViewMode::Library,
            layout: CompactLayout::Cover,
        };
        assert_eq!(persisted_mode_transition(current, false), (current, false));
    }

    #[test]
    fn failed_layout_persistence_restores_previous_layout_and_metrics() {
        let current = ViewTransition {
            mode: WindowViewMode::Compact,
            layout: CompactLayout::Bar,
        };
        let (transition, committed) =
            selected_layout_transition(current, CompactLayout::Card, false);
        assert!(!committed);
        assert_eq!(transition, current);
        assert_eq!(
            layout_metrics(transition.layout),
            layout_metrics(current.layout)
        );
    }
}
