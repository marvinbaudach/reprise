use rusqlite::Connection;

use super::{get_setting_in, set_setting_in};

pub const MISSING_AUTO_CLEAN_KEY: &str = "missing_auto_clean";
/// Unix-seconds "clock start" for auto-clean's grace period — see
/// [`AutoCleanSetting`]'s doc comment for why arming, not a track's own
/// `missing_since`, is the deadline's floor.
pub const AUTO_CLEAN_ARMED_AT_KEY: &str = "auto_clean_armed_at";

/// The "Missing files auto-clean" preference (Task 2.3): how long a
/// `missing_reason = 'deleted'` track must sit in the Deleted group before
/// it is hard-deleted unattended — see `queries::auto_clean_eligible`'s doc
/// comment for why `unmounted`/`unknown` rows can never become eligible no
/// matter this setting's value. `Off` is the only default this setting may
/// ever fall back to, both for a never-written key and for an unrecognized
/// stored value: auto-clean is destructive and irreversible (see `queries::
/// run_auto_clean`'s doc comment), so a corrupted or hand-edited setting
/// must degrade to "do nothing", never to "guess a duration and start
/// deleting".
///
/// `Days(u32)` is written as its decimal digits (`"30"`, `"90"` today), not
/// modeled as a fixed two-variant closed set — the GUI (a later task) is the
/// sole place that decides which day counts are offered; this type stays
/// honest about the setting's actual storage shape rather than baking
/// today's two choices into the type itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoCleanSetting {
    Off,
    Days(u32),
}

impl AutoCleanSetting {
    /// The exact string stored in `settings.value` for key
    /// `MISSING_AUTO_CLEAN_KEY` — the inverse of [`Self::parse`]. Returns an
    /// owned `String` rather than the `&'static str` `MissingReason::as_str`/
    /// `ImportErrorKind::as_str` return, because `Days`'s payload is a
    /// runtime value, not one of a small fixed set of variants.
    pub fn as_str(&self) -> String {
        match self {
            Self::Off => "off".to_string(),
            Self::Days(days) => days.to_string(),
        }
    }

    /// Parses a `MISSING_AUTO_CLEAN_KEY` value back into a setting. Anything
    /// that isn't `"off"` or a valid non-negative integer — including a
    /// value this version of the app has never written — falls back to
    /// `Off`, never erroring: see this type's own doc comment for why a
    /// destructive setting's fallback must always be the inert one.
    ///
    /// `Days(0)` is accepted as-is and is a REAL, IMMEDIATE-DELETION value:
    /// `queries::auto_clean_eligible`'s deadline is `max(missing_since,
    /// armed_at) + days*86400 <= now`, so `days == 0` collapses that to
    /// `max(missing_since, armed_at) <= now` — every currently-armed
    /// `deleted` row qualifies the instant `run_auto_clean` next runs, no
    /// grace period at all. This is deliberately not rejected here (YAGNI:
    /// the GUI is the only writer today and only ever offers off/30/90), but
    /// a future writer that passes an untrusted/hand-edited duration through
    /// unchecked would be handing this parser an immediate-delete switch.
    pub fn parse(s: &str) -> Self {
        match s {
            "off" => Self::Off,
            other => other.parse::<u32>().map_or(Self::Off, Self::Days),
        }
    }
}

/// Reads the current auto-clean preference, defaulting to
/// [`AutoCleanSetting::Off`] both when the key was never written and when
/// the stored value is unrecognized/corrupt or the read itself fails — see
/// the type's own doc comment for why `Off` is the only safe fallback for a
/// destructive setting.
pub(super) fn get_missing_auto_clean_in(conn: &Connection) -> AutoCleanSetting {
    match get_setting_in(conn, MISSING_AUTO_CLEAN_KEY) {
        Ok(Some(value)) => AutoCleanSetting::parse(&value),
        Ok(None) => AutoCleanSetting::Off,
        Err(error) => {
            tracing::warn!(%error, "could not read missing_auto_clean; using Off");
            AutoCleanSetting::Off
        }
    }
}

pub(super) fn set_missing_auto_clean_in(
    conn: &Connection,
    value: AutoCleanSetting,
) -> Result<(), rusqlite::Error> {
    set_setting_in(conn, MISSING_AUTO_CLEAN_KEY, &value.as_str())
}

/// Reads the unix-seconds timestamp auto-clean's grace period is measured
/// from, or `None` if the setting has never been armed. `None` here is the
/// fail-safe signal `queries::auto_clean_eligible` treats as "the feature is
/// inert" (Task 2.3 constraint: a `missing_auto_clean` duration alone must
/// never run without an explicit arming date) — including when the stored
/// value fails to parse as an integer, the same fallback direction
/// `get_missing_auto_clean` takes for a corrupt duration.
pub(crate) fn get_auto_clean_armed_at_in(
    conn: &Connection,
) -> Result<Option<i64>, rusqlite::Error> {
    let stored = get_setting_in(conn, AUTO_CLEAN_ARMED_AT_KEY)?;
    Ok(stored.and_then(|value| value.parse::<i64>().ok()))
}

/// Arms (or re-arms) auto-clean's grace period, starting its clock at
/// `armed_at`. The GUI (a later task) writes `armed_at = now` when the user
/// turns the setting on over an existing backlog and picks "start counting
/// from today" — `tracks.missing_since` is left untouched by this call, so
/// `queries::auto_clean_eligible`'s `max(missing_since, armed_at)` picks
/// whichever is later, never retroactively sweeping a backlog the instant
/// the setting is enabled.
pub(crate) fn set_auto_clean_armed_at_in(
    conn: &Connection,
    armed_at: i64,
) -> Result<(), rusqlite::Error> {
    set_setting_in(conn, AUTO_CLEAN_ARMED_AT_KEY, &armed_at.to_string())
}
