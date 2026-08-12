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
        gvfs_version: Some("1.60.2".into()),
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
os unknown · gnome unknown · unknown\n\
gtk unknown · libadwaita unknown\n\
rust unknown · gstreamer unknown (unknown)\n\
locale unknown\n\
db schema unknown · unknown · unknown tracks · unknown\n\
mtp gvfs unknown · unknown devices remembered"
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
    assert!(report.ends_with("mtp gvfs 1.60.2 · 1 device remembered"));
}

#[test]
fn missing_distribution_version_omits_only_its_token() {
    let mut facts = complete_facts();
    facts.os_version = None;

    let report = render_report(
        &facts,
        &DiagnosticLog::default(),
        &RedactionContext::default(),
    );

    assert!(report.contains("\nos fedora · gnome 49 · wayland\n"));
    assert!(!report.contains("os fedora unknown"));
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
fn rendered_events_redact_every_absolute_path_without_losing_sentence_context() {
    let mut log = DiagnosticLog::default();
    log.push(DiagnosticEvent::new(
        8 * 3_600 + 14 * 60 + 22,
        DiagnosticLevel::Warn,
        "device_sync",
        "device path:/run/media/marvin/BACKUP_DRIVE mounted; inspected `/srv/archive/private`, then moved /var/tmp/x.mp3 to /opt/share/y.mp3 successfully",
    ));

    let report = render_report(&complete_facts(), &log, &RedactionContext::default());

    for private in [
        "/run/media",
        "BACKUP_DRIVE",
        "/srv/archive",
        "private",
        "/var/tmp",
        "x.mp3",
        "/opt/share",
        "y.mp3",
    ] {
        assert!(
            !report.contains(private),
            "report leaked {private:?}:\n{report}"
        );
    }
    assert!(report.contains("path:… mounted"));
    assert!(report.contains("inspected `…"));
    assert!(report.contains("then moved … to … successfully"));
}

#[test]
fn rendered_events_redact_identifying_structured_fields() {
    let mut log = DiagnosticLog::default();
    log.push(DiagnosticEvent::new(
        8 * 3_600 + 14 * 60 + 22,
        DiagnosticLevel::Warn,
        "device_sync",
        "device_id=0123456789ABCDEF device_serial=SERIAL-PRIVATE playlist=Roadtrip playlist_name=SecretMix user_name=private-listener track_id=42",
    ));

    let report = render_report(&complete_facts(), &log, &RedactionContext::default());

    for private in [
        "0123456789ABCDEF",
        "SERIAL-PRIVATE",
        "Roadtrip",
        "SecretMix",
        "private-listener",
    ] {
        assert!(
            !report.contains(private),
            "report leaked {private:?}:\n{report}"
        );
    }
    assert!(report.contains("device_id=$REDACTED"));
    assert!(report.contains("playlist=$REDACTED"));
    assert!(report.contains("user_name=$REDACTED"));
    assert!(report.contains("track_id=42"));
}

#[test]
fn rendered_events_use_only_the_final_target_segment() {
    let mut log = DiagnosticLog::default();
    for (seconds, target, message) in [
        (
            0,
            "reprise::ui::track_list::track_list_menu_smoke",
            "nested target",
        ),
        (1, "scanner", "plain target"),
        (2, "", "empty target"),
        (3, "device_sync::", "trailing separator"),
    ] {
        log.push(DiagnosticEvent::new(
            seconds,
            DiagnosticLevel::Warn,
            target,
            message,
        ));
    }

    let report = render_report(&complete_facts(), &log, &RedactionContext::default());
    let lines: Vec<_> = report
        .split("last warnings\n")
        .nth(1)
        .unwrap()
        .lines()
        .collect();

    assert_eq!(
        lines,
        [
            "00:00:03 : trailing separator",
            "00:00:02 : empty target",
            "00:00:01 scanner: plain target",
            "00:00:00 track_list_menu_smoke: nested target",
        ]
    );
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
