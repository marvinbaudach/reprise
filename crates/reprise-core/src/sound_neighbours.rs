//! Pure neighbour ranking over library sound profiles.

use crate::db::DbError;
use crate::sound_distance::{sound_distance, DistanceWeights};
use crate::sound_features::{SoundFeatures, SOUND_FEATURES_FORMAT_VERSION};
use crate::sound_stats::SoundStats;

#[derive(Debug, Clone, PartialEq)]
pub struct SoundCandidate {
    pub track_id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub features: SoundFeatures,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoundNeighbourOptions {
    pub exclude_same_album: bool,
    pub exclude_same_artist: bool,
    pub limit: usize,
}

impl Default for SoundNeighbourOptions {
    fn default() -> Self {
        Self {
            exclude_same_album: true,
            exclude_same_artist: false,
            limit: 7,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SoundNeighbour {
    pub track_id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub distance: f32,
    /// Share of the full comparison population that lies farther away.
    pub percentile: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SoundNeighbourResult {
    pub library_count: usize,
    pub matches: Vec<SoundNeighbour>,
}

pub fn load_sound_candidates(db: &crate::db::Db) -> Result<Vec<SoundCandidate>, DbError> {
    let mut statement = db.conn().prepare(
        "SELECT t.id, t.path, t.title, t.artist, t.album, t.album_artist, f.data \
         FROM tracks t JOIN track_sound_features f ON f.track_id = t.id \
         WHERE t.missing_since IS NULL AND t.removed_at IS NULL \
           AND f.format_version = ?1 ORDER BY t.id",
    )?;
    let candidates = statement
        .query_map([SOUND_FEATURES_FORMAT_VERSION], |row| {
            let track_id = row.get::<_, i64>(0)?;
            let blob = row.get::<_, Vec<u8>>(6)?;
            // One unreadable row must not take the whole comparison population
            // down with it: skip it and rank against the rest.
            let Ok(features) = SoundFeatures::from_blob(&blob).inspect_err(|error| {
                tracing::warn!(%error, track_id, "skipping unreadable sound profile");
            }) else {
                return Ok(None);
            };
            Ok(Some(SoundCandidate {
                track_id,
                path: row.get(1)?,
                title: row.get(2)?,
                artist: row.get(3)?,
                album: row.get(4)?,
                album_artist: row.get(5)?,
                features,
            }))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(candidates.into_iter().flatten().collect())
}

pub fn rank_sound_neighbours(
    current: &SoundCandidate,
    candidates: &[SoundCandidate],
    stats: &SoundStats,
    weights: DistanceWeights,
    options: SoundNeighbourOptions,
) -> SoundNeighbourResult {
    let mut distances: Vec<(&SoundCandidate, f32)> = candidates
        .iter()
        .filter(|candidate| candidate.track_id != current.track_id)
        .map(|candidate| {
            (
                candidate,
                sound_distance(&current.features, &candidate.features, stats, weights).total,
            )
        })
        .collect();
    distances.sort_by(|(left, left_distance), (right, right_distance)| {
        left_distance
            .total_cmp(right_distance)
            .then_with(|| left.track_id.cmp(&right.track_id))
    });
    let library_count = distances.len();
    let mut matches = Vec::new();
    for (candidate, distance) in &distances {
        let farther = distances
            .iter()
            .filter(|(_, other)| other > distance)
            .count();
        let percentile = if library_count == 0 {
            0.0
        } else {
            farther as f32 / library_count as f32 * 100.0
        };
        if excluded(current, candidate, options) {
            continue;
        }
        matches.push(SoundNeighbour {
            track_id: candidate.track_id,
            path: candidate.path.clone(),
            title: candidate.title.clone(),
            artist: candidate.artist.clone(),
            album: candidate.album.clone(),
            album_artist: candidate.album_artist.clone(),
            distance: *distance,
            percentile,
        });
        if matches.len() == options.limit {
            break;
        }
    }
    SoundNeighbourResult {
        library_count,
        matches,
    }
}

fn excluded(
    current: &SoundCandidate,
    candidate: &SoundCandidate,
    options: SoundNeighbourOptions,
) -> bool {
    (options.exclude_same_album
        && same_nonempty(&current.album, &candidate.album)
        && same_nonempty(&current.album_artist, &candidate.album_artist))
        || (options.exclude_same_artist && same_nonempty(&current.artist, &candidate.artist))
}

fn same_nonempty(left: &str, right: &str) -> bool {
    let left = left.trim();
    let right = right.trim();
    !left.is_empty() && left.eq_ignore_ascii_case(right)
}
