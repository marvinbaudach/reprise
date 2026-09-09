//! Persistent Library/Compact window switching.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AdwApplicationWindowExt;
use reprise_core::db::Db;
use reprise_core::library::settings::{self, CompactLayout, WindowViewMode};

use super::compact_player::CompactPlayer;
use super::compact_player_layouts::{
    CARD_MARGIN, CSS_PASSTHROUGH, CSS_WINDOW_CLASS, MINI_HEIGHT, MINI_WIDTH,
};
use super::file_open::StartupOpenIntent;
use super::first_run::FirstRunDecision;
use super::strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) struct ViewTransition {
    pub(in crate::ui) mode: WindowViewMode,
    pub(in crate::ui) layout: CompactLayout,
}
pub(in crate::ui) fn startup_transition(
    persisted_mode: WindowViewMode,
    persisted_layout: CompactLayout,
    first_run: FirstRunDecision,
    intent: StartupOpenIntent,
) -> ViewTransition {
    let mode = if first_run == FirstRunDecision::ShowWizard {
        WindowViewMode::Library
    } else if intent == StartupOpenIntent::CompactPlayback {
        WindowViewMode::Compact
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

#[cfg(test)]
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

pub(in crate::ui) struct MinimalView {
    library_window: adw::ApplicationWindow,
    compact_window: Option<adw::ApplicationWindow>,
    compact: Option<CompactPlayer>,
    compact_root: Option<adw::ToastOverlay>,
    conn: Rc<Db>,
    transition: Cell<ViewTransition>,
    toast: Rc<dyn Fn(&str)>,
}

/// Tags (or clears) every container between the transparent compact window and
/// the card with [`CSS_PASSTHROUGH`], so no opaque container edge shows past the
/// card's rounded corners (MINI-1). Walks the live ancestor chain from the toast
/// overlay up to the window, so it never depends on libadwaita's internal CSS
/// node names — a node-name miss was what still leaked a background edge.
fn set_container_passthrough(
    compact_root: &adw::ToastOverlay,
    window: &adw::ApplicationWindow,
    on: bool,
) {
    let apply = |widget: &gtk4::Widget| {
        if on {
            widget.add_css_class(CSS_PASSTHROUGH);
        } else {
            widget.remove_css_class(CSS_PASSTHROUGH);
        }
    };
    // The toast overlay's child is the mini-player's WindowHandle.
    if let Some(child) = compact_root.child() {
        apply(&child);
    }
    // Every ancestor up to (not including) the window — its own transparency
    // comes from the `window.<class>` rule.
    let window = window.clone().upcast::<gtk4::Widget>();
    let mut node: Option<gtk4::Widget> = Some(compact_root.clone().upcast());
    while let Some(widget) = node {
        if widget == window {
            break;
        }
        apply(&widget);
        node = widget.parent();
    }
}

impl MinimalView {
    pub(in crate::ui) fn new(
        window: &adw::ApplicationWindow,
        compact: Option<&CompactPlayer>,
        conn: Rc<Db>,
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
        let compact = compact.cloned();
        let compact_root = compact.as_ref().map(|compact| {
            let overlay = adw::ToastOverlay::new();
            overlay.set_child(Some(compact.handle()));
            overlay
        });
        let compact_window = compact_root.as_ref().map(|compact_root| {
            let app = window
                .application()
                .expect("the Library window belongs to the application");
            let compact_window = adw::ApplicationWindow::builder()
                .application(&app)
                .title(strings::text(strings::APP_NAME))
                .transient_for(window)
                .decorated(true)
                .build();
            compact_window.set_content(Some(compact_root));
            compact_window.add_css_class(CSS_WINDOW_CLASS);
            set_container_passthrough(compact_root, &compact_window, true);
            apply_compact_metrics(&compact_window);
            let library_window = window.downgrade();
            compact_window.connect_close_request(move |_| {
                if let Some(library_window) = library_window.upgrade() {
                    library_window.close();
                }
                gtk4::glib::Propagation::Proceed
            });
            let compact_window_weak = compact_window.downgrade();
            window.connect_close_request(move |_| {
                if let Some(compact_window) = compact_window_weak.upgrade() {
                    compact_window.destroy();
                }
                gtk4::glib::Propagation::Proceed
            });
            compact_window
        });
        Rc::new(Self {
            library_window: window.clone(),
            compact_window,
            compact,
            compact_root,
            conn,
            transition: Cell::new(initial),
            toast,
        })
    }

    pub(in crate::ui) fn compact_window(&self) -> Option<adw::ApplicationWindow> {
        self.compact_window.clone()
    }

    pub(in crate::ui) fn active_window(&self) -> adw::ApplicationWindow {
        match self.transition.get().mode {
            WindowViewMode::Library => self.library_window.clone(),
            WindowViewMode::Compact => self
                .compact_window
                .clone()
                .unwrap_or_else(|| self.library_window.clone()),
        }
    }

    pub(in crate::ui) fn is_library_mode(&self) -> bool {
        self.transition.get().mode == WindowViewMode::Library
    }

    pub(in crate::ui) fn toggle(&self) {
        let current = self.transition.get();
        let desired = toggled_transition(current);
        if desired.mode == WindowViewMode::Compact && self.compact.is_none() {
            self.show_toast(strings::COMPACT_PLAYER_UNAVAILABLE);
            return;
        }
        let persisted = {
            let conn = &self.conn;
            settings::set_window_view_mode(conn, desired.mode)
        };
        if let Err(error) = persisted {
            tracing::warn!(%error, ?desired, "could not persist window view mode");
            self.show_toast(strings::VIEW_MODE_SAVE_FAILED);
            debug_assert_eq!(persisted_mode_transition(current, false), (current, false));
            return;
        }
        match desired.mode {
            WindowViewMode::Library => self.restore_library(),
            WindowViewMode::Compact => self.enter_compact(),
        }
        self.transition.set(desired);
        tracing::info!(mode = ?desired.mode, layout = ?desired.layout, "window view mode changed");
    }

    pub(in crate::ui) fn apply_initial(&self) {
        let initial = self.transition.get();
        match initial.mode {
            WindowViewMode::Library => self.restore_library(),
            WindowViewMode::Compact => self.enter_compact(),
        }
        tracing::info!(mode = ?initial.mode, layout = ?initial.layout, "initial window view applied");
    }

    pub(in crate::ui) fn refresh_geometry(&self) {
        if self.transition.get().mode == WindowViewMode::Compact {
            if let Some(window) = &self.compact_window {
                apply_compact_metrics(window);
            }
        }
    }

    fn enter_compact(&self) {
        let Some(compact_window) = &self.compact_window else {
            return;
        };
        compact_window.present();
        self.library_window.set_visible(false);
    }

    fn restore_library(&self) {
        self.library_window.present();
        if let Some(compact_window) = &self.compact_window {
            compact_window.set_visible(false);
        }
    }

    fn show_toast(&self, message: &str) {
        let message = strings::text(message);
        if self.transition.get().mode == WindowViewMode::Compact {
            if let Some(overlay) = &self.compact_root {
                overlay.add_toast(crate::ui::toasts::plain(&message));
                return;
            }
        }
        (self.toast)(&message);
    }
}

fn apply_compact_metrics(window: &adw::ApplicationWindow) {
    // The window is the card plus its shadow-room margin on every side, so the
    // card renders at full size with room for the drop shadow instead of
    // overflowing a too-small toplevel (MINI-1).
    let width = MINI_WIDTH + 2 * CARD_MARGIN;
    let height = MINI_HEIGHT + 2 * CARD_MARGIN;
    window.set_resizable(true);
    window.set_width_request(width);
    window.set_height_request(height);
    window.set_default_size(width, height);
    window.set_resizable(false);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::file_open::StartupOpenIntent;

    #[test]
    fn toggling_alternates_library_and_compact_modes() {
        let library = ViewTransition {
            mode: WindowViewMode::Library,
            layout: CompactLayout::Card,
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
    }

    #[test]
    fn first_run_always_forces_the_library() {
        let transition = startup_transition(
            WindowViewMode::Compact,
            CompactLayout::Pill,
            FirstRunDecision::ShowWizard,
            StartupOpenIntent::Library,
        );
        assert_eq!(transition.mode, WindowViewMode::Library);
        assert_eq!(transition.layout, CompactLayout::Pill);
    }

    #[test]
    fn file_open_intent_precedes_persisted_mode_but_not_first_run() {
        let cases = [
            (
                WindowViewMode::Compact,
                CompactLayout::Pill,
                FirstRunDecision::ShowWizard,
                StartupOpenIntent::CompactPlayback,
                WindowViewMode::Library,
            ),
            (
                WindowViewMode::Library,
                CompactLayout::Card,
                FirstRunDecision::AlreadyCompleted,
                StartupOpenIntent::CompactPlayback,
                WindowViewMode::Compact,
            ),
            (
                WindowViewMode::Compact,
                CompactLayout::Cover,
                FirstRunDecision::ExistingLibrary,
                StartupOpenIntent::Library,
                WindowViewMode::Compact,
            ),
            (
                WindowViewMode::Library,
                CompactLayout::Pill,
                FirstRunDecision::AlreadyCompleted,
                StartupOpenIntent::Library,
                WindowViewMode::Library,
            ),
        ];

        for (persisted_mode, layout, first_run, intent, expected_mode) in cases {
            assert_eq!(
                startup_transition(persisted_mode, layout, first_run, intent),
                ViewTransition {
                    mode: expected_mode,
                    layout,
                }
            );
        }
    }

    #[test]
    fn completed_or_existing_library_restores_compact() {
        for decision in [
            FirstRunDecision::AlreadyCompleted,
            FirstRunDecision::ExistingLibrary,
        ] {
            assert_eq!(
                startup_transition(
                    WindowViewMode::Compact,
                    CompactLayout::Cover,
                    decision,
                    StartupOpenIntent::Library,
                ),
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
            layout: CompactLayout::Cover,
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
            layout: CompactLayout::Pill,
        };
        let (transition, committed) =
            selected_layout_transition(current, CompactLayout::Card, false);
        assert!(!committed);
        assert_eq!(transition, current);
    }
}
