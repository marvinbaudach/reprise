//! Tiny key/value settings store (Stage 3 Task 8 — schema v4's `settings`
//! table, see `db.rs`'s `SCHEMA_V4` doc comment). The one consumer this task
//! adds is `library_root` (`LIBRARY_ROOT_KEY`): the folder the user last
//! scanned, persisted here so the folder watcher (`library::watcher`) knows
//! what to watch on startup without the user re-picking a folder every
//! launch. Deliberately generic (`get_setting`/`set_setting` take any `&str`
//! key) rather than one bespoke function per setting — a future setting is
//! then just one more constant and call site, not a new migration.

use rusqlite::{Connection, OptionalExtension};

/// The settings key `ui::window`'s scan flow writes the scanned folder under,
/// and `main.rs`/`ui::window` read at startup/after-scan to (re)start the
/// watcher. `pub` so both call sites share the exact same literal rather than
/// risking a typo'd duplicate string.
pub const LIBRARY_ROOT_KEY: &str = "library_root";
pub const ONBOARDING_COMPLETED_KEY: &str = "onboarding.completed";

/// Reads `key`'s current value, if any has ever been set. `Ok(None)` — not
/// an error — for a key that has never been written, matching every other
/// "not found" case in this codebase's query layer (e.g. `queries::query_
/// track_summary`).
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        rusqlite::params![key],
        |r| r.get(0),
    )
    .optional()
}

/// Writes `key` = `value`, overwriting any previous value — an upsert via
/// `ON CONFLICT`, not a delete-then-insert (keeps this a single statement,
/// no transaction needed).
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<(), rusqlite::Error> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = ?2",
        rusqlite::params![key, value],
    )?;
    Ok(())
}

/// Canonical stored forms for boolean settings. `get_bool` additionally
/// tolerates anything else by falling back to the caller's default (never
/// crash on a hand-edited database; log and move on — the same tolerance
/// posture as the scanner's).
const BOOL_TRUE: &str = "1";
const BOOL_FALSE: &str = "0";

pub fn get_bool(conn: &Connection, key: &str, default: bool) -> Result<bool, rusqlite::Error> {
    match get_setting(conn, key)? {
        None => Ok(default),
        Some(value) => match value.as_str() {
            BOOL_TRUE => Ok(true),
            BOOL_FALSE => Ok(false),
            other => {
                tracing::warn!(
                    key,
                    value = other,
                    "unrecognized boolean setting; using default"
                );
                Ok(default)
            }
        },
    }
}

pub fn set_bool(conn: &Connection, key: &str, value: bool) -> Result<(), rusqlite::Error> {
    set_setting(conn, key, if value { BOOL_TRUE } else { BOOL_FALSE })
}

/// Typed accessors for `LIBRARY_ROOT_KEY` — the one string setting with
/// scattered call sites today (main.rs dev hook, scan flow, watcher
/// startup). Stored as the same string the scanner writes; kept as String
/// (not PathBuf) because the scanner's path storage is string-based and a
/// lossy round-trip here could diverge from what `mark_vanished_under_root`
/// compares against.
pub fn get_library_root(conn: &Connection) -> Result<Option<String>, rusqlite::Error> {
    get_setting(conn, LIBRARY_ROOT_KEY)
}

pub fn set_library_root(conn: &Connection, root: &str) -> Result<(), rusqlite::Error> {
    set_setting(conn, LIBRARY_ROOT_KEY, root)
}

pub fn get_onboarding_completed(conn: &Connection) -> Result<bool, rusqlite::Error> {
    get_bool(conn, ONBOARDING_COMPLETED_KEY, false)
}

pub fn set_onboarding_completed(conn: &Connection, completed: bool) -> Result<(), rusqlite::Error> {
    set_bool(conn, ONBOARDING_COMPLETED_KEY, completed)
}

pub const PLAYER_BAR_POSITION_KEY: &str = "player_bar_position";
pub const COLUMN_LAYOUT_KEY: &str = "ui.column_layout";
/// User-adjusted per-column widths (`id:width` pairs), kept separate from the
/// order/visibility layout so the layout reducers and their tests stay untouched.
pub const COLUMN_WIDTHS_KEY: &str = "ui.column_widths";

