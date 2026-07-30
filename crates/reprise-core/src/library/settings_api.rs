use crate::db::Db;

use super::{
    get_auto_clean_armed_at_in, get_bool_in, get_browse_visible_in, get_color_scheme_in,
    get_compact_always_on_top_in, get_compact_layout_in, get_crossfade_seconds_in,
    get_equalizer_bands_in, get_equalizer_enabled_in, get_gapless_enabled_in,
    get_info_panel_visible_in, get_last_scan_relinked_in, get_last_viewed_import_errors_in,
    get_last_viewed_missing_in, get_library_root_in, get_list_density_in,
    get_missing_auto_clean_in, get_new_releases_fetch_completed_in, get_onboarding_completed_in,
    get_player_bar_position_in, get_replay_gain_mode_in, get_setting_in, get_sidebar_collapsed_in,
    get_sidebar_visible_in, get_status_visible_in, get_track_transition_in,
    get_window_decoration_mode_in, get_window_view_mode_in, set_auto_clean_armed_at_in,
    set_bool_in, set_browse_visible_in, set_color_scheme_in, set_compact_always_on_top_in,
    set_compact_layout_in, set_crossfade_seconds_in, set_equalizer_bands_in,
    set_equalizer_enabled_in, set_gapless_enabled_in, set_info_panel_visible_in,
    set_last_scan_relinked_in, set_last_viewed_import_errors_in, set_last_viewed_missing_in,
    set_library_root_in, set_list_density_in, set_missing_auto_clean_in,
    set_new_releases_fetch_completed_in, set_onboarding_completed_in, set_player_bar_position_in,
    set_replay_gain_mode_in, set_setting_in, set_sidebar_collapsed_in, set_sidebar_visible_in,
    set_status_visible_in, set_window_decoration_mode_in, set_window_view_mode_in,
    AutoCleanSetting, CompactLayout, ListDensity, PlayerBarPosition, ReplayGainMode,
    TrackTransition, WindowDecorationMode, WindowViewMode,
};

pub fn get_setting(db: &Db, key: &str) -> Result<Option<String>, rusqlite::Error> {
    let conn = db.conn();
    get_setting_in(conn, key)
}

pub fn set_setting(db: &Db, key: &str, value: &str) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_setting_in(conn, key, value)
}

pub fn get_bool(db: &Db, key: &str, default: bool) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    get_bool_in(conn, key, default)
}

pub fn set_bool(db: &Db, key: &str, value: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_bool_in(conn, key, value)
}

pub fn get_library_root(db: &Db) -> Result<Option<String>, rusqlite::Error> {
    let conn = db.conn();
    get_library_root_in(conn)
}

pub fn set_library_root(db: &Db, root: &str) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_library_root_in(conn, root)
}

pub fn get_last_scan_relinked(db: &Db) -> Result<Option<u32>, rusqlite::Error> {
    let conn = db.conn();
    get_last_scan_relinked_in(conn)
}

pub fn set_last_scan_relinked(db: &Db, count: u32) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_last_scan_relinked_in(conn, count)
}

pub fn get_onboarding_completed(db: &Db) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    get_onboarding_completed_in(conn)
}

pub fn set_onboarding_completed(db: &Db, completed: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_onboarding_completed_in(conn, completed)
}

pub fn get_new_releases_fetch_completed(db: &Db) -> Result<bool, rusqlite::Error> {
    let conn = db.conn();
    get_new_releases_fetch_completed_in(conn)
}

pub fn set_new_releases_fetch_completed(db: &Db, completed: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_new_releases_fetch_completed_in(conn, completed)
}

pub fn get_player_bar_position(db: &Db) -> PlayerBarPosition {
    let conn = db.conn();
    get_player_bar_position_in(conn)
}

pub fn set_player_bar_position(
    db: &Db,
    position: PlayerBarPosition,
) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_player_bar_position_in(conn, position)
}

pub fn get_window_view_mode(db: &Db) -> WindowViewMode {
    let conn = db.conn();
    get_window_view_mode_in(conn)
}

pub fn set_window_view_mode(db: &Db, value: WindowViewMode) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_window_view_mode_in(conn, value)
}

pub fn get_compact_always_on_top(db: &Db) -> bool {
    let conn = db.conn();
    get_compact_always_on_top_in(conn)
}

pub fn set_compact_always_on_top(db: &Db, above: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_compact_always_on_top_in(conn, above)
}

pub fn get_compact_layout(db: &Db) -> CompactLayout {
    let conn = db.conn();
    get_compact_layout_in(conn)
}

pub fn set_compact_layout(db: &Db, value: CompactLayout) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_compact_layout_in(conn, value)
}

