//! Everything the Sound surfaces show for one track, computed without a window.
//!
//! The GTK Sound tab used to derive this itself, which put a database read, the
//! ranking and the readiness rules inside the frontend. None of it needs a
//! widget: a snapshot is a pure function of the library and the caller's
//! options, so it lives here and every frontend renders the same answer
//! (`SIM-4`, `SIM-6`).

use crate::db::{Db, DbError};
use crate::sound_distance::DistanceWeights;
use crate::sound_file_info::SoundFileInfo;
use crate::sound_neighbours::{rank_sound_neighbours, SoundNeighbourOptions, SoundNeighbourResult};
use crate::sound_preferences::SoundSimilarityPreferences;
use crate::sound_profile::{profile_positions, ProfilePositions};
use crate::sound_stats::SoundStatsCache;

/// How many current profiles the library needs before matches are shown at all.
pub const MIN_READY_FEATURES: usize = 50;

/// How many identical inventory readings in a row end the re-checks. The
/// backfill stores one track at a time, so a library that is still catching up
/// moves the counts well inside this budget; ten seconds of complete standstill
/// mean nothing is deriving profiles any more and re-checking cannot help.
pub const PROGRESS_STALL_LIMIT: usize = 20;

/// What the caller wants ranked, in the shape the stored preferences take.
#[derive(Debug, Clone, Copy)]
pub struct SoundSnapshotOptions {
    pub exclude_same_album: bool,
    pub exclude_same_artist: bool,
    pub include_tempo: bool,
    pub weights: DistanceWeights,
    pub limit: usize,
}

impl Default for SoundSnapshotOptions {
    fn default() -> Self {
        Self {
            exclude_same_album: true,
            exclude_same_artist: false,
            include_tempo: false,
            weights: DistanceWeights::DEFAULT,
            limit: 7,
        }
    }
}

impl From<SoundSimilarityPreferences> for SoundSnapshotOptions {
    fn from(preferences: SoundSimilarityPreferences) -> Self {
        Self {
            exclude_same_album: preferences.exclude_same_album,
            exclude_same_artist: preferences.exclude_same_artist,
            include_tempo: preferences.include_tempo,
            weights: preferences.weighting.weights(),
            limit: preferences.match_count,
        }
    }
}

/// One complete answer for one track: what a frontend draws, and nothing about
/// how it draws it.
#[derive(Debug, Clone)]
pub enum SoundSnapshot {
    Progress {
        ready: usize,
        total: usize,
    },
    Ready {
        profile: ProfilePositions,
        file_info: Option<SoundFileInfo>,
        neighbours: SoundNeighbourResult,
    },
    /// The inventory stopped advancing before it could carry this track, so
    /// waiting longer changes nothing until something asks again.
    Unavailable,
    Error(String),
}

/// Watches whether the profile inventory still advances between re-checks.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProgressWatch {
    inventory: Option<(usize, usize)>,
    stalled: usize,
}

impl ProgressWatch {
    /// Folds one `(ready, total)` reading in. `None` means the counts have stood
    /// still for `PROGRESS_STALL_LIMIT` readings: the library is not catching up
    /// any more, so the caller settles instead of polling for the rest of the
    /// session. A later request starts a fresh watch.
    #[must_use]
    pub fn observe(self, inventory: (usize, usize)) -> Option<Self> {
        if self.inventory != Some(inventory) {
            return Some(Self {
                inventory: Some(inventory),
                stalled: 0,
            });
        }
        (self.stalled < PROGRESS_STALL_LIMIT).then_some(Self {
            inventory: self.inventory,
            stalled: self.stalled + 1,
        })
    }
}

/// A disabled module does no sound work at all: no worker thread, no library
/// query, no ranking. A frontend still remembers the track it was told about, so
/// switching the module on picks it up without waiting for the next track.
pub fn sound_work_allowed(enabled: bool, has_database: bool) -> bool {
    enabled && has_database
}

pub fn ready_for_matches(feature_count: usize, current_present: bool) -> bool {
    feature_count >= MIN_READY_FEATURES && current_present
}

/// The snapshot for `track_id`, or the progress the library has made towards
/// one. `stats_cache` is the caller's, so consecutive requests reuse the library
/// statistics instead of recomputing them per track.
pub fn sound_snapshot(
    db: &Db,
    stats_cache: &mut SoundStatsCache,
    track_id: i64,
    options: SoundSnapshotOptions,
) -> SoundSnapshot {
    calculate(db, stats_cache, track_id, options)
        .unwrap_or_else(|error| SoundSnapshot::Error(error.to_string()))
}

fn calculate(
    db: &Db,
    stats_cache: &mut SoundStatsCache,
    track_id: i64,
    options: SoundSnapshotOptions,
) -> Result<SoundSnapshot, DbError> {
    let (ready, total) = crate::db::sound_feature_inventory(db)?;
    let candidates = crate::sound_neighbours::load_sound_candidates(db)?;
    let current = candidates
        .iter()
        .find(|candidate| candidate.track_id == track_id);
    if !ready_for_matches(ready, current.is_some()) {
        return Ok(SoundSnapshot::Progress { ready, total });
    }
    stats_cache.refresh(db)?;
    let stats = stats_cache
        .stats()
        .expect("refresh installs sound statistics");
    let current = current.expect("readiness requires current features");
    let profile = profile_positions(&current.features, stats, options.include_tempo);
    let weights = if options.include_tempo {
        options.weights.with_tempo(true)
    } else {
        options.weights
    };
    let neighbours = rank_sound_neighbours(
        current,
        &candidates,
        stats,
        weights,
        SoundNeighbourOptions {
            exclude_same_album: options.exclude_same_album,
            exclude_same_artist: options.exclude_same_artist,
            limit: options.limit,
        },
    );
    let file_info = crate::sound_file_info::load_sound_file_info(db, track_id)?;
    Ok(SoundSnapshot::Ready {
        profile,
        file_info,
        neighbours,
    })
}
