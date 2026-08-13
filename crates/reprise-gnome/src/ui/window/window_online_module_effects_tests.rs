//! Display-level coverage for online-module transitions at the composition seam.

use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::connectivity::Connectivity;

fn build_online_module_handles() -> super::window_online_module_test_hook::OnlineModuleTestHandles {
    gtk4::init().expect("GTK test display");
    let app = adw::Application::builder()
        .application_id("de.reprise.Reprise.OnlineModuleTest")
        .build();
    app.register(None::<&gio::Cancellable>)
        .expect("register test application");
    let conn = Rc::new(crate::test_db::open().expect("open test database"));
    let db_path = conn
        .path()
        .expect("file-backed test database")
        .to_path_buf();
    let _handler = super::surface::build(
        &app,
        &conn,
        &db_path,
        crate::ui::file_open::StartupOpenIntent::Library,
    );
    super::window_online_module_test_hook::take()
        .expect("window composition publishes online-module test handles")
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn lyr_6_the_production_module_transition_starts_lyrics_once_even_offline() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_online_module_handles();
    handles.preferences.set_connectivity(Connectivity::Offline);
    let before = handles.lyrics_batch.generation_for_test();

    handles
        .preferences
        .set_module_enabled_for_test(
            &reprise_core::modules::ONLINE_LYRICS_MODULE,
            true,
            "LYR-6 production transition test",
        )
        .expect("enable Online Lyrics");
    assert_eq!(handles.lyrics_batch.generation_for_test(), before + 1);

    handles
        .preferences
        .set_module_enabled_for_test(
            &reprise_core::modules::ONLINE_LYRICS_MODULE,
            true,
            "LYR-6 repeated transition test",
        )
        .expect("keep Online Lyrics enabled");
    assert_eq!(handles.lyrics_batch.generation_for_test(), before + 1);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_5_enabling_artwork_through_preferences_starts_the_wired_cover_pass() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_online_module_handles();
    handles.preferences.set_connectivity(Connectivity::Online);
    let before = handles.cover_batch.generation_for_test();
    let surface_requests_before = [
        crate::ui::stats_view::StatsView::artwork_refresh_requests_for_test(),
        crate::ui::podcasts::PodcastsView::artwork_refresh_requests_for_test(),
        crate::ui::radio::RadioView::artwork_refresh_requests_for_test(),
    ];

    handles
        .preferences
        .set_module_enabled_for_test(
            &reprise_core::modules::ARTWORK_MODULE,
            true,
            "NET-5 production transition test",
        )
        .expect("enable Artwork");

    assert_eq!(handles.cover_batch.generation_for_test(), before + 1);
    assert_eq!(
        [
            crate::ui::stats_view::StatsView::artwork_refresh_requests_for_test(),
            crate::ui::podcasts::PodcastsView::artwork_refresh_requests_for_test(),
            crate::ui::radio::RadioView::artwork_refresh_requests_for_test(),
        ],
        [
            surface_requests_before[0] + 1,
            surface_requests_before[1] + 2,
            surface_requests_before[2] + 1,
        ],
        "the production callback must reach every visible-artwork refresh seam"
    );
}