pub fn get_window_decoration_mode(db: &Db) -> WindowDecorationMode {
    let conn = db.conn();
    get_window_decoration_mode_in(conn)
}

pub fn set_window_decoration_mode(
    db: &Db,
    value: WindowDecorationMode,
) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_window_decoration_mode_in(conn, value)
}

pub fn get_list_density(db: &Db) -> ListDensity {
    let conn = db.conn();
    get_list_density_in(conn)
}

pub fn set_list_density(db: &Db, value: ListDensity) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_list_density_in(conn, value)
}

pub fn get_sidebar_visible(db: &Db) -> bool {
    let conn = db.conn();
    get_sidebar_visible_in(conn)
}

pub fn set_sidebar_visible(db: &Db, value: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_sidebar_visible_in(conn, value)
}

pub fn get_sidebar_collapsed(db: &Db) -> bool {
    let conn = db.conn();
    get_sidebar_collapsed_in(conn)
}

pub fn set_sidebar_collapsed(db: &Db, collapsed: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_sidebar_collapsed_in(conn, collapsed)
}

pub fn get_browse_visible(db: &Db) -> bool {
    let conn = db.conn();
    get_browse_visible_in(conn)
}

pub fn set_browse_visible(db: &Db, value: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_browse_visible_in(conn, value)
}

pub fn get_status_visible(db: &Db) -> bool {
    let conn = db.conn();
    get_status_visible_in(conn)
}

pub fn set_status_visible(db: &Db, value: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_status_visible_in(conn, value)
}

pub fn get_info_panel_visible(db: &Db) -> bool {
    let conn = db.conn();
    get_info_panel_visible_in(conn)
}

pub fn set_info_panel_visible(db: &Db, visible: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_info_panel_visible_in(conn, visible)
}

pub fn get_equalizer_enabled(db: &Db) -> bool {
    let conn = db.conn();
    get_equalizer_enabled_in(conn)
}

pub fn set_equalizer_enabled(db: &Db, value: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_equalizer_enabled_in(conn, value)
}

pub fn get_equalizer_bands(db: &Db) -> [f64; 10] {
    let conn = db.conn();
    get_equalizer_bands_in(conn)
}

pub fn set_equalizer_bands(db: &Db, values: [f64; 10]) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_equalizer_bands_in(conn, values)
}

pub fn get_replay_gain_mode(db: &Db) -> ReplayGainMode {
    let conn = db.conn();
    get_replay_gain_mode_in(conn)
}

pub fn set_replay_gain_mode(db: &Db, value: ReplayGainMode) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_replay_gain_mode_in(conn, value)
}

pub fn get_gapless_enabled(db: &Db) -> bool {
    let conn = db.conn();
    get_gapless_enabled_in(conn)
}

pub fn set_gapless_enabled(db: &Db, enabled: bool) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_gapless_enabled_in(conn, enabled)
}

pub fn get_track_transition(db: &Db) -> TrackTransition {
    let conn = db.conn();
    get_track_transition_in(conn)
}

pub fn get_crossfade_seconds(db: &Db) -> u8 {
    let conn = db.conn();
    get_crossfade_seconds_in(conn)
}

pub fn set_crossfade_seconds(db: &Db, seconds: u8) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_crossfade_seconds_in(conn, seconds)
}

pub fn get_color_scheme(db: &Db) -> &'static str {
    let conn = db.conn();
    get_color_scheme_in(conn)
}

pub fn set_color_scheme(db: &Db, value: &str) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_color_scheme_in(conn, value)
}

pub fn get_missing_auto_clean(db: &Db) -> AutoCleanSetting {
    let conn = db.conn();
    get_missing_auto_clean_in(conn)
}

pub fn set_missing_auto_clean(db: &Db, value: AutoCleanSetting) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_missing_auto_clean_in(conn, value)
}

pub fn get_auto_clean_armed_at(db: &Db) -> Result<Option<i64>, rusqlite::Error> {
    let conn = db.conn();
    get_auto_clean_armed_at_in(conn)
}

pub fn set_auto_clean_armed_at(db: &Db, armed_at: i64) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_auto_clean_armed_at_in(conn, armed_at)
}

pub fn get_last_viewed_missing(db: &Db) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    get_last_viewed_missing_in(conn)
}

pub fn set_last_viewed_missing(db: &Db, now: i64) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_last_viewed_missing_in(conn, now)
}

pub fn get_last_viewed_import_errors(db: &Db) -> Result<i64, rusqlite::Error> {
    let conn = db.conn();
    get_last_viewed_import_errors_in(conn)
}

pub fn set_last_viewed_import_errors(db: &Db, now: i64) -> Result<(), rusqlite::Error> {
    let conn = db.conn();
    set_last_viewed_import_errors_in(conn, now)
}
