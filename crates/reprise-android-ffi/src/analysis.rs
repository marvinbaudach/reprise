//! Android's streaming track-analysis lifecycle and shaped rendering-data reads.

use std::sync::{Arc, Mutex};

use reprise_core::db::{
    get_track_spectrogram, get_waveform_peaks, set_track_render_data, track_source_fingerprint,
    SpectrogramStoreOutcome,
};
use reprise_core::pcm_resampler::PcmResampler;
use reprise_core::queries::query_present_track_by_id;
use reprise_core::spectrogram::{
    SpectrogramAccumulator, TrackSourceFingerprint, SPECTROGRAM_SAMPLE_RATE_HZ,
};
use reprise_core::waveform::{TrackRenderData, WaveformAccumulator, STORED_PEAK_COUNT};
use reprise_view::spectral_colour::{shape_centroid, spectral_colour};
use reprise_view::waveform::{shape_display_peaks, DisplayBar};

use crate::{LibraryError, MusicLibrary};

/// The atomic persistence result of a completed Android analysis pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq, uniffi::Enum)]
pub enum AndroidTrackAnalysisOutcome {
    Stored,
    /// The track's file identity changed after analysis began. Nothing from
    /// this session was stored, so callers must not present this as success.
    SourceChanged,
}

/// One width-shaped seek-bar cell with its Rust-owned spectral colour.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct AndroidTrackRenderBar {
    pub silence: bool,
    /// Audible height in `0..=1`; zero when [`silence`](Self::silence) is true.
    pub level: f32,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
}

/// One band's place on the shared spectral axis, as a colour.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct AndroidSpectralBandColour {
    pub red: f64,
    pub green: f64,
    pub blue: f64,
}

/// The colour of every band in a `band_count`-band column, low to high.
///
/// A spectrum column arrives as levels alone, and the surface must not turn a
/// band index into a colour itself: the axis is shared with the desktop, and a
/// second implementation of it is a second picture of the same song. The answer
/// depends on nothing but the count, so a caller reads it once and keeps it
/// rather than asking per frame.
///
/// A single band has no position on an axis; it is given the middle, which is
/// the one answer that claims nothing — the same choice `spectral_colour` makes
/// for a position it cannot place.
#[uniffi::export]
pub fn spectral_band_colours(band_count: u32) -> Vec<AndroidSpectralBandColour> {
    let count = band_count as usize;
    (0..count)
        .map(|band| {
            let position = if count <= 1 {
                0.5
            } else {
                band as f64 / (count - 1) as f64
            };
            let (red, green, blue) = spectral_colour(position);
            AndroidSpectralBandColour { red, green, blue }
        })
        .collect()
}

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum TrackAnalysisError {
    #[error("track {track_id} is no longer available for analysis")]
    TrackNotFound { track_id: i64 },
    #[error("track {track_id} has no positive stored duration")]
    InvalidDuration { track_id: i64 },
    #[error("invalid decoded PCM: {detail}")]
    InvalidPcm { detail: String },
    #[error("decoded PCM format changed during one track: {detail}")]
    FormatChanged { detail: String },
    #[error(
        "decoded only {decoded_samples} of the {expected_samples} samples the track's duration \
         declares, which is too little of it to store"
    )]
    TooShort {
        decoded_samples: u64,
        expected_samples: u64,
    },
    #[error("track analysis session has already ended")]
    Ended,
    #[error("track analysis database error: {detail}")]
    Database { detail: String },
}

