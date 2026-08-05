use super::*;

use std::time::Duration;

fn test_widgets(content: &impl IsA<gtk4::Widget>) -> PanelWidgets {
    let conn = crate::test_db::open().unwrap();
    let cover_loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    build_widgets(content, true, &Rc::new(conn), &cover_loader)
}

#[test]
fn npp_14_built_in_tabs_precede_the_sound_extension() {
    assert_eq!(TAB_ICONS_MAX_WIDTH, 224.0);
    assert_eq!(strings::text(strings::VISUAL), "Visuals");
    assert_eq!(strings::text(strings::SOUND), "Sound");
    assert_eq!(PanelTab::Visual.page_name(), VISUAL_PAGE);
    assert_eq!(
        PANEL_TABS,
        [
            PanelTab::UpNext,
            PanelTab::Lyrics,
            PanelTab::Visual,
            PanelTab::Sound,
        ]
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn four_tab_labels_fit_the_300_px_panel_without_ellipsizing() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_width_request(600);
    let widgets = test_widgets(&content);
    // Four tabs only exist while Sound Similarity is switched on; the module
    // ships off, so its page starts hidden. Enabling it here is what puts the
    // fourth label into the switcher — measuring the three-tab bar would not
    // answer the question E3 asks.
    widgets.sound_page.set_visible(true);
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(700)
        .child(widgets.column.widget())
        .build();
    window.present();
    crate::ui::test_settle::settle_for(Duration::from_millis(100));

    assert_eq!(
        widgets.tab_switcher.display_mode(),
        adw::InlineViewSwitcherDisplayMode::Labels
    );
    let labels = descendant_labels(widgets.tab_switcher.upcast_ref());
    for expected in ["Up Next", "Lyrics", "Visuals", "Sound"] {
        let label = labels
            .iter()
            .find(|label| label.text().as_str() == expected)
            .unwrap_or_else(|| panic!("missing tab label {expected:?}"));
        let (natural_width, _) = label.layout().pixel_size();
        assert!(
            !label.layout().is_ellipsized(),
            "tab label {expected:?} is ellipsized: allocation {} px, text {} px",
            label.width(),
            natural_width
        );
    }
    window.close();
}

fn descendant_labels(root: &gtk4::Widget) -> Vec<gtk4::Label> {
    let mut labels = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if let Ok(label) = widget.clone().downcast::<gtk4::Label>() {
            labels.push(label);
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    labels
}
