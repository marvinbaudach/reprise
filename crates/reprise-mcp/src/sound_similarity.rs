//! The Sound Similarity module over MCP: ranked neighbours for one track and
//! that track's stored profile.
//!
//! Every number here comes out of `reprise-core` — `load_sound_candidates`,
//! `compute_sound_stats`, `rank_sound_neighbours`, `profile_positions` and
//! `load_sound_file_info` — so this server and the GTK Sound tab answer the
//! same question the same way (`SIM-3`, `SIM-4`, `SIM-9`, `SIM-10`). Nothing
//! is re-derived here; the module only shapes parameters and responses.
//!
//! Both tools degrade honestly rather than returning an empty list: a library
//! whose profiles have not been derived yet, and a track that carries no
//! profile of its own, are reported as distinct states alongside the
//! `(ready, total)` inventory.

use std::path::Path;

use reprise_core::db::Db;
use reprise_core::modules::SOUND_SIMILARITY_MODULE;
use reprise_core::sound_features::SoundFeatures;
use reprise_core::sound_file_info::SoundFileInfo;
use reprise_core::sound_neighbours::{
    load_sound_candidates, rank_sound_neighbours, SoundCandidate, SoundNeighbourOptions,
};
use reprise_core::sound_preferences::SoundWeighting;
use reprise_core::sound_profile::profile_positions;
use reprise_core::sound_stats::compute_sound_stats;
use rmcp::schemars;
use serde::{Deserialize, Serialize};

use crate::data::{self, DataError};

/// How many matches a caller gets without asking. The module's own shipped
/// default (`SIM-5`/`SIM-6`), so both surfaces answer alike.
const DEFAULT_MATCH_LIMIT: usize = 7;
/// The match-count range the Sound Similarity preferences accept; a caller may
/// not push the tool past what the module itself offers.
const MIN_MATCH_LIMIT: usize = 1;
const MAX_MATCH_LIMIT: usize = 50;

/// Every match was ranked and returned.
const STATUS_RANKED: &str = "ranked";
/// Ranking is possible in principle, but this track carries no profile yet.
const STATUS_TRACK_NOT_ANALYSED: &str = "track_not_analysed";
/// The library carries no current profile at all — the normal state until the
/// backfill has run.
const STATUS_NO_PROFILES_YET: &str = "no_profiles_yet";
/// The profile was read and is complete.
const STATUS_READY: &str = "ready";

/// Parameters for `music_similar_tracks`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SimilarTracksParams {
    /// The track to find neighbours for. It must be present in the library.
    pub track_id: i64,
    /// How many matches to return (1..=50, default 7).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Which weighting to rank with: `default`, `timbre` or `dynamics`
    /// (default `default`).
    #[serde(default)]
    pub weighting: Option<String>,
    /// Whether tracks from the current track's album are excluded. Defaults to
    /// `true`, the module's shipped default.
    #[serde(default)]
    pub exclude_same_album: Option<bool>,
    /// Whether tracks by the current track's artist are excluded. Defaults to
    /// `false`, the module's shipped default — at most two matches carry the
    /// same artist either way (`SIM-10`).
    #[serde(default)]
    pub exclude_same_artist: Option<bool>,
}

/// Parameters for `music_sound_profile`.
#[derive(Debug, Clone, Deserialize, schemars::JsonSchema)]
pub struct SoundProfileParams {
    /// The track whose stored profile to read. It must be present in the
    /// library.
    pub track_id: i64,
}

/// One ranked neighbour — identity enough to act on plus its two numbers.
/// Never a file path (D19).
#[derive(Debug, Clone, Serialize)]
pub struct SimilarTrackDto {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    /// Weighted distance in the comparison space; smaller is nearer.
    pub distance: f32,
    /// Share of the compared population that lies farther away, ranked before
    /// any exclusion is applied (`SIM-3`).
    pub percentile: f32,
}