struct AnalysisState {
    waveform: WaveformAccumulator,
    spectrogram: SpectrogramAccumulator,
    resampler: Option<PcmResampler>,
    input_format: Option<(u32, u32)>,
    /// The bucket budget the waveform was built with, and how much of it the
    /// decoder has actually filled.
    ///
    /// A track's stored duration is an **estimate**, and the two ways it can be
    /// wrong are not symmetric. Both were found on real devices:
    ///
    /// * **Too short.** MP3 and Opus decode to slightly more samples than their
    ///   tagged duration — encoder delay, padding, pre-skip, rounding. The
    ///   waveform accumulator treats one sample past its bound as a decode
    ///   failure, so on a phone whose library is MP3 and Opus *every* analysis
    ///   ended with nothing stored and nothing said. The overflow now runs past
    ///   the accumulator into the spectrogram alone: the last bucket saturates,
    ///   which is what a bucket at the end of a track is for.
    /// * **Too long.** A truncated or replaced file decodes to far less than
    ///   the duration claims, and the unused buckets stay silent — a bar that is
    ///   mostly silence, stored as a finished analysis. [`finish`](Self::finish)
    ///   refuses that rather than presenting it as whole.
    expected_samples: u64,
    accepted_samples: u64,
}

impl AnalysisState {
    fn push(
        &mut self,
        interleaved: &[f32],
        sample_rate_hz: u32,
        channel_count: u32,
    ) -> Result<(), TrackAnalysisError> {
        if let Some((expected_rate, expected_channels)) = self.input_format {
            if (sample_rate_hz, channel_count) != (expected_rate, expected_channels) {
                return Err(TrackAnalysisError::FormatChanged {
                    detail: format!(
                        "expected {expected_rate} Hz/{expected_channels} channels, got {sample_rate_hz} Hz/{channel_count} channels"
                    ),
                });
            }
        } else {
            let channels =
                usize::try_from(channel_count).map_err(|_| TrackAnalysisError::InvalidPcm {
                    detail: "channel count does not fit this device".to_owned(),
                })?;
            self.resampler =
                Some(PcmResampler::new(sample_rate_hz, channels).map_err(invalid_pcm_error)?);
            self.input_format = Some((sample_rate_hz, channel_count));
        }

        let mono = self
            .resampler
            .as_mut()
            .expect("input format and resampler are initialized together")
            .push(interleaved)
            .map_err(invalid_pcm_error)?;
        self.push_mono(&mono)
    }

    fn push_mono(&mut self, mono: &[f32]) -> Result<(), TrackAnalysisError> {
        // The spectrogram takes everything: it has no declared length, and the
        // audio past the estimate is as real as the rest.
        self.spectrogram.push(mono);
        let room = self.expected_samples.saturating_sub(self.accepted_samples);
        let taken = usize::try_from(room).unwrap_or(usize::MAX).min(mono.len());
        if taken == 0 {
            return Ok(());
        }
        self.waveform
            .push(&mono[..taken])
            .map_err(|error| TrackAnalysisError::InvalidPcm {
                detail: error.to_string(),
            })?;
        self.accepted_samples += taken as u64;
        Ok(())
    }

    fn finish(mut self) -> Result<TrackRenderData, TrackAnalysisError> {
        if let Some(resampler) = self.resampler.take() {
            let tail = resampler.finish();
            self.push_mono(&tail)?;
        }
        // Far less audio than the track claims is not a shorter song; it is a
        // file that is not what the library thinks it is — truncated, replaced,
        // or a decode that stopped early. Storing it would draw a bar that ends
        // in the middle of the track and calls itself finished.
        if self.accepted_samples * 100 < self.expected_samples * u64::from(MINIMUM_COVERAGE_PERCENT)
        {
            return Err(TrackAnalysisError::TooShort {
                decoded_samples: self.accepted_samples,
                expected_samples: self.expected_samples,
            });
        }
        let waveform_peaks =
            self.waveform
                .finish()
                .map_err(|error| TrackAnalysisError::InvalidPcm {
                    detail: error.to_string(),
                })?;
        Ok(TrackRenderData {
            waveform_peaks,
            spectrogram: self.spectrogram.finish(),
        })
    }
}

/// How much of the declared duration a decode has to cover before its result is
/// a picture of the track rather than a picture of its first few seconds.
///
/// Ninety per cent leaves room for the ordinary disagreements between a tagged
/// duration and a real decode — a second or two at the end of a long track —
/// while the case this exists for is nowhere near it: two seconds of a
/// three-minute song is under two per cent.
const MINIMUM_COVERAGE_PERCENT: u32 = 90;