/// Where the player bar docks. `Bottom` is the default and the fallback for any
/// unknown/hand-edited value (same tolerance posture as `get_bool`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerBarPosition {
    Top,
    Bottom,
}

pub fn get_player_bar_position(conn: &Connection) -> PlayerBarPosition {
    match get_setting(conn, PLAYER_BAR_POSITION_KEY) {
        Ok(Some(v)) if v == "top" => PlayerBarPosition::Top,
        Ok(Some(v)) if v == "bottom" => PlayerBarPosition::Bottom,
        Ok(Some(other)) => {
            tracing::warn!(value = %other, "unrecognized player_bar_position; using Bottom");
            PlayerBarPosition::Bottom
        }
        Ok(None) => PlayerBarPosition::Bottom,
        Err(error) => {
            tracing::warn!(%error, "could not read player_bar_position; using Bottom");
            PlayerBarPosition::Bottom
        }
    }
}

pub fn set_player_bar_position(
    conn: &Connection,
    pos: PlayerBarPosition,
) -> Result<(), rusqlite::Error> {
    let value = match pos {
        PlayerBarPosition::Top => "top",
        PlayerBarPosition::Bottom => "bottom",
    };
    set_setting(conn, PLAYER_BAR_POSITION_KEY, value)
}

pub const LIST_DENSITY_KEY: &str = "ui.list_density";
pub const SIDEBAR_VISIBLE_KEY: &str = "ui.sidebar_visible";
pub const SIDEBAR_COLLAPSED_KEY: &str = "ui.sidebar_collapsed";
pub const BROWSE_VISIBLE_KEY: &str = "ui.browse_visible";
pub const STATUS_VISIBLE_KEY: &str = "ui.status_visible";
pub const INFO_PANEL_VISIBLE_KEY: &str = "ui.info_panel_visible";
pub const INFO_PANEL_TAB_KEY: &str = "ui.info_panel_tab";
pub const WINDOW_VIEW_MODE_KEY: &str = "ui.window_view_mode";
pub const COMPACT_LAYOUT_KEY: &str = "ui.compact_layout";
pub const WINDOW_DECORATION_MODE_KEY: &str = "ui.window_decoration_mode";
pub const COMPACT_ALWAYS_ON_TOP_KEY: &str = "ui.compact_always_on_top";
pub const EQUALIZER_ENABLED_KEY: &str = "playback.equalizer_enabled";
pub const EQUALIZER_BANDS_KEY: &str = "playback.equalizer_bands";
pub const REPLAY_GAIN_MODE_KEY: &str = "playback.replay_gain_mode";
pub const GAPLESS_ENABLED_KEY: &str = "playback.gapless_enabled";
pub const CROSSFADE_SECONDS_KEY: &str = "playback.crossfade_seconds";
pub const COLOR_SCHEME_KEY: &str = "ui.color_scheme";

