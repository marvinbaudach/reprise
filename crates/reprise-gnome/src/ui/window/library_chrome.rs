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
mod tests {
    use libadwaita::prelude::*;

    use super::*;

    fn test_content() -> gtk4::Label {
        gtk4::Label::new(Some("Library"))
    }

    /// The deferred collapse is a timer, not an idle: pumping a drained main
    /// context proves nothing about it, so these helpers pump against the
    /// clock instead.
    fn pump_for(duration: std::time::Duration) {
        let deadline = std::time::Instant::now() + duration;
        while std::time::Instant::now() < deadline {
            while gtk4::glib::MainContext::default().iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    fn settle_until(label: &str, condition: impl Fn() -> bool) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !condition() {
            while gtk4::glib::MainContext::default().iteration(false) {}
            assert!(std::time::Instant::now() < deadline, "timed out: {label}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_1_idle_is_icon_not_field() {
        gtk4::init().unwrap();
        let window = adw::ApplicationWindow::builder().build();
        let header = adw::HeaderBar::new();
        let entry = gtk4::SearchEntry::new();

        let chrome = build(&header, &test_content(), &entry, &window);

        assert!(!entry.is_ancestor(&header));
        assert!(chrome.search_toggle.is_ancestor(&header));
        assert_eq!(
            chrome.search_toggle.icon_name().as_deref(),
            Some("system-search-symbolic")
        );
        assert!(!chrome.search_bar.is_search_mode());
        let clamp = chrome
            .search_bar
            .child()
            .and_downcast::<adw::Clamp>()
            .expect("search entry must be wrapped by a clamp");
        assert!(entry.is_ancestor(&clamp));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_2b_bar_reveals_flush_under_headerbar() {
        gtk4::init().unwrap();
        let window = adw::ApplicationWindow::builder().build();
        let header = adw::HeaderBar::new();
        let entry = gtk4::SearchEntry::new();

        let content = test_content();
        let chrome = build(&header, &content, &entry, &window);
        let clamp = chrome
            .search_bar
            .child()
            .and_downcast::<adw::Clamp>()
            .expect("search strip child must be the width clamp");

        assert!(header.is_ancestor(&chrome.root));
        assert!(chrome.search_bar.is_ancestor(&chrome.root));
        assert_eq!(chrome.root.content().as_ref(), Some(content.upcast_ref()));
        assert!(entry.is_ancestor(&clamp));
        assert_eq!(clamp.maximum_size(), 450);
        assert!(chrome.search_bar.hexpands());
        assert!(chrome.search_bar.has_css_class("reprise-search-strip"));

        chrome.search_bar.set_search_mode(true);
        assert!(chrome.search_bar.is_search_mode());

        let css = css();
        let strip_css = css
            .split(".reprise-search-strip")
            .nth(1)
            .and_then(|rule| rule.split('}').next())
            .expect("search strip CSS rule");
        assert!(strip_css.contains("background-color: @headerbar_bg_color"));
        assert!(strip_css.contains("border-bottom: 1px solid"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_3_lens_checked_when_active() {
        gtk4::init().unwrap();
        let window = adw::ApplicationWindow::builder().build();
        let header = adw::HeaderBar::new();
        let entry = gtk4::SearchEntry::new();
        let chrome = build(&header, &test_content(), &entry, &window);

        assert!(!chrome.search_toggle.is_active());
        chrome.search_bar.set_search_mode(true);
        assert!(chrome.search_toggle.is_active());

        chrome.search_bar.set_search_mode(false);
        assert!(!chrome.search_toggle.is_active());
        entry.set_text("falling");

        assert!(search_toggle_active(
            chrome.search_bar.is_search_mode(),
            &entry.text()
        ));
        assert!(chrome.search_toggle.is_active());
        assert_eq!(
            crate::ui::browse::browse_bar_chips::chip_labels(
                "falling",
                &reprise_core::queries::BrowseFilter::default(),
                true,
            ),
            vec!["⌕ “falling” in any field"]
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_6_hidden_query_survives_as_chip() {
        gtk4::init().unwrap();
        let window = adw::ApplicationWindow::builder().build();
        let header = adw::HeaderBar::new();
        let entry = gtk4::SearchEntry::new();
        let chrome = build(&header, &test_content(), &entry, &window);
        chrome.search_bar.set_search_mode(true);
        entry.set_text("falling");

        chrome.search_toggle.emit_clicked();

        let chips = crate::ui::browse::browse_bar_chips::chip_labels(
            &entry.text(),
            &reprise_core::queries::BrowseFilter::default(),
            true,
        );

        assert!(!chrome.search_bar.is_search_mode());
        assert_eq!(entry.text(), "falling");
        assert!(chrome.search_toggle.is_active());
        assert_eq!(chips, vec!["⌕ “falling” in any field"]);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_5_an_explicitly_cleared_query_does_not_come_back_on_collapse() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let window = adw::ApplicationWindow::builder().build();
        let header = adw::HeaderBar::new();
        let entry = gtk4::SearchEntry::new();
        let chrome = build(&header, &test_content(), &entry, &window);
        chrome.search_bar.set_search_mode(true);
        entry.set_text("nomatch");

        // Esc stage one: clear the text while the bar stays open. The chip's X
        // and "Clear all" reach this same state, so all three are covered.
        entry.set_text("");
        assert!(chrome.search_bar.is_search_mode());

        // Esc stage two: collapse. SEARCH-6 restores a *preserved* query here,
        // but SEARCH-5 preserves it only until the user explicitly removes it —
        // which just happened, so nothing may be restored.
        chrome.search_bar.set_search_mode(false);
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert_eq!(
            entry.text(),
            "",
            "an explicitly cleared query must not be resurrected by the collapse"
        );
        assert!(!chrome.search_toggle.is_active());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_6_wipe_on_collapse_arrives_after_search_mode_is_already_false() {
        // The SEARCH-5 fix above rests on this ordering: the stash may treat an
        // empty entry as an explicit clear *only* because GtkSearchBar's own
        // wipe cannot arrive while search mode is still true. If GTK ever
        // reordered that, the clear-on-empty branch would eat the stash and
        // break SEARCH-6 — so assert the premise rather than trust it.
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let window = adw::ApplicationWindow::builder().build();
        let header = adw::HeaderBar::new();
        let entry = gtk4::SearchEntry::new();
        let chrome = build(&header, &test_content(), &entry, &window);
        chrome.search_bar.set_search_mode(true);
        entry.set_text("falling");

        let mode_when_emptied = Rc::new(RefCell::new(None::<bool>));
        let observed = mode_when_emptied.clone();
        let bar = chrome.search_bar.clone();
        entry.connect_changed(move |entry| {
            if entry.text().is_empty() && observed.borrow().is_none() {
                *observed.borrow_mut() = Some(bar.is_search_mode());
            }
        });

        chrome.search_bar.set_search_mode(false);
        while gtk4::glib::MainContext::default().iteration(false) {}

        assert_eq!(
            *mode_when_emptied.borrow(),
            Some(false),
            "GtkSearchBar wiped the entry while still in search mode; the \
             stash can no longer infer an explicit clear from an empty entry"
        );
        // And SEARCH-6 still holds: the query survives the collapse.
        assert_eq!(entry.text(), "falling");
    }

    #[test]
    fn search_toggle_projects_open_mode_or_non_empty_query() {
        assert!(!search_toggle_active(false, ""));
        assert!(search_toggle_active(true, ""));
        assert!(search_toggle_active(false, "falling"));
        assert!(!search_toggle_active(false, "   "));
    }

    #[test]
    fn search_4_explicit_clear_discards_the_preserved_query() {
        let mut preserved_query = "nomatch".to_string();

        update_preserved_query(true, "", &mut preserved_query);

        assert_eq!(preserved_query, "");
    }

    #[test]
    fn search_6_collapse_clear_keeps_the_preserved_query() {
        let mut preserved_query = "falling".to_string();

        update_preserved_query(false, "", &mut preserved_query);

        assert_eq!(preserved_query, "falling");
    }

    #[test]
    fn search_7_focus_loss_collapses_only_an_open_search() {
        assert!(should_collapse_search_after_focus_change(
            true, false, false
        ));
        assert!(!should_collapse_search_after_focus_change(
            true, true, false
        ));
        assert!(!should_collapse_search_after_focus_change(
            false, false, false
        ));
    }

    // FIL-1a: the press that blurs the entry is the first half of a click on
    // something below the strip — most often the search chip's ×. Collapsing
    // between press and release moves that target out from under the pointer
    // and the click is lost, so a held button postpones the collapse.
    #[test]
    fn search_7_a_held_pointer_button_postpones_the_collapse() {
        assert!(!should_collapse_search_after_focus_change(
            true, false, true
        ));
        assert!(should_collapse_search_after_focus_change(
            true, false, false
        ));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_7_blur_collapses_the_bar_and_preserves_the_filter() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.SearchBlurTest")
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk4::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        let header = adw::HeaderBar::new();
        let entry = gtk4::SearchEntry::new();
        let content = gtk4::Button::with_label("Library content");
        let chrome = build(&header, &content, &entry, &window);
        window.set_content(Some(&chrome.root));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        chrome.search_bar.set_search_mode(true);
        entry.set_text("falling");
        entry.grab_focus();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(chrome.search_bar.is_search_mode());

        content.grab_focus();
        settle_until("the blurred search strip collapses", || {
            !chrome.search_bar.is_search_mode()
        });

        assert!(!chrome.search_bar.is_search_mode());
        assert_eq!(entry.text(), "falling");
        assert!(chrome.search_toggle.is_active());
    }

    // FIL-1a regression: the strip must not collapse while the click that
    // blurred the entry is still held. The real failure this covers is the
    // search chip needing two clicks — the first press collapsed the strip,
    // the filter row jumped up by its height, and the release missed the
    // chip. Proven at the pointer level by
    // `scripts/ptr-e2e/search-chip.sh`; proven here without a device by
    // handing the "button is down" answer in.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn search_7_a_held_click_keeps_the_strip_in_place_until_release() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let app = adw::Application::builder()
            .application_id("org.reprise.Reprise.SearchHeldClickTest")
            .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
            .build();
        app.register(None::<&gtk4::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        let header = adw::HeaderBar::new();
        let entry = gtk4::SearchEntry::new();
        let content = gtk4::Button::with_label("Filter chip");
        let search_bar = gtk4::SearchBar::new();
        search_bar.set_child(Some(&entry));
        search_bar.connect_entry(&entry);
        let held = Rc::new(std::cell::Cell::new(true));
        let released = wire_search_focus_collapse_with(&search_bar, &entry, &held);
        let root = adw::ToolbarView::new();
        root.add_top_bar(&header);
        root.add_top_bar(&search_bar);
        root.set_content(Some(&content));
        window.set_content(Some(&root));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        search_bar.set_search_mode(true);
        entry.set_text("falling");
        entry.grab_focus();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(search_bar.is_search_mode());

        // The press: focus moves to the chip, the button is still down.
        content.grab_focus();
        pump_for(std::time::Duration::from_millis(200));
        assert!(
            search_bar.is_search_mode(),
            "the strip must stay put while the click is still held, or the \
             release lands on whatever moved into the chip's place"
        );

        // The release.
        held.set(false);
        released();
        settle_until("the strip collapses once the button is up", || {
            !search_bar.is_search_mode()
        });
    }

    #[test]
    fn search_5_collapsing_keeps_query_and_chip() {
        let query = "falling";
        let chips = crate::ui::browse::browse_bar_chips::chip_labels(
            query,
            &reprise_core::queries::BrowseFilter::default(),
            true,
        );

        assert!(search_toggle_active(false, query));
        assert_eq!(chips, vec!["⌕ “falling” in any field"]);
    }

    #[test]
    fn chrome_separator_css_defines_scoped_hairlines() {
        let css = css();

        assert!(css.contains(".reprise-library-split .reprise-library-sidebar"));
        assert!(css.contains("border-right: 1px solid rgba(255, 255, 255, 0.06)"));
        assert!(css.contains(".reprise-library-header"));
        assert!(css.contains("background-color: @headerbar_bg_color"));
        assert!(css.contains("border-bottom: 1px solid"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn chrome_separator_css_parses() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&css());

        assert!(errors.is_empty(), "CSS parse errors: {errors:?}");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_keeps_navigation_left_without_a_center_title() {
        if gtk4::init().is_err() {
            return;
        }
        let header = adw::HeaderBar::new();
        let title = adw::WindowTitle::new("Music", "");
        let search = gtk4::SearchEntry::new();
        header.set_title_widget(Some(&title));
        let navigation = test_navigation();
        let window = adw::ApplicationWindow::builder().build();
        let chrome = build(&header, &navigation, &search, &window);

        assert_eq!(chrome.root.top_bar_style(), adw::ToolbarStyle::Flat);
        assert!(header.has_css_class("reprise-library-header"));
        assert_eq!(
            chrome.root.content().as_ref(),
            Some(navigation.upcast_ref())
        );
        assert!(header.is_ancestor(&chrome.root));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_actions_are_compact_icons_with_accessible_tooltips() {
        if gtk4::init().is_err() {
            return;
        }
        let button = action_button("folder-open-symbolic", "Scan folder…");

        assert_eq!(button.icon_name().as_deref(), Some("folder-open-symbolic"));
        assert_eq!(button.tooltip_text().as_deref(), Some("Scan folder…"));
        assert!(button.label().is_none());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn tip_1d_library_chrome_buttons_follow_tooltip_discipline() {
        if gtk4::init().is_err() {
            return;
        }
        let actions = build_maintenance_actions();

        let violations =
            crate::ui::tooltip_discipline::tooltip_violations(actions.scan.upcast_ref());
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn maintenance_actions_keep_the_scan_trigger_out_of_the_header() {
        if gtk4::init().is_err() {
            return;
        }
        let header = adw::HeaderBar::new();

        let actions = build_maintenance_actions();

        assert!(!actions.scan.is_ancestor(&header));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn header_stays_above_a_player_bar_shell() {
        if gtk4::init().is_err() {
            return;
        }
        let header = adw::HeaderBar::new();
        let navigation = test_navigation();
        let player = gtk4::ActionBar::new();
        let shell = super::super::library_player_bar::LibraryPlayerBarShell::new(
            &navigation,
            Some(player.upcast_ref()),
            reprise_core::library::settings::PlayerBarPosition::Top,
        );
        let window = adw::ApplicationWindow::builder().build();
        let search = gtk4::SearchEntry::new();
        let chrome = build(&header, shell.widget(), &search, &window);

        assert!(header.is_ancestor(&chrome.root));
        assert_eq!(
            chrome.root.content().as_ref(),
            Some(shell.widget().upcast_ref())
        );
        assert_eq!(
            shell.widget().last_child().as_ref(),
            Some(navigation.upcast_ref::<gtk4::Widget>())
        );
        assert!(player.is_ancestor(shell.widget()));
    }

    fn test_navigation() -> adw::NavigationSplitView {
        let sidebar = adw::NavigationPage::builder()
            .title("Sidebar")
            .child(&gtk4::Label::new(Some("Sidebar")))
            .build();
        let content = adw::NavigationPage::builder()
            .title("Library")
            .child(&gtk4::Label::new(Some("Library")))
            .build();
        adw::NavigationSplitView::builder()
            .sidebar(&sidebar)
            .content(&content)
            .build()
    }
}

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
