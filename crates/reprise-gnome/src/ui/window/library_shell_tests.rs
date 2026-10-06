use super::*;

fn window_attached_collapsed_setters(source: &str) -> Vec<String> {
    let compact: String = source
        .chars()
        .filter(|char| !char.is_whitespace())
        .collect();
    compact
        .split(';')
        .filter(|statement| {
            statement.contains(".add_setter(") && statement.contains("\"collapsed\"")
        })
        .filter_map(|statement| {
            let prefix = statement.split_once(".add_setter(")?.0;
            prefix
                .rsplit(|char: char| !(char.is_ascii_alphanumeric() || char == '_'))
                .next()
                .map(str::to_owned)
        })
        .filter(|breakpoint| compact.contains(&format!("window.add_breakpoint({breakpoint})")))
        .collect()
}

#[test]
fn browse_1_music_builds_only_the_canonical_track_surface() {
    let window = include_str!("window.rs");
    let chrome = include_str!("library_chrome.rs");

    for obsolete in [
        "AlbumView::new",
        "ArtistView::new",
        "build_views(",
        "build_library_title(",
    ] {
        assert!(
            !window.contains(obsolete),
            "the main Music shell still constructs the obsolete `{obsolete}` surface"
        );
    }
    assert!(
        !chrome.contains("InlineViewSwitcher"),
        "the global header must not expose parallel Tracks/Albums/Artists modes"
    );
}

#[test]
fn window_breakpoints_never_own_split_view_collapse() {
    for (name, source) in [
        ("library shell", include_str!("library_shell.rs")),
        (
            "active content focus",
            include_str!("active_content_focus.rs"),
        ),
        (
            "responsive side panels",
            include_str!("responsive_side_panels.rs"),
        ),
        (
            "compact mode suggestion",
            include_str!("../compact/compact_mode_suggestion.rs"),
        ),
    ] {
        let offenders = window_attached_collapsed_setters(source);
        assert!(
            offenders.is_empty(),
            "{name} attaches collapsed-setter breakpoints {offenders:?} to the window"
        );
    }
}

#[test]
fn breakpoint_guard_recognizes_a_multiline_collapsed_setter() {
    let broken = concat!(
        "legacy.add_setter(&split_view, \"collapsed\", Some(&false.to_value()));\n",
        "window.",
        "add_breakpoint(legacy);\n",
        "breakpoint.add_setter(\n",
        "    &split_view,\n",
        "    \"collapsed\",\n",
        "    Some(&false.to_value()),\n",
        ");\n",
        "window.",
        "add_breakpoint(breakpoint);\n",
    );

    assert_eq!(
        window_attached_collapsed_setters(broken),
        ["legacy", "breakpoint"]
    );
}

#[test]
fn active_content_focus_resolves_every_shell_view() {
    assert_eq!(
        active_content_target(Some("library")),
        Some(ActiveContentTarget::Tracks)
    );
    assert_eq!(
        active_content_target(Some("stats")),
        Some(ActiveContentTarget::Stats)
    );
    assert_eq!(active_content_target(Some("device")), None);
    assert_eq!(
        active_content_target(Some("concerts")),
        Some(ActiveContentTarget::Concerts)
    );
    assert_eq!(
        active_content_target(Some("releases")),
        Some(ActiveContentTarget::Releases)
    );
    assert_eq!(
        active_content_target(Some("podcasts")),
        Some(ActiveContentTarget::Podcasts)
    );
    assert_eq!(
        active_content_target(Some("radio")),
        Some(ActiveContentTarget::Radio)
    );
    assert_eq!(
        active_content_target(Some("library-doctor")),
        Some(ActiveContentTarget::LibraryDoctor)
    );
    assert_eq!(active_content_target(Some("unknown")), None);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_escape_focuses_the_current_shell_view() {
    gtk4::init().unwrap();
    let tracks = gtk4::Button::with_label("Tracks focus");
    let stats = gtk4::Button::with_label("Stats focus");
    let content = gtk4::Stack::new();
    content.add_named(&tracks, Some("library"));
    content.add_named(&stats, Some("stats"));
    content.set_visible_child_name("library");
    let window = gtk4::Window::builder().child(&content).build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let tracks_focus = {
        let tracks = tracks.downgrade();
        Rc::new(move || tracks.upgrade().is_some_and(|widget| widget.grab_focus()))
    };
    let focus = ActiveContentFocus::from_focus_action(&content, tracks_focus);

    for (content_name, expected) in [
        ("library", tracks.upcast_ref::<gtk4::Widget>()),
        ("stats", stats.upcast_ref()),
    ] {
        content.set_visible_child_name(content_name);
        assert!(focus.focus());
        assert_eq!(
            gtk4::prelude::GtkWindowExt::focus(&window).as_ref(),
            Some(expected)
        );
    }

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn style_7_sidebar_reserves_a_real_slot_at_1024_px() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let sidebar_page = adw::NavigationPage::builder()
        .title("Sidebar")
        .child(&gtk4::Label::new(Some("Sidebar")))
        .build();
    let content_label = gtk4::Label::new(Some("Content"));
    let content = adw::NavigationView::new();
    content.add(
        &adw::NavigationPage::builder()
            .title("Content")
            .child(&content_label)
            .build(),
    );

    let split = build_split_view(&sidebar_page, &content);
    split.set_show_sidebar(true);
    let window = gtk4::Window::builder().child(&split).build();
    window.set_default_size(1_024, 768);
    window.set_size_request(1_024, 768);
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    let content_bounds = content
        .compute_bounds(&window)
        .expect("content must share the window coordinate space");
    assert!(
        !split.is_collapsed(),
        "1024 px library split entered overlay mode: window={} split={} content={content_bounds:?}",
        window.width(),
        split.width(),
    );
    assert!(
        content_bounds.x() > 100.0,
        "the open library sidebar did not reserve a content slot: {content_bounds:?}"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_sidebar_keeps_its_column_at_a_narrow_viewport() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let sidebar_page = adw::NavigationPage::builder()
        .title("Sidebar")
        .child(&gtk4::Label::new(Some("Sidebar")))
        .build();
    let content_label = gtk4::Label::new(Some("Content"));
    let content = adw::NavigationView::new();
    content.add(
        &adw::NavigationPage::builder()
            .title("Content")
            .child(&content_label)
            .build(),
    );

    let split = build_split_view(&sidebar_page, &content);
    crate::ui::sidebar::sidebar_presentation::style_overlay_split_view(&split);
    split.set_show_sidebar(true);
    let window = gtk4::Window::builder().child(&split).build();
    window.set_default_size(600, 700);
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    assert!(
        !split.is_collapsed(),
        "a narrow viewport turned the library sidebar into an overlay: \
         window={}",
        window.width(),
    );
    let content_bounds = content
        .compute_bounds(&window)
        .expect("content must share the window coordinate space");
    assert!(
        content_bounds.x() > 100.0,
        "the sidebar stopped reserving a content slot when narrow: \
         {content_bounds:?}"
    );
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn library_split_is_scoped_for_chrome_separators() {
    gtk4::init().unwrap();
    let sidebar_page = adw::NavigationPage::builder()
        .title("Sidebar")
        .child(&gtk4::Label::new(Some("Sidebar")))
        .build();
    let content = adw::NavigationView::new();

    let split = build_split_view(&sidebar_page, &content);

    assert!(split.has_css_class("reprise-library-split"));
    assert!(sidebar_page.has_css_class("reprise-library-sidebar"));
}
