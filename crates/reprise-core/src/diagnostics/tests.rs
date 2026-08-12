use super::{
    render_report, DiagnosticEvent, DiagnosticFacts, DiagnosticLevel, DiagnosticLog, PackageKind,
    RedactionContext, DIAGNOSTIC_EVENT_CAPACITY,
};

fn complete_facts() -> DiagnosticFacts {
    DiagnosticFacts {
        version: Some("0.1.1".into()),
        git_sha: Some("8d062859de".into()),
        build_profile: Some("release".into()),
        package: Some(PackageKind::Flatpak {
            app_id: Some("io.github.marvinbaudach.Reprise".into()),
        }),
        os_name: Some("fedora".into()),
        os_version: Some("43".into()),
        gnome_version: Some("49".into()),
        display_server: Some("wayland".into()),
        gtk_version: Some("4.20.1".into()),
        libadwaita_version: Some("1.8.2".into()),
        rust_version: Some("1.91".into()),
        gstreamer_version: Some("1.28".into()),
        audio_backend: Some("pipewire".into()),
        locale: Some("de_DE.UTF-8".into()),
        db_schema: Some(41),
        db_journal_mode: Some("wal".into()),
        track_count: Some(2_165),
        db_size_bytes: Some(19_293_798),
        libmtp_version: Some("1.1.22".into()),
        remembered_device_count: Some(1),
    }
}

#[test]
fn missing_facts_render_as_unknown_in_every_fixed_slot() {
    let report = render_report(
        &DiagnosticFacts::default(),
        &DiagnosticLog::default(),
        &RedactionContext::default(),
    );

    assert_eq!(
        report,
        "reprise unknown (unknown, unknown)\n\
unknown\n\
os unknown unknown · gnome unknown · unknown\n\
gtk unknown · libadwaita unknown\n\
rust unknown · gstreamer unknown (unknown)\n\
locale unknown\n\
db schema unknown · unknown · unknown tracks · unknown\n\
mtp libmtp unknown · unknown devices remembered"
    );
}

#[test]
fn empty_log_omits_the_last_warnings_block() {
    let report = render_report(
        &complete_facts(),
        &DiagnosticLog::default(),
        &RedactionContext::default(),
    );

    assert!(!report.contains("last warnings"));
    assert!(report.ends_with("mtp libmtp 1.1.22 · 1 device remembered"));
}

#[test]
fn rendered_events_do_not_expose_paths_uris_filenames_users_or_credentials() {
    let mut log = DiagnosticLog::default();
    log.push(DiagnosticEvent::new(
        9 * 3_600 + 25 * 60 + 14,
        DiagnosticLevel::Warn,
        "scanner",
        "alice failed /home/marvin/Music/Private Album/secret.flac, /home/marvin/.config/reprise/token.json, /var/tmp/private.log, file:///home/marvin/Music/private.flac, https://example.org/private?id=alice, cover-secret.jpg, lastfm_username=private-listener, access_token=hunter2",
    ));
    let context = RedactionContext {
        music_dir: Some("/home/marvin/Music".into()),
        home_dir: Some("/home/marvin".into()),
        username: Some("alice".into()),
    };

    let report = render_report(&complete_facts(), &log, &context);

    for private in [
        "/home/marvin/Music",
        "/home/marvin",
        "/var/tmp/private.log",
        "private.flac",
        "token.json",
        "private.log",
        "cover-secret.jpg",
        "example.org",
        "private-listener",
        "hunter2",
        "alice",
    ] {
        assert!(
            !report.contains(private),
            "report leaked {private:?}:\n{report}"
        );
    }
    assert!(report.contains("$XDG_MUSIC_DIR/…"));
    assert!(report.contains("$HOME/…"));
    assert!(report.contains("file://…"));
    assert!(report.contains("https://…"));
    assert!(report.contains("$USER"));
}

#[test]
fn overflowing_log_keeps_capacity_and_renders_latest_ten_newest_first() {
    let mut log = DiagnosticLog::default();
    for sequence in 0..DIAGNOSTIC_EVENT_CAPACITY + 5 {
        log.push(DiagnosticEvent::new(
            sequence as u32,
            DiagnosticLevel::Error,
            "worker",
            format!("event-{sequence}"),
        ));
    }

    assert_eq!(log.len(), DIAGNOSTIC_EVENT_CAPACITY);
    let report = render_report(&complete_facts(), &log, &RedactionContext::default());
    let rendered_events = report.split("last warnings\n").nth(1).unwrap();
    let lines: Vec<_> = rendered_events.lines().collect();

    assert_eq!(lines.len(), 10);
    assert!(lines[0].ends_with("worker: event-204"));
    assert!(lines[9].ends_with("worker: event-195"));
    assert!(!report.contains("event-194"));
}
