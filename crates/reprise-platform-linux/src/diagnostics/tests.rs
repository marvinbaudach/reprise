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
        log_tail: None,
        previous_log_tail: None,
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
fn fb_19_the_platform_report_lists_modules_and_the_supplied_log_tail() {
    let db = Db::open_in_memory().unwrap();
    let input = DesktopDiagnosticInput {
        version: "9.8.7".into(),
        git_sha: None,
        build_profile: "release".into(),
        app_id: "org.example.Reprise".into(),
        display_server: None,
        gtk_version: "4.20.1".into(),
        libadwaita_version: "1.8.2".into(),
        rust_version: None,
        log_tail: Some("INFO reprise: started".into()),
        previous_log_tail: None,
    };

    let report = build_report(
        &db,
        Path::new("/missing/reprise.db"),
        &input,
        &DiagnosticLog::default(),
    );

    assert!(report.contains("\nmodules "));
    assert!(report.contains("log, this run\nINFO reprise: started"));
    assert!(!report.contains("log, previous run"));
}

#[test]
fn a_module_switched_off_is_not_listed_as_enabled() {
    let db = Db::open_in_memory().unwrap();
    reprise_core::modules::set_enabled(&db, &reprise_core::modules::SONG_VISUALS_MODULE, false)
        .unwrap();

    let ids = super::enabled_module_ids(&db).unwrap();

    assert!(!ids.contains(&"song_visuals".to_string()));
    assert!(ids.contains(&"library_doctor".to_string()));
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
fn os_release_parser_uses_build_id_when_version_id_is_empty() {
    let release = parse_os_release("ID=manjaro\nVERSION_ID=\"\"\nBUILD_ID=rolling\n");

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