/// Crossfade overlap in whole seconds. `0` means crossfade is off (the slider's
/// "Off" position); `1..=MAX` is an active overlap. `DEFAULT` (off) applies when
/// the stored value is missing or out of range. The `TrackTransition` mode is
/// *derived* from this plus `GAPLESS_ENABLED_KEY` (see `get_track_transition`):
/// any crossfade > 0 wins, else gapless-on means Gapless, else Off.
pub const CROSSFADE_SECONDS_MIN: u8 = 0;
pub const CROSSFADE_SECONDS_MAX: u8 = 10;
pub const CROSSFADE_SECONDS_DEFAULT: u8 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListDensity {
    Comfortable,
    Standard,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowViewMode {
    Library,
    Compact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactLayout {
    Cover,
    Pill,
    Card,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowDecorationMode {
    Client,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayGainMode {
    Off,
    Track,
    Album,
}

/// How the player transitions between consecutive tracks.
/// - `Off`: hard cut (stop the pipeline, start the next) — the pre-gapless
///   behavior.
/// - `Gapless`: seamless hand-off via `playbin3`'s `about-to-finish`, no
///   pipeline restart, no silence between tracks (Phase A).
/// - `Crossfade`: overlap the tail of the current track with the head of the
///   next over `crossfade_seconds` (Phase B — dual pipeline + mixer).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackTransition {
    Off,
    Gapless,
    Crossfade,
}

fn typed_value(conn: &Connection, key: &str, default: &'static str) -> String {
    match get_setting(conn, key) {
        Ok(Some(value)) => value,
        Ok(None) => default.to_string(),
        Err(error) => {
            tracing::warn!(%error, key, "could not read typed setting; using default");
            default.to_string()
        }
    }
}

pub fn get_window_view_mode(conn: &Connection) -> WindowViewMode {
    match typed_value(conn, WINDOW_VIEW_MODE_KEY, "library").as_str() {
        "compact" => WindowViewMode::Compact,
        "library" => WindowViewMode::Library,
        value => {
            tracing::warn!(value, "unrecognized window view mode; using Library");
            WindowViewMode::Library
        }
    }
}

pub fn set_window_view_mode(
    conn: &Connection,
    value: WindowViewMode,
) -> Result<(), rusqlite::Error> {
    let value = match value {
        WindowViewMode::Library => "library",
        WindowViewMode::Compact => "compact",
    };
    set_setting(conn, WINDOW_VIEW_MODE_KEY, value)
}

pub fn get_compact_always_on_top(conn: &Connection) -> bool {
    get_bool(conn, COMPACT_ALWAYS_ON_TOP_KEY, false).unwrap_or(false)
}

pub fn set_compact_always_on_top(conn: &Connection, above: bool) -> Result<(), rusqlite::Error> {
    set_setting(
        conn,
        COMPACT_ALWAYS_ON_TOP_KEY,
        if above { BOOL_TRUE } else { BOOL_FALSE },
    )
}

pub fn get_compact_layout(conn: &Connection) -> CompactLayout {
    match typed_value(conn, COMPACT_LAYOUT_KEY, "card").as_str() {
        "cover" => CompactLayout::Cover,
        "pill" => CompactLayout::Pill,
        "card" => CompactLayout::Card,
        "bar" => {
            tracing::info!("legacy compact Bar layout mapped to Card");
            CompactLayout::Card
        }
        value => {
            tracing::warn!(value, "unrecognized compact layout; using Card");
            CompactLayout::Card
        }
    }
}

pub fn set_compact_layout(conn: &Connection, value: CompactLayout) -> Result<(), rusqlite::Error> {
    let value = match value {
        CompactLayout::Cover => "cover",
        CompactLayout::Pill => "pill",
        CompactLayout::Card => "card",
    };
    set_setting(conn, COMPACT_LAYOUT_KEY, value)
}

pub fn get_window_decoration_mode(conn: &Connection) -> WindowDecorationMode {
    match typed_value(conn, WINDOW_DECORATION_MODE_KEY, "client").as_str() {
        "system" => WindowDecorationMode::System,
        "client" => WindowDecorationMode::Client,
        value => {
            tracing::warn!(value, "unrecognized window decoration mode; using Client");
            WindowDecorationMode::Client
        }
    }
}

pub fn set_window_decoration_mode(
    conn: &Connection,
    value: WindowDecorationMode,
) -> Result<(), rusqlite::Error> {
    let value = match value {
        WindowDecorationMode::Client => "client",
        WindowDecorationMode::System => "system",
    };
    set_setting(conn, WINDOW_DECORATION_MODE_KEY, value)
}

pub fn get_list_density(conn: &Connection) -> ListDensity {
    match typed_value(conn, LIST_DENSITY_KEY, "standard").as_str() {
        "comfortable" => ListDensity::Comfortable,
        "compact" => ListDensity::Compact,
        "standard" => ListDensity::Standard,
        value => {
            tracing::warn!(value, "unrecognized list density; using Standard");
            ListDensity::Standard
        }
    }
}

pub fn set_list_density(conn: &Connection, value: ListDensity) -> Result<(), rusqlite::Error> {
    let value = match value {
        ListDensity::Comfortable => "comfortable",
        ListDensity::Standard => "standard",
        ListDensity::Compact => "compact",
    };
    set_setting(conn, LIST_DENSITY_KEY, value)
}

pub fn get_sidebar_visible(conn: &Connection) -> bool {
    get_bool(conn, SIDEBAR_VISIBLE_KEY, true).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read sidebar visibility; using visible");
        true
    })
}

pub fn set_sidebar_visible(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    set_bool(conn, SIDEBAR_VISIBLE_KEY, value)
}

/// Whether the user manually collapsed the sidebar column via the headerbar
/// toggle. Distinct from `SIDEBAR_VISIBLE_KEY` (the preferences switch that
/// removes the sidebar slot entirely): this remembers the in-window toggle
/// so the next session starts with the same layout.
pub fn get_sidebar_collapsed(conn: &Connection) -> bool {
    get_bool(conn, SIDEBAR_COLLAPSED_KEY, false).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read sidebar collapse state; using expanded");
        false
    })
}

