//! Validated, frontend-neutral intent and bounded candidate projection for mixes.

use rusqlite::types::Value;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::audio_analysis::CURRENT_EXTRACTOR_VERSION;
use crate::sound_profile::CURRENT_PROFILE_VERSION;

#[path = "mix_planner_plan.rs"]
mod plan;
pub use plan::{plan_candidates, MixDiagnostic, MixDraft, MixDraftTrack, SelectionReason};
#[path = "mix_planner_storage.rs"]
mod storage;
pub use storage::{
    approve_mix_draft, cleanup_expired_mix_drafts, load_mix_draft, plan_mix_draft, PlaylistCommit,
};

pub const MAX_CANDIDATES: usize = 500;
pub const MAX_EXPLICIT_TRACK_IDS: usize = 500;
const MAX_SEED_TRACK_IDS: usize = 100;
const MAX_DURATION_MS: i64 = 24 * 60 * 60 * 1000;

#[derive(Debug, thiserror::Error)]
pub enum MixPlannerError {
    #[error("invalid mix intent: {0}")]
    InvalidIntent(&'static str),
    #[error("invalid mix intent JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum MixSource {
    Library,
    Playlist(i64),
    Artist(String),
    Album(String),
    Tracks(Vec<i64>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CriteriaMode {
    AudioCharacter,
    Genre,
    RelatedArtists,
    Balanced,
}

impl CriteriaMode {
    fn requires_profile(self) -> bool {
        self == Self::AudioCharacter
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Familiarity {
    Familiar,
    Balanced,
    Discover,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Variety {
    Cohesive,
    Balanced,
    Wide,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnergyCurve {
    Flat,
    Rise,
    Fall,
    Arc,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileTarget {
    intensity: f64,
    brightness: f64,
    dynamicity: f64,
    rhythmicity: f64,
}

impl ProfileTarget {
    pub fn new(
        intensity: f64,
        brightness: f64,
        dynamicity: f64,
        rhythmicity: f64,
    ) -> Result<Self, MixPlannerError> {
        let target = Self {
            intensity,
            brightness,
            dynamicity,
            rhythmicity,
        };
        target.validate()?;
        Ok(target)
    }

    pub fn neutral() -> Self {
        Self {
            intensity: 0.5,
            brightness: 0.5,
            dynamicity: 0.5,
            rhythmicity: 0.5,
        }
    }

    fn validate(self) -> Result<(), MixPlannerError> {
        if [
            self.intensity,
            self.brightness,
            self.dynamicity,
            self.rhythmicity,
        ]
        .into_iter()
        .all(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        {
            Ok(())
        } else {
            Err(MixPlannerError::InvalidIntent(
                "profile targets must be finite values from zero to one",
            ))
        }
    }

    pub fn values(self) -> [f64; 4] {
        [
            self.intensity,
            self.brightness,
            self.dynamicity,
            self.rhythmicity,
        ]
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MixIntent {
    source: MixSource,
    seed_track_ids: Vec<i64>,
    criteria: CriteriaMode,
    target: ProfileTarget,
    target_duration_ms: i64,
    familiarity: Familiarity,
    variety: Variety,
    energy_curve: EnergyCurve,
    min_confidence: f64,
    include_seeds: bool,
    excluded_track_ids: Vec<i64>,
    #[serde(default)]
    target_genres: Vec<String>,
}

impl MixIntent {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: MixSource,
        seed_track_ids: Vec<i64>,
        criteria: CriteriaMode,
        target: ProfileTarget,
        target_duration_ms: i64,
        familiarity: Familiarity,
        variety: Variety,
        energy_curve: EnergyCurve,
    ) -> Result<Self, MixPlannerError> {
        if seed_track_ids.is_empty() {
            return Err(MixPlannerError::InvalidIntent(
                "selected-seed mixes require at least one seed track",
            ));
        }
        let mut intent = Self {
            source,
            seed_track_ids,
            criteria,
            target,
            target_duration_ms,
            familiarity,
            variety,
            energy_curve,
            min_confidence: 0.0,
            include_seeds: false,
            excluded_track_ids: Vec::new(),
            target_genres: Vec::new(),
        };
        intent.normalize_and_validate()?;
        Ok(intent)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_target(
        source: MixSource,
        target: ProfileTarget,
        target_duration_ms: i64,
        familiarity: Familiarity,
        variety: Variety,
        energy_curve: EnergyCurve,
    ) -> Result<Self, MixPlannerError> {
        let mut intent = Self {
            source,
            seed_track_ids: Vec::new(),
            criteria: CriteriaMode::AudioCharacter,
            target,
            target_duration_ms,
            familiarity,
            variety,
            energy_curve,
            min_confidence: 0.0,
            include_seeds: false,
            excluded_track_ids: Vec::new(),
            target_genres: Vec::new(),
        };
        intent.normalize_and_validate()?;
        Ok(intent)
    }

    pub fn from_json(json: &str) -> Result<Self, MixPlannerError> {
        let mut intent: Self = serde_json::from_str(json)?;
        intent.normalize_and_validate()?;
        Ok(intent)
    }

    pub fn to_json(&self) -> Result<String, MixPlannerError> {
        Ok(serde_json::to_string(self)?)
    }

    pub fn excluding_tracks(mut self, ids: Vec<i64>) -> Result<Self, MixPlannerError> {
        self.excluded_track_ids = ids;
        self.normalize_and_validate()?;
        Ok(self)
    }

    pub fn including_seeds(mut self, include: bool) -> Self {
        self.include_seeds = include;
        self
    }

    pub fn with_min_confidence(mut self, confidence: f64) -> Result<Self, MixPlannerError> {
        self.min_confidence = confidence;
        self.normalize_and_validate()?;
        Ok(self)
    }

    pub fn with_genres(mut self, genres: Vec<String>) -> Result<Self, MixPlannerError> {
        self.target_genres = genres;
        self.normalize_and_validate()?;
        Ok(self)
    }

    pub fn source(&self) -> &MixSource {
        &self.source
    }
    pub fn seeds(&self) -> &[i64] {
        &self.seed_track_ids
    }
    pub fn criteria(&self) -> CriteriaMode {
        self.criteria
    }
    pub fn target(&self) -> ProfileTarget {
        self.target
    }
    pub fn target_duration_ms(&self) -> i64 {
        self.target_duration_ms
    }
    pub fn familiarity(&self) -> Familiarity {
        self.familiarity
    }
    pub fn variety(&self) -> Variety {
        self.variety
    }
    pub fn energy_curve(&self) -> EnergyCurve {
        self.energy_curve
    }
    pub fn target_genres(&self) -> &[String] {
        &self.target_genres
    }

    fn normalize_and_validate(&mut self) -> Result<(), MixPlannerError> {
        self.target.validate()?;
        normalize_ids(&mut self.seed_track_ids, MAX_SEED_TRACK_IDS, true)?;
        normalize_ids(&mut self.excluded_track_ids, MAX_EXPLICIT_TRACK_IDS, true)?;
        self.target_genres = self
            .target_genres
            .iter()
            .map(|genre| crate::library::group_key::normalize_group_key(genre))
            .filter(|genre| !genre.is_empty())
            .collect();
        self.target_genres.sort();
        self.target_genres.dedup();
        if self.target_genres.len() > 20 {
            return Err(MixPlannerError::InvalidIntent("too many target genres"));
        }
        if self.target_duration_ms <= 0 || self.target_duration_ms > MAX_DURATION_MS {
            return Err(MixPlannerError::InvalidIntent(
                "target duration is out of range",
            ));
        }
        if !self.min_confidence.is_finite() || !(0.0..=1.0).contains(&self.min_confidence) {
            return Err(MixPlannerError::InvalidIntent(
                "minimum confidence is out of range",
            ));
        }
        match &mut self.source {
            MixSource::Tracks(ids) => normalize_ids(ids, MAX_EXPLICIT_TRACK_IDS, false)?,
            MixSource::Playlist(id) if *id <= 0 => {
                return Err(MixPlannerError::InvalidIntent(
                    "playlist id must be positive",
                ));
            }
            MixSource::Artist(value) | MixSource::Album(value) => {
                *value = value.trim().to_string();
                if value.is_empty() {
                    return Err(MixPlannerError::InvalidIntent(
                        "source name cannot be empty",
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn normalize_ids(ids: &mut Vec<i64>, max: usize, allow_empty: bool) -> Result<(), MixPlannerError> {
    if ids.len() > max {
        return Err(MixPlannerError::InvalidIntent(
            "track id list is invalid or too large",
        ));
    }
    ids.sort_unstable();
    ids.dedup();
    if ids.iter().any(|id| *id <= 0) || (!allow_empty && ids.is_empty()) {
        return Err(MixPlannerError::InvalidIntent(
            "track id list is invalid or too large",
        ));
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CandidateProfile {
    pub intensity: f64,
    pub brightness: f64,
    pub dynamicity: f64,
    pub rhythmicity: f64,
    pub tempo_bpm: Option<f64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MixCandidate {
    pub track_id: i64,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub genre: String,
    pub duration_ms: i64,
    pub rating: i32,
    pub play_count: i64,
    pub profile: Option<CandidateProfile>,
}

pub fn query_candidates(
    conn: &Connection,
    intent: &MixIntent,
) -> Result<Vec<MixCandidate>, MixPlannerError> {
    let mut values = Vec::<Value>::new();
    let mut joins = String::new();
    let mut conditions = vec![
        "t.missing_since IS NULL".to_string(),
        "t.removed_at IS NULL".to_string(),
    ];
    match intent.source() {
        MixSource::Library => {}
        MixSource::Playlist(id) => {
            joins.push_str(" JOIN playlist_tracks pt ON pt.track_id = t.id");
            values.push((*id).into());
            conditions.push("pt.playlist_id = ?1".to_string());
        }
        MixSource::Artist(name) => {
            values.push(name.trim().to_lowercase().into());
            conditions.push("lower(trim(t.artist)) = ?1".to_string());
        }
        MixSource::Album(name) => {
            values.push(name.trim().to_lowercase().into());
            conditions.push("lower(trim(t.album)) = ?1".to_string());
        }
        MixSource::Tracks(ids) => {
            push_id_condition(&mut conditions, &mut values, "t.id", ids, false);
        }
    }
    let mut excluded = intent.excluded_track_ids.clone();
    if !intent.include_seeds {
        excluded.extend_from_slice(intent.seeds());
    }
    excluded.sort_unstable();
    excluded.dedup();
    push_id_condition(&mut conditions, &mut values, "t.id", &excluded, true);
    if intent.criteria().requires_profile() {
        conditions.extend([
            "a.status = 'ready'".to_string(),
            "a.source_mtime = t.file_mtime".to_string(),
            "a.source_size = t.file_size".to_string(),
            format!("a.extractor_version = {CURRENT_EXTRACTOR_VERSION}"),
            format!("a.profile_version = {CURRENT_PROFILE_VERSION}"),
        ]);
        values.push(intent.min_confidence.into());
        let confidence_param = values.len();
        conditions.push(format!(
            "MIN(a.intensity_confidence, a.brightness_confidence, \
             a.dynamicity_confidence, a.rhythmicity_confidence) >= ?{confidence_param}"
        ));
    } else if intent.criteria() == CriteriaMode::RelatedArtists {
        let related = crate::related_artists::cached_local_track_ids(conn, intent.seeds())
            .map_err(|error| {
                tracing::warn!(%error, "related artist cache could not be read");
                MixPlannerError::InvalidIntent("related-artist evidence is unavailable")
            })?;
        push_id_condition(&mut conditions, &mut values, "t.id", &related, false);
        if related.is_empty() {
            conditions.push("0 = 1".to_string());
        }
    }
    let sql = format!(
        "SELECT t.id, t.title, t.artist, t.album, t.genre, t.duration_ms, t.rating,
                t.play_count, a.intensity, a.brightness, a.dynamicity, a.rhythmicity,
                a.tempo_bpm
         FROM tracks t{joins}
         LEFT JOIN track_audio_analysis a ON a.track_id = t.id
         WHERE {}
         ORDER BY t.id LIMIT {MAX_CANDIDATES}",
        conditions.join(" AND ")
    );
    let mut statement = conn.prepare(&sql)?;
    let rows = statement.query_map(rusqlite::params_from_iter(values), |row| {
        let intensity: Option<f64> = row.get(8)?;
        Ok(MixCandidate {
            track_id: row.get(0)?,
            title: row.get(1)?,
            artist: row.get(2)?,
            album: row.get(3)?,
            genre: row.get(4)?,
            duration_ms: row.get(5)?,
            rating: row.get(6)?,
            play_count: row.get(7)?,
            profile: match intensity {
                Some(intensity) => Some(CandidateProfile {
                    intensity,
                    brightness: row.get(9)?,
                    dynamicity: row.get(10)?,
                    rhythmicity: row.get(11)?,
                    tempo_bpm: row.get(12)?,
                }),
                None => None,
            },
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn plan_mix(conn: &Connection, intent: &MixIntent) -> Result<MixDraft, MixPlannerError> {
    let mut normalized = intent.clone();
    let mut missing_genre_evidence = false;
    if matches!(
        intent.criteria(),
        CriteriaMode::Genre | CriteriaMode::Balanced
    ) && intent.target_genres().is_empty()
    {
        let (genres, complete) = genres_for_tracks(conn, intent.seeds())?;
        missing_genre_evidence = !complete;
        if genres.is_empty() && intent.criteria() == CriteriaMode::Genre {
            return Err(MixPlannerError::InvalidIntent(
                "genre seeds have no usable genre evidence",
            ));
        }
        normalized = normalized.with_genres(genres)?;
    }
    let mut draft = plan_candidates(&normalized, query_candidates(conn, &normalized)?)?;
    if missing_genre_evidence {
        draft.diagnostics.push(MixDiagnostic::MissingGenreEvidence);
    }
    Ok(draft)
}

fn genres_for_tracks(
    conn: &Connection,
    track_ids: &[i64],
) -> Result<(Vec<String>, bool), MixPlannerError> {
    let mut genres = Vec::new();
    let mut found = 0_usize;
    for track_id in track_ids {
        let genre = conn
            .query_row(
                "SELECT genre FROM tracks WHERE id = ?1 AND missing_since IS NULL AND removed_at IS NULL",
                [track_id],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        if let Some(genre) = genre {
            found += 1;
            let normalized = crate::library::group_key::normalize_group_key(&genre);
            if !normalized.is_empty() {
                genres.push(normalized);
            }
        }
    }
    genres.sort();
    genres.dedup();
    Ok((genres, found == track_ids.len()))
}

pub fn profile_target_for_tracks(
    conn: &Connection,
    track_ids: &[i64],
) -> Result<ProfileTarget, MixPlannerError> {
    if track_ids.is_empty() || track_ids.len() > MAX_SEED_TRACK_IDS {
        return Err(MixPlannerError::InvalidIntent("seed track list is invalid"));
    }
    let mut ids = track_ids.to_vec();
    normalize_ids(&mut ids, MAX_SEED_TRACK_IDS, false)?;
    let mut values = Vec::<Value>::new();
    let mut conditions = vec![
        "t.missing_since IS NULL".to_string(),
        "t.removed_at IS NULL".to_string(),
    ];
    push_id_condition(&mut conditions, &mut values, "t.id", &ids, false);
    let sql = format!(
        "SELECT AVG(a.intensity), AVG(a.brightness), AVG(a.dynamicity), AVG(a.rhythmicity), COUNT(*)
         FROM tracks t JOIN track_audio_analysis a ON a.track_id = t.id
         WHERE {} AND a.status = 'ready' AND a.source_mtime = t.file_mtime
           AND a.source_size = t.file_size AND a.extractor_version = {CURRENT_EXTRACTOR_VERSION}
           AND a.profile_version = {CURRENT_PROFILE_VERSION}",
        conditions.join(" AND ")
    );
    let row: (Option<f64>, Option<f64>, Option<f64>, Option<f64>, i64) =
        conn.query_row(&sql, rusqlite::params_from_iter(values), |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?;
    if row.4 != i64::try_from(ids.len()).unwrap_or(i64::MAX) {
        return Err(MixPlannerError::InvalidIntent(
            "every audio-character seed must have a current profile",
        ));
    }
    ProfileTarget::new(
        row.0.ok_or(MixPlannerError::InvalidIntent(
            "seed profile is unavailable",
        ))?,
        row.1.ok_or(MixPlannerError::InvalidIntent(
            "seed profile is unavailable",
        ))?,
        row.2.ok_or(MixPlannerError::InvalidIntent(
            "seed profile is unavailable",
        ))?,
        row.3.ok_or(MixPlannerError::InvalidIntent(
            "seed profile is unavailable",
        ))?,
    )
}

pub fn profile_target_for_available_tracks(
    conn: &Connection,
    track_ids: &[i64],
) -> Result<Option<ProfileTarget>, MixPlannerError> {
    if track_ids.is_empty() || track_ids.len() > MAX_SEED_TRACK_IDS {
        return Err(MixPlannerError::InvalidIntent("seed track list is invalid"));
    }
    let mut ids = track_ids.to_vec();
    normalize_ids(&mut ids, MAX_SEED_TRACK_IDS, false)?;
    let mut values = Vec::<Value>::new();
    let mut conditions = vec![
        "t.missing_since IS NULL".to_string(),
        "t.removed_at IS NULL".to_string(),
    ];
    push_id_condition(&mut conditions, &mut values, "t.id", &ids, false);
    let sql = format!(
        "SELECT AVG(a.intensity), AVG(a.brightness), AVG(a.dynamicity), AVG(a.rhythmicity), COUNT(*)
         FROM tracks t JOIN track_audio_analysis a ON a.track_id = t.id
         WHERE {} AND a.status = 'ready' AND a.source_mtime = t.file_mtime
           AND a.source_size = t.file_size AND a.extractor_version = {CURRENT_EXTRACTOR_VERSION}
           AND a.profile_version = {CURRENT_PROFILE_VERSION}",
        conditions.join(" AND ")
    );
    let row: (Option<f64>, Option<f64>, Option<f64>, Option<f64>, i64) =
        conn.query_row(&sql, rusqlite::params_from_iter(values), |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?;
    if row.4 == 0 {
        return Ok(None);
    }
    Ok(Some(ProfileTarget::new(
        row.0.ok_or(MixPlannerError::InvalidIntent(
            "seed profile is unavailable",
        ))?,
        row.1.ok_or(MixPlannerError::InvalidIntent(
            "seed profile is unavailable",
        ))?,
        row.2.ok_or(MixPlannerError::InvalidIntent(
            "seed profile is unavailable",
        ))?,
        row.3.ok_or(MixPlannerError::InvalidIntent(
            "seed profile is unavailable",
        ))?,
    )?))
}

fn push_id_condition(
    conditions: &mut Vec<String>,
    values: &mut Vec<Value>,
    column: &str,
    ids: &[i64],
    negated: bool,
) {
    if ids.is_empty() {
        return;
    }
    let start = values.len() + 1;
    values.extend(ids.iter().copied().map(Value::Integer));
    let placeholders = (start..start + ids.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(",");
    let expression = format!(
        "{column} {} ({placeholders})",
        if negated { "NOT IN" } else { "IN" }
    );
    conditions.push(expression);
}
