use std::path::Path;

use reprise_core::db::Db;
use reprise_core::diagnostics::DiagnosticLog;

use super::{
    build_report, parse_gnome_version, parse_gvfs_version, parse_os_release, DesktopDiagnosticInput,
};

#[test]
fn linux_report_collection_keeps_frontend_runtime_facts() {
    let db = Db::open_in_memory().unwrap();
    let input = DesktopDiagnosticInput {
        version: "9.8.7".into(),
        git_sha: Some("abc123".into()),
        build_profile: "release".into(),
        app_id: "org.example.Reprise".into(),
        display_server: Some("x11".into()),
        gtk_version: "4.20.1".into(),
        libadwaita_version: "1.8.2".into(),
        rust_version: Some("1.91".into()),
    };

    let report = build_report(
        &db,
        Path::new("/missing/reprise.db"),
        &input,
        &DiagnosticLog::default(),
    );

    assert_eq!(
        report.lines().next(),
        Some("reprise 9.8.7 (abc123, release)")
    );
    assert!(report.contains("gtk 4.20.1 · libadwaita 1.8.2"));
    assert!(report.contains("rust 1.91 · gstreamer "));
    assert!(report.contains("os "));
    assert!(report.contains("db schema "));
}

#[test]
fn os_release_parser_prefers_id_and_unquotes_values() {
    let release = parse_os_release(
        "NAME=\"Fedora Linux\"\nID=fedora\nVERSION_ID=\"43\"\nBUILD_ID=Rawhide\nPRETTY_NAME=ignored\n",
    );

    assert_eq!(release.name.as_deref(), Some("fedora"));
    assert_eq!(release.version.as_deref(), Some("43"));
}

#[test]
fn os_release_parser_uses_build_id_when_version_id_is_missing() {
    let release = parse_os_release("ID=manjaro\nBUILD_ID=rolling\n");

    assert_eq!(release.name.as_deref(), Some("manjaro"));
    assert_eq!(release.version.as_deref(), Some("rolling"));
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
fn gvfs_version_parser_accepts_the_daemon_version_only() {
    assert_eq!(parse_gvfs_version("gvfs 1.60.2\n"), Some("1.60.2".into()));
    assert_eq!(parse_gvfs_version("gvfs unavailable"), None);
}
