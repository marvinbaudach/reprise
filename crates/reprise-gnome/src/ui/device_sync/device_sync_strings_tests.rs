use super::*;
use chrono::TimeZone;
use reprise_core::device_sync::SyncStep;

#[test]
fn step_glyph_uses_one_sync_vocabulary() {
    assert_eq!(step_glyph(&SyncStep::Transcoding), "⟳ transcoding ·");
    assert_eq!(step_glyph(&SyncStep::Copying), "↑");
    assert_eq!(step_glyph(&SyncStep::WritingAnalysis), "↑ analysis ·");
    assert_eq!(step_glyph(&SyncStep::WritingLyrics), "↑ lyrics ·");
    assert_eq!(step_glyph(&SyncStep::Removing), "− removing ·");
    assert_eq!(step_glyph(&SyncStep::WritingPlaylists), "≡");
    assert_eq!(step_glyph(&SyncStep::WritingTrackMetadata), "≡ metadata ·");
}

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
fn retired_media_notice_names_the_configured_target_folder() {
    assert_eq!(
        legacy_media_notice("/Audio/Reprise"),
        "Podcast and YouTube files are no longer synced and were left untouched outside /Audio/Reprise."
    );
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
        playlists_rewritten: 2,
    };

    assert_eq!(
        balance_text(&balance),
        "14 files to copy · 2.6 GiB · 3 to remove · 2 playlists to update"
    );
}

#[test]
fn mtp_22_deletions_only_balance_never_claims_zero_bytes_to_copy() {
    use reprise_core::device_sync::SyncBalance;

    let balance = SyncBalance {
        files_to_copy: 0,
        bytes_to_copy: 0,
        files_to_remove: 3,
        bytes_freed: 148 * 1024 * 1024,
        playlists_rewritten: 0,
    };

    assert_eq!(balance_text(&balance), "3 to remove");
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
