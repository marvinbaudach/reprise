use reprise_core::db::{get_track_spectrogram, get_waveform_peaks};
use reprise_core::queries::query_present_track_by_id;
use reprise_view::spectral_colour::{
    shape_centroid, smooth_centroid_over_seconds, spectral_colour, CENTROID_WINDOW_S,
};
use reprise_view::waveform::{shape_display_peaks, DisplayBar};

use crate::{LibraryError, MusicLibrary};

const MAX_TRACK_RENDER_BAR_COUNT: usize = 4_096;

/// One fully shaped seek-bar cell with its Rust-owned spectral colour.
#[derive(Clone, Copy, Debug, PartialEq, uniffi::Record)]
pub struct AndroidTrackRenderBar {
    pub silence: bool,
    /// Audible height in `0..=1`; zero when [`silence`](Self::silence) is true.
    pub level: f32,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
}

#[uniffi::export]
impl MusicLibrary {
    /// Returns finished seek-bar cells for one track and requested bar count.
    ///
    /// `Ok(None)` is the ordinary no-analysis answer. Heights, the fixed
    /// seconds-based centroid averaging, width shaping, and spectral colours
    /// all stay in Rust so Compose receives drawing data rather than another
    /// opportunity to reinterpret the stored analysis.
    pub fn track_render_bars(
        &self,
        track_id: i64,
        bar_count: u32,
    ) -> Result<Option<Vec<AndroidTrackRenderBar>>, LibraryError> {
        let state = self.lock()?;
        let track = query_present_track_by_id(&state.db, track_id)
            .map_err(query_error)?
            .ok_or(LibraryError::TrackNotFound { track_id })?;
        let peaks = get_waveform_peaks(&state.db, track_id).map_err(database_error)?;
        let spectrogram = get_track_spectrogram(&state.db, track_id).map_err(database_error)?;
        let Some((peaks, spectrogram)) = peaks.zip(spectrogram) else {
            return Ok(None);
        };
        let count = (bar_count as usize)
            .min(MAX_TRACK_RENDER_BAR_COUNT)
            .min(usize::MAX / peaks.len().max(1));
        let display_bars = shape_display_peaks(&peaks, count);
        if display_bars.is_empty() {
            return Ok(None);
        }

        let raw_centroid = spectrogram.centroid_curve(peaks.len());
        let duration_s = track.duration_ms as f64 / 1_000.0;
        let smoothed = smooth_centroid_over_seconds(&raw_centroid, duration_s, CENTROID_WINDOW_S);
        let mut positions = shape_centroid(&smoothed, count);
        if positions.is_empty() {
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
}

fn database_error(error: impl std::fmt::Display) -> LibraryError {
    LibraryError::Database {
        detail: error.to_string(),
    }
}

fn query_error(error: impl std::fmt::Display) -> LibraryError {
    LibraryError::Query {
        detail: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use reprise_core::db::{set_track_render_data, track_source_fingerprint, Db};
    use reprise_core::library::scanner::scan_folder;
    use reprise_core::queries::{query_library_text_search, WindowRange};
    use reprise_core::spectrogram::{TrackSpectrogram, SPECTROGRAM_BAND_COUNT};
    use reprise_core::waveform::TrackRenderData;
    use reprise_view::spectral_colour::spectral_colour;

    use crate::MusicLibrary;

    fn library_with_two_tracks() -> (tempfile::TempDir, MusicLibrary, i64, i64) {
        let directory = tempfile::tempdir().unwrap();
        let music = directory.path().join("music");
        std::fs::create_dir(&music).unwrap();
        let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../android/app/src/main/assets/sine.flac");
        std::fs::copy(&source, music.join("analysed.flac")).unwrap();
        std::fs::copy(&source, music.join("plain.flac")).unwrap();
        let database_path = directory.path().join("reprise.db");
        let db = Db::open_migrated(Some(&database_path)).unwrap();
        scan_folder(&db, &music).unwrap();
        let tracks = query_library_text_search(
            &db,
            "",
            WindowRange {
                offset: 0,
                limit: 2,
            },
        )
        .unwrap()
        .rows;
        let analysed_id = tracks
            .iter()
            .find(|track| track.title == "analysed")
            .unwrap()
            .id;
        let plain_id = tracks
            .iter()
            .find(|track| track.title == "plain")
            .unwrap()
            .id;
        let source = track_source_fingerprint(&db, analysed_id).unwrap().unwrap();
        let mut cells = vec![0; SPECTROGRAM_BAND_COUNT * 2];
        cells[0] = u8::MAX;
        cells[SPECTROGRAM_BAND_COUNT * 2 - 1] = u8::MAX;
        set_track_render_data(
            &db,
            analysed_id,
            source,
            &TrackRenderData {
                waveform_peaks: vec![0, u8::MAX],
                spectrogram: TrackSpectrogram::from_cells(cells).unwrap(),
            },
        )
        .unwrap();
        drop(db);
        let library = MusicLibrary::open(
            directory.path().to_str().unwrap(),
            directory.path().join("cache").to_str().unwrap(),
        )
        .unwrap();
        (directory, library, analysed_id, plain_id)
    }

    #[test]
    fn real_library_render_data_crosses_the_boundary_with_view_owned_colours() {
        let (_directory, library, analysed_id, plain_id) = library_with_two_tracks();

        let bars = library.track_render_bars(analysed_id, 2).unwrap().unwrap();

        assert_eq!(bars.len(), 2);
        assert!(bars[0].silence);
        assert_eq!(bars[0].level, 0.0);
        assert!(!bars[1].silence);
        assert!(
            (bars[1].level - 0.247_407_72).abs() < 1.0e-6,
            "second shaped level was {}",
            bars[1].level,
        );
        let expected_colour = spectral_colour(f64::from(u8::MAX / 2 + 1) / f64::from(u8::MAX));
        for actual in [
            (bars[0].red, bars[0].green, bars[0].blue),
            (bars[1].red, bars[1].green, bars[1].blue),
        ] {
            for (channel, expected) in [actual.0, actual.1, actual.2].into_iter().zip([
                expected_colour.0,
                expected_colour.1,
                expected_colour.2,
            ]) {
                assert!(
                    (channel - expected).abs() < 1.0e-6,
                    "channel was {channel}, expected {expected}",
                );
            }
        }
        assert_eq!(library.track_render_bars(plain_id, 2).unwrap(), None);
    }

    #[test]
    fn public_bar_count_is_bounded_before_bucket_arithmetic() {
        let (_directory, library, analysed_id, _plain_id) = library_with_two_tracks();

        let bars = library
            .track_render_bars(analysed_id, 4_097)
            .unwrap()
            .unwrap();

        assert_eq!(bars.len(), 4_096);
    }
}
