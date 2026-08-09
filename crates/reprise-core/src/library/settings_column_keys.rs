//! Persisted layout and width keys for editable table columns.

pub const COLUMN_LAYOUT_KEY: &str = "ui.column_layout";
/// User-adjusted per-column widths (`id:width` pairs), kept separate from the
/// order/visibility layout so the layout reducers and their tests stay untouched.
pub const COLUMN_WIDTHS_KEY: &str = "ui.column_widths";
pub const RELEASES_COLUMN_LAYOUT_KEY: &str = "ui.column_layout.releases";
pub const RELEASES_COLUMN_WIDTHS_KEY: &str = "ui.column_widths.releases";
pub const CONCERTS_COLUMN_LAYOUT_KEY: &str = "ui.column_layout.concerts";
pub const CONCERTS_COLUMN_WIDTHS_KEY: &str = "ui.column_widths.concerts";
pub const RADIO_COLUMN_LAYOUT_KEY: &str = "ui.column_layout.radio";
pub const RADIO_COLUMN_WIDTHS_KEY: &str = "ui.column_widths.radio";
