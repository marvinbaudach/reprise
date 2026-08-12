//! Display-dependent contracts for the device-page horizontal bars.

use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn mtp_65_the_sync_dock_uses_the_page_clamp_and_stays_a_direct_child() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().expect("GTK test display");
    let (surface, _root) = DeviceSyncPage::new(
        &device(),
        PageActions {
            set_profile: Rc::new(|_| {}),
            set_playlist: Rc::new(|_, _| {}),
            start: Rc::new(|| {}),
            cancel: Rc::new(|| {}),
            eject: Rc::new(|| {}),
        },
        &no_op_content_actions(),
    );

    assert_eq!(
        surface.dashboard.dock.root().parent().as_ref(),
        Some(surface.dashboard.root.upcast_ref()),
        "the dock must remain a direct dashboard child beside the scroller"
    );
    let clamp = surface
        .dashboard
        .dock
        .root()
        .first_child()
        .and_downcast::<adw::Clamp>()
        .expect("the dock content must use the page clamp");
    assert_eq!(
        clamp.maximum_size(),
        super::device_sync_page_layout::CONTENT_MAX_WIDTH
    );
    let dock_content = clamp
        .child()
        .and_downcast::<gtk4::Box>()
        .expect("dock content inside clamp");
    assert_eq!(
        dock_content.margin_start(),
        surface.dashboard.content.margin_start()
    );
    assert_eq!(
        dock_content.margin_end(),
        surface.dashboard.content.margin_end()
    );
}

#[test]
fn mtp_65_the_sync_dock_has_a_named_surface_and_top_edge() {
    let css = crate::ui::style::app_css_for_test();
    let rule = css
        .split(".reprise-device-sync-dock {")
        .nth(1)
        .and_then(|tail| tail.split('}').next())
        .expect("the installed stylesheet must own the device dock surface");

    assert!(rule.contains("background-color: @headerbar_bg_color"));
    assert!(rule.contains("border-top: 1px solid alpha(@window_fg_color"));
}
