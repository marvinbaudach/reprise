//! Full-width library chrome from design mockup 7a.
use std::cell::RefCell;
use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;

use super::strings;

pub(in crate::ui) struct LibraryChrome {
    pub(in crate::ui) root: adw::ToolbarView,
    pub(in crate::ui) search_bar: gtk4::SearchBar,
    /// SEARCH-8 needs the lens outside tests too: a section without a list
    /// makes it insensitive and re-labels it.
    pub(in crate::ui) search_toggle: gtk4::ToggleButton,
}

pub(in crate::ui) struct LibraryMaintenanceActions {
    pub(in crate::ui) scan: gtk4::Button,
}

pub(in crate::ui) fn build(
    header: &adw::HeaderBar,
    content: &impl IsA<gtk4::Widget>,
    search_entry: &gtk4::SearchEntry,
    key_capture_widget: &impl IsA<gtk4::Widget>,
) -> LibraryChrome {
    header.add_css_class("reprise-library-header");
    let search_toggle = gtk4::ToggleButton::builder()
        .icon_name("system-search-symbolic")
        .tooltip_text(strings::shortcut_tooltip(
            strings::SEARCH_PLACEHOLDER,
            strings::SHORTCUT_SEARCH,
        ))
        .css_classes(["flat", "reprise-panel-toggle"])
        .build();
    search_toggle.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::SEARCH_PLACEHOLDER,
    ))]);
    header.pack_end(&search_toggle);

    let search_clamp = adw::Clamp::builder()
        .maximum_size(450)
        .child(search_entry)
        .build();
    let search_bar = gtk4::SearchBar::new();
    search_bar.set_hexpand(true);
    search_bar.add_css_class("reprise-search-strip");
    search_bar.set_child(Some(&search_clamp));
    search_bar.connect_entry(search_entry);
    search_bar.set_key_capture_widget(Some(key_capture_widget));
    wire_search_toggle(&search_toggle, &search_bar, search_entry);
    wire_search_focus_collapse(&search_bar, search_entry, key_capture_widget);

    let root = adw::ToolbarView::new();
    root.set_top_bar_style(adw::ToolbarStyle::Flat);
    root.add_top_bar(header);
    root.add_top_bar(&search_bar);
    root.set_content(Some(content));
    LibraryChrome {
        root,
        search_bar,
        search_toggle,
    }
}

pub(in crate::ui) fn build_navigation_back_button() -> gtk4::Button {
    let label = strings::text(strings::NAVIGATE_BACK);
    let button = gtk4::Button::builder()
        .icon_name("go-previous-symbolic")
        .tooltip_text(&label)
        .action_name("win.nav-back")
        .css_classes(["flat", "reprise-panel-toggle"])
        .build();
    button.update_property(&[gtk4::accessible::Property::Label(&label)]);
    button
}

pub(in crate::ui) fn search_toggle_active(search_mode: bool, query: &str) -> bool {
    search_mode || !query.trim().is_empty()
}

fn should_collapse_search_after_focus_change(
    search_mode: bool,
    entry_has_focus: bool,
    pointer_button_held: bool,
) -> bool {
    // `pointer_button_held` is the whole reason this is not just
    // `search_mode && !entry_has_focus`. A press moves focus out of the entry
    // *before* the release that completes the click. Collapsing in that gap
    // removes a whole top bar, so everything below it — the filter row with
    // the search chip, "Clear all" and the facet pills — jumps up by the
    // strip's height, the release lands on whatever moved into its place, and
    // GTK never emits `clicked`. What the user saw was the strip vanishing on
    // the first click and the chip needing a second one. So: never collapse
    // mid-click; the release hook below finishes the job.
    search_mode && !entry_has_focus && !pointer_button_held
}

