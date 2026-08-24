use super::*;

fn migrated_conn() -> crate::db::Db {
    crate::db::Db::open_in_memory().unwrap()
}

#[test]
fn get_setting_returns_none_when_never_set() {
    let conn = migrated_conn();
    assert_eq!(get_setting(&conn, "nope").unwrap(), None);
}

#[test]
fn set_then_get_round_trips() {
    let conn = migrated_conn();
    set_setting(&conn, LIBRARY_ROOT_KEY, "/music/library").unwrap();
    assert_eq!(
        get_setting(&conn, LIBRARY_ROOT_KEY).unwrap(),
        Some("/music/library".to_string())
    );
}

#[test]
fn set_setting_overwrites_a_previous_value() {
    let conn = migrated_conn();
    set_setting(&conn, LIBRARY_ROOT_KEY, "/first").unwrap();
    set_setting(&conn, LIBRARY_ROOT_KEY, "/second").unwrap();
    assert_eq!(
        get_setting(&conn, LIBRARY_ROOT_KEY).unwrap(),
        Some("/second".to_string())
    );
    // Exactly one row for this key — the upsert never leaves a stale
    // duplicate behind.
    let count: i64 = conn
        .conn()
        .query_row(
            "SELECT count(*) FROM settings WHERE key = ?1",
            rusqlite::params![LIBRARY_ROOT_KEY],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 1);
}

#[test]
fn set_setting_dedups_identical_values_and_logs_only_real_changes() {
    let conn = migrated_conn();
    let change_events = || -> i64 {
        conn.conn()
            .query_row(
                "SELECT count(*) FROM change_log WHERE entity = 'settings' AND entity_id = ?1",
                rusqlite::params![COLOR_SCHEME_KEY],
                |r| r.get(0),
            )
            .unwrap()
    };

    set_setting(&conn, COLOR_SCHEME_KEY, "dark").unwrap();
    // Writing the identical value again is a no-op: no second change_log row.
    set_setting(&conn, COLOR_SCHEME_KEY, "dark").unwrap();
    assert_eq!(change_events(), 1, "an identical write logs no new event");

    // A different value is a real change: a second event lands.
    set_setting(&conn, COLOR_SCHEME_KEY, "light").unwrap();
    assert_eq!(change_events(), 2, "a changed value logs a second event");
    assert_eq!(
        get_setting(&conn, COLOR_SCHEME_KEY).unwrap(),
        Some("light".to_string())
    );
}

#[test]
fn sidebar_collapse_round_trips_and_defaults_to_expanded() {
    let conn = migrated_conn();
    assert!(!get_sidebar_collapsed(&conn));
    set_sidebar_collapsed(&conn, true).unwrap();
    assert!(get_sidebar_collapsed(&conn));
    set_sidebar_collapsed(&conn, false).unwrap();
    assert!(!get_sidebar_collapsed(&conn));
}

#[test]
fn different_keys_do_not_clobber_each_other() {
    let conn = migrated_conn();
    set_setting(&conn, "a", "1").unwrap();
    set_setting(&conn, "b", "2").unwrap();
    assert_eq!(get_setting(&conn, "a").unwrap(), Some("1".to_string()));
    assert_eq!(get_setting(&conn, "b").unwrap(), Some("2".to_string()));
}

#[test]
fn get_bool_returns_default_when_never_set() {
    let conn = migrated_conn();
    assert!(get_bool(&conn, "module.mpris.enabled", true).unwrap());
    assert!(!get_bool(&conn, "module.mpris.enabled", false).unwrap());
}

#[test]
fn set_bool_round_trips_both_values() {
    let conn = migrated_conn();
    set_bool(&conn, "flag", true).unwrap();
    assert!(get_bool(&conn, "flag", false).unwrap());
    set_bool(&conn, "flag", false).unwrap();
    assert!(!get_bool(&conn, "flag", true).unwrap());
}

#[test]
fn get_bool_falls_back_to_default_on_unrecognized_value() {
    // A hand-edited or future-version value must never crash or silently
    // flip a feature: unrecognized -> default, with a warning logged.
    let conn = migrated_conn();
    set_setting(&conn, "flag", "banana").unwrap();
    assert!(get_bool(&conn, "flag", true).unwrap());
    assert!(!get_bool(&conn, "flag", false).unwrap());
}

#[test]
fn library_root_typed_accessors_round_trip() {
    let conn = migrated_conn();
    assert_eq!(get_library_root(&conn).unwrap(), None);
    set_library_root(&conn, "/music/library").unwrap();
    assert_eq!(
        get_library_root(&conn).unwrap(),
        Some("/music/library".to_string())
    );
}