/// The `music_similar_tracks` result body.
#[derive(Debug, Clone, Serialize)]
pub struct SimilarTracksResult {
    pub track_id: i64,
    /// `ranked`, `track_not_analysed`, or `no_profiles_yet`.
    pub status: &'static str,
    /// Matches in nearest-first order; empty unless `status` is `ranked`.
    pub matches: Vec<SimilarTrackDto>,
    /// How many other profiled tracks the ranks were formed over.
    pub compared_tracks: usize,
    /// Library tracks carrying a current sound profile.
    pub profiles_ready: usize,
    /// Present library tracks in total.
    pub library_tracks: usize,
    /// Whether the optional Sound Similarity module is switched on; profiles
    /// are only derived while it is.
    pub module_enabled: bool,
    /// The effective options this answer used.
    pub weighting: &'static str,
    pub exclude_same_album: bool,
    pub exclude_same_artist: bool,
    pub limit: usize,
    /// A plain-language note on the readiness above, so an empty list is never
    /// mistaken for "nothing sounds similar".
    pub readiness_hint: String,
}

/// The three axis positions the Sound tab shows, as library-wide percentiles
/// (0..=100).
#[derive(Debug, Clone, Serialize)]
pub struct SoundAxesDto {
    /// Timbre, dark (0) to bright (100).
    pub timbre: f32,
    /// Dynamics, dense (0) to open (100).
    pub dynamics: f32,
    /// Tempo, slow (0) to fast (100); null when the track carries no stable
    /// tempo estimate.
    pub tempo: Option<f32>,
}

/// The file facts shown beside the profile. Format and size only — never a
/// path (D19).
#[derive(Debug, Clone, Serialize)]
pub struct SoundFileDto {
    /// Uppercase container extension, e.g. `FLAC`.
    pub format: String,
    pub bit_depth: Option<u8>,
    pub sample_rate_hz: Option<u32>,
    pub bitrate_kbps: Option<u32>,
    pub file_size_bytes: u64,
    /// Highest frequency the stored spectrogram still shows energy in; null
    /// while no spectrogram is cached.
    pub occupied_upper_hz: Option<u32>,
}

/// The `music_sound_profile` result body.
#[derive(Debug, Clone, Serialize)]
pub struct SoundProfileResult {
    pub track_id: i64,
    /// `ready`, `track_not_analysed`, or `no_profiles_yet`.
    pub status: &'static str,
    /// The axis positions, or null while the track has no profile.
    pub axes: Option<SoundAxesDto>,
    /// The file line; present whenever the track's row could be read.
    pub file: Option<SoundFileDto>,
    pub profiles_ready: usize,
    pub library_tracks: usize,
    pub module_enabled: bool,
    pub readiness_hint: String,
}

/// The library-wide state both tools answer against, read from one database
/// handle so the inventory and the candidates describe the same snapshot.
struct SoundLibrary {
    ready: usize,
    total: usize,
    module_enabled: bool,
    candidates: Vec<SoundCandidate>,
}

impl SoundLibrary {
    fn load(db: &Db) -> Result<Self, DataError> {
        let (ready, total) = reprise_core::db::sound_feature_inventory(db).map_err(db_error)?;
        Ok(Self {
            ready,
            total,
            module_enabled: reprise_core::modules::is_enabled(db, &SOUND_SIMILARITY_MODULE)
                .map_err(DataError::Db)?,
            candidates: load_sound_candidates(db).map_err(db_error)?,
        })
    }

    fn current(&self, track_id: i64) -> Option<&SoundCandidate> {
        self.candidates
            .iter()
            .find(|candidate| candidate.track_id == track_id)
    }

    /// The status of a track that carries no profile: a library with none at
    /// all is a different answer from one that simply has not reached this
    /// track yet.
    fn missing_status(&self) -> &'static str {
        if self.ready == 0 {
            STATUS_NO_PROFILES_YET
        } else {
            STATUS_TRACK_NOT_ANALYSED
        }
    }

    /// Library statistics over exactly the population the ranking compares
    /// against — the same derivation `SoundStatsCache` performs for the GTK
    /// panel, computed from the candidates already in hand rather than by
    /// reading them a second time.
    fn stats(&self) -> reprise_core::sound_stats::SoundStats {
        let features: Vec<SoundFeatures> = self
            .candidates
            .iter()
            .map(|candidate| candidate.features.clone())
            .collect();
        compute_sound_stats(&features)
    }
}

