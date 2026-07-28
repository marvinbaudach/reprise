//! Translatable copy and compact formatting for Android synchronization.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

pub const EJECT_DEVICE: &str = N_!("Eject device");
pub const OPEN_DEVICE: &str = N_!("Open {name}");
pub const EJECT_BLOCKED_SYNCING: &str = N_!("Eject device — Sync in progress");

/// Spinner tooltip while syncing, e.g. "Syncing Pixel 8 · 42%".
pub fn syncing_spinner_tooltip(name: &str, percent: u64) -> String {
    let percent = percent.to_string();
    formatted(
        N_!("Syncing {name} · {percent}%"),
        &[("name", name), ("percent", &percent)],
    )
}

pub fn open_device_label(name: &str) -> String {
    formatted(OPEN_DEVICE, &[("name", name)])
}

/// TIP-2a: a disabled eject keeps its tooltip and appends the reason.
pub fn eject_tooltip(syncing: bool) -> String {
    text(if syncing {
        EJECT_BLOCKED_SYNCING
    } else {
        EJECT_DEVICE
    })
}

fn formatted(message: &str, values: &[(&str, &str)]) -> String {
    crate::i18n::format_message(&text(message), values)
}

pub const SPACE_UNKNOWN: &str = N_!("Available space unknown");
pub const SYNC_PROGRESS: &str = N_!("Synchronization Progress");

pub fn available_space(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || text(SPACE_UNKNOWN),
        |bytes| {
            let size = format_bytes(bytes);
            formatted(N_!("{size} available"), &[("size", &size)])
        },
    )
}

pub fn free_space(bytes: Option<u64>) -> String {
    bytes.map_or_else(
        || text(SPACE_UNKNOWN),
        |bytes| {
            let size = format_bytes(bytes);
            formatted(N_!("{size} free"), &[("size", &size)])
        },
    )
}

pub fn track_progress(completed: usize, total: usize) -> String {
    let completed = completed.to_string();
    let total = total.to_string();
    formatted(
        N_!("{completed} of {total} tracks"),
        &[("completed", &completed), ("total", &total)],
    )
}

/// Rough USB throughput used for the remaining-time hint — the same
/// assumption the delta card's estimate is built on, so both agree.
const ESTIMATED_BYTES_PER_SECOND: u64 = 5 * 1_024 * 1_024;

/// `98 %` — kept in its own label (never folded into the subtitle) so the
/// track text beside it cannot make the number jump around.
pub fn sync_percent(bytes_done: u64, bytes_total: u64) -> String {
    let percent = bytes_done
        .saturating_mul(100)
        .checked_div(bytes_total)
        .unwrap_or(0)
        .min(100);
    formatted(N_!("{percent} %"), &[("percent", &percent.to_string())])
}

/// `↑ Immortal — Lorna Shore` — the prefix says what is happening to the
/// named track and the runtime supplies both title and artist.
pub fn sync_activity(step: &str, current_track: &str) -> String {
    if current_track.is_empty() {
        return step.to_string();
    }
    format!("{step} {current_track}")
}

/// `28 of 82 · 340.0 MiB of 1.2 GiB · ~2 min left · Immortal`
pub fn sync_tooltip(
    done: u32,
    total: u32,
    bytes_done: u64,
    bytes_total: u64,
    current_track: &str,
) -> String {
    let mut parts = vec![
        track_progress(done as usize, total as usize),
        formatted(
            N_!("{done} of {total}"),
            &[
                ("done", &format_bytes(bytes_done)),
                ("total", &format_bytes(bytes_total)),
            ],
        ),
    ];
    if let Some(remaining) = remaining_hint(bytes_done, bytes_total) {
        parts.push(remaining);
    }
    if !current_track.is_empty() {
        parts.push(current_track.to_string());
    }
    parts.join(" · ")
}

fn remaining_hint(bytes_done: u64, bytes_total: u64) -> Option<String> {
    let remaining = bytes_total
        .checked_sub(bytes_done)
        .filter(|left| *left > 0)?;
    let seconds = remaining.div_ceil(ESTIMATED_BYTES_PER_SECOND);
    let text = if seconds >= 60 {
        let minutes = seconds.div_ceil(60);
        formatted(
            N_!("~{minutes} min left"),
            &[("minutes", &minutes.to_string())],
        )
    } else {
        formatted(
            N_!("~{seconds} s left"),
            &[("seconds", &seconds.max(1).to_string())],
        )
    };
    Some(text)
}