fn wire_search_focus_collapse(
    search_bar: &gtk4::SearchBar,
    search_entry: &gtk4::SearchEntry,
    pointer_root: &impl IsA<gtk4::Widget>,
) {
    let held = Rc::new(std::cell::Cell::new(false));
    let released = wire_search_focus_collapse_with(search_bar, search_entry, &held);

    // Why an event controller and not `GdkDevice::modifier_state`: on X11
    // that state is only refreshed from events the toolkit itself received,
    // and it read "no button down" throughout a real held click — measured,
    // not assumed. The button events are the authority.
    //
    // Legacy (not `GtkGestureClick`) and capture phase: this watcher must
    // observe every press and release anywhere in the window without ever
    // claiming the sequence, or it would eat the very clicks it exists to
    // protect.
    let controller = gtk4::EventControllerLegacy::new();
    controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
    controller.connect_event(move |_, event| {
        match event.event_type() {
            gtk4::gdk::EventType::ButtonPress => held.set(true),
            gtk4::gdk::EventType::ButtonRelease => {
                held.set(false);
                released();
            }
            _ => {}
        }
        gtk4::glib::Propagation::Proceed
    });
    pointer_root.as_ref().add_controller(controller);
}

/// The wiring above with the pointer-button state handed in, so a test can
/// hold a button down without a device. Returns the hook to call when the
/// pointer is released: a collapse that was postponed mid-click runs then.
fn wire_search_focus_collapse_with(
    search_bar: &gtk4::SearchBar,
    search_entry: &gtk4::SearchEntry,
    held: &Rc<std::cell::Cell<bool>>,
) -> impl Fn() + 'static {
    let focus = gtk4::EventControllerFocus::new();
    let postponed = Rc::new(std::cell::Cell::new(false));

    // One collapse, deferred to an idle by both callers. Pointer activation
    // transfers focus before emitting `clicked`, so running inside the event
    // would let the search toggle read the blur-driven collapse as a request
    // to reopen the bar. When the idle finds a button still down it does not
    // collapse; it records that one is owed, and the release hook re-runs it.
    let collapse = {
        let bar = search_bar.downgrade();
        let focus = focus.downgrade();
        let held = held.clone();
        let postponed = postponed.clone();
        Rc::new(move || {
            let (bar, focus, held, postponed) =
                (bar.clone(), focus.clone(), held.clone(), postponed.clone());
            gtk4::glib::idle_add_local_once(move || {
                let (Some(bar), Some(focus)) = (bar.upgrade(), focus.upgrade()) else {
                    return;
                };
                if !bar.is_search_mode() || focus.contains_focus() {
                    postponed.set(false);
                    return;
                }
                if should_collapse_search_after_focus_change(
                    bar.is_search_mode(),
                    focus.contains_focus(),
                    held.get(),
                ) {
                    bar.set_search_mode(false);
                    postponed.set(false);
                    return;
                }
                // Mid-click: the release hook owns this collapse now.
                postponed.set(true);
            });
        })
    };

    let collapse_on_blur = collapse.clone();
    focus.connect_contains_focus_notify(move |_| {
        collapse_on_blur();
    });
    search_entry.add_controller(focus);

    move || {
        if postponed.replace(false) {
            collapse();
        }
    }
}

fn update_preserved_query(search_mode: bool, query: &str, preserved_query: &mut String) {
    if search_mode {
        *preserved_query = query.to_string();
    }
}