/// Ranks the library against one track.
pub fn similar_tracks(
    path: &Path,
    params: &SimilarTracksParams,
) -> Result<SimilarTracksResult, DataError> {
    let db = data::open(path)?;
    data::require_read(&db)?;
    let weighting = resolve_weighting(params.weighting.as_deref())?;
    // The module's own shipped defaults, overridden per request (`SIM-6`).
    let shipped = SoundNeighbourOptions::default();
    let options = SoundNeighbourOptions {
        exclude_same_album: params
            .exclude_same_album
            .unwrap_or(shipped.exclude_same_album),
        exclude_same_artist: params
            .exclude_same_artist
            .unwrap_or(shipped.exclude_same_artist),
        limit: resolve_limit(params.limit),
    };
    data::reject_absent_track_ids(&db, &[params.track_id])?;

    let library = SoundLibrary::load(&db)?;
    let Some(current) = library.current(params.track_id) else {
        let status = library.missing_status();
        return Ok(SimilarTracksResult {
            track_id: params.track_id,
            status,
            matches: Vec::new(),
            compared_tracks: 0,
            profiles_ready: library.ready,
            library_tracks: library.total,
            module_enabled: library.module_enabled,
            weighting: weighting.setting(),
            exclude_same_album: options.exclude_same_album,
            exclude_same_artist: options.exclude_same_artist,
            limit: options.limit,
            readiness_hint: readiness_hint(status, &library, 0),
        });
    };

    let ranked = rank_sound_neighbours(
        current,
        &library.candidates,
        &library.stats(),
        weighting.weights(),
        options,
    );
    Ok(SimilarTracksResult {
        track_id: params.track_id,
        status: STATUS_RANKED,
        matches: ranked
            .matches
            .iter()
            .map(|neighbour| SimilarTrackDto {
                track_id: neighbour.track_id,
                title: neighbour.title.clone(),
                artist: neighbour.artist.clone(),
                album: neighbour.album.clone(),
                album_artist: neighbour.album_artist.clone(),
                distance: neighbour.distance,
                percentile: neighbour.percentile,
            })
            .collect(),
        compared_tracks: ranked.library_count,
        profiles_ready: library.ready,
        library_tracks: library.total,
        module_enabled: library.module_enabled,
        weighting: weighting.setting(),
        exclude_same_album: options.exclude_same_album,
        exclude_same_artist: options.exclude_same_artist,
        limit: options.limit,
        readiness_hint: readiness_hint(STATUS_RANKED, &library, ranked.library_count),
    })
}

/// Reads one track's stored profile: the three axis positions and the file
/// line. Tempo is always included here — the panel's tempo switch changes the
/// ranking, not what the stored profile says — so the axis is null only when
/// no stable estimate was found.
pub fn sound_profile(
    path: &Path,
    params: &SoundProfileParams,
) -> Result<SoundProfileResult, DataError> {
    let db = data::open(path)?;
    data::require_read(&db)?;
    data::reject_absent_track_ids(&db, &[params.track_id])?;

    let library = SoundLibrary::load(&db)?;
    let file = reprise_core::sound_file_info::load_sound_file_info(&db, params.track_id)
        .map_err(db_error)?
        .as_ref()
        .map(file_dto);
    let Some(current) = library.current(params.track_id) else {
        let status = library.missing_status();
        return Ok(SoundProfileResult {
            track_id: params.track_id,
            status,
            axes: None,
            file,
            profiles_ready: library.ready,
            library_tracks: library.total,
            module_enabled: library.module_enabled,
            readiness_hint: readiness_hint(status, &library, 0),
        });
    };

    let positions = profile_positions(&current.features, &library.stats(), true);
    Ok(SoundProfileResult {
        track_id: params.track_id,
        status: STATUS_READY,
        axes: Some(SoundAxesDto {
            timbre: positions.timbre,
            dynamics: positions.dynamics,
            tempo: positions.tempo,
        }),
        file,
        profiles_ready: library.ready,
        library_tracks: library.total,
        module_enabled: library.module_enabled,
        readiness_hint: readiness_hint(STATUS_READY, &library, 0),
    })
}