pub fn file_size(bytes: u64) -> String {
    format_bytes(bytes)
}

/// Design 7a: "Playlists" / "YouTube audio" / "Podcast episodes" — the
/// Content section and Next synchronization panel's category labels.
pub fn category_name(kind: reprise_core::device_sync::SyncTargetKind) -> &'static str {
    use reprise_core::device_sync::SyncTargetKind;
    match kind {
        SyncTargetKind::Playlists => "Playlists",
        SyncTargetKind::YoutubeAudio => "YouTube audio",
        SyncTargetKind::PodcastEpisodes => "Podcast episodes",
    }
}

/// Design 7a's cap column: "Cap 8.0 GiB" or "No cap".
pub fn cap_text(cap_bytes: Option<u64>) -> String {
    cap_bytes.map_or_else(
        || "No cap".to_string(),
        |bytes| format!("Cap {}", file_size(bytes)),
    )
}

/// `MTP-22`'s exact vocabulary for one category's `CategoryReading` — used
/// both by the Next synchronization panel's per-category row and (via
/// [`balance_text`]) by the sidebar card's full-balance tooltip, so the two
/// surfaces never drift into different wording for the same numbers.
pub fn category_reading_text(reading: &reprise_core::device_sync::CategoryReading) -> String {
    use reprise_core::device_sync::CategoryReading;
    match reading {
        CategoryReading::SourceOff => "Source off".to_string(),
        CategoryReading::UnavailableKeptOnPhone => "Unavailable, kept on phone".to_string(),
        CategoryReading::Diff(diff) => balance_parts(
            diff.files_to_copy,
            diff.bytes_to_copy,
            diff.files_to_remove,
            diff.bytes_freed,
            diff.playlists_rewritten,
        ),
    }
}

/// `MTP-22`'s aggregate balance line — "To copy 14 files · 2.6 GiB", "To
/// remove 3 files · 148 MiB", "Playlists rewritten 2".
pub fn balance_text(balance: &reprise_core::device_sync::SyncBalance) -> String {
    balance_parts(
        balance.files_to_copy,
        balance.bytes_to_copy,
        balance.files_to_remove,
        balance.bytes_freed,
        balance.playlists_rewritten,
    )
}

fn balance_parts(
    files_to_copy: usize,
    bytes_to_copy: u64,
    files_to_remove: usize,
    bytes_freed: u64,
    playlists_rewritten: usize,
) -> String {
    let mut parts = Vec::new();
    if files_to_copy > 0 {
        parts.push(format!(
            "To copy {} · {}",
            counted_files(files_to_copy),
            file_size(bytes_to_copy)
        ));
    }
    if files_to_remove > 0 {
        parts.push(format!(
            "To remove {} · {}",
            counted_files(files_to_remove),
            file_size(bytes_freed)
        ));
    }
    if playlists_rewritten > 0 {
        parts.push(format!("Playlists rewritten {playlists_rewritten}"));
    }
    if parts.is_empty() {
        return "Nothing pending".to_string();
    }
    parts.join(" · ")
}

fn counted_files(count: usize) -> String {
    if count == 1 {
        "1 file".to_string()
    } else {
        format!("{count} files")
    }
}

/// Design 7a: "175.0 GiB free → 172.4 GiB after this sync". Collapses to a
/// single figure when this sync would not move the free-space needle at
/// all, so a device with nothing pending never shows a pointless arrow to
/// the same number.
pub fn free_space_line(free_before_bytes: u64, free_after_bytes: u64) -> String {
    if free_before_bytes == free_after_bytes {
        return format!("{} free", file_size(free_before_bytes));
    }
    format!(
        "{} free \u{2192} {} after this sync",
        file_size(free_before_bytes),
        file_size(free_after_bytes)
    )
}

/// Design 7c: "synced 12 min ago". Coarse buckets are deliberate — the
/// sidebar card is a glance surface, not a log.
pub fn relative_time(
    now: chrono::DateTime<chrono::Utc>,
    then: chrono::DateTime<chrono::Utc>,
) -> String {
    let minutes = now.signed_duration_since(then).num_minutes().max(0);
    if minutes < 1 {
        "just now".to_string()
    } else if minutes < 60 {
        format!("{minutes} min ago")
    } else if minutes < 24 * 60 {
        format!("{} h ago", minutes / 60)
    } else {
        format!("{} d ago", minutes / (24 * 60))
    }
}

