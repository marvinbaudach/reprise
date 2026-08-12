use std::sync::{Arc, Mutex};

use reprise_core::diagnostics::{render_report, DiagnosticFacts, DiagnosticLog, RedactionContext};
use tracing_subscriber::layer::SubscriberExt;

use super::{parse_gnome_version, parse_os_release, SessionDiagnosticLayer};

#[test]
fn os_release_parser_prefers_id_and_unquotes_values() {
    let release = parse_os_release(
        "NAME=\"Fedora Linux\"\nID=fedora\nVERSION_ID=\"43\"\nPRETTY_NAME=ignored\n",
    );

    assert_eq!(release.name.as_deref(), Some("fedora"));
    assert_eq!(release.version.as_deref(), Some("43"));
}

#[test]
fn gnome_version_parser_rejects_output_without_a_version() {
    assert_eq!(
        parse_gnome_version("GNOME Shell 49.1\n"),
        Some("49.1".into())
    );
    assert_eq!(parse_gnome_version("gnome-shell unavailable"), None);
}

#[test]
fn session_layer_keeps_warn_and_error_but_not_info() {
    let log = Arc::new(Mutex::new(DiagnosticLog::default()));
    let subscriber = tracing_subscriber::registry().with(SessionDiagnosticLayer::new(log.clone()));

    tracing::subscriber::with_default(subscriber, || {
        tracing::info!(ignored = 1, "ordinary progress");
        tracing::warn!(path = "/private/song.flac", "scan warning");
        tracing::error!(reason = "decoder", "playback failed");
    });

    let report = render_report(
        &DiagnosticFacts::default(),
        &log.lock().unwrap(),
        &RedactionContext::default(),
    );
    assert!(!report.contains("ordinary progress"));
    assert!(report.contains("scan warning"));
    assert!(report.contains("path="));
    assert!(report.contains("playback failed"));
    assert!(report.contains("reason="));
}