#[test]
fn onboarding_completed_typed_accessors_round_trip() {
    let conn = migrated_conn();
    assert!(!get_onboarding_completed(&conn).unwrap());
    set_onboarding_completed(&conn, true).unwrap();
    assert!(get_onboarding_completed(&conn).unwrap());
    set_onboarding_completed(&conn, false).unwrap();
    assert!(!get_onboarding_completed(&conn).unwrap());
}

#[test]
fn online_discovery_banner_completed_typed_accessors_round_trip() {
    let conn = migrated_conn();
    assert!(!get_online_discovery_banner_completed(&conn).unwrap());
    set_online_discovery_banner_completed(&conn, true).unwrap();
    assert!(get_online_discovery_banner_completed(&conn).unwrap());
    set_online_discovery_banner_completed(&conn, false).unwrap();
    assert!(!get_online_discovery_banner_completed(&conn).unwrap());
}

#[test]
fn new_releases_fetch_completed_defaults_false_and_round_trips() {
    let conn = migrated_conn();
    assert!(!get_new_releases_fetch_completed(&conn).unwrap());
    set_new_releases_fetch_completed(&conn, true).unwrap();
    assert!(get_new_releases_fetch_completed(&conn).unwrap());
}

#[test]
fn new_releases_last_completed_at_defaults_none_and_round_trips() {
    let conn = migrated_conn();
    assert_eq!(get_new_releases_last_completed_at(&conn).unwrap(), None);
    set_new_releases_last_completed_at(&conn, 1_000_360).unwrap();
    assert_eq!(
        get_new_releases_last_completed_at(&conn).unwrap(),
        Some(1_000_360)
    );
}

#[test]
fn new_releases_last_completed_at_tolerates_an_invalid_stored_value() {
    let conn = migrated_conn();
    set_setting(&conn, NEW_RELEASES_LAST_COMPLETED_AT_KEY, "invalid").unwrap();
    assert_eq!(get_new_releases_last_completed_at(&conn).unwrap(), None);
}

#[test]
fn player_bar_position_defaults_to_bottom() {
    let conn = migrated_conn();
    assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Bottom);
}

#[test]
fn player_bar_position_round_trips_both_values() {
    let conn = migrated_conn();
    set_player_bar_position(&conn, PlayerBarPosition::Top).unwrap();
    assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Top);
    set_player_bar_position(&conn, PlayerBarPosition::Bottom).unwrap();
    assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Bottom);
}

#[test]
fn player_bar_position_falls_back_to_bottom_on_unknown_value() {
    let conn = migrated_conn();
    set_setting(&conn, PLAYER_BAR_POSITION_KEY, "sideways").unwrap();
    assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Bottom);
}

#[test]
fn layout_preferences_default_to_visible() {
    let conn = migrated_conn();
    assert!(get_sidebar_visible(&conn));
    assert!(get_status_visible(&conn));
}

#[test]
fn window_decoration_mode_defaults_to_client_side() {
    let conn = migrated_conn();
    assert_eq!(
        get_window_decoration_mode(&conn),
        WindowDecorationMode::Client
    );
}

#[test]
fn window_decoration_mode_round_trips_every_variant() {
    let conn = migrated_conn();
    set_window_decoration_mode(&conn, WindowDecorationMode::System).unwrap();
    assert_eq!(
        get_window_decoration_mode(&conn),
        WindowDecorationMode::System
    );
    set_window_decoration_mode(&conn, WindowDecorationMode::Client).unwrap();
    assert_eq!(
        get_window_decoration_mode(&conn),
        WindowDecorationMode::Client
    );
}

#[test]
fn unknown_window_decoration_mode_falls_back_to_client_side() {
    let conn = migrated_conn();
    set_setting(&conn, WINDOW_DECORATION_MODE_KEY, "frameless").unwrap();
    assert_eq!(
        get_window_decoration_mode(&conn),
        WindowDecorationMode::Client
    );
}

#[test]
fn layout_preferences_round_trip() {
    let conn = migrated_conn();
    set_sidebar_visible(&conn, false).unwrap();
    set_status_visible(&conn, false).unwrap();
    assert!(!get_sidebar_visible(&conn));
    assert!(!get_status_visible(&conn));
}

#[test]
fn information_panel_defaults_hidden_and_round_trips() {
    let conn = migrated_conn();
    assert!(!get_info_panel_visible(&conn));
    set_info_panel_visible(&conn, true).unwrap();
    assert!(get_info_panel_visible(&conn));
    set_info_panel_visible(&conn, false).unwrap();
    assert!(!get_info_panel_visible(&conn));
}

