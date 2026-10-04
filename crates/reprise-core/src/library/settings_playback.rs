use rusqlite::Connection;

use super::{get_bool_in, get_setting_in, set_bool_in, set_setting_in, typed_value};

pub const EQUALIZER_ENABLED_KEY: &str = "playback.equalizer_enabled";
pub const EQUALIZER_CURVE_KEY: &str = crate::db_equalizer::EQUALIZER_CURVE_KEY;
pub const REPLAY_GAIN_MODE_KEY: &str = "playback.replay_gain_mode";
pub const GAPLESS_ENABLED_KEY: &str = "playback.gapless_enabled";
pub const VOLUME_KEY_SKIP_GESTURE_ENABLED_KEY: &str = "playback.volume_key_skip_gesture_enabled";
pub const CROSSFADE_SECONDS_KEY: &str = "playback.crossfade_seconds";
/// Crossfade overlap in whole seconds. `0` means crossfade is off (the slider's
/// "Off" position); `1..=MAX` is an active overlap. `DEFAULT` (off) applies when
/// the stored value is missing or out of range. The `TrackTransition` mode is
/// *derived* from this plus `GAPLESS_ENABLED_KEY` (see `get_track_transition`):
/// any crossfade > 0 wins, else gapless-on means Gapless, else Off.
pub const CROSSFADE_SECONDS_MIN: u8 = 0;
pub const CROSSFADE_SECONDS_MAX: u8 = 10;
pub const CROSSFADE_SECONDS_DEFAULT: u8 = 0;
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
pub(in crate::library) fn get_equalizer_enabled_in(conn: &Connection) -> bool {
    get_bool_in(conn, EQUALIZER_ENABLED_KEY, false).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read equalizer state; using disabled");
        false
    })
}

pub(in crate::library) fn set_equalizer_enabled_in(
    conn: &Connection,
    value: bool,
) -> Result<(), rusqlite::Error> {
    set_bool_in(conn, EQUALIZER_ENABLED_KEY, value)
}

/// The ten-band *picture* of the stored curve, for backends whose equalizer is
/// GStreamer's fixed ten centres. Never persisted: see
/// [`set_equalizer_bands_in`].
pub(in crate::library) fn get_equalizer_bands_in(conn: &Connection) -> [f64; 10] {
    get_equalizer_curve_in(conn).project_to_gstreamer()
}

/// Replaces the whole curve with ten values at GStreamer's centres.
///
/// The write is accepted even when the stored curve was authored somewhere
/// else — refusing it would leave the desktop equalizer looking alive and
/// doing nothing, which is a worse failure than the loss it would prevent. But
/// it is no longer *silent*: a ten-slider surface can only express those ten
/// centres, so when the stored curve is some other shape (a phone's five
/// bands, say, carried over in a copied library) nine of the ten values being
/// written back are projections of it rather than authored points, and the
/// authored ones do not survive. That is a decision on the record here, not an
/// accident in the caller.
///
/// A real answer needs the editing surface to say what it is about to replace,
/// and `crates/reprise-gnome` may not change in this package — see the M6
/// residual note in `docs/superpowers/plans/2026-08-04-mobile-m3.md`.
pub(in crate::library) fn set_equalizer_bands_in(
    conn: &Connection,
    values: [f64; 10],
) -> Result<(), rusqlite::Error> {
    let stored = get_equalizer_curve_in(conn);
    if !stored.is_gstreamer_ten_band() {
        tracing::warn!(
            stored_points = stored.points().len(),
            "a ten-band equalizer edit is replacing a curve authored on another backend; \
             its points are lost and the written values are a projection of it"
        );
    }
    set_equalizer_curve_in(
        conn,
        &crate::equalizer::EqualizerCurve::from_gstreamer_levels(values),
    )
}