pub fn set_sidebar_collapsed(conn: &Connection, collapsed: bool) -> Result<(), rusqlite::Error> {
    set_bool(conn, SIDEBAR_COLLAPSED_KEY, collapsed)
}

pub fn get_browse_visible(conn: &Connection) -> bool {
    get_bool(conn, BROWSE_VISIBLE_KEY, true).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read browse bar visibility; using visible");
        true
    })
}

pub fn set_browse_visible(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    set_bool(conn, BROWSE_VISIBLE_KEY, value)
}

pub fn get_status_visible(conn: &Connection) -> bool {
    get_bool(conn, STATUS_VISIBLE_KEY, true).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read status visibility; using visible");
        true
    })
}

pub fn set_status_visible(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    set_bool(conn, STATUS_VISIBLE_KEY, value)
}

pub fn get_info_panel_visible(conn: &Connection) -> bool {
    get_bool(conn, INFO_PANEL_VISIBLE_KEY, true).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read information panel visibility; using visible");
        true
    })
}

pub fn set_info_panel_visible(conn: &Connection, visible: bool) -> Result<(), rusqlite::Error> {
    set_bool(conn, INFO_PANEL_VISIBLE_KEY, visible)
}

/// The information panel's selected tab. The variant names double as the
/// GTK stack page names the frontend uses, so a persisted value can be fed
/// straight into `set_visible_child_name`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InfoPanelTab {
    #[default]
    Information,
    Lyrics,
}

impl InfoPanelTab {
    pub fn name(self) -> &'static str {
        match self {
            Self::Information => "information",
            Self::Lyrics => "lyrics",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "information" => Some(Self::Information),
            "lyrics" => Some(Self::Lyrics),
            _ => None,
        }
    }
}

pub fn get_info_panel_tab(conn: &Connection) -> InfoPanelTab {
    let stored = get_setting(conn, INFO_PANEL_TAB_KEY).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read information panel tab; using default");
        None
    });
    stored
        .as_deref()
        .and_then(InfoPanelTab::from_name)
        .unwrap_or_default()
}

pub fn set_info_panel_tab(conn: &Connection, tab: InfoPanelTab) -> Result<(), rusqlite::Error> {
    set_setting(conn, INFO_PANEL_TAB_KEY, tab.name())
}

pub fn get_equalizer_enabled(conn: &Connection) -> bool {
    get_bool(conn, EQUALIZER_ENABLED_KEY, false).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read equalizer state; using disabled");
        false
    })
}

pub fn set_equalizer_enabled(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    set_bool(conn, EQUALIZER_ENABLED_KEY, value)
}

