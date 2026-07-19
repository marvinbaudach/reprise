//! Application and window keyboard shortcuts. Space toggles playback only
//! when the focused widget does not own Space itself. Escape clears search
//! first, then collapses the search bar and returns focus to the active content
//! surface. Ctrl+F toggles the search bar while preserving its query.
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
//! - **Space** uses a capture-phase `EventControllerKey`, not an application
//!   accelerator. The controller inspects the actual focused widget before
//!   dispatch: local controls continue through GTK's native key path, while
//!   passive content activates `win.toggle-play-pause`. A global accelerator
//!   cannot provide that contract because it consumes the key before a
//!   no-op action handler can return it to the focused control.
//!
//! - **Escape** is handled by a capture-phase key controller on the search
//!   bar, scoped to focus inside the bar so dialogs keep their own Escape
//!   behavior. Note this does *not* intercept a `stop-search` default, as this
//!   doc long claimed: the entry never emits `stop-search` here. What actually
//!   happens is that `GtkSearchBar`'s own key-capture controller — installed
//!   on the *window* by `set_key_capture_widget` — forwards the key to the
//!   entry connected via `connect_entry`, the entry consumes Escape, and the
//!   bar reads that as typing and re-opens itself after our handler returns.
//!   `Propagation::Stop` cannot prevent it, because that controller sits at a
//!   different dispatch step. Stage two therefore collapses on idle; see
//!   `apply_search_escape`.
//!
//! ## What's verified headlessly vs. manually
//!
//! The predicates below (`space_should_toggle`, `focused_widget_owns_space`'s
//! boolean-in shape, `escape_action_for`) are pure functions,
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
fn focused_widget_owns_space(focus: Option<&gtk4::Widget>) -> bool {
    focus.is_some_and(|widget| {
        widget.is::<gtk4::Editable>()
            || widget.is::<gtk4::Button>()
            || widget.is::<gtk4::CheckButton>()
            || widget.is::<gtk4::Switch>()
            || widget.is::<gtk4::Range>()
            || widget.is::<gtk4::DropDown>()
            || widget.is::<gtk4::ListBox>()
            || widget.is::<gtk4::FlowBox>()
            || widget.is::<gtk4::ListView>()
            || widget.is::<gtk4::GridView>()
            || widget.is::<gtk4::ColumnView>()
    })
}

/// What Escape should do, given whether the search entry currently has any
/// text — see the module doc's `## Escape` bullet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EscapeAction {
    /// First press (search has text): clear it, keep focus in the entry.
    ClearSearchText,
    /// Second press (search is already empty): collapse the bar and hand focus
    /// back to the active content view.
    CollapseSearchBarAndFocusActiveContent,
}

/// Pure decision for Escape's two-stage behavior.
pub fn escape_action_for(search_has_text: bool) -> EscapeAction {
    if search_has_text {
        EscapeAction::ClearSearchText
    } else {
        EscapeAction::CollapseSearchBarAndFocusActiveContent
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
    focus_active_content: Rc<dyn Fn() -> bool>,
    player: Option<Rc<PlayerController>>,
) {
    wire_toggle_play_pause(app, window, player);
    wire_window_lifecycle(app, window);
    wire_focus_search(app, window, search_bar, search_entry);
    wire_escape(search_bar, search_entry, focus_active_content);
}

/// Space: `win.toggle-play-pause`, dispatched by a focus-sensitive capture
/// controller. See the module doc's `## Space` bullet.
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
    action.connect_activate(move |_, _| match &player {
        Some(player) => player.toggle_pause(),
        None => tracing::warn!("space: player unavailable; ignoring play/pause toggle"),
    });
    window.add_action(&action);
    app.set_accels_for_action(&format!("win.{ACTION_TOGGLE_PLAY_PAUSE}"), &[]);

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    let window_weak = window.downgrade();
    key_controller.connect_key_pressed(move |_, key, _, modifiers| {
        let shortcut_modifiers = gtk4::gdk::ModifierType::CONTROL_MASK
            | gtk4::gdk::ModifierType::ALT_MASK
            | gtk4::gdk::ModifierType::SHIFT_MASK
            | gtk4::gdk::ModifierType::SUPER_MASK
            | gtk4::gdk::ModifierType::META_MASK;
        if key != gtk4::gdk::Key::space || modifiers.intersects(shortcut_modifiers) {
            return gtk4::glib::Propagation::Proceed;
        }
        let Some(window) = window_weak.upgrade() else {
            return gtk4::glib::Propagation::Proceed;
        };
        let focus = gtk4::prelude::GtkWindowExt::focus(&window);
        if !space_should_toggle(focused_widget_owns_space(focus.as_ref())) {
            return gtk4::glib::Propagation::Proceed;
        }
        if let Err(error) = gtk4::prelude::WidgetExt::activate_action(
            &window,
            &format!("win.{ACTION_TOGGLE_PLAY_PAUSE}"),
            None,
        ) {
            tracing::warn!(%error, "space: could not activate play/pause action");
            return gtk4::glib::Propagation::Proceed;
        }
        gtk4::glib::Propagation::Stop
    });
    window.add_controller(key_controller);
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
    let pending_focus = Rc::new(std::cell::Cell::new(false));
    let pending_focus_on_map = pending_focus.clone();
    let window_on_map = window.downgrade();
    search_entry.connect_map(move |entry| {
        if pending_focus_on_map.replace(false) {
            if let Some(window) = window_on_map.upgrade() {
                focus_search_entry(&window, entry);
            }
        }
    });

    let action = gio::SimpleAction::new(ACTION_FOCUS_SEARCH, None);
    let search_bar_weak = search_bar.downgrade();
    let search_entry_weak = search_entry.downgrade();
    let window_weak = window.downgrade();
    let pending_focus_on_activate = pending_focus.clone();
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
        pending_focus_on_activate.set(search_mode);
        search_bar.set_search_mode(search_mode);
        if search_mode && search_entry.is_mapped() && pending_focus_on_activate.replace(false) {
            if let Some(window) = window_weak.upgrade() {
                focus_search_entry(&window, &search_entry);
            }
        }
    });
    window.add_action(&action);
    app.set_accels_for_action(&format!("win.{ACTION_FOCUS_SEARCH}"), &["<Control>f"]);
}

