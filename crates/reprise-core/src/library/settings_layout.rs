use rusqlite::Connection;

use super::{
    get_bool_in, get_setting_in, set_bool_in, set_setting_in, typed_value, BOOL_FALSE, BOOL_TRUE,
};

pub const PLAYER_BAR_POSITION_KEY: &str = "player_bar_position";

/// Where the player bar docks. `Bottom` is the default and the fallback for any
/// unknown/hand-edited value (same tolerance posture as `get_bool`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerBarPosition {
    Top,
    Bottom,
}

pub(super) fn get_player_bar_position_in(conn: &Connection) -> PlayerBarPosition {
    match get_setting_in(conn, PLAYER_BAR_POSITION_KEY) {
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

pub(super) fn set_player_bar_position_in(
    conn: &Connection,
    pos: PlayerBarPosition,
) -> Result<(), rusqlite::Error> {
    let value = match pos {
        PlayerBarPosition::Top => "top",
        PlayerBarPosition::Bottom => "bottom",
    };
    set_setting_in(conn, PLAYER_BAR_POSITION_KEY, value)
}

pub const SIDEBAR_VISIBLE_KEY: &str = "ui.sidebar_visible";
pub const SIDEBAR_COLLAPSED_KEY: &str = "ui.sidebar_collapsed";
pub const BROWSE_VISIBLE_KEY: &str = "ui.browse_visible";
pub const STATUS_VISIBLE_KEY: &str = "ui.status_visible";
pub const INFO_PANEL_VISIBLE_KEY: &str = "ui.info_panel_visible";
pub const WINDOW_VIEW_MODE_KEY: &str = "ui.window_view_mode";
pub const COMPACT_LAYOUT_KEY: &str = "ui.compact_layout";
pub const WINDOW_DECORATION_MODE_KEY: &str = "ui.window_decoration_mode";
pub const COMPACT_ALWAYS_ON_TOP_KEY: &str = "ui.compact_always_on_top";
pub const COLOR_SCHEME_KEY: &str = "ui.color_scheme";
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
pub(super) fn get_window_view_mode_in(conn: &Connection) -> WindowViewMode {
    match typed_value(conn, WINDOW_VIEW_MODE_KEY, "library").as_str() {
        "compact" => WindowViewMode::Compact,
        "library" => WindowViewMode::Library,
        value => {
            tracing::warn!(value, "unrecognized window view mode; using Library");
            WindowViewMode::Library
        }
    }
}

pub(super) fn set_window_view_mode_in(
    conn: &Connection,
    value: WindowViewMode,
) -> Result<(), rusqlite::Error> {
    let value = match value {
        WindowViewMode::Library => "library",
        WindowViewMode::Compact => "compact",
    };
    set_setting_in(conn, WINDOW_VIEW_MODE_KEY, value)
}

pub(super) fn get_compact_always_on_top_in(conn: &Connection) -> bool {
    get_bool_in(conn, COMPACT_ALWAYS_ON_TOP_KEY, false).unwrap_or(false)
}

pub(super) fn set_compact_always_on_top_in(
    conn: &Connection,
    above: bool,
) -> Result<(), rusqlite::Error> {
    set_setting_in(
        conn,
        COMPACT_ALWAYS_ON_TOP_KEY,
        if above { BOOL_TRUE } else { BOOL_FALSE },
    )
}

pub(super) fn get_compact_layout_in(conn: &Connection) -> CompactLayout {
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

pub(super) fn set_compact_layout_in(
    conn: &Connection,
    value: CompactLayout,
) -> Result<(), rusqlite::Error> {
    let value = match value {
        CompactLayout::Cover => "cover",
        CompactLayout::Pill => "pill",
        CompactLayout::Card => "card",
    };
    set_setting_in(conn, COMPACT_LAYOUT_KEY, value)
}

pub(super) fn get_window_decoration_mode_in(conn: &Connection) -> WindowDecorationMode {
    match typed_value(conn, WINDOW_DECORATION_MODE_KEY, "client").as_str() {
        "system" => WindowDecorationMode::System,
        "client" => WindowDecorationMode::Client,
        value => {
            tracing::warn!(value, "unrecognized window decoration mode; using Client");
            WindowDecorationMode::Client
        }
    }
}

pub(super) fn set_window_decoration_mode_in(
    conn: &Connection,
    value: WindowDecorationMode,
) -> Result<(), rusqlite::Error> {
    let value = match value {
        WindowDecorationMode::Client => "client",
        WindowDecorationMode::System => "system",
    };
    set_setting_in(conn, WINDOW_DECORATION_MODE_KEY, value)
}

pub(super) fn get_sidebar_visible_in(conn: &Connection) -> bool {
    get_bool_in(conn, SIDEBAR_VISIBLE_KEY, true).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read sidebar visibility; using visible");
        true
    })
}

pub(super) fn set_sidebar_visible_in(
    conn: &Connection,
    value: bool,
) -> Result<(), rusqlite::Error> {
    set_bool_in(conn, SIDEBAR_VISIBLE_KEY, value)
}

/// Whether the user manually collapsed the sidebar column via the headerbar
/// toggle. Distinct from `SIDEBAR_VISIBLE_KEY` (the preferences switch that
/// removes the sidebar slot entirely): this remembers the in-window toggle
/// so the next session starts with the same layout.
pub(super) fn get_sidebar_collapsed_in(conn: &Connection) -> bool {
    get_bool_in(conn, SIDEBAR_COLLAPSED_KEY, false).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read sidebar collapse state; using expanded");
        false
    })
}

pub(super) fn set_sidebar_collapsed_in(
    conn: &Connection,
    collapsed: bool,
) -> Result<(), rusqlite::Error> {
    set_bool_in(conn, SIDEBAR_COLLAPSED_KEY, collapsed)
}

pub(super) fn get_browse_visible_in(conn: &Connection) -> bool {
    get_bool_in(conn, BROWSE_VISIBLE_KEY, true).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read browse bar visibility; using visible");
        true
    })
}

pub(super) fn set_browse_visible_in(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    set_bool_in(conn, BROWSE_VISIBLE_KEY, value)
}

pub(super) fn get_status_visible_in(conn: &Connection) -> bool {
    get_bool_in(conn, STATUS_VISIBLE_KEY, true).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read status visibility; using visible");
        true
    })
}

pub(super) fn set_status_visible_in(conn: &Connection, value: bool) -> Result<(), rusqlite::Error> {
    set_bool_in(conn, STATUS_VISIBLE_KEY, value)
}

pub(super) fn get_info_panel_visible_in(conn: &Connection) -> bool {
    get_bool_in(conn, INFO_PANEL_VISIBLE_KEY, false).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read information panel visibility; using hidden");
        false
    })
}

pub(super) fn set_info_panel_visible_in(
    conn: &Connection,
    visible: bool,
) -> Result<(), rusqlite::Error> {
    set_bool_in(conn, INFO_PANEL_VISIBLE_KEY, visible)
}
pub(super) fn get_color_scheme_in(conn: &Connection) -> &'static str {
    match get_setting_in(conn, COLOR_SCHEME_KEY).ok().flatten() {
        Some(ref v) if v == "light" => "light",
        Some(ref v) if v == "dark" => "dark",
        _ => "system",
    }
}

pub(super) fn set_color_scheme_in(conn: &Connection, value: &str) -> Result<(), rusqlite::Error> {
    set_setting_in(conn, COLOR_SCHEME_KEY, value)
}
