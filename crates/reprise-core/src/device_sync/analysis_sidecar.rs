//! Versioned binary rendering data carried beside one synchronized track.
//!
//! The format is deliberately owned by Core so every producer and consumer
//! uses the same parser. It starts with `RPA-SIDE`, a little-endian `u16`
//! version, the existing [`TrackSourceFingerprint`], two little-endian `u32`
//! byte lengths, then the raw spectrogram cells and waveform peaks.

use std::path::Path;

use crate::spectrogram::{TrackSourceFingerprint, TrackSpectrogram};

const MAGIC: &[u8; 8] = b"RPA-SIDE";
pub const FORMAT_VERSION: u16 = 1;
pub const EXTENSION: &str = "reprise-analysis";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisSidecar {
    pub source: TrackSourceFingerprint,
    pub spectrogram: TrackSpectrogram,
    pub waveform_peaks: Vec<u8>,
}

impl AnalysisSidecar {
    pub fn new(
        source: TrackSourceFingerprint,
        spectrogram: TrackSpectrogram,
        waveform_peaks: Vec<u8>,
    ) -> Self {
        Self {
            source,
            spectrogram,
            waveform_peaks,
        }
    }

    pub fn encode(&self) -> Result<Vec<u8>, AnalysisSidecarError> {
        let spectrogram_len = u32::try_from(self.spectrogram.cells().len())
            .map_err(|_| AnalysisSidecarError::TooLarge)?;
        let waveform_len =
            u32::try_from(self.waveform_peaks.len()).map_err(|_| AnalysisSidecarError::TooLarge)?;
        let mut bytes = Vec::with_capacity(
            8 + 2 + 16 + 18 + 8 + self.spectrogram.cells().len() + self.waveform_peaks.len(),
        );
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.source.mtime_seconds.to_le_bytes());
        bytes.extend_from_slice(&self.source.size_bytes.to_le_bytes());
        encode_optional_i64(&mut bytes, self.source.device);
        encode_optional_i64(&mut bytes, self.source.inode);
        bytes.extend_from_slice(&spectrogram_len.to_le_bytes());
        bytes.extend_from_slice(&waveform_len.to_le_bytes());
        bytes.extend_from_slice(self.spectrogram.cells());
        bytes.extend_from_slice(&self.waveform_peaks);
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, AnalysisSidecarError> {
        let mut reader = Reader::new(bytes);
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(AnalysisSidecarError::InvalidMagic);
        }
        let version = reader.u16()?;
        if version != FORMAT_VERSION {
            return Err(AnalysisSidecarError::UnsupportedVersion(version));
        }
        let source = TrackSourceFingerprint {
            mtime_seconds: reader.i64()?,
            size_bytes: reader.i64()?,
            device: reader.optional_i64()?,
            inode: reader.optional_i64()?,
        };
        let spectrogram_len = reader.u32()? as usize;
        let waveform_len = reader.u32()? as usize;
        let spectrogram_cells = reader.take(spectrogram_len)?.to_vec();
        let waveform_peaks = reader.take(waveform_len)?.to_vec();
        if !reader.is_empty() {
            return Err(AnalysisSidecarError::TrailingBytes);
        }
        let spectrogram = TrackSpectrogram::from_cells(spectrogram_cells).map_err(|error| {
            AnalysisSidecarError::InvalidSpectrogram {
                byte_count: match error {
                    crate::spectrogram::SpectrogramFormatError::IncompleteFrame { byte_count } => {
                        byte_count
                    }
                },
            }
        })?;
        Ok(Self::new(source, spectrogram, waveform_peaks))
    }

    /// Loads one complete, currently source-valid rendering dataset.
    ///
    /// The fingerprint comes from the same database function that guards the
    /// render cache; the sync format never derives a competing identity.
    pub fn for_track(
        db: &crate::db::Db,
        track_id: i64,
    ) -> Result<Option<Self>, crate::db::DbError> {
        let Some(source) = crate::db_spectrogram::track_source_fingerprint(db, track_id)? else {
            return Ok(None);
        };
        let Some(spectrogram) = crate::db_spectrogram::get_track_spectrogram(db, track_id)? else {
            return Ok(None);
        };
        let Some(waveform_peaks) = crate::db_spectrogram::get_waveform_peaks(db, track_id)? else {
            return Ok(None);
        };
        Ok(Some(Self::new(source, spectrogram, waveform_peaks)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AnalysisSidecarError {
    #[error("analysis sidecar has the wrong magic")]
    InvalidMagic,
    #[error("analysis sidecar version {0} is not supported")]
    UnsupportedVersion(u16),
    #[error("analysis sidecar ended before its declared data")]
    UnexpectedEnd,
    #[error("analysis sidecar has trailing bytes")]
    TrailingBytes,
    #[error("analysis sidecar spectrogram has {byte_count} bytes, not complete frames")]
    InvalidSpectrogram { byte_count: usize },
    #[error("analysis sidecar data is too large")]
    TooLarge,
}

pub fn device_path_for_track(device_path: &str) -> Option<String> {
    let device_path = Path::new(device_path);
    device_path.file_name()?;
    Some(
        device_path
            .with_extension(EXTENSION)
            .to_string_lossy()
            .into_owned(),
    )
}

pub fn is_sidecar_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case(EXTENSION))
}

fn encode_optional_i64(bytes: &mut Vec<u8>, value: Option<i64>) {
    match value {
        Some(value) => {
            bytes.push(1);
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        None => bytes.push(0),
    }
}

struct Reader<'a> {
    remaining: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], AnalysisSidecarError> {
        let Some((head, tail)) = self.remaining.split_at_checked(count) else {
            return Err(AnalysisSidecarError::UnexpectedEnd);
        };
        self.remaining = tail;
        Ok(head)
    }

    fn u16(&mut self) -> Result<u16, AnalysisSidecarError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two bytes were requested"),
        ))
    }

    fn u32(&mut self) -> Result<u32, AnalysisSidecarError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes were requested"),
        ))
    }

    fn i64(&mut self) -> Result<i64, AnalysisSidecarError> {
        Ok(i64::from_le_bytes(
            self.take(8)?
                .try_into()
                .expect("eight bytes were requested"),
        ))
    }

    fn optional_i64(&mut self) -> Result<Option<i64>, AnalysisSidecarError> {
        match self.take(1)?[0] {
            0 => Ok(None),
            1 => self.i64().map(Some),
            _ => Err(AnalysisSidecarError::InvalidMagic),
        }
    }

    fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }
}

#[cfg(test)]
#[path = "analysis_sidecar_tests.rs"]
mod tests;
