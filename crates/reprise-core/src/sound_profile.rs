//! Versioned, local audio evidence and its normalized sound profile.

use rusqlite::{Connection, OptionalExtension};

#[path = "sound_profile_record.rs"]
mod record;
use record::RawReadyAnalysis;

/// Bump when evidence-to-profile projection changes. Evidence with the current
/// extractor version can then be reprojected without decoding source audio.
pub const CURRENT_PROFILE_VERSION: u32 = 1;

/// A finite scalar in the inclusive `0.0..=1.0` range.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Normalized(f64);

impl Normalized {
    pub fn new(value: f64) -> Result<Self, SoundProfileError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(SoundProfileError::InvalidNormalizedValue(value));
        }
        Ok(Self(value))
    }

    pub fn get(self) -> f64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceFingerprint {
    mtime: i64,
    size: i64,
}

impl SourceFingerprint {
    pub fn new(mtime: i64, size: i64) -> Result<Self, SoundProfileError> {
        if mtime < 0 || size < 0 {
            return Err(SoundProfileError::InvalidFingerprint { mtime, size });
        }
        Ok(Self { mtime, size })
    }

    pub fn mtime(self) -> i64 {
        self.mtime
    }

    pub fn size(self) -> i64 {
        self.size
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AnalysisVersions {
    extractor: u32,
    profile: u32,
}

impl AnalysisVersions {
    pub fn new(extractor: u32, profile: u32) -> Result<Self, SoundProfileError> {
        if extractor == 0 || profile == 0 {
            return Err(SoundProfileError::InvalidVersion);
        }
        Ok(Self { extractor, profile })
    }

    /// The versions the running build analyzes at — the current extractor
    /// ([`crate::audio_analysis::CURRENT_EXTRACTOR_VERSION`]) paired with the
    /// current profile ([`CURRENT_PROFILE_VERSION`]). Infallible: both are
    /// compile-time non-zero constants, so a caller that just needs "today's
    /// versions" (coverage queries, the MCP summary) skips the fallible
    /// [`Self::new`] and its error mapping.
    pub fn current() -> Self {
        Self {
            extractor: crate::audio_analysis::CURRENT_EXTRACTOR_VERSION,
            profile: CURRENT_PROFILE_VERSION,
        }
    }

    pub fn extractor(self) -> u32 {
        self.extractor
    }

    pub fn profile(self) -> u32 {
        self.profile
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoEstimate {
    bpm: f64,
    confidence: Normalized,
}

impl TempoEstimate {
    pub fn new(bpm: f64, confidence: f64) -> Result<Self, SoundProfileError> {
        if !bpm.is_finite() || bpm <= 0.0 {
            return Err(SoundProfileError::InvalidEvidence("tempo_bpm", bpm));
        }
        Ok(Self {
            bpm,
            confidence: Normalized::new(confidence)?,
        })
    }

    pub fn bpm(self) -> f64 {
        self.bpm
    }

    pub fn confidence(self) -> Normalized {
        self.confidence
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioEvidence {
    loudness_rms: f64,
    dynamic_range: f64,
    spectral_centroid_hz: f64,
    spectral_rolloff_hz: f64,
    spectral_flux: f64,
    onset_rate: f64,
    tempo: Option<TempoEstimate>,
}

#[path = "sound_profile_accessors.rs"]
mod accessors;

impl AudioEvidence {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        loudness_rms: f64,
        dynamic_range: f64,
        spectral_centroid_hz: f64,
        spectral_rolloff_hz: f64,
        spectral_flux: f64,
        onset_rate: f64,
        tempo: Option<TempoEstimate>,
    ) -> Result<Self, SoundProfileError> {
        for (name, value) in [
            ("loudness_rms", loudness_rms),
            ("dynamic_range", dynamic_range),
            ("spectral_centroid_hz", spectral_centroid_hz),
            ("spectral_rolloff_hz", spectral_rolloff_hz),
            ("spectral_flux", spectral_flux),
            ("onset_rate", onset_rate),
        ] {
            if !value.is_finite() || value < 0.0 {
                return Err(SoundProfileError::InvalidEvidence(name, value));
            }
        }
        Ok(Self {
            loudness_rms,
            dynamic_range,
            spectral_centroid_hz,
            spectral_rolloff_hz,
            spectral_flux,
            onset_rate,
            tempo,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ProfileDimension {
    value: Normalized,
    confidence: Normalized,
}

impl ProfileDimension {
    pub fn new(value: f64, confidence: f64) -> Result<Self, SoundProfileError> {
        Ok(Self {
            value: Normalized::new(value)?,
            confidence: Normalized::new(confidence)?,
        })
    }

    pub fn value(self) -> Normalized {
        self.value
    }

    pub fn confidence(self) -> Normalized {
        self.confidence
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SoundProfile {
    pub intensity: ProfileDimension,
    pub brightness: ProfileDimension,
    pub dynamicity: ProfileDimension,
    pub rhythmicity: ProfileDimension,
}

impl SoundProfile {
    pub fn new(
        intensity: ProfileDimension,
        brightness: ProfileDimension,
        dynamicity: ProfileDimension,
        rhythmicity: ProfileDimension,
    ) -> Self {
        Self {
            intensity,
            brightness,
            dynamicity,
            rhythmicity,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReadyAnalysis {
    pub source: SourceFingerprint,
    pub versions: AnalysisVersions,
    pub analyzed_at: i64,
    pub evidence: AudioEvidence,
    pub profile: SoundProfile,
}

impl ReadyAnalysis {
    pub fn new(
        source: SourceFingerprint,
        versions: AnalysisVersions,
        analyzed_at: i64,
        evidence: AudioEvidence,
        profile: SoundProfile,
    ) -> Result<Self, SoundProfileError> {
        if analyzed_at < 0 {
            return Err(SoundProfileError::InvalidTimestamp(analyzed_at));
        }
        Ok(Self {
            source,
            versions,
            analyzed_at,
            evidence,
            profile,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TrackAnalysis {
    Ready(ReadyAnalysis),
    Failed(FailedAnalysis),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FailureKind {
    Io,
    Decode,
    UnsupportedFormat,
    Cancelled,
    Unknown,
}

impl FailureKind {
    fn stored(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Decode => "decode",
            Self::UnsupportedFormat => "unsupported_format",
            Self::Cancelled => "cancelled",
            Self::Unknown => "unknown",
        }
    }

    fn from_stored(value: &str) -> Self {
        match value {
            "io" => Self::Io,
            "decode" => Self::Decode,
            "unsupported_format" => Self::UnsupportedFormat,
            "cancelled" => Self::Cancelled,
            _ => Self::Unknown,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailedAnalysis {
    pub source: SourceFingerprint,
    pub versions: AnalysisVersions,
    pub failed_at: i64,
    pub kind: FailureKind,
    pub detail: String,
    pub retry_count: u32,
    pub retry_after: Option<i64>,
}

impl FailedAnalysis {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: SourceFingerprint,
        versions: AnalysisVersions,
        failed_at: i64,
        kind: FailureKind,
        detail: impl Into<String>,
        retry_count: u32,
        retry_after: Option<i64>,
    ) -> Result<Self, SoundProfileError> {
        if failed_at < 0 || retry_after.is_some_and(|value| value < 0) {
            return Err(SoundProfileError::InvalidTimestamp(
                retry_after.unwrap_or(failed_at),
            ));
        }
        Ok(Self {
            source,
            versions,
            failed_at,
            kind,
            detail: detail.into(),
            retry_count,
            retry_after,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnalysisState {
    Ineligible,
    Pending,
    Ready,
    Failed,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PendingTrack {
    pub id: i64,
    pub path: String,
    pub source: SourceFingerprint,
    pub waveform_missing: bool,
    pub work: PendingWork,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PendingWork {
    Decode,
    Reproject,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Coverage {
    pub analyzed: u64,
    pub total: u64,
}

impl Coverage {
    pub const fn new(analyzed: u64, total: u64) -> Self {
        Self { analyzed, total }
    }
}

pub fn pending_tracks(
    conn: &Connection,
    versions: AnalysisVersions,
) -> Result<Vec<PendingTrack>, SoundProfileError> {
    let mut statement = conn.prepare(&format!(
        "SELECT t.id, t.path, t.file_mtime, t.file_size,
                t.waveform_peaks IS NULL, a.status, a.source_mtime,
                a.source_size, a.extractor_version
         FROM tracks t
         LEFT JOIN track_audio_analysis a ON a.track_id = t.id
         WHERE {} AND (
           a.track_id IS NULL OR a.source_mtime <> t.file_mtime OR
           a.source_size <> t.file_size OR a.extractor_version <> ?1 OR
           a.profile_version <> ?2
         )
         ORDER BY t.id",
        crate::queries::PRESENT
    ))?;
    let rows = statement
        .query_map(
            rusqlite::params![versions.extractor(), versions.profile()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, Option<String>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                ))
            },
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    rows.into_iter()
        .map(
            |(
                id,
                path,
                mtime,
                size,
                waveform_missing,
                status,
                stored_mtime,
                stored_size,
                extractor,
            )| {
                let can_reproject = status.as_deref() == Some("ready")
                    && stored_mtime == Some(mtime)
                    && stored_size == Some(size)
                    && extractor == Some(i64::from(versions.extractor()));
                Ok(PendingTrack {
                    id,
                    path,
                    source: SourceFingerprint::new(mtime, size)?,
                    waveform_missing,
                    work: if can_reproject {
                        PendingWork::Reproject
                    } else {
                        PendingWork::Decode
                    },
                })
            },
        )
        .collect()
}

pub fn library_coverage(
    conn: &Connection,
    versions: AnalysisVersions,
) -> Result<Coverage, SoundProfileError> {
    coverage_query(
        conn,
        &format!(
            "SELECT
               SUM(CASE WHEN a.status = 'ready' AND
                             a.source_mtime = t.file_mtime AND
                             a.source_size = t.file_size AND
                             a.extractor_version = ?1 AND
                             a.profile_version = ?2 THEN 1 ELSE 0 END),
               COUNT(*)
             FROM tracks t
             LEFT JOIN track_audio_analysis a ON a.track_id = t.id
             WHERE {}",
            crate::queries::PRESENT
        ),
        rusqlite::params![versions.extractor(), versions.profile()],
    )
}

pub fn listen_coverage(
    conn: &Connection,
    versions: AnalysisVersions,
    start_inclusive: i64,
    end_exclusive: i64,
) -> Result<Coverage, SoundProfileError> {
    if start_inclusive > end_exclusive {
        return Err(SoundProfileError::InvalidPeriod);
    }
    let sql = format!(
        "SELECT
           SUM(CASE WHEN a.status = 'ready' AND
                         a.source_mtime = t.file_mtime AND
                         a.source_size = t.file_size AND
                         a.extractor_version = ?1 AND
                         a.profile_version = ?2 THEN 1 ELSE 0 END),
           COUNT(*)
         FROM listen_events e
         JOIN tracks t ON t.id = e.track_id
         LEFT JOIN track_audio_analysis a ON a.track_id = t.id
         WHERE {} AND e.played_at >= ?3 AND e.played_at < ?4",
        crate::queries::PRESENT
    );
    coverage_query(
        conn,
        &sql,
        rusqlite::params![
            versions.extractor(),
            versions.profile(),
            start_inclusive,
            end_exclusive
        ],
    )
}

fn coverage_query<P>(conn: &Connection, sql: &str, params: P) -> Result<Coverage, SoundProfileError>
where
    P: rusqlite::Params,
{
    let (analyzed, total): (Option<i64>, i64) =
        conn.query_row(sql, params, |row| Ok((row.get(0)?, row.get(1)?)))?;
    Ok(Coverage::new(
        u64::try_from(analyzed.unwrap_or(0))
            .map_err(|_| SoundProfileError::CorruptData("negative analyzed count"))?,
        u64::try_from(total)
            .map_err(|_| SoundProfileError::CorruptData("negative coverage total"))?,
    ))
}

pub fn analysis_state(
    conn: &Connection,
    track_id: i64,
    versions: AnalysisVersions,
) -> Result<AnalysisState, SoundProfileError> {
    let row = conn
        .query_row(
            "SELECT t.missing_since IS NULL AND t.removed_at IS NULL,
                    t.file_mtime, t.file_size, a.status, a.source_mtime,
                    a.source_size, a.extractor_version, a.profile_version
             FROM tracks t
             LEFT JOIN track_audio_analysis a ON a.track_id = t.id
             WHERE t.id = ?1",
            [track_id],
            |row| {
                Ok((
                    row.get::<_, bool>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, Option<i64>>(4)?,
                    row.get::<_, Option<i64>>(5)?,
                    row.get::<_, Option<i64>>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                ))
            },
        )
        .optional()?;
    let Some((present, mtime, size, status, stored_mtime, stored_size, extractor, profile)) = row
    else {
        return Ok(AnalysisState::Ineligible);
    };
    if !present {
        return Ok(AnalysisState::Ineligible);
    }
    let Some(status) = status else {
        return Ok(AnalysisState::Pending);
    };
    let current = stored_mtime == Some(mtime)
        && stored_size == Some(size)
        && extractor == Some(i64::from(versions.extractor()))
        && profile == Some(i64::from(versions.profile()));
    if !current {
        return Ok(AnalysisState::Stale);
    }
    match status.as_str() {
        "ready" => Ok(AnalysisState::Ready),
        "failed" => Ok(AnalysisState::Failed),
        _ => Err(SoundProfileError::CorruptData("unknown analysis status")),
    }
}

pub fn save_ready_analysis(
    conn: &Connection,
    track_id: i64,
    analysis: &ReadyAnalysis,
) -> Result<bool, SoundProfileError> {
    let evidence = analysis.evidence;
    let tempo_bpm = evidence.tempo.map(TempoEstimate::bpm);
    let tempo_confidence = evidence.tempo.map(|tempo| tempo.confidence().get());
    let changed = conn.execute(
        &format!(
            "INSERT INTO track_audio_analysis (
           track_id, source_mtime, source_size, extractor_version, profile_version,
           analyzed_at, status, loudness_rms, dynamic_range, spectral_centroid_hz,
           spectral_rolloff_hz, spectral_flux, onset_rate, tempo_bpm, tempo_confidence,
           intensity, intensity_confidence, brightness, brightness_confidence,
           dynamicity, dynamicity_confidence, rhythmicity, rhythmicity_confidence,
           failure_kind, failure_detail, retry_count, retry_after
         ) SELECT
           ?1, ?2, ?3, ?4, ?5, ?6, 'ready', ?7, ?8, ?9, ?10, ?11, ?12, ?13,
           ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, NULL, NULL, 0, NULL
         FROM tracks t
         WHERE t.id = ?1 AND t.file_mtime = ?2 AND t.file_size = ?3 AND {}
         ON CONFLICT(track_id) DO UPDATE SET
           source_mtime = excluded.source_mtime,
           source_size = excluded.source_size,
           extractor_version = excluded.extractor_version,
           profile_version = excluded.profile_version,
           analyzed_at = excluded.analyzed_at,
           status = 'ready',
           loudness_rms = excluded.loudness_rms,
           dynamic_range = excluded.dynamic_range,
           spectral_centroid_hz = excluded.spectral_centroid_hz,
           spectral_rolloff_hz = excluded.spectral_rolloff_hz,
           spectral_flux = excluded.spectral_flux,
           onset_rate = excluded.onset_rate,
           tempo_bpm = excluded.tempo_bpm,
           tempo_confidence = excluded.tempo_confidence,
           intensity = excluded.intensity,
           intensity_confidence = excluded.intensity_confidence,
           brightness = excluded.brightness,
           brightness_confidence = excluded.brightness_confidence,
           dynamicity = excluded.dynamicity,
           dynamicity_confidence = excluded.dynamicity_confidence,
           rhythmicity = excluded.rhythmicity,
           rhythmicity_confidence = excluded.rhythmicity_confidence,
           failure_kind = NULL, failure_detail = NULL, retry_count = 0, retry_after = NULL",
            crate::queries::PRESENT
        ),
        rusqlite::params![
            track_id,
            analysis.source.mtime(),
            analysis.source.size(),
            analysis.versions.extractor(),
            analysis.versions.profile(),
            analysis.analyzed_at,
            evidence.loudness_rms,
            evidence.dynamic_range,
            evidence.spectral_centroid_hz,
            evidence.spectral_rolloff_hz,
            evidence.spectral_flux,
            evidence.onset_rate,
            tempo_bpm,
            tempo_confidence,
            analysis.profile.intensity.value().get(),
            analysis.profile.intensity.confidence().get(),
            analysis.profile.brightness.value().get(),
            analysis.profile.brightness.confidence().get(),
            analysis.profile.dynamicity.value().get(),
            analysis.profile.dynamicity.confidence().get(),
            analysis.profile.rhythmicity.value().get(),
            analysis.profile.rhythmicity.confidence().get(),
        ],
    )?;
    Ok(changed == 1)
}

pub fn save_failed_analysis(
    conn: &Connection,
    track_id: i64,
    failure: &FailedAnalysis,
) -> Result<bool, SoundProfileError> {
    let changed = conn.execute(
        &format!(
            "INSERT INTO track_audio_analysis (
           track_id, source_mtime, source_size, extractor_version, profile_version,
           analyzed_at, status, failure_kind, failure_detail, retry_count, retry_after
         ) SELECT ?1, ?2, ?3, ?4, ?5, ?6, 'failed', ?7, ?8, ?9, ?10
         FROM tracks t
         WHERE t.id = ?1 AND t.file_mtime = ?2 AND t.file_size = ?3 AND {}
         ON CONFLICT(track_id) DO UPDATE SET
           source_mtime = excluded.source_mtime,
           source_size = excluded.source_size,
           extractor_version = excluded.extractor_version,
           profile_version = excluded.profile_version,
           analyzed_at = excluded.analyzed_at,
           status = 'failed',
           loudness_rms = NULL, dynamic_range = NULL,
           spectral_centroid_hz = NULL, spectral_rolloff_hz = NULL,
           spectral_flux = NULL, onset_rate = NULL, tempo_bpm = NULL,
           tempo_confidence = NULL, intensity = NULL,
           intensity_confidence = NULL, brightness = NULL,
           brightness_confidence = NULL, dynamicity = NULL,
           dynamicity_confidence = NULL, rhythmicity = NULL,
           rhythmicity_confidence = NULL,
           failure_kind = excluded.failure_kind,
           failure_detail = excluded.failure_detail,
           retry_count = excluded.retry_count,
           retry_after = excluded.retry_after",
            crate::queries::PRESENT
        ),
        rusqlite::params![
            track_id,
            failure.source.mtime(),
            failure.source.size(),
            failure.versions.extractor(),
            failure.versions.profile(),
            failure.failed_at,
            failure.kind.stored(),
            failure.detail,
            failure.retry_count,
            failure.retry_after,
        ],
    )?;
    Ok(changed == 1)
}

pub fn load_analysis(
    conn: &Connection,
    track_id: i64,
) -> Result<Option<TrackAnalysis>, SoundProfileError> {
    let status = conn
        .query_row(
            "SELECT status FROM track_audio_analysis WHERE track_id = ?1",
            [track_id],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    match status.as_deref() {
        None => return Ok(None),
        Some("failed") => return load_failed_analysis(conn, track_id).map(Some),
        Some("ready") => {}
        Some(_) => return Err(SoundProfileError::CorruptData("unknown analysis status")),
    }
    let raw = conn
        .query_row(
            "SELECT source_mtime, source_size, extractor_version, profile_version,
                    analyzed_at, status, loudness_rms, dynamic_range,
                    spectral_centroid_hz, spectral_rolloff_hz, spectral_flux,
                    onset_rate, tempo_bpm, tempo_confidence, intensity,
                    intensity_confidence, brightness, brightness_confidence,
                    dynamicity, dynamicity_confidence, rhythmicity,
                    rhythmicity_confidence
             FROM track_audio_analysis WHERE track_id = ?1",
            [track_id],
            |row| {
                Ok(RawReadyAnalysis {
                    source_mtime: row.get(0)?,
                    source_size: row.get(1)?,
                    extractor_version: row.get(2)?,
                    profile_version: row.get(3)?,
                    analyzed_at: row.get(4)?,
                    status: row.get(5)?,
                    loudness_rms: row.get(6)?,
                    dynamic_range: row.get(7)?,
                    spectral_centroid_hz: row.get(8)?,
                    spectral_rolloff_hz: row.get(9)?,
                    spectral_flux: row.get(10)?,
                    onset_rate: row.get(11)?,
                    tempo_bpm: row.get(12)?,
                    tempo_confidence: row.get(13)?,
                    intensity: row.get(14)?,
                    intensity_confidence: row.get(15)?,
                    brightness: row.get(16)?,
                    brightness_confidence: row.get(17)?,
                    dynamicity: row.get(18)?,
                    dynamicity_confidence: row.get(19)?,
                    rhythmicity: row.get(20)?,
                    rhythmicity_confidence: row.get(21)?,
                })
            },
        )
        .optional()?;
    raw.map(RawReadyAnalysis::try_into_ready)
        .transpose()
        .map(|analysis| analysis.map(TrackAnalysis::Ready))
}

fn load_failed_analysis(
    conn: &Connection,
    track_id: i64,
) -> Result<TrackAnalysis, SoundProfileError> {
    let raw = conn.query_row(
        "SELECT source_mtime, source_size, extractor_version, profile_version,
                analyzed_at, failure_kind, failure_detail, retry_count, retry_after
         FROM track_audio_analysis WHERE track_id = ?1",
        [track_id],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<String>>(6)?,
                row.get::<_, i64>(7)?,
                row.get::<_, Option<i64>>(8)?,
            ))
        },
    )?;
    let (mtime, size, extractor, profile, failed_at, kind, detail, retry_count, retry_after) = raw;
    let failure = FailedAnalysis::new(
        SourceFingerprint::new(mtime, size)?,
        AnalysisVersions::new(
            u32::try_from(extractor).map_err(|_| SoundProfileError::InvalidVersion)?,
            u32::try_from(profile).map_err(|_| SoundProfileError::InvalidVersion)?,
        )?,
        failed_at,
        FailureKind::from_stored(kind.as_deref().unwrap_or("unknown")),
        detail.unwrap_or_default(),
        u32::try_from(retry_count)
            .map_err(|_| SoundProfileError::CorruptData("invalid retry count"))?,
        retry_after,
    )?;
    Ok(TrackAnalysis::Failed(failure))
}

#[derive(Debug, thiserror::Error)]
pub enum SoundProfileError {
    #[error("normalized sound-profile value must be finite and between zero and one: {0}")]
    InvalidNormalizedValue(f64),
    #[error("invalid source fingerprint: mtime={mtime}, size={size}")]
    InvalidFingerprint { mtime: i64, size: i64 },
    #[error("audio-analysis versions must be greater than zero")]
    InvalidVersion,
    #[error("invalid audio evidence {0}: {1}")]
    InvalidEvidence(&'static str, f64),
    #[error("invalid audio-analysis timestamp: {0}")]
    InvalidTimestamp(i64),
    #[error("corrupt stored audio analysis: {0}")]
    CorruptData(&'static str),
    #[error("audio-analysis period start must not follow its end")]
    InvalidPeriod,
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),
}

#[cfg(test)]
#[path = "sound_profile_tests.rs"]
mod tests;