fn invalid_pcm_error(error: impl std::fmt::Display) -> TrackAnalysisError {
    TrackAnalysisError::InvalidPcm {
        detail: error.to_string(),
    }
}

/// One all-or-nothing decoded-PCM pass for a single library track.
///
/// Dropping this object or calling [`cancel`](Self::cancel) discards both
/// accumulators. Only [`finish`](Self::finish) can write render data.
#[derive(uniffi::Object)]
pub struct TrackAnalysisSession {
    library: Arc<MusicLibrary>,
    track_id: i64,
    source: TrackSourceFingerprint,
    state: Mutex<Option<AnalysisState>>,
}

#[uniffi::export]
impl TrackAnalysisSession {
    /// Begins analysis from the track's current source identity and stored
    /// duration.
    ///
    /// The duration is converted to a ceiling 32 kHz sample count for the
    /// waveform's fixed bucket mapping. A shorter real decode leaves its
    /// unused trailing buckets silent. A longer decode exceeds that declared
    /// bound, ends the session with [`TrackAnalysisError::InvalidPcm`], and
    /// stores neither waveform nor spectrogram.
    #[uniffi::constructor]
    pub fn begin(
        library: Arc<MusicLibrary>,
        track_id: i64,
    ) -> Result<Arc<Self>, TrackAnalysisError> {
        let (source, duration_ms) = {
            let state = library
                .lock()
                .map_err(|error| library_analysis_error(&error))?;
            let track = query_present_track_by_id(&state.db, track_id)
                .map_err(database_analysis_error)?
                .ok_or(TrackAnalysisError::TrackNotFound { track_id })?;
            let source = track_source_fingerprint(&state.db, track_id)
                .map_err(database_analysis_error)?
                .ok_or(TrackAnalysisError::TrackNotFound { track_id })?;
            (source, track.duration_ms)
        };
        let duration_ms = u64::try_from(duration_ms)
            .ok()
            .filter(|duration| *duration > 0)
            .ok_or(TrackAnalysisError::InvalidDuration { track_id })?;
        let expected_samples = duration_ms
            .checked_mul(u64::from(SPECTROGRAM_SAMPLE_RATE_HZ))
            .and_then(|samples| samples.checked_add(999))
            .map(|samples| samples / 1_000)
            .ok_or(TrackAnalysisError::InvalidDuration { track_id })?;
        let waveform = WaveformAccumulator::new(expected_samples, STORED_PEAK_COUNT)
            .map_err(invalid_pcm_error)?;
        Ok(Arc::new(Self {
            library,
            track_id,
            source,
            state: Mutex::new(Some(AnalysisState {
                waveform,
                spectrogram: SpectrogramAccumulator::new(),
                resampler: None,
                input_format: None,
                expected_samples,
                accepted_samples: 0,
            })),
        }))
    }

    /// Pushes at most one second of interleaved decoded PCM. The sample rate
    /// and channel count are fixed by the first chunk for the session.
    #[allow(clippy::needless_pass_by_value)] // UniFFI sequence arguments are owned `Vec`s.
    pub fn push(
        &self,
        interleaved: Vec<f32>,
        sample_rate_hz: u32,
        channel_count: u32,
    ) -> Result<(), TrackAnalysisError> {
        let mut slot = self.state.lock().map_err(session_poisoned_error)?;
        let mut state = slot.take().ok_or(TrackAnalysisError::Ended)?;
        state.push(&interleaved, sample_rate_hz, channel_count)?;
        *slot = Some(state);
        Ok(())
    }

    /// Finalizes and atomically stores both datasets against the source
    /// identity captured by [`begin`](Self::begin).
    pub fn finish(&self) -> Result<AndroidTrackAnalysisOutcome, TrackAnalysisError> {
        let state = self
            .state
            .lock()
            .map_err(session_poisoned_error)?
            .take()
            .ok_or(TrackAnalysisError::Ended)?;
        let data = state.finish()?;
        let library = self
            .library
            .lock()
            .map_err(|error| library_analysis_error(&error))?;
        match set_track_render_data(&library.db, self.track_id, self.source, &data)
            .map_err(database_analysis_error)?
        {
            SpectrogramStoreOutcome::Stored => Ok(AndroidTrackAnalysisOutcome::Stored),
            SpectrogramStoreOutcome::SourceChanged => {
                Ok(AndroidTrackAnalysisOutcome::SourceChanged)
            }
        }
    }

