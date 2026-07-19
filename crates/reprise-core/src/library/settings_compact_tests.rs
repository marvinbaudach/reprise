use super::*;

fn migrated_conn() -> Connection {
    let conn = crate::db::open(None).unwrap();
    crate::db::migrate(&conn).unwrap();
    conn
}

#[test]
fn audio_analysis_is_opt_in_and_round_trips() {
    let conn = migrated_conn();

    assert!(!get_audio_analysis_enabled(&conn));
    set_audio_analysis_enabled(&conn, true).unwrap();
    assert!(get_audio_analysis_enabled(&conn));
    set_audio_analysis_enabled(&conn, false).unwrap();
    assert!(!get_audio_analysis_enabled(&conn));
}

#[test]
fn compact_view_settings_default_to_library_and_card() {
    let conn = migrated_conn();

    assert_eq!(get_window_view_mode(&conn), WindowViewMode::Library);
    assert_eq!(get_compact_layout(&conn), CompactLayout::Card);
}

#[test]
fn every_window_view_mode_round_trips() {
    let conn = migrated_conn();

    for mode in [WindowViewMode::Library, WindowViewMode::Compact] {
        set_window_view_mode(&conn, mode).unwrap();
        assert_eq!(get_window_view_mode(&conn), mode);
    }
}

#[test]
fn every_compact_layout_round_trips() {
    let conn = migrated_conn();

    for layout in [
        CompactLayout::Cover,
        CompactLayout::Pill,
        CompactLayout::Card,
    ] {
        set_compact_layout(&conn, layout).unwrap();
        assert_eq!(get_compact_layout(&conn), layout);
    }
}

#[test]
fn unknown_compact_view_values_fall_back_independently() {
    let conn = migrated_conn();
    set_setting(&conn, WINDOW_VIEW_MODE_KEY, "floating").unwrap();
    set_setting(&conn, COMPACT_LAYOUT_KEY, "vinyl").unwrap();

    assert_eq!(get_window_view_mode(&conn), WindowViewMode::Library);
    assert_eq!(get_compact_layout(&conn), CompactLayout::Card);
}

#[test]
fn unknown_layout_does_not_change_a_valid_window_mode() {
    let conn = migrated_conn();
    set_window_view_mode(&conn, WindowViewMode::Compact).unwrap();
    set_setting(&conn, COMPACT_LAYOUT_KEY, "unknown").unwrap();

    assert_eq!(get_window_view_mode(&conn), WindowViewMode::Compact);
    assert_eq!(get_compact_layout(&conn), CompactLayout::Card);
}

#[test]
fn legacy_bar_layout_loads_as_card() {
    let conn = migrated_conn();
    set_setting(&conn, COMPACT_LAYOUT_KEY, "bar").unwrap();

    assert_eq!(get_compact_layout(&conn), CompactLayout::Card);
}