pub(in crate::library) fn get_equalizer_curve_in(
    conn: &Connection,
) -> crate::equalizer::EqualizerCurve {
    let Some(value) = get_setting_in(conn, EQUALIZER_CURVE_KEY).ok().flatten() else {
        return crate::equalizer::EqualizerCurve::flat_gstreamer();
    };
    crate::equalizer::EqualizerCurve::parse(&value).unwrap_or_else(|error| {
        tracing::warn!(%error, "invalid equalizer curve; using flat preset");
        crate::equalizer::EqualizerCurve::flat_gstreamer()
    })
}

pub(in crate::library) fn set_equalizer_curve_in(
    conn: &Connection,
    curve: &crate::equalizer::EqualizerCurve,
) -> Result<(), rusqlite::Error> {
    set_setting_in(conn, EQUALIZER_CURVE_KEY, &curve.serialize())
}

pub(in crate::library) fn get_replay_gain_mode_in(conn: &Connection) -> ReplayGainMode {
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

pub(in crate::library) fn set_replay_gain_mode_in(
    conn: &Connection,
    value: ReplayGainMode,
) -> Result<(), rusqlite::Error> {
    let value = match value {
        ReplayGainMode::Off => "off",
        ReplayGainMode::Track => "track",
        ReplayGainMode::Album => "album",
    };
    set_setting_in(conn, REPLAY_GAIN_MODE_KEY, value)
}

/// Whether gapless playback is enabled. Independent of crossfade: it only takes
/// effect (as the `Gapless` transition) when no crossfade overlap is set.
/// Default `true` — the expected modern behavior for a music player.
pub(super) fn get_gapless_enabled_in(conn: &Connection) -> bool {
    get_bool_in(conn, GAPLESS_ENABLED_KEY, true).unwrap_or(true)
}

pub(super) fn set_gapless_enabled_in(
    conn: &Connection,
    enabled: bool,
) -> Result<(), rusqlite::Error> {
    set_bool_in(conn, GAPLESS_ENABLED_KEY, enabled)
}

/// Whether a volume-key rock skips tracks while playback owns remote volume.
/// Default `true`; Android is currently the only surface that exposes it.
pub(super) fn get_volume_key_skip_gesture_enabled_in(conn: &Connection) -> bool {
    get_bool_in(conn, VOLUME_KEY_SKIP_GESTURE_ENABLED_KEY, true).unwrap_or(true)
}

pub(super) fn set_volume_key_skip_gesture_enabled_in(
    conn: &Connection,
    enabled: bool,
) -> Result<(), rusqlite::Error> {
    set_bool_in(conn, VOLUME_KEY_SKIP_GESTURE_ENABLED_KEY, enabled)
}

/// The effective transition mode, *derived* from the two independent playback
/// preferences (`crossfade_seconds` + `gapless_enabled`): any crossfade overlap
/// wins, else gapless-on means `Gapless`, else `Off`. There is no separately
/// stored mode — the two controls in the Audio Transitions settings are the
/// single source of truth.
pub(super) fn get_track_transition_in(conn: &Connection) -> TrackTransition {
    if get_crossfade_seconds_in(conn) > 0 {
        TrackTransition::Crossfade
    } else if get_gapless_enabled_in(conn) {
        TrackTransition::Gapless
    } else {
        TrackTransition::Off
    }
}

pub(super) fn get_crossfade_seconds_in(conn: &Connection) -> u8 {
    typed_value(conn, CROSSFADE_SECONDS_KEY, "")
        .parse::<u8>()
        .ok()
        .filter(|s| (CROSSFADE_SECONDS_MIN..=CROSSFADE_SECONDS_MAX).contains(s))
        .unwrap_or(CROSSFADE_SECONDS_DEFAULT)
}

pub(super) fn set_crossfade_seconds_in(
    conn: &Connection,
    seconds: u8,
) -> Result<(), rusqlite::Error> {
    let clamped = seconds.clamp(CROSSFADE_SECONDS_MIN, CROSSFADE_SECONDS_MAX);
    set_setting_in(conn, CROSSFADE_SECONDS_KEY, &clamped.to_string())
}