    /// Ends the session without finalizing or storing either accumulator.
    pub fn cancel(&self) -> Result<(), TrackAnalysisError> {
        self.state.lock().map_err(session_poisoned_error)?.take();
        Ok(())
    }
}

fn session_poisoned_error<T>(_: std::sync::PoisonError<T>) -> TrackAnalysisError {
    TrackAnalysisError::Ended
}

fn library_analysis_error(error: &LibraryError) -> TrackAnalysisError {
    TrackAnalysisError::Database {
        detail: error.to_string(),
    }
}

fn database_analysis_error(error: impl std::fmt::Display) -> TrackAnalysisError {
    TrackAnalysisError::Database {
        detail: error.to_string(),
    }
}

#[uniffi::export]
impl MusicLibrary {
    /// Returns width-shaped waveform bars with Rust-computed spectral colours.
    ///
    /// `Ok(None)` means the track exists but has no complete rendering data;
    /// this is the ordinary flat-bar state. `Err(_)` means the library could
    /// not answer and must not be presented as merely unanalysed.
    pub fn track_render_bars(
        &self,
        track_id: i64,
        bar_count: u32,
    ) -> Result<Option<Vec<AndroidTrackRenderBar>>, LibraryError> {
        let state = self.lock()?;
        let peaks = get_waveform_peaks(&state.db, track_id).map_err(library_database_error)?;
        let spectrogram =
            get_track_spectrogram(&state.db, track_id).map_err(library_database_error)?;
        let Some((peaks, spectrogram)) = peaks.zip(spectrogram) else {
            return Ok(None);
        };
        let count = bar_count as usize;
        let display_bars = shape_display_peaks(&peaks, count);
        let centroid = spectrogram.centroid_curve(count);
        let mut positions = shape_centroid(&centroid, count);
        if positions.is_empty() && !display_bars.is_empty() {
            positions.resize(display_bars.len(), 0.5);
        }
        Ok(Some(
            display_bars
                .into_iter()
                .zip(positions)
                .map(|(bar, position)| {
                    let (silence, level) = match bar {
                        DisplayBar::Silence => (true, 0.0),
                        DisplayBar::Level(level) => (false, level),
                    };
                    let (red, green, blue) = spectral_colour(f64::from(position));
                    AndroidTrackRenderBar {
                        silence,
                        level,
                        red,
                        green,
                        blue,
                    }
                })
                .collect(),
        ))
    }

    /// Returns the 24 stored spectral bytes nearest `position` in the track.
    ///
    /// `Ok(None)` is an unanalysed track. A successfully analysed empty stream
    /// is `Ok(Some(vec![]))`, preserving Core's absent-versus-computed-empty
    /// distinction; a library failure remains `Err(_)`.
    pub fn track_spectrum_column(
        &self,
        track_id: i64,
        position: f64,
    ) -> Result<Option<Vec<u8>>, LibraryError> {
        let state = self.lock()?;
        if track_source_fingerprint(&state.db, track_id)
            .map_err(library_database_error)?
            .is_none()
        {
            return Err(LibraryError::TrackNotFound { track_id });
        }
        let Some(spectrogram) =
            get_track_spectrogram(&state.db, track_id).map_err(library_database_error)?
        else {
            return Ok(None);
        };
        if spectrogram.frame_count() == 0 {
            return Ok(Some(Vec::new()));
        }
        let position = if position.is_nan() { 0.0 } else { position };
        let frame =
            (position.clamp(0.0, 1.0) * (spectrogram.frame_count() - 1) as f64).round() as usize;
        Ok(Some(
            spectrogram
                .frame(frame)
                .expect("the clamped frame index is inside a non-empty spectrogram")
                .to_vec(),
        ))
    }
}