fn focus_search_entry(window: &adw::ApplicationWindow, entry: &gtk4::SearchEntry) {
    gtk4::prelude::GtkWindowExt::set_focus(window, Some(entry));
    entry.select_region(0, -1);
}

#[cfg(test)]
fn widget_contains_focus(window: &adw::ApplicationWindow, widget: &gtk4::Widget) -> bool {
    gtk4::prelude::GtkWindowExt::focus(window)
        .is_some_and(|focus| focus == *widget || focus.is_ancestor(widget))
}

fn apply_search_escape(
    search_bar: &gtk4::SearchBar,
    search_entry: &gtk4::SearchEntry,
    focus_active_content: &Rc<dyn Fn() -> bool>,
) {
    match escape_action_for(!search_entry.text().is_empty()) {
        EscapeAction::ClearSearchText => {
            search_entry.set_text("");
            search_bar.set_search_mode(true);
            search_entry.grab_focus();
        }
        EscapeAction::CollapseSearchBarAndFocusActiveContent => {
            // Collapsing synchronously here does not stick. `GtkSearchBar` has
            // its own key-capture controller on the window (installed by
            // `set_key_capture_widget`), which forwards the key to the entry
            // connected via `connect_entry`. The entry consumes Escape, the
            // search bar reads that as typing and re-asserts search mode —
            // after our handler has returned. Returning `Propagation::Stop`
            // does not help: that controller sits on the window, a different
            // dispatch step from ours on the bar.
            //
            // Deferring to idle puts the collapse after that re-assertion.
            // Moving our controller to the window would also work but would
            // steal Escape from a dialog opened while search mode is on, which
            // is the scoping this module deliberately keeps (see the module
            // doc) so ACC-5's "Esc closes exactly this layer" holds.
            let bar = search_bar.downgrade();
            let focus_active_content = Rc::clone(focus_active_content);
            gtk4::glib::idle_add_local_once(move || {
                let Some(bar) = bar.upgrade() else {
                    return;
                };
                bar.set_search_mode(false);
                if !focus_active_content() {
                    tracing::warn!("escape: could not move focus to the active content view");
                }
            });
        }
    }
}

fn wire_escape(
    search_bar: &gtk4::SearchBar,
    search_entry: &gtk4::SearchEntry,
    focus_active_content: Rc<dyn Fn() -> bool>,
) {
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
        apply_search_escape(&bar, &entry, &focus_active_content);
        gtk4::glib::Propagation::Stop
    });
    search_bar.add_controller(controller);
}