pub fn get_equalizer_bands(conn: &Connection) -> [f64; 10] {
    let value = typed_value(conn, EQUALIZER_BANDS_KEY, "0,0,0,0,0,0,0,0,0,0");
    let values = value
        .split(',')
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>();
    let Ok(values) = values else {
        tracing::warn!("invalid equalizer bands; using flat preset");
        return [0.0; 10];
    };
    let Ok(values) = <Vec<f64> as TryInto<[f64; 10]>>::try_into(values) else {
        tracing::warn!("wrong equalizer band count; using flat preset");
        return [0.0; 10];
    };
    values.map(|value| value.clamp(-12.0, 12.0))
}

pub fn set_equalizer_bands(conn: &Connection, values: [f64; 10]) -> Result<(), rusqlite::Error> {
    let value = values
        .map(|value| value.clamp(-12.0, 12.0).to_string())
        .join(",");
    set_setting(conn, EQUALIZER_BANDS_KEY, &value)
}

pub fn get_replay_gain_mode(conn: &Connection) -> ReplayGainMode {
    match typed_value(conn, REPLAY_GAIN_MODE_KEY, "off").as_str() {
        "track" => ReplayGainMode::Track,
        "album" => ReplayGainMode::Album,
        "off" => ReplayGainMode::Off,
        value => {
            tracing::warn!(value, "unrecognized ReplayGain mode; using Off");
            ReplayGainMode::Off
        }
    }
}

pub fn set_replay_gain_mode(
    conn: &Connection,
    value: ReplayGainMode,
) -> Result<(), rusqlite::Error> {
    let value = match value {
        ReplayGainMode::Off => "off",
        ReplayGainMode::Track => "track",
        ReplayGainMode::Album => "album",
    };
    set_setting(conn, REPLAY_GAIN_MODE_KEY, value)
}

/// Whether gapless playback is enabled. Independent of crossfade: it only takes
/// effect (as the `Gapless` transition) when no crossfade overlap is set.
/// Default `true` — the expected modern behavior for a music player.
pub fn get_gapless_enabled(conn: &Connection) -> bool {
    get_bool(conn, GAPLESS_ENABLED_KEY, true).unwrap_or(true)
}

pub fn set_gapless_enabled(conn: &Connection, enabled: bool) -> Result<(), rusqlite::Error> {
    set_bool(conn, GAPLESS_ENABLED_KEY, enabled)
}

/// The effective transition mode, *derived* from the two independent playback
/// preferences (`crossfade_seconds` + `gapless_enabled`): any crossfade overlap
/// wins, else gapless-on means `Gapless`, else `Off`. There is no separately
/// stored mode — the two controls in the Audio Transitions settings are the
/// single source of truth.
pub fn get_track_transition(conn: &Connection) -> TrackTransition {
    if get_crossfade_seconds(conn) > 0 {
        TrackTransition::Crossfade
    } else if get_gapless_enabled(conn) {
        TrackTransition::Gapless
    } else {
        TrackTransition::Off
    }
}

pub fn get_crossfade_seconds(conn: &Connection) -> u8 {
    typed_value(conn, CROSSFADE_SECONDS_KEY, "")
        .parse::<u8>()
        .ok()
        .filter(|s| (CROSSFADE_SECONDS_MIN..=CROSSFADE_SECONDS_MAX).contains(s))
        .unwrap_or(CROSSFADE_SECONDS_DEFAULT)
}

pub fn set_crossfade_seconds(conn: &Connection, seconds: u8) -> Result<(), rusqlite::Error> {
    let clamped = seconds.clamp(CROSSFADE_SECONDS_MIN, CROSSFADE_SECONDS_MAX);
    set_setting(conn, CROSSFADE_SECONDS_KEY, &clamped.to_string())
}

pub fn get_color_scheme(conn: &Connection) -> &'static str {
    match get_setting(conn, COLOR_SCHEME_KEY).ok().flatten() {
        Some(ref v) if v == "light" => "light",
        Some(ref v) if v == "dark" => "dark",
        _ => "system",
    }
}

pub fn set_color_scheme(conn: &Connection, value: &str) -> Result<(), rusqlite::Error> {
    set_setting(conn, COLOR_SCHEME_KEY, value)
}

#[cfg(test)]
#[path = "settings_compact_tests.rs"]
mod compact_tests;

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
