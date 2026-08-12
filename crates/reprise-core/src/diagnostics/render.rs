use std::fmt::Write;

use super::model::{DiagnosticFacts, DiagnosticLog, PackageKind, RedactionContext};
use super::redact::redact_log_message;

const RENDERED_EVENT_LIMIT: usize = 10;
const UNKNOWN: &str = "unknown";

pub fn render_report(
    facts: &DiagnosticFacts,
    log: &DiagnosticLog,
    redaction: &RedactionContext,
) -> String {
    let mut report = String::new();
    let version = optional(&facts.version);
    let git_sha = optional(&facts.git_sha);
    let profile = optional(&facts.build_profile);
    writeln!(report, "reprise {version} ({git_sha}, {profile})").unwrap();
    writeln!(report, "{}", package_line(facts.package.as_ref())).unwrap();
    writeln!(
        report,
        "os {} {} · gnome {} · {}",
        optional(&facts.os_name),
        optional(&facts.os_version),
        optional(&facts.gnome_version),
        optional(&facts.display_server)
    )
    .unwrap();
    writeln!(
        report,
        "gtk {} · libadwaita {}",
        optional(&facts.gtk_version),
        optional(&facts.libadwaita_version)
    )
    .unwrap();
    writeln!(
        report,
        "rust {} · gstreamer {} ({})",
        optional(&facts.rust_version),
        optional(&facts.gstreamer_version),
        optional(&facts.audio_backend)
    )
    .unwrap();
    writeln!(report, "locale {}", optional(&facts.locale)).unwrap();
    writeln!(
        report,
        "db schema {} · {} · {} tracks · {}",
        optional_display(facts.db_schema),
        optional(&facts.db_journal_mode),
        optional_display(facts.track_count),
        database_size(facts.db_size_bytes)
    )
    .unwrap();
    write!(
        report,
        "mtp gvfs {} · {}",
        optional(&facts.gvfs_version),
        remembered_devices(facts.remembered_device_count)
    )
    .unwrap();

    if !log.is_empty() {
        report.push_str("\n\nlast warnings\n");
        for (index, event) in log.latest(RENDERED_EVENT_LIMIT).enumerate() {
            let seconds = event.seconds_since_midnight % 86_400;
            let hours = seconds / 3_600;
            let minutes = seconds % 3_600 / 60;
            let seconds = seconds % 60;
            let target = redact_log_message(&event.target, redaction);
            let target = final_target_segment(&target);
            let message = redact_log_message(&event.message, redaction);
            let _level = event.level;
            write!(
                report,
                "{hours:02}:{minutes:02}:{seconds:02} {target}: {message}"
            )
            .unwrap();
            if index + 1 < log.len().min(RENDERED_EVENT_LIMIT) {
                report.push('\n');
            }
        }
    }
    report
}

fn final_target_segment(target: &str) -> &str {
    target.rsplit("::").next().unwrap_or(target)
}

fn optional(value: &Option<String>) -> &str {
    value
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or(UNKNOWN)
}

fn optional_display(value: Option<impl std::fmt::Display>) -> String {
    value.map_or_else(|| UNKNOWN.into(), |value| value.to_string())
}

fn package_line(package: Option<&PackageKind>) -> String {
    match package {
        Some(PackageKind::Native) => "native".into(),
        Some(PackageKind::Flatpak { app_id }) => format!("flatpak {}", optional(app_id)),
        None => UNKNOWN.into(),
    }
}

fn database_size(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || UNKNOWN.into(),
        |bytes| format!("{:.1} MiB", bytes as f64 / 1_048_576.0),
    )
}

fn remembered_devices(count: Option<usize>) -> String {
    match count {
        Some(1) => "1 device remembered".into(),
        Some(count) => format!("{count} devices remembered"),
        None => "unknown devices remembered".into(),
    }
}
