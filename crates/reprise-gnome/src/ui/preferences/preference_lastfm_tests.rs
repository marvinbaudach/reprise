use super::*;

#[test]
fn authorization_steps_require_credentials_browser_then_confirmation() {
    assert_eq!(
        authorization_decision("", "secret", false),
        AuthorizationDecision::Configure
    );
    assert_eq!(
        authorization_decision("key", "secret", false),
        AuthorizationDecision::OpenBrowser
    );
    assert_eq!(
        authorization_decision("key", "secret", true),
        AuthorizationDecision::Exchange
    );
}

#[test]
fn connected_status_includes_lastfm_queued_count() {
    let text = status_text(&ConnectionStatus::Connected {
        user_name: "listener".to_string(),
        pending: 2,
        submitted: 0,
    });
    assert!(text.contains("listener"));
    assert!(text.contains('2'));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn expander_row_has_enable_switch_credentials_and_action_buttons() {
    gtk4::init().unwrap();
    let surface = build_lastfm_expander(false, false, "Not connected");
    assert!(surface.expander.shows_enable_switch());
    assert!(!surface.expander.enables_expansion());
    assert!(surface.api_key.is::<adw::PasswordEntryRow>());
    assert!(surface.shared_secret.is::<adw::PasswordEntryRow>());
    assert!(!surface.open_browser.is_sensitive());

    // Disconnect button's parent row is hidden when not connected
    assert!(surface.disconnect.parent().is_some_and(|p| !p.is_visible()));

    // Credentials gate Open Browser
    surface.api_key.set_text("key");
    assert!(!surface.open_browser.is_sensitive());
    surface.shared_secret.set_text("secret");
    assert!(surface.open_browser.is_sensitive());
    surface.api_key.set_text("  ");
    assert!(!surface.open_browser.is_sensitive());

    // When enabled + connected, body rows are sensitive and disconnect visible
    let enabled_surface = build_lastfm_expander(true, true, "Connected as listener");
    assert!(enabled_surface.expander.enables_expansion());
    assert!(enabled_surface.api_key.is_sensitive());
    assert!(enabled_surface
        .disconnect
        .parent()
        .is_some_and(|p| p.is_visible()));
}
