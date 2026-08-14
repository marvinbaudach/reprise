//! Display-level coverage for online-module transitions at the composition seam.

use std::rc::Rc;

use gtk4::gio;
use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::ActionRowExt;
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

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn rad_5_real_preferences_return_resumes_the_open_near_you_intent() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_online_module_handles();
    reprise_core::online_sources::set_enabled(&handles.preferences.conn, true).unwrap();
    reprise_core::modules::set_enabled(
        &handles.preferences.conn,
        &reprise_core::modules::RADIO_MODULE,
        true,
    )
    .unwrap();

    handles.radio.open_near_you_location_preferences_for_test();
    crate::ui::source_context_surface::settle_layout();

    assert!(handles.preferences.preferences_dialog().is_some());
    assert!(handles.radio.add_dialog_is_visible_for_test());
    assert!(handles.radio.add_dialog_needs_location_for_test());

    handles
        .preferences
        .store_location_for_test(52.52, 13.405, "Berlin", Some("DE"));

    assert!(
        handles.radio.add_dialog_is_searching_for_test(),
        "the still-open Add Station dialog must resume without another chip click"
    );
    assert!(handles.radio.add_dialog_is_visible_for_test());
}

/// `SET-15` last sentence: "Disabling Concerts … never suppresses the
/// app-wide location-change announcement." Drives the real
/// `PreferencesContext::apply_location()` write path (via `surface::build`,
/// not `RadioAddDialog` built standalone) with `CONCERTS_MODULE` explicitly
/// off, and proves the resulting `LocationBroadcast::notify()` still reaches
/// Radio's pending "Near you" intent.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_15_disabling_concerts_never_blocks_the_real_near_you_search() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_online_module_handles();
    reprise_core::online_sources::set_enabled(&handles.preferences.conn, true).unwrap();
    reprise_core::modules::set_enabled(
        &handles.preferences.conn,
        &reprise_core::modules::RADIO_MODULE,
        true,
    )
    .unwrap();
    reprise_core::modules::set_enabled(
        &handles.preferences.conn,
        &reprise_core::modules::CONCERTS_MODULE,
        false,
    )
    .unwrap();

    handles.radio.open_near_you_location_preferences_for_test();
    crate::ui::source_context_surface::settle_layout();

    assert!(handles.preferences.preferences_dialog().is_some());
    assert!(handles.radio.add_dialog_is_visible_for_test());
    assert!(handles.radio.add_dialog_needs_location_for_test());

    handles
        .preferences
        .store_location_for_test(52.52, 13.405, "Berlin", Some("DE"));

    assert!(
        handles.radio.add_dialog_is_searching_for_test(),
        "Concerts being off must not swallow the app-wide location announcement"
    );
    assert!(handles.radio.add_dialog_is_visible_for_test());
}

/// `SET-15`: "Disabling Concerts or online sources never makes the stored
/// location or radius unreadable." Stores the location, switches the
/// online-sources gate off, then opens the real Location preferences page
/// for the first time — proving the read path (`preference_location::build_surface`
/// via `PreferencesContext::location_page`) is not behind the same gate that
/// blocks new geocoding/portal *requests*.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_15_the_stored_location_stays_readable_once_online_sources_are_off() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let handles = build_online_module_handles();
    reprise_core::online_sources::set_enabled(&handles.preferences.conn, true).unwrap();
    reprise_core::location::store(
        &handles.preferences.conn,
        52.52,
        13.405,
        "Berlin",
        Some("DE"),
    )
    .unwrap();

    reprise_core::online_sources::set_enabled(&handles.preferences.conn, false).unwrap();
    handles.preferences.present_location_settings();
    crate::ui::source_context_surface::settle_layout();

    let city = handles
        .preferences
        .location_city_row
        .borrow()
        .upgrade()
        .expect("present_location_settings must build the real Location page");
    assert_eq!(
        city.subtitle().as_deref(),
        Some("Berlin"),
        "the stored city must stay readable once the online gate is off"
    );
}
