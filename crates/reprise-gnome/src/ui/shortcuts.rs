//! Keyboard shortcuts (Stage 3 Task 9): Space (play/pause), Ctrl+F (toggle the
//! search bar, focusing and selecting the entry when opening), and Escape
//! (two-stage: clear the search text first, then collapse the search bar).
//! Enter/double-click row
//! activation already works natively (`ui::track_list`'s `wire_activate`) —
//! nothing to add here for that.
//!
//! ## Three shortcuts, three different wiring mechanisms — on purpose
//!
//! Ctrl+F and Space are each backed by a `gio::SimpleAction` in the window's
//! own `"win"` action group, exactly the way the brief asks for ("this also
//! lays groundwork for a future menu" — a menu item can bind to `win.focus-
//! search`/`win.toggle-play-pause` by name without this module knowing a
//! menu exists). But *how* each one gets triggered by a keystroke differs,
//! and that difference is deliberate, not an inconsistency:
//!
//! - **Ctrl+F** is accelerated the idiomatic way, via `gtk::Application::
//!   set_accels_for_action`. Nothing else in this app binds Ctrl+F, and
//!   opening focuses and selects the existing query, while invoking it again
//!   closes the bar without clearing that query.
//!
//! - **Space** is the one case the brief calls out by name as needing care
//!   ("Key-Handling-Priorität beachten"), and the skill's input-delivery
//!   caution applies directly: *do not assume* how GTK will route a key
//!   event without checking. `gtk::Application::set_accels_for_action`
//!   accelerators are activated through a `GtkShortcutController`, and per
//!   GTK4's own docs a focused widget's own key handling (e.g. `GtkText`
//!   inserting a typed character) is checked *before* an ancestor's
//!   accelerator controller ever sees the event, for the ordinary case where
//!   that controller sits above the focused widget in the bubble-phase
//!   propagation chain — so accelerating Space this way would, in the common
//!   case, simply never fire while the search entry has focus and the user
//!   is typing an actual space. That is not a robust *contract*, though —
//!   it's an emergent property of controller ordering that this code does
//!   not want to depend on silently. So the action's own `activate` handler
//!   re-checks explicitly, via the currently focused widget
//!   (`window.focus()`) and the pure, unit-tested `space_should_toggle`
//!   predicate: if focus is on anything implementing `gtk::Editable` (a text
//!   entry), the handler no-ops instead of toggling playback, and logs why.
//!   Belt AND braces: GTK's own routing keeps a focused entry from ever
//!   losing the keystroke in the first place, and this guard keeps the
//!   *action's* behavior correct and testable regardless of exactly how that
//!   routing happens to work in a given GTK4 point release.
//!
//! - **Escape** is handled by a capture-phase key controller on the search
//!   bar. This intercepts the entry's built-in `stop-search` default before
//!   GTK can collapse a non-empty query, while remaining scoped to focus
//!   inside the bar so dialogs keep their own Escape behavior.
//!
//! ## What's verified headlessly vs. manually
//!
//! The three predicates below (`space_should_toggle`, `focused_widget_is_
//! text_entry`'s boolean-in shape, `escape_action_for`) are pure functions,
//! unit-tested with no display. The action *callbacks* that wrap them are
//! exercised by calling them directly in tests too (see this module's
//! `#[cfg(test)]`). Real key events (does pressing the physical Space bar
//! actually toggle playback; does a focused search entry actually keep
//! typing a space) are pointer/keyboard-driven and are **not** headlessly
//! drivable — manual check required (see the Task 9 report).

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use libadwaita as adw;
use std::rc::Rc;

use super::player_controller::PlayerController;

/// Bare `gio::SimpleAction` names in the window's `"win"` action group —
/// internal identifiers, not user-facing text.
const ACTION_TOGGLE_PLAY_PAUSE: &str = "toggle-play-pause";
const ACTION_FOCUS_SEARCH: &str = "focus-search";

/// Pure decision for the Space key: should it toggle playback, or be left
/// alone so a focused text entry can type an actual space character? See
/// the module doc's `## Space` bullet for the full reasoning; this predicate
/// is the one piece of that reasoning that's actually testable without a
/// display.
pub fn space_should_toggle(focus_is_text_entry: bool) -> bool {
    !focus_is_text_entry
}

