//! Full-width library chrome from design mockup 7a.
use std::cell::RefCell;
use std::rc::Rc;

use libadwaita as adw;
use libadwaita::prelude::*;

use super::strings;

const LIBRARY_TITLE_SOURCE: &str = "source";
const LIBRARY_TITLE_SWITCHER: &str = "library-switcher";
const VIEW_SWITCHER_BREAKPOINT_WIDTH: i32 = 600;

pub(in crate::ui) struct LibraryChrome {
    pub(in crate::ui) root: adw::ToolbarView,
    pub(in crate::ui) search_bar: gtk4::SearchBar,
    #[cfg(test)]
    pub(in crate::ui) search_toggle: gtk4::ToggleButton,
}

pub(in crate::ui) struct LibraryMaintenanceActions {
    pub(in crate::ui) scan: gtk4::Button,
}

pub(in crate::ui) struct LibraryTitle {
    pub(in crate::ui) root: gtk4::Stack,
    #[cfg(test)]
    pub(in crate::ui) switcher: adw::InlineViewSwitcher,
}

impl LibraryTitle {
    pub(in crate::ui) fn set_library_navigation_visible(&self, visible: bool) {
        let name = if visible {
            LIBRARY_TITLE_SWITCHER
        } else {
            LIBRARY_TITLE_SOURCE
        };
        self.root.set_visible_child_name(name);
    }
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
        .tooltip_text(strings::text(strings::SEARCH_PLACEHOLDER))
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

    let root = adw::ToolbarView::new();
    root.set_top_bar_style(adw::ToolbarStyle::Flat);
    root.add_top_bar(header);
    root.add_top_bar(&search_bar);
    root.set_content(Some(content));
    LibraryChrome {
        root,
        search_bar,
        #[cfg(test)]
        search_toggle,
    }
}

pub(in crate::ui) fn search_toggle_active(search_mode: bool, query: &str) -> bool {
    search_mode || !query.trim().is_empty()
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

pub(in crate::ui) fn build_library_title(
    window: &adw::ApplicationWindow,
    header: &adw::HeaderBar,
    source_title: &adw::WindowTitle,
    views: &adw::ViewStack,
) -> LibraryTitle {
    // `source_title` is the header's initial title before the library switcher
    // exists. Detach it before moving it into the two-state title stack;
    // GTK widgets cannot have two parents, and a failed reparent silently
    // leaves non-library pages without their source title.
    if header
        .title_widget()
        .as_ref()
        .is_some_and(|widget| widget == source_title.upcast_ref::<gtk4::Widget>())
    {
        header.set_title_widget(gtk4::Widget::NONE);
    }
    let switcher = adw::InlineViewSwitcher::builder()
        .stack(views)
        .display_mode(adw::InlineViewSwitcherDisplayMode::Labels)
        .can_shrink(true)
        .homogeneous(true)
        .build();
    switcher.add_css_class("reprise-view-switcher");
    let root = gtk4::Stack::new();
    root.add_named(source_title, Some(LIBRARY_TITLE_SOURCE));
    root.add_named(&switcher, Some(LIBRARY_TITLE_SWITCHER));
    root.set_visible_child_name(LIBRARY_TITLE_SWITCHER);
    header.set_show_title(true);
    header.set_title_widget(Some(&root));

    let condition = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        f64::from(VIEW_SWITCHER_BREAKPOINT_WIDTH),
        adw::LengthUnit::Px,
    );
    let breakpoint = adw::Breakpoint::new(condition);
    breakpoint.add_setter(
        &switcher,
        "display-mode",
        Some(&adw::InlineViewSwitcherDisplayMode::Icons.to_value()),
    );
    window.add_breakpoint(breakpoint);
    LibraryTitle {
        root,
        #[cfg(test)]
        switcher,
    }
}

/// The Tracks/Albums/Artists `AdwInlineViewSwitcher` styled as a rounded pill
/// group (design mockup 14a): a subtle white-tint container with a soft
/// radius, and segment buttons that shed the default `.linked` hard edges —
/// the active segment tinted + bold, inactive quiet, hover a hair brighter.
/// `@window_fg_color` (near-white on the dark theme) keeps it theme-aware.
/// Installed app-wide by [`super::style`].
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
       color: @reprise_secondary_fg_color; }\n\
     .reprise-view-switcher { \
       background-color: alpha(@window_fg_color, 0.06); \
       border: none; border-radius: 8px; padding: 2px; box-shadow: none; }\n\
     /* Resting look only. Hover, press and the focus ring come from \
        `style::buttons`, which reaches these Adwaita-internal buttons by \
        selector (BTN-4). No `outline: none` here — that deleted the keyboard \
        focus ring along with the frame. */\n\
     .reprise-view-switcher > button { \
       border: none; border-radius: 6px; box-shadow: none; \
       min-height: 0; margin: 0; padding: 2px 14px; \
       background-color: transparent; background-image: none; \
       color: alpha(@window_fg_color, 0.60); font-weight: 400; }\n\
     .reprise-view-switcher > button:checked { \
       background-color: alpha(@window_fg_color, 0.14); \
       color: @window_fg_color; font-weight: 700; }"
        .to_string()
}

#[cfg(test)]
#[path = "library_chrome_npp_tests.rs"]
mod npp_tests;

#[cfg(test)]
mod tests {
    use libadwaita::prelude::*;

    use super::*;

    fn test_content() -> gtk4::Label {
        gtk4::Label::new(Some("Library"))
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
            crate::ui::browse::browse_bar::chip_labels(
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

        let chips = crate::ui::browse::browse_bar::chip_labels(
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
    fn search_5_collapsing_keeps_query_and_chip() {
        let query = "falling";
        let chips = crate::ui::browse::browse_bar::chip_labels(
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
        assert!(!css.contains(".reprise-view-switcher > button:focus-visible"));

        let button_css = crate::ui::style::buttons::css();
        assert!(button_css.contains(".reprise-view-switcher > button:focus-visible"));
        assert!(button_css.contains("outline: 2px solid @accent_color"));
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
    fn tip_1c_library_chrome_buttons_follow_tooltip_discipline() {
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
    fn library_view_title_switches_between_source_title_and_view_switcher() {
        if gtk4::init().is_err() {
            return;
        }
        let window = adw::ApplicationWindow::builder().build();
        let title = adw::WindowTitle::new("Music", "");
        let views = adw::ViewStack::new();
        views.add_titled_with_icon(
            &gtk4::Label::new(Some("Tracks")),
            Some("tracks"),
            "Tracks",
            "view-list-symbolic",
        );

        let header = adw::HeaderBar::new();
        header.set_title_widget(Some(&title));
        let library_title = build_library_title(&window, &header, &title, &views);

        assert_eq!(library_title.switcher.stack(), Some(views.clone()));
        assert_eq!(title.parent(), Some(library_title.root.clone().upcast()));
        assert!(library_title.root.is_ancestor(&header));
        assert_eq!(
            header.title_widget(),
            Some(library_title.root.clone().upcast())
        );
        assert!(header.shows_title());
        assert_eq!(
            library_title.root.visible_child_name().as_deref(),
            Some(LIBRARY_TITLE_SWITCHER)
        );
        library_title.set_library_navigation_visible(false);
        assert_eq!(
            library_title.root.visible_child_name().as_deref(),
            Some(LIBRARY_TITLE_SOURCE)
        );
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
