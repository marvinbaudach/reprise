use super::*;

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
    let conn = crate::test_db::open().unwrap();
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
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
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
    let conn = Rc::new(crate::test_db::open().unwrap());
    let runtime = ConcertsRuntime::setup(&conn);
    let preferences = build(&conn, &runtime, true);
    let credential = &preferences.inner.credentials[0];

    assert!(credential.row.shows_apply_button());
    assert!(!credential.status.is_visible());
    assert_eq!(
        credential.status.accessible_role(),
        gtk4::AccessibleRole::Status
    );
}
