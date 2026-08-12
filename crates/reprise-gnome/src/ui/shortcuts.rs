//! Application and window keyboard shortcuts. Space toggles playback only
//! when the focused widget does not own Space itself. The search popover owns
//! Escape and Enter locally; a closed search chip gives unhandled Escape one
//! window-level clear action; Ctrl+F toggles the popover.
//!
//! ## Shortcut wiring mechanisms
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
//!   opening focuses the existing query with its caret at the end, while
//!   invoking it again closes the popover without clearing that query.
//!
//! - **Space** uses a capture-phase `EventControllerKey`, not an application
//!   accelerator. The controller inspects the actual focused widget before
//!   dispatch: local controls continue through GTK's native key path, while
//!   passive content activates `win.toggle-play-pause`. A global accelerator
//!   cannot provide that contract because it consumes the key before a
//!   no-op action handler can return it to the focused control.
//!
//! - **Escape and Enter** are capture-phase entry controls installed by
//!   `SearchPopover`. Enter commits and closes; Escape clears through the
//!   active section and closes. A bubble-phase window controller clears a
//!   committed chip only after local dialogs, popovers, menus and views had
//!   their chance to consume Escape.
//!
//! ## What's verified headlessly vs. manually
//!
//! The predicates below (`space_should_toggle` and
//! `focused_widget_owns_space`'s boolean-in shape) are pure functions,
//! unit-tested with no display. The action *callbacks* that wrap them are
//! exercised by calling them directly in tests too (see this module's
//! `#[cfg(test)]`). The isolated CUA suite additionally delivers real key
//! events to passive views, direct controls, text entry, and the selected
//! Now Playing view tab.

use gtk4::gio;
use gtk4::gio::prelude::*;
use gtk4::prelude::*;
use libadwaita as adw;
use std::rc::Rc;

use super::player_controller::PlayerController;
use super::window::search_popover::{SearchPopover, WeakSearchPopover};

/// Bare `gio::SimpleAction` names in the window's `"win"` action group —
/// internal identifiers, not user-facing text.
const ACTION_TOGGLE_PLAY_PAUSE: &str = "toggle-play-pause";
const ACTION_FOCUS_SEARCH: &str = "focus-search";
pub(in crate::ui) const SIDEBAR_TOGGLE_CSS_CLASS: &str = "reprise-sidebar-toggle";

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

/// Whether `focus` (the window's currently focused widget, if any) owns an
/// unmodified Space press. Text entry and direct controls keep their native
/// activation semantics. Passive collection views deliberately do not: ACC-4
/// reserves Space there for global play/pause.
fn local_control_owns_space(
    is_selected_view_tab: bool,
    is_sidebar_toggle: bool,
    is_local_control: bool,
) -> bool {
    !is_selected_view_tab && !is_sidebar_toggle && is_local_control
}

fn focused_widget_owns_space(focus: Option<&gtk4::Widget>) -> bool {
    focus.is_some_and(|widget| {
        let is_selected_view_tab = widget
            .downcast_ref::<gtk4::ToggleButton>()
            .is_some_and(gtk4::ToggleButton::is_active)
            && widget
                .ancestor(adw::InlineViewSwitcher::static_type())
                .is_some();
        let is_sidebar_toggle = widget.has_css_class(SIDEBAR_TOGGLE_CSS_CLASS);
        let is_local_control = widget.is::<gtk4::Editable>()
            || widget.is::<gtk4::Button>()
            || widget.is::<gtk4::CheckButton>()
            || widget.is::<gtk4::Switch>()
            || widget.is::<gtk4::Range>()
            || widget.is::<gtk4::DropDown>();
        local_control_owns_space(is_selected_view_tab, is_sidebar_toggle, is_local_control)
    })
}

/// Wires the window shortcuts. Called once from `window::build`, after
/// `window`, `search`, and `player` all exist. `player`
/// is `None` when GStreamer was unavailable at startup (see `window::
/// build`'s own doc comment on that degradation) — the Space action still
/// registers in that case (so the accelerator itself is harmless to press),
/// it just logs and no-ops instead of toggling anything.
pub(in crate::ui) fn wire(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    search: &SearchPopover,
    hooks: ShortcutHooks,
    player: Option<Rc<PlayerController>>,
) {
    let ShortcutHooks {
        focus_active_content,
        search_available,
        clear_active_search,
    } = hooks;
    wire_toggle_play_pause(app, window, player);
    wire_window_lifecycle(app, window);
    search.set_focus_on_close(focus_active_content);
    wire_focus_search(app, window, search.downgrade(), search_available);
    wire_search_escape(window, clear_active_search);
}