fn wire_search_toggle(
    toggle: &gtk4::ToggleButton,
    search_bar: &gtk4::SearchBar,
    search_entry: &gtk4::SearchEntry,
) {
    let bar = search_bar.downgrade();
    let entry = search_entry.downgrade();
    toggle.connect_clicked(move |toggle| {
        let (Some(bar), Some(entry)) = (bar.upgrade(), entry.upgrade()) else {
            return;
        };
        bar.set_search_mode(crate::ui::shortcuts::next_search_mode(bar.is_search_mode()));
        toggle.set_active(search_toggle_active(bar.is_search_mode(), &entry.text()));
    });

    let toggle_weak = toggle.downgrade();
    let entry = search_entry.downgrade();
    // GtkSearchBar clears its connected entry when search mode ends. SEARCH-6
    // forbids that: hiding the bar must never drop the query — it lives on as
    // a chip and the lens stays checked. Restore the text the bar just wiped.
    let preserved_query = Rc::new(RefCell::new(String::new()));
    let stash = preserved_query.clone();
    search_bar.connect_search_mode_enabled_notify(move |bar| {
        let (Some(toggle), Some(entry)) = (toggle_weak.upgrade(), entry.upgrade()) else {
            return;
        };
        if bar.is_search_mode() {
            stash.borrow_mut().clear();
        } else {
            let restored = stash.borrow().clone();
            if !restored.is_empty() && entry.text().is_empty() {
                entry.set_text(&restored);
            }
        }
        toggle.set_active(search_toggle_active(bar.is_search_mode(), &entry.text()));
    });

    let toggle_weak = toggle.downgrade();
    let bar = search_bar.downgrade();
    // `connect_changed`, not `connect_search_changed`: the latter is debounced
    // so the query can settle before re-running it, but the lens only reflects
    // "a query exists" (SEARCH-3) and must not lag behind typing by ~150 ms.
    let stash = preserved_query.clone();
    search_entry.connect_changed(move |entry| {
        let (Some(toggle), Some(bar)) = (toggle_weak.upgrade(), bar.upgrade()) else {
            return;
        };
        let query = entry.text();
        // While the bar is open the stash tracks the entry verbatim, empty
        // included. Only assigning on non-empty left it stale after an
        // explicit clear, and the collapse below then resurrected a query the
        // user had removed — violating SEARCH-5, which preserves the query
        // only *until* Esc, the chip's X or "Clear all" removes it. All three
        // funnel through `set_text("")` while the bar is open, so clearing
        // here covers them in one place.
        //
        // `is_search_mode()` is what separates the two kinds of empty entry: a
        // user-initiated clear arrives while the bar is still open, whereas
        // GtkSearchBar's own wipe is a consequence of search mode having been
        // turned off and so cannot reach this branch — which is what makes
        // SEARCH-6 survive.
        update_preserved_query(bar.is_search_mode(), &query, &mut stash.borrow_mut());
        toggle.set_active(search_toggle_active(bar.is_search_mode(), &query));
    });
}

pub(in crate::ui) fn action_button(icon_name: &str, label: &str) -> gtk4::Button {
    let button = gtk4::Button::builder()
        .icon_name(icon_name)
        .tooltip_text(label)
        .build();
    button.update_property(&[gtk4::accessible::Property::Label(label)]);
    button
}

pub(in crate::ui) fn build_maintenance_actions() -> LibraryMaintenanceActions {
    let scan = action_button("folder-open-symbolic", &strings::text(strings::SCAN_FOLDER));
    LibraryMaintenanceActions { scan }
}

pub(in crate::ui) fn css() -> String {
    ".reprise-library-split .reprise-library-sidebar { \
       background-color: @sidebar_bg_color; \
       border-right: 1px solid rgba(255, 255, 255, 0.06); }\n\
     .reprise-library-header { \
       background-color: @headerbar_bg_color; \
       border-bottom: 1px solid rgba(255, 255, 255, 0.06); }\n\
     .reprise-search-strip { \
       background-color: @headerbar_bg_color; \
       border-bottom: 1px solid rgba(255, 255, 255, 0.06); }\n\
     .reprise-library-sidebar .caption-heading { \
       color: @reprise_secondary_fg_color; }"
        .to_string()
}

#[cfg(test)]
#[path = "library_chrome_tests.rs"]
mod tests;

#[cfg(test)]
mod style_guard {
    /// UX STYLE-1: every chrome surface that should read as its own plane
    /// declares a background and a bottom edge explicitly.
    #[test]
    fn style_1_chrome_surfaces_declare_background_and_edge() {
        let css = super::css();

        for class in [".reprise-library-header", ".reprise-search-strip"] {
            let block = css
                .split(class)
                .nth(1)
                .unwrap_or_else(|| panic!("{class} has no rule in the chrome CSS"));
            let block = block.split('}').next().unwrap_or_default();
            assert!(
                block.contains("background-color:"),
                "{class} inherits its background"
            );
            assert!(
                block.contains("border-bottom:"),
                "{class} has no bottom edge against the content"
            );
        }
    }
}