fn library_database_error(error: impl std::fmt::Display) -> LibraryError {
    LibraryError::Database {
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use reprise_core::db::{
        get_track_spectrogram, get_waveform_peaks, set_track_render_data, track_source_fingerprint,
        Db, SpectrogramStoreOutcome,
    };
    use reprise_core::library::scanner::scan_folder;
    use reprise_core::queries::{query_library_text_search, WindowRange};
    use reprise_core::spectrogram::{TrackSpectrogram, SPECTROGRAM_BAND_COUNT};
    use reprise_core::waveform::TrackRenderData;

    use crate::{MusicLibrary, TrackAnalysisSession};

    use super::{AndroidTrackAnalysisOutcome, TrackAnalysisError};

    struct Fixture {
        _directory: tempfile::TempDir,
        library: Arc<MusicLibrary>,
        database_path: PathBuf,
        track_path: PathBuf,
        track_id: i64,
        duration_ms: i64,
    }

    fn fixture() -> Fixture {
        let directory = tempfile::tempdir().unwrap();
        let music = directory.path().join("music");
        std::fs::create_dir(&music).unwrap();
        let track_path = music.join("analysis.flac");
        std::fs::copy(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../android/app/src/main/assets/sine.flac"),
            &track_path,
        )
        .unwrap();
        let database_path = directory.path().join("reprise.db");
        let db = Db::open_migrated(Some(&database_path)).unwrap();
        scan_folder(&db, &music).unwrap();
        let track = query_library_text_search(
            &db,
            "",
            WindowRange {
                offset: 0,
                limit: 1,
            },
        )
        .unwrap()
        .rows
        .remove(0);
        drop(db);
        let library = Arc::new(
            MusicLibrary::open(
                directory.path().to_str().unwrap(),
                directory.path().join("cache").to_str().unwrap(),
            )
            .unwrap(),
        );
        Fixture {
            _directory: directory,
            library,
            database_path,
            track_path,
            track_id: track.id,
            duration_ms: track.duration_ms,
        }
    }

    fn complete_pcm(duration_ms: i64) -> Vec<f32> {
        let samples = (duration_ms as u64 * 32_000).div_ceil(1_000) as usize;
        (0..samples)
            .map(|index| {
                let phase = index as f64 / 32_000.0 * std::f64::consts::TAU * 440.0;
                (0.4 * phase.sin()) as f32
            })
            .collect()
    }

    fn push_complete(session: &TrackAnalysisSession, duration_ms: i64) {
        for chunk in complete_pcm(duration_ms).chunks(32_000) {
            session.push(chunk.to_vec(), 32_000, 1).unwrap();
        }
    }

    fn push_complete_stereo_44_100(session: &TrackAnalysisSession, duration_ms: i64) {
        let frames = (duration_ms as u64 * 44_100).div_ceil(1_000) as usize;
        let interleaved = (0..frames)
            .flat_map(|index| {
                let phase = index as f64 / 44_100.0 * std::f64::consts::TAU * 440.0;
                let mono = (0.4 * phase.sin()) as f32;
                [mono + 0.1, mono - 0.1]
            })
            .collect::<Vec<_>>();
        for chunk in interleaved.chunks(44_100 * 2) {
            session.push(chunk.to_vec(), 44_100, 2).unwrap();
        }
    }

    /// The regression a phone found and no emulator could: MP3 and Opus decode
    /// to slightly more samples than their tagged duration — encoder delay,
    /// padding, pre-skip, rounding — and one sample past the waveform's bound
    /// used to end the whole pass with nothing stored and nothing said. On a
    /// library of those two formats that was every track, forever.
    #[test]
    fn a_decode_that_runs_a_little_past_the_declared_duration_is_still_stored() {
        let fixture = fixture();
        let session =
            TrackAnalysisSession::begin(fixture.library.clone(), fixture.track_id).unwrap();
        push_complete(&session, fixture.duration_ms);
        // A quarter of a second more than the duration claims, which is the
        // order of magnitude a real encoder's padding adds.
        session.push(vec![0.2; 8_000], 32_000, 1).unwrap();

        assert_eq!(
            session.finish().unwrap(),
            AndroidTrackAnalysisOutcome::Stored,
        );
        let db = Db::open_migrated(Some(&fixture.database_path)).unwrap();
        assert!(get_waveform_peaks(&db, fixture.track_id).unwrap().is_some());
        assert!(get_track_spectrogram(&db, fixture.track_id)
            .unwrap()
            .is_some());
    }

    /// The other half of the same wrong assumption, found on the emulator: a
    /// truncated or replaced file decodes to a fraction of what the duration
    /// claims, and the unused buckets stay silent. Storing that is a bar that
    /// ends in the middle of the track and calls itself finished.
    #[test]
    fn a_decode_far_shorter_than_the_declared_duration_stores_nothing() {
        let fixture = fixture();
        let session =
            TrackAnalysisSession::begin(fixture.library.clone(), fixture.track_id).unwrap();
        // A tenth of a second of a track the fixture believes is 1.16 s: the
        // proportion is what matters, and it is the proportion a two-second
        // stand-in of a three-minute song has.
        session.push(vec![0.25; 3_200], 32_000, 1).unwrap();

        assert!(matches!(
            session.finish(),
            Err(TrackAnalysisError::TooShort { .. })
        ));
        assert_no_render_data(&fixture.database_path, fixture.track_id);
    }

    fn assert_no_render_data(database_path: &Path, track_id: i64) {
        let db = Db::open_ready(database_path).unwrap();
        assert!(
            get_waveform_peaks(&db, track_id).unwrap().is_none(),
            "waveform data was stored"
        );
        assert!(
            get_track_spectrogram(&db, track_id).unwrap().is_none(),
            "spectrogram data was stored"
        );
    }

    #[test]
    fn finishing_stores_both_complete_render_datasets() {
        let fixture = fixture();
        let session =
            TrackAnalysisSession::begin(Arc::clone(&fixture.library), fixture.track_id).unwrap();
        push_complete_stereo_44_100(&session, fixture.duration_ms);

        assert_eq!(
            session.finish().unwrap(),
            AndroidTrackAnalysisOutcome::Stored
        );
        let db = Db::open_ready(&fixture.database_path).unwrap();
        assert_eq!(
            get_waveform_peaks(&db, fixture.track_id)
                .unwrap()
                .unwrap()
                .len(),
            reprise_core::waveform::STORED_PEAK_COUNT
        );
        assert!(
            get_track_spectrogram(&db, fixture.track_id)
                .unwrap()
                .unwrap()
                .frame_count()
                > 0
        );
    }

    #[test]
    fn a_source_change_is_not_success_and_stores_nothing() {
        let fixture = fixture();
        let session =
            TrackAnalysisSession::begin(Arc::clone(&fixture.library), fixture.track_id).unwrap();
        push_complete(&session, fixture.duration_ms);
        {
            let state = fixture.library.lock().unwrap();
            let report = reprise_core::library::tag_edit::apply_patch_batch(
                &state.db,
                &[(fixture.track_id, fixture.track_path.clone())],
                &reprise_core::library::tag_edit::TagPatch {
                    title: Some("Changed during analysis".into()),
                    ..Default::default()
                },
            );
            assert_eq!(report.updated_ids, vec![fixture.track_id]);
        }

        assert_eq!(
            session.finish().unwrap(),
            AndroidTrackAnalysisOutcome::SourceChanged
        );
        assert_no_render_data(&fixture.database_path, fixture.track_id);
    }

    #[test]
    fn cancelling_a_partial_session_stores_nothing() {
        let fixture = fixture();
        let session =
            TrackAnalysisSession::begin(Arc::clone(&fixture.library), fixture.track_id).unwrap();
        session.push(vec![0.25; 8_000], 32_000, 1).unwrap();
        session.cancel().unwrap();

        assert!(matches!(session.finish(), Err(TrackAnalysisError::Ended)));
        assert_no_render_data(&fixture.database_path, fixture.track_id);
    }

    #[test]
    fn dropping_a_partial_session_stores_nothing() {
        let fixture = fixture();
        let session =
            TrackAnalysisSession::begin(Arc::clone(&fixture.library), fixture.track_id).unwrap();
        session.push(vec![0.25; 8_000], 32_000, 1).unwrap();

        drop(session);
        assert_no_render_data(&fixture.database_path, fixture.track_id);
    }

    #[test]
    fn shaped_reads_keep_no_data_distinct_from_an_error() {
        let fixture = fixture();

        assert!(matches!(
            fixture.library.track_render_bars(fixture.track_id, 12),
            Ok(None)
        ));
        assert!(matches!(
            fixture.library.track_spectrum_column(fixture.track_id, 0.5),
            Ok(None)
        ));
        assert!(fixture.library.track_render_bars(i64::MAX, 12).is_err());
        assert!(fixture
            .library
            .track_spectrum_column(i64::MAX, 0.5)
            .is_err());
    }

    #[test]
    fn read_side_shapes_bars_colours_and_one_exact_spectrum_column() {
        let fixture = fixture();
        let db = Db::open_ready(&fixture.database_path).unwrap();
        let source = track_source_fingerprint(&db, fixture.track_id)
            .unwrap()
            .unwrap();
        let mut cells = Vec::new();
        for (frame, active_band) in [0, 0, 23, 23].into_iter().enumerate() {
            let mut column = vec![0; SPECTROGRAM_BAND_COUNT];
            column[active_band] = 80 + frame as u8;
            cells.extend(column);
        }
        assert_eq!(
            set_track_render_data(
                &db,
                fixture.track_id,
                source,
                &TrackRenderData {
                    waveform_peaks: [vec![0; 500], vec![255; 500]].concat(),
                    spectrogram: TrackSpectrogram::from_cells(cells.clone()).unwrap(),
                },
            )
            .unwrap(),
            SpectrogramStoreOutcome::Stored
        );
        drop(db);

        let bars = fixture
            .library
            .track_render_bars(fixture.track_id, 2)
            .unwrap()
            .unwrap();
        assert_eq!(bars.len(), 2);
        assert!(bars[0].silence);
        assert!(!bars[1].silence);
        for (actual, expected) in [
            ((bars[0].red, bars[0].green, bars[0].blue), (255, 111, 94)),
            ((bars[1].red, bars[1].green, bars[1].blue), (79, 219, 212)),
        ] {
            for (channel, byte) in [actual.0, actual.1, actual.2]
                .into_iter()
                .zip([expected.0, expected.1, expected.2])
            {
                assert!((channel - f64::from(byte) / 255.0).abs() < 1.0e-6);
            }
        }
        assert_eq!(
            fixture
                .library
                .track_spectrum_column(fixture.track_id, 0.5)
                .unwrap(),
            Some(cells[SPECTROGRAM_BAND_COUNT * 2..SPECTROGRAM_BAND_COUNT * 3].to_vec())
        );
    }

    #[test]
    fn band_colours_walk_the_whole_axis_from_the_low_end_to_the_high_one() {
        let colours = super::spectral_band_colours(SPECTROGRAM_BAND_COUNT as u32);
        assert_eq!(colours.len(), SPECTROGRAM_BAND_COUNT);

        let ends = |position: f64| {
            let (red, green, blue) = reprise_view::spectral_colour::spectral_colour(position);
            super::AndroidSpectralBandColour { red, green, blue }
        };
        assert_eq!(colours[0], ends(0.0));
        assert_eq!(colours[SPECTROGRAM_BAND_COUNT - 1], ends(1.0));
        assert!(
            colours.windows(2).any(|pair| pair[0] != pair[1]),
            "every band was given the same colour"
        );
    }

    /// One band has no position on an axis, and the middle claims nothing.
    #[test]
    fn a_single_band_is_given_the_middle_rather_than_an_end() {
        let (red, green, blue) = reprise_view::spectral_colour::spectral_colour(0.5);
        assert_eq!(
            super::spectral_band_colours(1),
            vec![super::AndroidSpectralBandColour { red, green, blue }]
        );
        assert!(super::spectral_band_colours(0).is_empty());
    }
}