#[test]
fn browse_bar_defaults_visible_and_round_trips() {
    let conn = migrated_conn();
    assert!(get_browse_visible(&conn));
    set_browse_visible(&conn, false).unwrap();
    assert!(!get_browse_visible(&conn));
    set_browse_visible(&conn, true).unwrap();
    assert!(get_browse_visible(&conn));
}

#[test]
fn seek_colouring_defaults_to_frequency_and_round_trips() {
    let conn = migrated_conn();
    assert_eq!(get_seek_colouring(&conn), SeekColouring::Frequency);
    set_seek_colouring(&conn, SeekColouring::Solid).unwrap();
    assert_eq!(get_seek_colouring(&conn), SeekColouring::Solid);
    set_seek_colouring(&conn, SeekColouring::Frequency).unwrap();
    assert_eq!(get_seek_colouring(&conn), SeekColouring::Frequency);
    // A hand-edited or future value must not leave the bar uncoloured.
    set_setting(&conn, SEEK_COLOURING_KEY, "rainbow").unwrap();
    assert_eq!(get_seek_colouring(&conn), SeekColouring::Frequency);
}

#[test]
fn the_colour_legend_counts_three_showings_and_then_stops() {
    let conn = migrated_conn();
    assert_eq!(get_seek_legend_seen(&conn), 0);
    for shown in 1..=SEEK_LEGEND_SHOWS {
        set_seek_legend_seen(&conn, shown).unwrap();
        assert_eq!(get_seek_legend_seen(&conn), shown);
    }
    // The count is a cap, not a running total: nothing beyond the third
    // showing is worth storing, and a corrupt value must not hide the legend
    // forever either.
    set_seek_legend_seen(&conn, 99).unwrap();
    assert_eq!(get_seek_legend_seen(&conn), SEEK_LEGEND_SHOWS);
    set_setting(&conn, SEEK_LEGEND_SEEN_KEY, "later").unwrap();
    assert_eq!(get_seek_legend_seen(&conn), 0);
}

#[test]
fn unknown_typed_preferences_fall_back_safely() {
    let conn = migrated_conn();
    set_setting(&conn, REPLAY_GAIN_MODE_KEY, "loudest").unwrap();
    assert_eq!(get_replay_gain_mode(&conn), ReplayGainMode::Off);
}

#[test]
fn playback_effect_preferences_round_trip() {
    let conn = migrated_conn();
    set_equalizer_enabled(&conn, true).unwrap();
    set_equalizer_bands(
        &conn,
        [1.0, 2.0, 3.0, 4.0, 5.0, -1.0, -2.0, -3.0, -4.0, -5.0],
    )
    .unwrap();
    set_replay_gain_mode(&conn, ReplayGainMode::Album).unwrap();
    assert!(get_equalizer_enabled(&conn));
    assert_eq!(
        get_equalizer_bands(&conn),
        [1.0, 2.0, 3.0, 4.0, 5.0, -1.0, -2.0, -3.0, -4.0, -5.0]
    );
    assert_eq!(get_replay_gain_mode(&conn), ReplayGainMode::Album);
}

#[test]
fn track_transition_is_derived_from_gapless_and_crossfade() {
    let conn = migrated_conn();
    // Default (gapless on, no crossfade): Gapless.
    assert_eq!(get_track_transition(&conn), TrackTransition::Gapless);

    // Gapless off, no crossfade: Off.
    set_gapless_enabled(&conn, false).unwrap();
    assert_eq!(get_track_transition(&conn), TrackTransition::Off);

    // Any crossfade overlap wins, regardless of the gapless toggle.
    set_crossfade_seconds(&conn, 4).unwrap();
    assert_eq!(get_track_transition(&conn), TrackTransition::Crossfade);
    set_gapless_enabled(&conn, true).unwrap();
    assert_eq!(get_track_transition(&conn), TrackTransition::Crossfade);

    // Crossfade back to 0 falls through to the gapless toggle.
    set_crossfade_seconds(&conn, 0).unwrap();
    assert_eq!(get_track_transition(&conn), TrackTransition::Gapless);
}

