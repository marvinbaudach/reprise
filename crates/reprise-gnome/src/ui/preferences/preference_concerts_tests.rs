use super::*;

#[test]
fn concerts_location_reference_copy_distinguishes_missing_and_stored_values() {
    assert_eq!(
        location_reference_copy(None, 1_000),
        ("Location · not set".to_owned(), "Set location →".to_owned())
    );
    let location = reprise_core::location::AppLocation {
        latitude: 52.52,
        longitude: 13.405,
        name: "Berlin".to_owned(),
        country: Some("Deutschland".to_owned()),
        country_code: Some("DE".to_owned()),
    };
    assert_eq!(
        location_reference_copy(Some(&location), 1_000),
        (
            "Location · Berlin, Deutschland, within 1000 km".to_owned(),
            "Change in Location →".to_owned(),
        )
    );
}

#[test]
fn stored_credentials_are_preferred_and_similar_count_clamps() {
    let conn = crate::test_db::open().unwrap();
    let credentials = reprise_core::concerts::config::credentials(&conn).unwrap();
    assert_eq!(
        credentials.bandsintown_app_id.as_deref(),
        Some("io.github.marvinbaudach.Reprise")
    );

    reprise_core::library::settings::set_setting(
        &conn,
        reprise_core::concerts::config::BANDSINTOWN_APP_ID_KEY,
        "stored-app",
    )
    .unwrap();
    reprise_core::library::settings::set_setting(
        &conn,
        reprise_core::concerts::config::SIMILAR_COUNT_KEY,
        "99",
    )
    .unwrap();

    let credentials = reprise_core::concerts::config::credentials(&conn).unwrap();
    let similar = reprise_core::concerts::config::similar_config(&conn).unwrap();

    assert_eq!(
        credentials.bandsintown_app_id.as_deref(),
        Some("stored-app")
    );
    assert_eq!(similar.count, 25);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn conc_9a_concerts_preferences_contain_no_password_entry_row() {
    gtk4::init().unwrap();
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let broadcast = Rc::new(LocationBroadcast::default());
    let on_location: OnLocation = Rc::new(|| {});
    let preferences = build(&conn, &runtime, &broadcast, &on_location, true);

    let password_rows = preferences
        .inner
        .rows
        .iter()
        .filter(|row| row.is::<adw::PasswordEntryRow>())
        .count();
    assert_eq!(password_rows, 0);
    assert!(!preferences.inner.similar_count.is_sensitive());
    preferences.inner.similar_enabled.set_active(true);
    assert!(preferences.inner.similar_count.is_sensitive());
    preferences.set_sensitive(false);
    assert!(!preferences.inner.similar_count.is_sensitive());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_15_concerts_location_reference_is_first_and_refreshes_on_app_broadcast() {
    gtk4::init().unwrap();
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let broadcast = Rc::new(LocationBroadcast::default());
    let on_location: OnLocation = Rc::new(|| {});
    let preferences = build(&conn, &runtime, &broadcast, &on_location, true);

    assert_eq!(
        preferences.inner.location_reference.title(),
        "Location · not set"
    );
    assert_eq!(
        preferences.inner.rows[0],
        preferences
            .inner
            .location_reference
            .clone()
            .upcast::<gtk4::Widget>()
    );
    preferences.set_sensitive(false);
    assert!(preferences
        .inner
        .location_reference
        .property::<bool>("sensitive"));

    reprise_core::location::store(
        &conn,
        52.52,
        13.405,
        reprise_core::location::LocationName::with_country("Berlin", Some("Deutschland")),
        Some("DE"),
    )
    .unwrap();
    broadcast.notify();
    assert_eq!(
        preferences.inner.location_reference.title(),
        "Location · Berlin, Deutschland, within 1000 km"
    );
}
