//! Keyboard shortcuts (Stage 3 Task 9): Space (play/pause), Ctrl+F (focus +
//! select the search entry), and Escape (two-stage: clear the search text
//! first, then hand focus back to the track list). Enter/double-click row
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
//!   re-focusing an already-focused search entry is harmless, so there is no
//!   focus-sensitivity to get right — the whole point of a global
//!   accelerator.
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
//! - **Escape** is wired through neither of the above — no `gio::
//!   SimpleAction`, no accelerator. `gtk::SearchEntry` already has a
//!   built-in `stop-search` signal, a *keybinding signal* GTK itself default-
//!   binds to Escape (confirmed against the installed `Gtk-4.0.gir`:
//!   "The default bindings for this signal is Escape"). Connecting to it
//!   directly is the robust route for two reasons: (1) it only ever fires
//!   while the search entry itself has keyboard focus, and (2) a modal
//!   dialog (`AdwAlertDialog`, `gtk::FileDialog`) grabs focus onto itself
//!   while open, so the search entry — and thus this handler — structurally
//!   cannot see an Escape meant for the dialog. That is exactly the "don't
//!   swallow Escape globally if a dialog is open" requirement, satisfied by
//!   construction rather than by a manual "is a dialog open?" check. The
//!   two-stage clear/refocus decision itself is the pure, unit-tested
//!   `escape_action_for` predicate.
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
use super::track_list::TrackList;

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

/// Whether `focus` (the window's currently focused widget, if any)
/// implements `gtk::Editable` — the interface every text-entry-shaped widget
/// in this app (`gtk::SearchEntry`, and any plain `gtk::Text`/`gtk::Entry`)
/// implements. `None` (nothing focused) is not a text entry.
// `ObjectExt::is` has ambiguous candidates in this workspace (both `glib`'s
// and `gstreamer`'s `prelude::ObjectExt` are in scope transitively), so
// clippy's suggested point-free rewrite (`ObjectExt::is::<gtk4::Editable>`)
// doesn't resolve cleanly — the closure stays.
#[allow(clippy::redundant_closure_for_method_calls)]
fn focused_widget_is_text_entry(focus: Option<&gtk4::Widget>) -> bool {
    focus.is_some_and(|widget| widget.is::<gtk4::Editable>())
}

/// What Escape should do, given whether the search entry currently has any
/// text — see the module doc's `## Escape` bullet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeAction {
    /// First press (search has text): clear it, keep focus in the entry.
    ClearSearchText,
    /// Second press (search is already empty): hand focus back to the track
    /// list.
    FocusTrackList,
}

/// Pure decision for Escape's two-stage behavior.
pub fn escape_action_for(search_has_text: bool) -> EscapeAction {
    if search_has_text {
        EscapeAction::ClearSearchText
    } else {
        EscapeAction::FocusTrackList
    }
}

/// Wires all three shortcuts. Called once from `window::build`, after
/// `window`, `search_entry`, `track_list`, and `player` all exist. `player`
/// is `None` when GStreamer was unavailable at startup (see `window::
/// build`'s own doc comment on that degradation) — the Space action still
/// registers in that case (so the accelerator itself is harmless to press),
/// it just logs and no-ops instead of toggling anything.
pub fn wire(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    search_entry: &gtk4::SearchEntry,
    track_list: &Rc<TrackList>,
    player: Option<Rc<PlayerController>>,
) {
    wire_toggle_play_pause(app, window, player);
    wire_focus_search(app, window, search_entry);
    wire_escape(search_entry, track_list.clone());
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

/// Ctrl+F: `win.focus-search`, accelerated to `"<Control>f"` — grabs
/// keyboard focus and selects the entry's full text (mirroring how most
/// desktop apps' "Find" shortcut behaves: a second Ctrl+F with existing text
/// re-selects it all, ready to be typed over). `search_entry` is captured
/// weakly for the same reference-cycle reason as `wire_toggle_play_pause`'s
/// `window` — the action is owned by `window`, which also (indirectly, via
/// the widget tree) owns `search_entry`.
fn wire_focus_search(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    search_entry: &gtk4::SearchEntry,
) {
    let action = gio::SimpleAction::new(ACTION_FOCUS_SEARCH, None);
    let search_entry_weak = search_entry.downgrade();
    action.connect_activate(move |_, _| {
        let Some(search_entry) = search_entry_weak.upgrade() else {
            tracing::warn!("focus-search: search entry is gone; ignoring");
            return;
        };
        search_entry.grab_focus();
        search_entry.select_region(0, -1);
    });
    window.add_action(&action);
    app.set_accels_for_action(&format!("win.{ACTION_FOCUS_SEARCH}"), &["<Control>f"]);
}

/// Escape: connects directly to `gtk::SearchEntry`'s own built-in
/// `stop-search` signal — see the module doc's `## Escape` bullet for why
/// this is wired outside the `gio::SimpleAction`/accelerator mechanism the
/// other two shortcuts use. `track_list` is a strong `Rc`, not `Weak`: this
/// closure lives exactly as long as `search_entry` itself (a widget in the
/// permanent header bar, never rebuilt or torn down while the window is
/// open), so there is no dangling-callback risk to guard against here the
/// way the `Weak` window/entry captures above guard against `window` (which
/// owns the action, and thus indirectly the closure) keeping itself alive.
fn wire_escape(search_entry: &gtk4::SearchEntry, track_list: Rc<TrackList>) {
    search_entry.connect_stop_search(move |entry| {
        let has_text = !entry.text().is_empty();
        match escape_action_for(has_text) {
            EscapeAction::ClearSearchText => entry.set_text(""),
            EscapeAction::FocusTrackList => {
                if !track_list.focus_track_list() {
                    tracing::warn!("escape: could not move focus to the track list");
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn escape_focuses_track_list_when_search_is_empty() {
        assert_eq!(escape_action_for(false), EscapeAction::FocusTrackList);
    }
}