fn file_dto(info: &SoundFileInfo) -> SoundFileDto {
    SoundFileDto {
        format: info.format.clone(),
        bit_depth: info.bit_depth,
        sample_rate_hz: info.sample_rate_hz,
        bitrate_kbps: info.bitrate_kbps,
        file_size_bytes: info.file_size,
        occupied_upper_hz: info.occupied_upper_hz,
    }
}

/// Maps the weighting token to the module's own weighting. An unknown token is
/// caller-fixable input rather than a silent fall back to Default: an agent
/// that misspells `timbre` must learn that, not receive a differently ranked
/// list without being told.
fn resolve_weighting(value: Option<&str>) -> Result<SoundWeighting, DataError> {
    let Some(value) = value else {
        return Ok(SoundWeighting::default());
    };
    SoundWeighting::from_setting_name(value.trim()).ok_or_else(|| {
        let offered = SoundWeighting::ALL.map(SoundWeighting::setting).join(", ");
        DataError::InvalidInput(format!(
            "unknown weighting '{value}'; expected one of {offered}"
        ))
    })
}

fn resolve_limit(limit: Option<u32>) -> usize {
    match limit {
        None => DEFAULT_MATCH_LIMIT,
        Some(requested) => (requested as usize).clamp(MIN_MATCH_LIMIT, MAX_MATCH_LIMIT),
    }
}

/// The honest note that goes with every answer: what the inventory says, and —
/// when nothing could be ranked — why, including a module that is switched off
/// and therefore derives nothing.
fn readiness_hint(status: &str, library: &SoundLibrary, compared: usize) -> String {
    let inventory = format!(
        "{} of {} present library track(s) carry a current sound profile.",
        library.ready, library.total
    );
    let mut hint = match status {
        STATUS_RANKED => format!("Ranked against {compared} other profiled track(s). {inventory}"),
        STATUS_READY => inventory,
        STATUS_TRACK_NOT_ANALYSED => {
            format!("This track has no sound profile yet, so it cannot be compared. {inventory}")
        }
        // `STATUS_NO_PROFILES_YET` — the only state left, and the normal one
        // until the backfill has run at least once.
        _ => format!("No sound profile has been derived yet. {inventory}"),
    };
    if !library.module_enabled {
        hint.push_str(
            " The Sound Similarity module is switched off, so no further profiles are being \
             derived; enable it in Reprise and leave the app running.",
        );
    } else if library.ready < library.total {
        hint.push_str(
            " Profiles are derived in the background while the Reprise app runs; ask again later.",
        );
    }
    hint
}

/// Folds the engine's combined open-or-query failure into this crate's two
/// internal channels, so a query fault is not reported as an open fault.
/// Neither reaches the caller: both are logged and answered opaquely.
fn db_error(error: reprise_core::db::DbError) -> DataError {
    match error {
        reprise_core::db::DbError::Sqlite(error) => DataError::Db(error),
        other => DataError::Open(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weighting_defaults_and_rejects_an_unknown_token() {
        assert_eq!(resolve_weighting(None).unwrap(), SoundWeighting::Default);
        assert_eq!(
            resolve_weighting(Some("dynamics")).unwrap(),
            SoundWeighting::Dynamics
        );
        assert!(matches!(
            resolve_weighting(Some("loudness")),
            Err(DataError::InvalidInput(_))
        ));
    }

    #[test]
    fn limit_defaults_to_the_modules_own_seven_and_stays_in_range() {
        assert_eq!(resolve_limit(None), DEFAULT_MATCH_LIMIT);
        assert_eq!(resolve_limit(Some(0)), MIN_MATCH_LIMIT);
        assert_eq!(resolve_limit(Some(3)), 3);
        assert_eq!(resolve_limit(Some(5_000)), MAX_MATCH_LIMIT);
    }
}
