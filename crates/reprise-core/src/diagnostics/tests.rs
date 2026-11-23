use super::{
    render_report, DiagnosticEvent, DiagnosticFacts, DiagnosticLevel, DiagnosticLog, PackageKind,
    DIAGNOSTIC_EVENT_CAPACITY,
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
        ..DiagnosticFacts::default()
    }
}

#[test]
fn missing_facts_render_as_unknown_in_every_fixed_slot() {
    let report = render_report(&DiagnosticFacts::default(), &DiagnosticLog::default());

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
    let report = render_report(&complete_facts(), &DiagnosticLog::default());

    assert!(!report.contains("last warnings"));
    assert!(report.ends_with("mtp gvfs 1.60.2 · 1 device remembered"));
}

#[test]
fn missing_distribution_version_omits_only_its_token() {
    let mut facts = complete_facts();
    facts.os_version = None;

    let report = render_report(&facts, &DiagnosticLog::default());

    assert!(report.contains("\nos fedora · gnome 49 · wayland\n"));
    assert!(!report.contains("os fedora unknown"));
}

#[test]
fn a_session_event_is_embedded_as_stored_with_its_call_site() {
    let mut log = DiagnosticLog::default();
    log.push(
        DiagnosticEvent::new(
            9 * 3_600 + 25 * 60 + 14,
            DiagnosticLevel::Warn,
            "reprise_core::library::watcher",
            "could not move the previous log aside; error=$REDACTED count=3",
        )
        .with_location(Some("crates/reprise-core/src/run_log.rs:42")),
    );
    log.push(DiagnosticEvent::new(
        9 * 3_600 + 25 * 60 + 15,
        DiagnosticLevel::Error,
        "foreign_transport::connection",
        "…",
    ));

    let report = render_report(&complete_facts(), &log);

    // The newest event comes first; a foreign one has no location.
    assert!(
        report.ends_with(
            "\n\nlast warnings\n\
             09:25:15 connection: …\n\
             09:25:14 watcher: could not move the previous log aside; error=$REDACTED count=3 \
             @ crates/reprise-core/src/run_log.rs:42"
        ),
        "{report}"
    );
}

#[test]
fn the_report_does_not_search_what_it_embeds() {
    // What was stored is safe by construction, so the renderer leaves it as
    // it is: a path-shaped or secret-shaped word is not its business.
    let mut log = DiagnosticLog::default();
    log.push(DiagnosticEvent::new(
        1,
        DiagnosticLevel::Warn,
        "scanner",
        "see /srv/archive/x.mp3 and https://example.org/a token=abc",
    ));

    let report = render_report(&complete_facts(), &log);

    assert!(
        report.ends_with(
            "00:00:01 scanner: see /srv/archive/x.mp3 and https://example.org/a token=abc"
        ),
        "{report}"
    );
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

    let report = render_report(&complete_facts(), &log);
    let lines: Vec<_> = report
        .split("last warnings\n")
        .nth(1)
        .unwrap()
        .lines()
        .collect();

    assert_eq!(
        lines,
        [
            "00:00:03 trailing separator",
            "00:00:02 empty target",
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
    let report = render_report(&complete_facts(), &log);
    let rendered_events = report.split("last warnings\n").nth(1).unwrap();
    let lines: Vec<_> = rendered_events.lines().collect();

    assert_eq!(lines.len(), 10);
    assert!(lines[0].ends_with("worker: event-204"));
    assert!(lines[9].ends_with("worker: event-195"));
    assert!(!report.contains("event-194"));
}

#[test]
fn an_event_made_from_a_safe_event_carries_its_target_body_and_call_site() {
    use super::{safe_event, Callsite, LogLevel, RecordedEvent};

    let callsite = Callsite {
        file: Some("crates/reprise-core/src/run_log.rs"),
        line: Some(42),
        target: "reprise_core::run_log",
        level: LogLevel::Error,
    };
    let recorded = RecordedEvent {
        message: Some("zqxuser wrote /home/zqxuser/secret".to_owned()),
        fields: Vec::new(),
    };
    let safe = safe_event(&callsite, &recorded);

    let event = DiagnosticEvent::from_safe(&safe, 9 * 3_600 + 25 * 60 + 14);

    assert_eq!(event.level, DiagnosticLevel::Error);
    assert_eq!(event.target, "reprise_core::run_log");
    assert_eq!(event.message, safe.body());
    assert_eq!(event.location.as_deref(), safe.location());
    assert!(!event.message.contains("zqxuser"));
}
