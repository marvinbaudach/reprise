use std::path::Path;

use reprise_core::db::Db;
use reprise_core::diagnostics::DiagnosticLog;

use super::{
    build_report, gnome_version_from, parse_flatpak_gnome_runtime, parse_gnome_version,
    parse_gvfs_version, parse_os_release, plain_audio_sink, plain_locale, DesktopDiagnosticInput,
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

#[test]
fn flatpak_runtime_parser_names_the_gnome_platform_version() {
    let info = "[Application]\nname=io.github.marvinbaudach.Reprise\n\
runtime=runtime/org.gnome.Platform/x86_64/50\nsdk=runtime/org.gnome.Sdk/x86_64/50\n";

    assert_eq!(
        parse_flatpak_gnome_runtime(info),
        Some("50 (Flatpak runtime)".into())
    );
}

#[test]
fn flatpak_runtime_parser_ignores_other_runtimes_and_other_sections() {
    assert_eq!(
        parse_flatpak_gnome_runtime("[Application]\nruntime=runtime/org.kde.Platform/x86_64/6.9\n"),
        None
    );
    assert_eq!(
        parse_flatpak_gnome_runtime("[Instance]\nruntime=runtime/org.gnome.Platform/x86_64/50\n"),
        None
    );
    assert_eq!(
        parse_flatpak_gnome_runtime("[Application]\nruntime=runtime/org.gnome.Platform/x86_64/\n"),
        None
    );
    assert_eq!(parse_flatpak_gnome_runtime(""), None);
}

#[test]
fn the_host_shell_version_wins_over_the_flatpak_runtime() {
    assert_eq!(
        gnome_version_from(Some("49.1"), Some("50 (Flatpak runtime)")),
        Some("49.1".into())
    );
    assert_eq!(
        gnome_version_from(None, Some("50 (Flatpak runtime)")),
        Some("50 (Flatpak runtime)".into())
    );
    assert_eq!(gnome_version_from(None, None), None);
}

// The template table is keyed by `file!()` as written from the workspace root
// (`reprise-core` pins the reasoning). A build that changes the form would turn
// every message of this crate into `…` in the report, so the form is pinned.
#[test]
fn a_call_sites_file_has_the_template_tables_path_form() {
    assert_eq!(
        file!(),
        "crates/reprise-platform-linux/src/diagnostics/tests.rs"
    );
}

// The facts header is the one place the report prints environment text, so a
// value is shown only in the shape it is meant to have (FB-19).
#[test]
fn a_locale_is_shown_only_in_the_shape_of_a_locale() {
    for plain in [
        "en",
        "de_DE",
        "de_DE.UTF-8",
        "en_US.utf8",
        "sr_RS@latin",
        "ca_ES.UTF-8@valencia",
        "ast",
        "C",
        "POSIX",
        "C.UTF-8",
    ] {
        assert_eq!(plain_locale(plain), plain);
    }
    for needle in [
        "zqx_Marvin",
        "/home/zqxuser/locale",
        "marvin",
        "zqxuser@example.org",
        "en_US.UTF-8 zqxuser",
        "de_de",
        "de_DEU",
        "en_US.",
        "en_US@",
        "en_US@Latin",
        "e",
        "abcd",
        "C_US",
        "",
    ] {
        assert_eq!(plain_locale(needle), "unknown", "{needle:?}");
    }
}

#[test]
fn an_audio_sink_is_shown_only_when_it_is_a_known_sink_factory() {
    for known in [
        "fakesink",
        "autoaudiosink",
        "pulsesink",
        "pipewiresink",
        "alsasink",
    ] {
        assert_eq!(plain_audio_sink(known), known);
    }
    for needle in [
        "/home/zqxuser/sink",
        "zqxuser",
        "fakesink sync=false",
        "filesink location=/home/zqxuser/x",
        "FakeSink",
        "",
    ] {
        assert_eq!(plain_audio_sink(needle), "custom", "{needle:?}");
    }
}
