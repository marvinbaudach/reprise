use super::*;

fn test_widgets(content: &impl IsA<gtk4::Widget>) -> PanelWidgets {
    let conn = crate::test_db::open().unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    build_widgets(content, true, &Rc::new(conn), &cover_loader)
}

#[test]
fn npp_14_has_three_built_in_tabs_in_order() {
    assert_eq!(strings::text(strings::VISUAL), "Visuals");
    assert_eq!(PanelTab::Visual.page_name(), VISUAL_PAGE);
    assert_eq!(
        PANEL_TABS,
        [PanelTab::UpNext, PanelTab::Lyrics, PanelTab::Visual]
    );
}

#[test]
fn npp_14_built_in_tabs_have_distinct_semantic_icons() {
    assert_eq!(PanelTab::UpNext.icon_name(), "view-list-symbolic");
    assert_eq!(PanelTab::Lyrics.icon_name(), "reprise-lyrics-symbolic");
    assert_eq!(PanelTab::Visual.icon_name(), "reprise-visual-bars-symbolic");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn npp_14_tabs_are_always_icon_only_with_installed_labeled_symbols() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    crate::register_app_resources();
    gtk4::init().unwrap();
    crate::install_app_icon_resource_path();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let widgets = test_widgets(&content);

    assert_eq!(
        widgets.tab_switcher.display_mode(),
        adw::InlineViewSwitcherDisplayMode::Icons
    );
    assert!(widgets
        .tab_switcher
        .parent()
        .is_some_and(|parent| parent.is::<gtk4::Box>()));

    let icon_theme = gtk4::IconTheme::for_display(&gtk4::gdk::Display::default().unwrap());
    for (index, expected) in [
        ("Up Next", "view-list-symbolic"),
        ("Lyrics", "reprise-lyrics-symbolic"),
        ("Visuals", "reprise-visual-bars-symbolic"),
    ]
    .into_iter()
    .enumerate()
    {
        let page = widgets
            .tab_stack
            .pages()
            .item(index as u32)
            .and_downcast::<adw::ViewStackPage>()
            .unwrap();
        assert_eq!(page.title().as_deref(), Some(expected.0));
        assert_eq!(page.icon_name().as_deref(), Some(expected.1));
        assert!(icon_theme.has_icon(expected.1));
    }
}