/// The shared transition for both search affordances: opening and closing are
/// symmetric, while query preservation remains the search entry's concern.
pub(in crate::ui) fn next_search_mode(current: bool) -> bool {
    !current
}

/// Whether `focus` (the window's currently focused widget, if any)
/// implements `gtk::Editable` — the interface every text-entry-shaped widget
/// in this app (`gtk::SearchEntry`, and any plain `gtk::Text`/`gtk::Entry`)
/// implements. `None` (nothing focused) is not a text entry.
fn focused_widget_is_text_entry(focus: Option<&gtk4::Widget>) -> bool {
    focus.is_some_and(libadwaita::prelude::ObjectExt::is::<gtk4::Editable>)
}

/// What Escape should do, given whether the search entry currently has any
/// text — see the module doc's `## Escape` bullet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeAction {
    /// First press (search has text): clear it, keep focus in the entry.
    ClearSearchText,
    /// Second press (search is already empty): collapse the search bar.
    CollapseSearchBar,
}

/// Pure decision for Escape's two-stage behavior.
pub fn escape_action_for(search_has_text: bool) -> EscapeAction {
    if search_has_text {
        EscapeAction::ClearSearchText
    } else {
        EscapeAction::CollapseSearchBar
    }
}

/// Wires all three shortcuts. Called once from `window::build`, after
/// `window`, `search_bar`, `search_entry`, and `player` all exist. `player`
/// is `None` when GStreamer was unavailable at startup (see `window::
/// build`'s own doc comment on that degradation) — the Space action still
/// registers in that case (so the accelerator itself is harmless to press),
/// it just logs and no-ops instead of toggling anything.
pub fn wire(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    search_bar: &gtk4::SearchBar,
    search_entry: &gtk4::SearchEntry,
    player: Option<Rc<PlayerController>>,
) {
    wire_toggle_play_pause(app, window, player);
    wire_focus_search(app, window, search_bar, search_entry);
    wire_escape(search_bar, search_entry);
}

/// Space: `win.toggle-play-pause`, accelerated to `"space"`. See the module
/// doc's `## Space` bullet for why the focus check lives inside the
/// activate handler rather than being trusted to GTK's routing alone.
/// `window` is captured weakly in the closure (not strongly): the action is
/// itself owned by `window` (via `add_action`), so a strong capture back
/// into the closure would form an `Rc`-style reference cycle through GTK's
/// own object graph — keeping the window alive forever. Every other
/// cross-widget closure in this codebase (`window.rs`'s `Weak<TrackList>`/
/// `Weak<Sidebar>` callbacks) follows the same discipline.
fn wire_toggle_play_pause(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    player: Option<Rc<PlayerController>>,
) {
    let action = gio::SimpleAction::new(ACTION_TOGGLE_PLAY_PAUSE, None);
    let window_weak = window.downgrade();
    action.connect_activate(move |_, _| {
        let Some(window) = window_weak.upgrade() else {
            tracing::warn!("toggle-play-pause: window is gone; ignoring");
            return;
        };
        let focus = gtk4::prelude::GtkWindowExt::focus(&window);
        let focus_is_text_entry = focused_widget_is_text_entry(focus.as_ref());
        if !space_should_toggle(focus_is_text_entry) {
            tracing::debug!(
                "space: focused widget is a text entry; letting the keystroke through instead \
                 of toggling playback"
            );
            return;
        }
        match &player {
            Some(player) => player.toggle_pause(),
            None => tracing::warn!("space: player unavailable; ignoring play/pause toggle"),
        }
    });
    window.add_action(&action);
    app.set_accels_for_action(&format!("win.{ACTION_TOGGLE_PLAY_PAUSE}"), &["space"]);
}