fn wire_window_lifecycle(app: &adw::Application, window: &adw::ApplicationWindow) {
    let close = gio::SimpleAction::new("close", None);
    let window_weak = window.downgrade();
    close.connect_activate(move |_, _| {
        if let Some(window) = window_weak.upgrade() {
            window.close();
        }
    });
    window.add_action(&close);
    app.set_accels_for_action("win.close", &["<Control>w"]);

    let quit = gio::SimpleAction::new("quit", None);
    let app_weak = app.downgrade();
    quit.connect_activate(move |_, _| {
        if let Some(app) = app_weak.upgrade() {
            app.quit();
        }
    });
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<Control>q"]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use libadwaita::prelude::AdwApplicationWindowExt;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_2a_ctrl_f_reveals_and_focuses() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.SearchShortcutTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        let invoker = gtk4::Button::with_label("Open search");
        let entry = gtk4::SearchEntry::new();
        let search_bar = gtk4::SearchBar::new();
        search_bar.set_child(Some(&entry));
        search_bar.connect_entry(&entry);
        search_bar.set_key_capture_widget(Some(&window));
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.append(&invoker);
        content.append(&search_bar);
        window.set_content(Some(&content));
        wire_focus_search(&app, &window, &search_bar, &entry);
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        gtk4::prelude::GtkWindowExt::set_focus(&window, Some(&invoker));
        assert_eq!(
            gtk4::prelude::GtkWindowExt::focus(&window),
            Some(invoker.upcast())
        );

        ActionGroupExt::activate_action(&window, "focus-search", None);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while !widget_contains_focus(&window, entry.upcast_ref()) {
            assert!(
                std::time::Instant::now() < deadline,
                "search entry did not receive focus after reveal"
            );
            gtk4::glib::MainContext::default().iteration(true);
        }

        assert!(search_bar.is_search_mode());
        assert!(widget_contains_focus(&window, entry.upcast_ref()));
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

        let focus_calls = Rc::new(std::cell::Cell::new(0));
        let counted = Rc::clone(&focus_calls);
        let focus_active_content: Rc<dyn Fn() -> bool> = Rc::new(move || {
            counted.set(counted.get() + 1);
            true
        });

        apply_search_escape(&search_bar, &entry, &focus_active_content);
        assert_eq!(entry.text(), "");
        assert!(search_bar.is_search_mode());
        assert_eq!(focus_calls.get(), 0);

        apply_search_escape(&search_bar, &entry, &focus_active_content);
        // Stage two is deferred to idle so `GtkSearchBar` cannot re-assert
        // search mode behind it, so the effect lands only once the loop runs.
        assert!(search_bar.is_search_mode());
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(!search_bar.is_search_mode());
        assert_eq!(focus_calls.get(), 1);
    }

    #[test]
    fn search_6_ctrl_f_toggles_open_and_closed() {
        assert!(next_search_mode(false));
        assert!(!next_search_mode(true));
    }

    #[test]
    fn nav_6_escape_changes_search_state_without_navigation() {
        assert_eq!(escape_action_for(true), EscapeAction::ClearSearchText);
        assert_eq!(
            escape_action_for(false),
            EscapeAction::CollapseSearchBarAndFocusActiveContent
        );
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
    #[ignore = "requires a display; run via xvfb-run"]
    fn local_controls_keep_space_instead_of_toggling_playback() {
        gtk4::init().unwrap();
        let local_controls: Vec<gtk4::Widget> = vec![
            gtk4::SearchEntry::new().upcast(),
            gtk4::Button::new().upcast(),
            gtk4::CheckButton::new().upcast(),
            gtk4::Switch::new().upcast(),
            gtk4::Scale::with_range(gtk4::Orientation::Horizontal, 0.0, 1.0, 0.1).upcast(),
            gtk4::ListBox::new().upcast(),
            gtk4::GridView::new(None::<gtk4::SelectionModel>, None::<gtk4::ListItemFactory>)
                .upcast(),
            gtk4::ColumnView::new(None::<gtk4::SelectionModel>).upcast(),
        ];
        for control in local_controls {
            assert!(
                focused_widget_owns_space(Some(&control)),
                "{} must keep its native Space behavior",
                control.type_().name()
            );
        }
        let passive = gtk4::Label::new(Some("Passive")).upcast::<gtk4::Widget>();
        assert!(!focused_widget_owns_space(Some(&passive)));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_4_space_uses_capture_controller_so_local_controls_keep_the_key() {
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.ShortcutTest")
            .build();
        let window = adw::ApplicationWindow::new(&app);

        wire_toggle_play_pause(&app, &window, None);

        assert!(
            app.accels_for_action(&format!("win.{ACTION_TOGGLE_PLAY_PAUSE}"))
                .is_empty(),
            "a global accelerator consumes Space before the focused control"
        );
        let controllers = window.observe_controllers();
        let has_capture_key_controller = (0..controllers.n_items()).any(|index| {
            controllers
                .item(index)
                .and_then(|item| item.downcast::<gtk4::EventControllerKey>().ok())
                .is_some_and(|controller| {
                    controller.propagation_phase() == gtk4::PropagationPhase::Capture
                })
        });
        assert!(has_capture_key_controller);
    }

    #[test]
    fn escape_clears_text_when_search_has_text() {
        assert_eq!(escape_action_for(true), EscapeAction::ClearSearchText);
    }

    #[test]
    fn escape_collapses_search_bar_and_focuses_active_content_when_empty() {
        assert_eq!(
            escape_action_for(false),
            EscapeAction::CollapseSearchBarAndFocusActiveContent
        );
    }
}