/// The shell decisions the shortcuts have to ask about rather than make:
/// where a search close hands focus and — SEARCH-8a —
/// whether the visible section has a list for Ctrl+F to filter at all.
pub struct ShortcutHooks {
    pub focus_active_content: Rc<dyn Fn() -> bool>,
    pub search_available: Rc<dyn Fn() -> bool>,
    pub clear_active_search: Rc<dyn Fn() -> bool>,
}

fn wire_search_escape(window: &adw::ApplicationWindow, clear_active_search: Rc<dyn Fn() -> bool>) {
    let keys = gtk4::EventControllerKey::new();
    keys.set_propagation_phase(gtk4::PropagationPhase::Bubble);
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        handle_search_escape(key, modifiers, clear_active_search.as_ref())
    });
    window.add_controller(keys);
}

fn handle_search_escape(
    key: gtk4::gdk::Key,
    modifiers: gtk4::gdk::ModifierType,
    clear_active_search: &dyn Fn() -> bool,
) -> gtk4::glib::Propagation {
    if key != gtk4::gdk::Key::Escape || !modifiers.is_empty() {
        return gtk4::glib::Propagation::Proceed;
    }
    if clear_active_search() {
        gtk4::glib::Propagation::Stop
    } else {
        gtk4::glib::Propagation::Proceed
    }
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
        let focus_owns_space = focused_widget_owns_space(focus.as_ref());
        if !space_should_toggle(focus_owns_space) {
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
/// search popover. Opening grabs keyboard focus with the caret at the end;
/// closing preserves the query so the active filter remains visible as a chip.
/// The weak handle avoids an action → popover → window ownership cycle.
fn wire_focus_search(
    app: &adw::Application,
    window: &adw::ApplicationWindow,
    search: WeakSearchPopover,
    search_available: Rc<dyn Fn() -> bool>,
) {
    let action = gio::SimpleAction::new(ACTION_FOCUS_SEARCH, None);
    action.connect_activate(move |_, _| {
        // SEARCH-8a: where there is no list, Ctrl+F is a no-op — the same
        // truth the insensitive lens tells, said in the keyboard's language.
        if !search_available() {
            tracing::debug!("focus-search: the visible section has nothing to filter");
            return;
        }
        if next_search_mode(search.is_open()) {
            search.open();
        } else {
            search.close();
        }
    });
    window.add_action(&action);
    app.set_accels_for_action(&format!("win.{ACTION_FOCUS_SEARCH}"), &["<Control>f"]);
}

#[cfg(test)]
fn widget_contains_focus(window: &adw::ApplicationWindow, widget: &gtk4::Widget) -> bool {
    gtk4::prelude::GtkWindowExt::focus(window)
        .is_some_and(|focus| focus == *widget || focus.is_ancestor(widget))
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
    fn search_2c_ctrl_f_opens_and_focuses() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.SearchShortcutTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        let invoker = gtk4::Button::with_label("Open search");
        let entry = gtk4::SearchEntry::new();
        let lens = gtk4::ToggleButton::new();
        let search = SearchPopover::new(&lens, &entry);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.append(&invoker);
        content.append(&lens);
        window.set_content(Some(&content));
        wire_focus_search(&app, &window, search.downgrade(), Rc::new(|| true));
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

        assert!(search.is_open());
        assert!(widget_contains_focus(&window, entry.upcast_ref()));
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_4a_escape_closes_and_discards_the_query() {
        gtk4::init().unwrap();
        let lens = gtk4::ToggleButton::new();
        let entry = gtk4::SearchEntry::new();
        let search = SearchPopover::new(&lens, &entry);
        let window = gtk4::Window::new();
        window.set_child(Some(&lens));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        search.open();
        entry.set_text("falling");

        let focus_calls = Rc::new(std::cell::Cell::new(0));
        let counted = Rc::clone(&focus_calls);
        let focus_active_content: Rc<dyn Fn() -> bool> = Rc::new(move || {
            counted.set(counted.get() + 1);
            true
        });
        search.set_focus_on_close(focus_active_content);

        assert_eq!(
            search.press_close_key(gtk4::gdk::Key::Escape),
            gtk4::glib::Propagation::Stop
        );
        assert!(!search.is_open());
        assert!(entry.text().is_empty());
        assert_eq!(focus_calls.get(), 1);
        window.close();
    }

    #[test]
    fn search_4a_closed_search_escape_consumes_only_an_active_query() {
        let calls = std::cell::Cell::new(0);
        assert_eq!(
            handle_search_escape(
                gtk4::gdk::Key::Escape,
                gtk4::gdk::ModifierType::empty(),
                &|| {
                    calls.set(calls.get() + 1);
                    true
                },
            ),
            gtk4::glib::Propagation::Stop
        );
        assert_eq!(calls.get(), 1);

        assert_eq!(
            handle_search_escape(
                gtk4::gdk::Key::Escape,
                gtk4::gdk::ModifierType::empty(),
                &|| false,
            ),
            gtk4::glib::Propagation::Proceed,
            "Escape without a query or pill stays available to navigation"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_4a_window_escape_waits_for_local_escape_owners() {
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.SearchEscapePhaseTest")
            .flags(gio::ApplicationFlags::NON_UNIQUE)
            .build();
        let window = adw::ApplicationWindow::new(&app);
        wire_search_escape(&window, Rc::new(|| false));

        let phases: Vec<_> = window
            .observe_controllers()
            .into_iter()
            .flatten()
            .filter_map(|controller| controller.downcast::<gtk4::EventControllerKey>().ok())
            .map(|controller| controller.propagation_phase())
            .collect();
        assert!(phases.contains(&gtk4::PropagationPhase::Bubble));
        assert!(!phases.contains(&gtk4::PropagationPhase::Capture));
    }

    #[test]
    fn search_6_ctrl_f_toggles_open_and_closed() {
        assert!(next_search_mode(false));
        assert!(!next_search_mode(true));
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
    fn acc_4a_sidebar_toggle_never_owns_space_decision() {
        for _ in 0..6 {
            assert!(
                !local_control_owns_space(false, true, true),
                "Space must remain global play/pause while the sidebar toggle has focus"
            );
        }
        assert!(
            local_control_owns_space(false, false, true),
            "other focused buttons keep their native Space action"
        );
        assert!(
            !local_control_owns_space(true, false, true),
            "an already selected passive view tab leaves Space global"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_4a_sidebar_toggle_never_owns_space() {
        gtk4::init().unwrap();
        let sidebar_toggle = gtk4::ToggleButton::new();
        sidebar_toggle.add_css_class(SIDEBAR_TOGGLE_CSS_CLASS);

        for _ in 0..6 {
            assert!(
                !focused_widget_owns_space(Some(sidebar_toggle.upcast_ref())),
                "Space must remain global play/pause while the sidebar toggle has focus"
            );
        }
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
    fn selected_view_tab_leaves_space_to_global_playback() {
        fn collect_toggles(root: &gtk4::Widget, toggles: &mut Vec<gtk4::ToggleButton>) {
            let mut child = root.first_child();
            while let Some(widget) = child {
                if let Some(toggle) = widget.downcast_ref::<gtk4::ToggleButton>() {
                    toggles.push(toggle.clone());
                }
                collect_toggles(&widget, toggles);
                child = widget.next_sibling();
            }
        }

        gtk4::init().unwrap();
        let switcher = adw::InlineViewSwitcher::new();
        let stack = adw::ViewStack::new();
        stack.add_titled(&gtk4::Box::default(), Some("queue"), "Queue");
        stack.add_titled(&gtk4::Box::default(), Some("visual"), "Visual");
        switcher.set_stack(Some(&stack));
        let window = gtk4::Window::new();
        window.set_child(Some(&switcher));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        let mut tabs = Vec::new();
        collect_toggles(switcher.upcast_ref(), &mut tabs);
        let selected = tabs
            .iter()
            .find(|tab| tab.is_active())
            .expect("view switcher has one selected tab");
        let inactive = tabs
            .iter()
            .find(|tab| !tab.is_active())
            .expect("view switcher has one inactive tab");

        assert!(
            !focused_widget_owns_space(Some(selected.upcast_ref())),
            "an already selected view tab has no useful local Space action"
        );
        assert!(
            focused_widget_owns_space(Some(inactive.upcast_ref())),
            "an inactive view tab keeps Space so it can activate locally"
        );
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn space_remains_global_from_passive_collection_views() {
        gtk4::init().unwrap();
        let collection_views: Vec<gtk4::Widget> = vec![
            gtk4::ListBox::new().upcast(),
            gtk4::ListView::new(None::<gtk4::SelectionModel>, None::<gtk4::ListItemFactory>)
                .upcast(),
            gtk4::GridView::new(None::<gtk4::SelectionModel>, None::<gtk4::ListItemFactory>)
                .upcast(),
            gtk4::ColumnView::new(None::<gtk4::SelectionModel>).upcast(),
        ];

        for view in collection_views {
            assert!(
                !focused_widget_owns_space(Some(&view)),
                "{} must leave Space to global play/pause",
                view.type_().name()
            );
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn acc_4a_space_uses_capture_controller_so_local_controls_keep_the_key() {
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.ShortcutTest")
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
}