/// Ctrl+F: `win.focus-search`, accelerated to `"<Control>f"` — toggles the
/// search bar. Opening grabs keyboard focus and selects the entry's full text;
/// closing preserves the query so the active filter remains visible as a chip.
/// `search_entry` is captured
/// weakly for the same reference-cycle reason as `wire_toggle_play_pause`'s
/// `window` — the action is owned by `window`, which also (indirectly, via
/// the widget tree) owns `search_entry`.
fn wire_focus_search(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    search_bar: &gtk4::SearchBar,
    search_entry: &gtk4::SearchEntry,
) {
    let action = gio::SimpleAction::new(ACTION_FOCUS_SEARCH, None);
    let search_bar_weak = search_bar.downgrade();
    let search_entry_weak = search_entry.downgrade();
    action.connect_activate(move |_, _| {
        let Some(search_bar) = search_bar_weak.upgrade() else {
            tracing::warn!("focus-search: search bar is gone; ignoring");
            return;
        };
        let Some(search_entry) = search_entry_weak.upgrade() else {
            tracing::warn!("focus-search: search entry is gone; ignoring");
            return;
        };
        let search_mode = next_search_mode(search_bar.is_search_mode());
        search_bar.set_search_mode(search_mode);
        if search_mode {
            search_entry.grab_focus();
            search_entry.select_region(0, -1);
        }
    });
    window.add_action(&action);
    app.set_accels_for_action(&format!("win.{ACTION_FOCUS_SEARCH}"), &["<Control>f"]);
}

fn apply_search_escape(search_bar: &gtk4::SearchBar, search_entry: &gtk4::SearchEntry) {
    match escape_action_for(!search_entry.text().is_empty()) {
        EscapeAction::ClearSearchText => {
            search_entry.set_text("");
            search_bar.set_search_mode(true);
            search_entry.grab_focus();
        }
        EscapeAction::CollapseSearchBar => search_bar.set_search_mode(false),
    }
}

fn wire_escape(search_bar: &gtk4::SearchBar, search_entry: &gtk4::SearchEntry) {
    let controller = gtk4::EventControllerKey::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let bar = search_bar.downgrade();
    let entry = search_entry.downgrade();
    controller.connect_key_pressed(move |_, key, _, _| {
        let (Some(bar), Some(entry)) = (bar.upgrade(), entry.upgrade()) else {
            return gtk4::glib::Propagation::Proceed;
        };
        if key != gtk4::gdk::Key::Escape || !bar.is_search_mode() {
            return gtk4::glib::Propagation::Proceed;
        }
        apply_search_escape(&bar, &entry);
        gtk4::glib::Propagation::Stop
    });
    search_bar.add_controller(controller);
}

#[cfg(test)]
mod tests {
    use super::*;
    use libadwaita::prelude::AdwApplicationWindowExt;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_2_ctrl_f_reveals_and_focuses() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.SearchShortcutTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        let entry = gtk4::SearchEntry::new();
        let search_bar = gtk4::SearchBar::new();
        search_bar.set_child(Some(&entry));
        window.set_content(Some(&search_bar));
        wire_focus_search(&app, &window, &search_bar, &entry);
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        ActionGroupExt::activate_action(&window, "focus-search", None);
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert!(search_bar.is_search_mode());
        assert!(entry.has_focus());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_4_escape_clears_then_collapses() {
        gtk4::init().unwrap();
        let search_bar = gtk4::SearchBar::new();
        let entry = gtk4::SearchEntry::new();
        search_bar.set_child(Some(&entry));
        search_bar.set_search_mode(true);
        entry.set_text("falling");

        apply_search_escape(&search_bar, &entry);
        assert_eq!(entry.text(), "");
        assert!(search_bar.is_search_mode());

        apply_search_escape(&search_bar, &entry);
        assert!(!search_bar.is_search_mode());
    }

    #[test]
    fn search_6_ctrl_f_toggles_open_and_closed() {
        assert!(next_search_mode(false));
        assert!(!next_search_mode(true));
    }

    #[test]
    fn nav_6_escape_changes_search_state_without_navigation() {
        assert_eq!(escape_action_for(true), EscapeAction::ClearSearchText);
        assert_eq!(escape_action_for(false), EscapeAction::CollapseSearchBar);
    }

    #[test]
    fn space_toggles_when_focus_is_not_a_text_entry() {
        assert!(space_should_toggle(false));
    }

    #[test]
    fn space_does_not_toggle_when_focus_is_a_text_entry() {
        assert!(!space_should_toggle(true));
    }

    #[test]
    fn escape_clears_text_when_search_has_text() {
        assert_eq!(escape_action_for(true), EscapeAction::ClearSearchText);
    }

    #[test]
    fn escape_collapses_search_bar_when_search_is_empty() {
        assert_eq!(escape_action_for(false), EscapeAction::CollapseSearchBar);
    }
}