fn format_bytes(bytes: u64) -> String {
    const KIB: f64 = 1_024.0;
    const MIB: f64 = KIB * 1_024.0;
    const GIB: f64 = MIB * 1_024.0;
    let bytes = bytes as f64;
    if bytes >= GIB {
        format!("{:.1} GiB", bytes / GIB)
    } else if bytes >= MIB {
        format!("{:.1} MiB", bytes / MIB)
    } else if bytes >= KIB {
        format!("{:.1} KiB", bytes / KIB)
    } else {
        format!("{} B", bytes as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn byte_formatting_uses_compact_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1_024), "1.0 KiB");
        assert_eq!(format_bytes(2 * 1_024 * 1_024), "2.0 MiB");
        assert_eq!(free_space(Some(2 * 1_024 * 1_024)), "2.0 MiB free");
    }

    #[test]
    fn tip_2a_eject_tooltip_names_reason_while_syncing() {
        assert_eq!(eject_tooltip(true), "Eject device — Sync in progress");
        assert_eq!(eject_tooltip(false), "Eject device");
    }

    #[test]
    fn device_open_action_names_its_target() {
        assert_eq!(open_device_label("Pixel 8"), "Open Pixel 8");
    }

    #[test]
    fn track_progress_keeps_both_counts_visible() {
        assert_eq!(track_progress(2, 5), "2 of 5 tracks");
    }

    #[test]
    fn mtp_22_balance_text_matches_the_designs_exact_vocabulary() {
        use reprise_core::device_sync::SyncBalance;

        let balance = SyncBalance {
            files_to_copy: 14,
            bytes_to_copy: (2.6 * 1024.0 * 1024.0 * 1024.0) as u64,
            files_to_remove: 3,
            bytes_freed: 148 * 1024 * 1024,
            files_waiting_for_download: 0,
            playlists_rewritten: 2,
        };

        assert_eq!(
            balance_text(&balance),
            "To copy 14 files · 2.6 GiB · To remove 3 files · 148.0 MiB · Playlists rewritten 2"
        );
    }

    #[test]
    fn mtp_22_deletions_only_balance_reads_frees_not_zero_bytes_to_copy() {
        use reprise_core::device_sync::SyncBalance;

        let balance = SyncBalance {
            files_to_copy: 0,
            bytes_to_copy: 0,
            files_to_remove: 3,
            bytes_freed: 148 * 1024 * 1024,
            files_waiting_for_download: 0,
            playlists_rewritten: 0,
        };

        assert_eq!(balance_text(&balance), "To remove 3 files · 148.0 MiB");
    }

    #[test]
    fn cap_text_names_the_cap_or_says_there_is_none() {
        assert_eq!(cap_text(Some(8 * 1024 * 1024 * 1024)), "Cap 8.0 GiB");
        assert_eq!(cap_text(None), "No cap");
    }

    #[test]
    fn free_space_line_shows_the_arrow_only_when_this_sync_moves_the_needle() {
        const GIB: u64 = 1024 * 1024 * 1024;
        assert_eq!(
            free_space_line(175 * GIB, (172.4 * GIB as f64) as u64),
            "175.0 GiB free \u{2192} 172.4 GiB after this sync"
        );
        assert_eq!(free_space_line(64 * GIB, 64 * GIB), "64.0 GiB free");
    }

    #[test]
    fn relative_time_buckets_minutes_hours_and_days() {
        let now = chrono::Utc.with_ymd_and_hms(2026, 7, 28, 12, 0, 0).unwrap();
        assert_eq!(
            relative_time(now, now - chrono::Duration::seconds(10)),
            "just now"
        );
        assert_eq!(
            relative_time(now, now - chrono::Duration::minutes(12)),
            "12 min ago"
        );
        assert_eq!(
            relative_time(now, now - chrono::Duration::hours(3)),
            "3 h ago"
        );
        assert_eq!(
            relative_time(now, now - chrono::Duration::days(2)),
            "2 d ago"
        );
    }
}