#[test]
fn crossfade_seconds_clamp_and_default() {
    let conn = migrated_conn();
    // Default: 0 (off).
    assert_eq!(get_crossfade_seconds(&conn), 0);
    set_crossfade_seconds(&conn, 99).unwrap();
    assert_eq!(get_crossfade_seconds(&conn), CROSSFADE_SECONDS_MAX);
    set_crossfade_seconds(&conn, 0).unwrap();
    assert_eq!(get_crossfade_seconds(&conn), 0);
    set_crossfade_seconds(&conn, 6).unwrap();
    assert_eq!(get_crossfade_seconds(&conn), 6);
    // Corrupt stored value → default (off).
    set_setting(&conn, CROSSFADE_SECONDS_KEY, "loud").unwrap();
    assert_eq!(get_crossfade_seconds(&conn), CROSSFADE_SECONDS_DEFAULT);
}

#[test]
fn equalizer_bands_reject_corrupt_values_and_clamp_writes() {
    let conn = migrated_conn();
    set_setting(&conn, EQUALIZER_CURVE_KEY, "not a curve").unwrap();
    assert_eq!(get_equalizer_bands(&conn), [0.0; 10]);
    set_equalizer_bands(&conn, [50.0; 10]).unwrap();
    assert_eq!(get_equalizer_bands(&conn), [12.0; 10]);
}

/// A ten-band write over a curve authored somewhere else — the case a copied
/// library makes real the moment a phone-authored curve and desktop
/// preferences meet. The write is accepted on purpose: refusing it would
/// leave the desktop's sliders moving and its equalizer doing nothing. What
/// must not happen is that it happens *quietly*. Deleting the `tracing::warn!`
/// in `set_equalizer_bands_in`, or making `is_gstreamer_ten_band` answer true
/// for any ten points, turns this red.
#[test]
fn a_ten_band_write_over_a_foreign_curve_is_accepted_and_said_out_loud() {
    use crate::equalizer::{EqualizerCurve, EqualizerPoint};

    let conn = migrated_conn();
    let phone = EqualizerCurve::new(
        [60.0, 230.0, 910.0, 3_600.0, 14_000.0]
            .into_iter()
            .map(|frequency_hz| EqualizerPoint {
                frequency_hz,
                gain_db: 4.0,
            })
            .collect(),
    )
    .unwrap();
    set_equalizer_curve(&conn, &phone).unwrap();

    let logs = crate::log_capture::CapturedLogs::default();
    logs.capture(|| {
        set_equalizer_bands(&conn, [3.0; 10]).unwrap();
    });

    assert_eq!(
        get_equalizer_bands(&conn),
        [3.0; 10],
        "the desktop edit must land; a silently disabled equalizer is worse",
    );
    let stored = get_equalizer_curve(&conn);
    assert!(
        stored.is_gstreamer_ten_band(),
        "the phone's five authored points are gone, replaced by the ten centres",
    );
    assert!(
        logs.joined().contains("authored on another backend"),
        "the replacement must be on the record, logged: {}",
        logs.joined(),
    );

    // A desktop edit on top of a desktop curve replaces nothing, and says
    // nothing. Otherwise the warning would be noise every preference change.
    let quiet = crate::log_capture::CapturedLogs::default();
    quiet.capture(|| {
        set_equalizer_bands(&conn, [1.0; 10]).unwrap();
    });
    assert!(
        !quiet.joined().contains("authored on another backend"),
        "logged: {}",
        quiet.joined(),
    );
}

#[test]
fn last_viewed_missing_defaults_to_zero_round_trips_and_tolerates_corruption() {
    let conn = migrated_conn();
    // Never written — "never viewed" reads back as 0 (queries::count_new_
    // missing then treats every missing row as new).
    assert_eq!(get_last_viewed_missing(&conn).unwrap(), 0);

    set_last_viewed_missing(&conn, 1_700_000_000).unwrap();
    assert_eq!(get_last_viewed_missing(&conn).unwrap(), 1_700_000_000);

    // A hand-edited/corrupt value falls back to 0 — same "never viewed"
    // fallback as a missing key, never an error: a badge that fails closed
    // (shows too much) is far safer than one that panics or silently hides
    // real issues.
    set_setting(&conn, LAST_VIEWED_MISSING_KEY, "not-a-number").unwrap();
    assert_eq!(get_last_viewed_missing(&conn).unwrap(), 0);
}

#[test]
fn last_viewed_import_errors_defaults_to_zero_round_trips_and_tolerates_corruption() {
    let conn = migrated_conn();
    assert_eq!(get_last_viewed_import_errors(&conn).unwrap(), 0);

    set_last_viewed_import_errors(&conn, 1_700_000_500).unwrap();
    assert_eq!(get_last_viewed_import_errors(&conn).unwrap(), 1_700_000_500);

    set_setting(&conn, LAST_VIEWED_IMPORT_ERRORS_KEY, "nope").unwrap();
    assert_eq!(get_last_viewed_import_errors(&conn).unwrap(), 0);
}
