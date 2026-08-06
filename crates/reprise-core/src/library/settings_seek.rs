//! The seek bar's two persisted decisions: how it is coloured, and how often
//! its colour-scale legend has already explained itself.
//!
//! Split out of `settings.rs` to keep that file under the project's 800-line
//! cap; the accessors are re-exported there, so callers see one module.

use rusqlite::Connection;

use super::{set_setting_in, typed_value};

pub const SEEK_COLOURING_KEY: &str = "ui.seek_colouring";
pub const SEEK_LEGEND_SEEN_KEY: &str = "ui.seek_legend_seen";

/// How many track changes the colour-scale legend appears for before it stops
/// appearing on its own.
///
/// A count, not a timestamp: "seen it three times" is a better measure of
/// "understood it" than "shown two days ago". After that it stays reachable
/// from the seek bar's context menu — a one-off hint nobody can call back is a
/// trap for everyone who missed it the first time.
pub const SEEK_LEGEND_SHOWS: u32 = 3;

/// How the seek bar is coloured.
///
/// Two colourings, not a feature and its "off": `Solid` is the quieter one and
/// a legitimate taste, so it says what it does rather than what it lacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeekColouring {
    /// The played and the coming side both carry the track's own frequency
    /// centroid, averaged over time; progress reads as an opacity step.
    Frequency,
    /// Played in the accent, coming in grey, with hairlines where the music
    /// changes.
    Solid,
}

impl SeekColouring {
    pub const DEFAULT: Self = Self::Frequency;

    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Frequency => "frequency",
            Self::Solid => "solid",
        }
    }

    #[must_use]
    pub fn from_id(value: &str) -> Self {
        match value {
            "solid" => Self::Solid,
            "frequency" => Self::Frequency,
            other => {
                tracing::warn!(
                    value = other,
                    "unrecognized seek colouring; using Frequency"
                );
                Self::DEFAULT
            }
        }
    }
}

pub(super) fn get_seek_colouring_in(conn: &Connection) -> SeekColouring {
    SeekColouring::from_id(&typed_value(
        conn,
        SEEK_COLOURING_KEY,
        SeekColouring::DEFAULT.id(),
    ))
}

pub(super) fn set_seek_colouring_in(
    conn: &Connection,
    value: SeekColouring,
) -> Result<(), rusqlite::Error> {
    set_setting_in(conn, SEEK_COLOURING_KEY, value.id())
}

/// How often the colour-scale legend has appeared on its own so far, capped at
/// [`SEEK_LEGEND_SHOWS`]. A stored value that is missing or unparseable counts
/// as "never shown": the cost of showing a small legend once more is lower than
/// the cost of never explaining the scale at all.
pub(super) fn get_seek_legend_seen_in(conn: &Connection) -> u32 {
    typed_value(conn, SEEK_LEGEND_SEEN_KEY, "0")
        .parse::<u32>()
        .unwrap_or(0)
        .min(SEEK_LEGEND_SHOWS)
}

pub(super) fn set_seek_legend_seen_in(
    conn: &Connection,
    count: u32,
) -> Result<(), rusqlite::Error> {
    set_setting_in(
        conn,
        SEEK_LEGEND_SEEN_KEY,
        &count.min(SEEK_LEGEND_SHOWS).to_string(),
    )
}
