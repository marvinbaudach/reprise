use super::*;
use chrono::TimeZone;
use reprise_core::device_sync::PreparationPhase;

#[test]
fn mtp_43_preparation_overview_is_absent_for_absent_and_nothing_missing() {
    assert_eq!(preparation_overview(&PreparationPhase::Absent), None);
    assert_eq!(
        preparation_overview(&PreparationPhase::NothingMissing),
        None
    );
}

#[test]
fn mtp_43_preparation_overview_names_files_and_size_when_offered_or_planned() {
    assert_eq!(
        preparation_overview(&PreparationPhase::Offered {
            files: 2,
            bytes: 312 * 1024 * 1024,
        }),
        Some("2 files to download · 312.0 MiB".to_string())
    );
    assert_eq!(
        preparation_overview(&PreparationPhase::Planned {
            files: 1,
            bytes: 1024,
        }),
        Some("1 file to download · 1.0 KiB".to_string())
    );
}

#[test]
fn mtp_43_preparation_overview_omits_a_bogus_zero_byte_figure() {
    assert_eq!(
        preparation_overview(&PreparationPhase::Planned { files: 3, bytes: 0 }),
        Some("3 files to download".to_string())
    );
}

#[test]
fn mtp_43_preparation_titles_name_a_few_and_count_the_rest() {
    assert_eq!(preparation_titles(&[]), None);
    assert_eq!(
        preparation_titles(&["Abendrot", "Nachtwind"]),
        Some("Abendrot, Nachtwind".to_string())
    );
    assert_eq!(
        preparation_titles(&["One", "Two", "Three"]),
        Some("One, Two, Three".to_string())
    );
    assert_eq!(
        preparation_titles(&["One", "Two", "Three", "Four", "Five"]),
        Some("One, Two, Three … and 2 more".to_string())
    );
}

#[test]
fn mtp_43_preparation_overview_reads_skipped_episodes_while_offline() {
    assert_eq!(
        preparation_overview(&PreparationPhase::SkippedOffline { files: 2 }),
        Some("2 episodes skipped · not downloaded".to_string())
    );
    assert_eq!(
        preparation_overview(&PreparationPhase::SkippedOffline { files: 1 }),
        Some("1 episode skipped · not downloaded".to_string())
    );
}

#[test]
fn mtp_43_preparation_step_progress_is_always_step_one_of_two() {
    assert_eq!(
        preparation_step_progress(0, 2, 62),
        "Step 1 of 2 · Downloading 1 of 2 · 62%"
    );
    assert_eq!(
        preparation_step_progress(1, 2, 5),
        "Step 1 of 2 · Downloading 2 of 2 · 5%"
    );
}

#[test]
fn mtp_43_two_phase_title_prefixes_the_transfer_title_as_step_two() {
    assert_eq!(
        two_phase_title("Copying · 3 of 10"),
        "Step 2 of 2 · Copying · 3 of 10"
    );
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
        files_waiting_for_download: 0,
        playlists_rewritten: 0,
    };

    assert_eq!(balance_text(&balance), "3 to remove");
}

#[test]
fn free_space_line_shows_the_arrow_only_when_this_sync_moves_the_needle() {
    const GIB: u64 = 1024 * 1024 * 1024;
    assert_eq!(
        free_space_line(175 * GIB, (172.4 * GIB as f64) as u64),
        "175.0 → 172.4 GiB free"
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
