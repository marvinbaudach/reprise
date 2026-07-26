use super::*;

#[test]
fn location_apply_decisions_store_success_and_keep_errors_visible() {
    assert_eq!(
        geocode_decision(Ok(Some(reprise_core::concerts::GeocodedLocation {
            lat: 48.137,
            lon: 11.575,
            display_name: "Munich, Bavaria".into(),
        }))),
        LocationDecision::Store {
            latitude: 48.137,
            longitude: 11.575,
            name: "Munich, Bavaria".into(),
        }
    );
    assert!(matches!(
        geocode_decision(Ok(None)),
        LocationDecision::Error(_)
    ));
    assert_eq!(
        portal_decision(&Ok(reprise_platform_linux::location::PortalLocation {
            latitude: 47.376,
            longitude: 8.541,
            accuracy_m: Some(1_000.0),
        })),
        LocationDecision::Store {
            latitude: 47.376,
            longitude: 8.541,
            name: crate::ui::strings::text(crate::ui::strings::CONCERTS_CURRENT_LOCATION),
        }
    );
    assert!(matches!(
        portal_decision(&Err("denied".into())),
        LocationDecision::Error(error)
            if error == crate::ui::strings::text(
                crate::ui::strings::CONCERTS_LOCATION_NOT_FOUND
            )
    ));
}

#[test]
fn current_location_button_is_disabled_with_pending_feedback() {
    assert_eq!(
        current_location_button_state(false),
        CurrentLocationButtonState {
            sensitive: true,
            show_spinner: false,
        }
    );
    assert_eq!(
        current_location_button_state(true),
        CurrentLocationButtonState {
            sensitive: false,
            show_spinner: true,
        }
    );
}

#[test]
fn set_4_credential_apply_requires_successful_persistence() {
    assert_eq!(
        credential_apply_decision("", true),
        CredentialApplyDecision::Reset
    );
    assert_eq!(
        credential_apply_decision("key", false),
        CredentialApplyDecision::CouldNotVerify
    );
    assert_eq!(
        credential_apply_decision("key", true),
        CredentialApplyDecision::Verify
    );
}

#[test]
fn conc_8_credential_feedback_projects_every_verification_outcome_inline() {
    assert_eq!(
        credential_feedback_message(reprise_core::concerts::CredentialVerification::Empty),
        None
    );
    assert_eq!(
        credential_feedback_message(reprise_core::concerts::CredentialVerification::Valid),
        Some(strings::CONCERTS_CREDENTIAL_VALID)
    );
    assert_eq!(
        credential_feedback_message(reprise_core::concerts::CredentialVerification::Rejected),
        Some(strings::CONCERTS_CREDENTIAL_REJECTED)
    );
    assert_eq!(
        credential_feedback_message(reprise_core::concerts::CredentialVerification::CouldNotVerify),
        Some(strings::CONCERTS_CREDENTIAL_UNVERIFIED)
    );
}

#[test]
fn conc_9_ticketmaster_build_credential_is_not_user_editable() {
    let credentials = credential_preference_specs();

    assert_eq!(credentials.len(), 1);
    assert_eq!(
        credentials[0].provider,
        reprise_core::concerts::ProviderKind::Bandsintown
    );
    assert_eq!(
        credentials[0].key,
        reprise_core::concerts::config::BANDSINTOWN_APP_ID_KEY
    );
}

#[test]
fn stored_credentials_are_preferred_and_similar_count_clamps() {
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
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
fn concerts_preferences_expose_only_bandsintown_and_link_similar_sensitivity() {
    gtk4::init().unwrap();
    let _main_context = crate::ui::test_main_context::lock_main_context();
    let conn = reprise_core::db::open(None).unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    let conn = Rc::new(RefCell::new(conn));
    let runtime = ConcertsRuntime::setup(&conn.borrow());
    let preferences = build(&conn, &runtime, true);

    assert!(preferences.inner.rows[0].is::<adw::PasswordEntryRow>());
    assert!(!preferences.inner.rows[1].is::<adw::PasswordEntryRow>());
    assert_eq!(preferences.inner.credentials.len(), 1);
    assert!(!preferences.inner.similar_count.is_sensitive());
    preferences.inner.similar_enabled.set_active(true);
    assert!(preferences.inner.similar_count.is_sensitive());
    preferences.set_sensitive(false);
    assert!(!preferences.inner.similar_count.is_sensitive());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn set_4_concert_credentials_expose_apply_and_inline_status() {
    gtk4::init().unwrap();
    let conn = Rc::new(RefCell::new(Connection::open_in_memory().unwrap()));
    reprise_core::db::migrate(&conn.borrow()).unwrap();
    let runtime = ConcertsRuntime::setup(&conn.borrow());
    let preferences = build(&conn, &runtime, true);
    let credential = &preferences.inner.credentials[0];

    assert!(credential.row.shows_apply_button());
    assert!(!credential.status.is_visible());
    assert_eq!(
        credential.status.accessible_role(),
        gtk4::AccessibleRole::Status
    );
}
