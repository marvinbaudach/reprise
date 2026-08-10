//! Tests for the library chrome surface. Split out of `library_chrome.rs`
//! so that file stays under the 800-line gate.

use libadwaita::prelude::*;

use super::*;

fn test_content() -> gtk4::Label {
    gtk4::Label::new(Some("Library"))
}

/// UX SEARCH-2c: the chrome's own wiring must not outlive the window.
///
/// The lens's `:checked` state follows every keystroke, so a closure holding
/// the popover sits in the *entry's* handler list — and the popover holds that
/// same entry. Captured strongly, the loop closes onto itself: GObject
/// finalize is the only thing that would disconnect the handler, and it can
/// never run. The entry, the popover and its caption then outlive the window
/// forever. `SectionSearch` documents the same rule at length; this is the
/// test that keeps the chrome honest about it.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_2c_the_chrome_wiring_does_not_outlive_its_window() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();

    let (entry_weak, popover_weak) = {
        let window = adw::ApplicationWindow::builder().build();
        let header = adw::HeaderBar::new();
        let entry = gtk4::SearchEntry::new();
        let content = test_content();
        let chrome = build(&header, &content, &entry, &window);
        let weak = (entry.downgrade(), chrome.search.widget().downgrade());
        window.close();
        weak
    };

    while gtk4::glib::MainContext::default().iteration(false) {}

    assert!(
        entry_weak.upgrade().is_none(),
        "the search entry is still alive after its window closed"
    );
    assert!(
        popover_weak.upgrade().is_none(),
        "the search popover is still alive after its window closed"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_7c_the_doctor_uses_the_shared_window_chrome() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let settings = gtk4::Settings::default().unwrap();
    let animations_were_enabled = settings.is_gtk_enable_animations();
    settings.set_gtk_enable_animations(false);

    let window = adw::ApplicationWindow::builder()
        .default_width(900)
        .default_height(700)
        .build();
    let library_header = adw::HeaderBar::new();
    let source_title = adw::WindowTitle::new("Music", "");
    library_header.set_title_widget(Some(&source_title));
    let entry = gtk4::SearchEntry::new();
    let scan_button = gtk4::Button::with_label("Scan");
    let content_stack = gtk4::Stack::new();
    content_stack.add_named(&gtk4::Label::new(Some("Library")), Some("library"));
    let doctor_navigation = adw::NavigationView::new();
    content_stack.add_named(&doctor_navigation, Some("library-doctor"));
    content_stack.set_visible_child_name("library");

    let root_page = adw::NavigationPage::builder()
        .title("Library Doctor")
        .tag("library-doctor")
        .child(&gtk4::Label::new(Some("Start or result")))
        .build();
    doctor_navigation.add(&root_page);

    let review_actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    let all = gtk4::Button::with_label("All");
    let none = gtk4::Button::with_label("None");
    review_actions.append(&all);
    review_actions.append(&none);
    let review_page = adw::NavigationPage::builder()
        .title("Review Changes")
        .tag("library-doctor-review")
        .child(&gtk4::Label::new(Some("Review")))
        .build();

    let chrome = build(&library_header, &content_stack, &entry, &window);
    let doctor_chrome = wire_content_stack(
        &chrome,
        &content_stack,
        &doctor_navigation,
        &source_title,
        &scan_button,
    );
    doctor_chrome.set_review_actions(&review_actions);
    let decorations = crate::ui::window::window_decorations::WindowDecorations::new(
        &window,
        &library_header,
        None,
    );
    decorations.content_host().set_content(&chrome.root);
    decorations.apply(reprise_core::library::settings::WindowDecorationMode::Client);
    window.present();
    crate::ui::window::content_stack::show_page(&content_stack, "library-doctor");
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert!(chrome.root.reveals_top_bars());
    assert!(library_header.shows_start_title_buttons());
    assert!(library_header.shows_end_title_buttons());
    assert!(!source_title.is_visible());
    assert!(!chrome.search_toggle.is_visible());
    assert!(!scan_button.is_visible());
    assert_eq!(visible_header_title(&library_header), "Library Doctor");
    assert!(!review_actions.is_visible());
    assert_eq!(mapped_adw_header_rows(chrome.root.upcast_ref()), 1);

    doctor_navigation.push(&review_page);
    while gtk4::glib::MainContext::default().iteration(false) {}
    assert!(review_actions.is_visible());
    assert!(all.is_ancestor(&library_header));
    assert!(none.is_ancestor(&library_header));
    assert_eq!(mapped_adw_header_rows(chrome.root.upcast_ref()), 1);

    window.close();
    settings.set_gtk_enable_animations(animations_were_enabled);
}

fn visible_header_title(header: &adw::HeaderBar) -> String {
    header
        .title_widget()
        .expect("the shared header has a title")
        .downcast::<adw::WindowTitle>()
        .expect("the shared header title uses AdwWindowTitle")
        .title()
        .into()
}

fn mapped_adw_header_rows(root: &gtk4::Widget) -> usize {
    let mut rows = usize::from(root.is::<adw::HeaderBar>() && root.is_mapped());
    let mut child = root.first_child();
    while let Some(widget) = child {
        rows += mapped_adw_header_rows(&widget);
        child = widget.next_sibling();
    }
    rows
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_1a_idle_is_icon_not_field() {
    gtk4::init().unwrap();
    let window = adw::ApplicationWindow::builder().build();
    let header = adw::HeaderBar::new();
    let entry = gtk4::SearchEntry::new();

    let chrome = build(&header, &test_content(), &entry, &window);

    assert!(chrome.search_toggle.is_ancestor(&header));
    assert_eq!(
        chrome.search_toggle.icon_name().as_deref(),
        Some("system-search-symbolic")
    );
    assert!(!chrome.search.is_open());
    assert!(!entry.is_visible());
    assert!(entry.is_ancestor(chrome.search.widget()));
    assert_eq!(
        chrome.search.widget().parent().as_ref(),
        Some(chrome.search_toggle.upcast_ref())
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_2c_popover_floats_without_reflowing() {
    gtk4::init().unwrap();
    let window = adw::ApplicationWindow::builder().build();
    let header = adw::HeaderBar::new();
    let entry = gtk4::SearchEntry::new();

    let content = test_content();
    let chrome = build(&header, &content, &entry, &window);

    assert!(header.is_ancestor(&chrome.root));
    assert_eq!(chrome.root.content().as_ref(), Some(content.upcast_ref()));
    assert_eq!(
        chrome.search.widget().position(),
        gtk4::PositionType::Bottom
    );
    assert_eq!(chrome.search.widget().halign(), gtk4::Align::End);
    assert!(!chrome.search.widget().has_arrow());
    assert!(chrome
        .search
        .widget()
        .has_css_class("reprise-search-popover"));

    let css = css();
    let popover_css = css
        .split(".reprise-search-popover > contents")
        .nth(1)
        .and_then(|rule| rule.split('}').next())
        .expect("search popover CSS rule");
    assert!(popover_css.contains("background-color: @headerbar_bg_color"));
    assert!(popover_css.contains("border: 1px solid"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_3_lens_checked_when_active() {
    gtk4::init().unwrap();
    let window = adw::ApplicationWindow::builder().build();
    let header = adw::HeaderBar::new();
    let entry = gtk4::SearchEntry::new();
    let chrome = build(&header, &test_content(), &entry, &window);
    window.set_content(Some(&chrome.root));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert!(!chrome.search_toggle.is_active());
    chrome.search.open();
    assert!(chrome.search_toggle.is_active());

    chrome.search.close();
    assert!(!chrome.search_toggle.is_active());
    entry.set_text("falling");

    assert!(search_toggle_active(chrome.search.is_open(), &entry.text()));
    assert!(chrome.search_toggle.is_active());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_6_hidden_query_survives_as_chip() {
    gtk4::init().unwrap();
    let window = adw::ApplicationWindow::builder().build();
    let header = adw::HeaderBar::new();
    let entry = gtk4::SearchEntry::new();
    let chrome = build(&header, &test_content(), &entry, &window);
    window.set_content(Some(&chrome.root));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    chrome.search.open();
    entry.set_text("falling");

    chrome.search_toggle.emit_clicked();

    assert!(!chrome.search.is_open());
    assert_eq!(entry.text(), "falling");
    assert!(chrome.search_toggle.is_active());
    window.close();
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
    window.set_content(Some(&chrome.root));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    chrome.search.open();
    entry.set_text("nomatch");

    // The entry clear icon, chip's ×, and Clear all all reach this state.
    entry.set_text("");
    assert!(chrome.search.is_open());
    chrome.search.close();
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert_eq!(
        entry.text(),
        "",
        "an explicitly cleared query must not be resurrected by the collapse"
    );
    assert!(!chrome.search_toggle.is_active());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_6_closing_the_popover_never_wipes_the_query() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let window = adw::ApplicationWindow::builder().build();
    let header = adw::HeaderBar::new();
    let entry = gtk4::SearchEntry::new();
    let chrome = build(&header, &test_content(), &entry, &window);
    window.set_content(Some(&chrome.root));
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}
    chrome.search.open();
    entry.set_text("falling");

    chrome.search.close();
    assert_eq!(entry.text(), "falling");
    window.close();
}

#[test]
fn search_toggle_projects_open_mode_or_non_empty_query() {
    assert!(!search_toggle_active(false, ""));
    assert!(search_toggle_active(true, ""));
    assert!(search_toggle_active(false, "falling"));
    assert!(!search_toggle_active(false, "   "));
}

// SEARCH-4a's "clearing is a separate act" is proved in
// `search_popover_tests.rs`, against the popover and the committed chip. The
// test that used to sit here only set and re-read a bare `GtkSearchEntry`: it
// outlived the preserved-query stash it was written to guard, and afterwards
// asserted nothing but that GTK's own `set_text` works.

#[test]
fn search_5_collapsing_keeps_the_query_active() {
    let query = "falling";

    assert!(search_toggle_active(false, query));
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

    let violations = crate::ui::tooltip_discipline::tooltip_violations(actions.scan.upcast_ref());
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
