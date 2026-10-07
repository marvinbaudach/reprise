//! Versioned rating/play-count list written once at the sync root.
//!
//! The binary layout is `RPT-LIST`, a little-endian `u16` version, a
//! little-endian `u32` entry count, then for each entry a `u32` UTF-8 path
//! length, path bytes, the real little-endian `i32` rating, and the
//! little-endian `i64` play count. Version 2 follows with a second counted
//! section, one entry per track a CUE sheet cut from a synced file: the path
//! as above, the track's little-endian `i64` start in milliseconds, then the
//! rating and play count (CUE-17). The device-relative path, with the start
//! for a CUE track, is the identity shared by the desktop plan and the phone's
//! scan; database row ids are not.

const MAGIC: &[u8; 8] = b"RPT-LIST";
pub const FORMAT_VERSION: u16 = 2;
pub const FILE_NAME: &str = "reprise-track-metadata.rpl";

pub fn is_list_path(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(FILE_NAME))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackMetadataEntry {
    pub device_path: String,
    pub rating: i32,
    pub play_count: i64,
}

/// The rating and play count of a track a CUE sheet cut from a synced file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SegmentMetadataEntry {
    pub device_path: String,
    pub segment_start_ms: i64,
    pub rating: i32,
    pub play_count: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackMetadataList {
    /// Whole-file tracks.
    pub entries: Vec<TrackMetadataEntry>,
    /// Tracks a CUE sheet cut from a synced file.
    pub segments: Vec<SegmentMetadataEntry>,
}

impl TrackMetadataList {
    pub fn new(entries: Vec<TrackMetadataEntry>) -> Self {
        Self {
            entries,
            segments: Vec::new(),
        }
    }

    pub fn with_segments(self, segments: Vec<SegmentMetadataEntry>) -> Self {
        Self { segments, ..self }
    }

    pub fn encode(&self) -> Result<Vec<u8>, TrackMetadataListError> {
        let count =
            u32::try_from(self.entries.len()).map_err(|_| TrackMetadataListError::TooLarge)?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&count.to_le_bytes());
        for entry in &self.entries {
            let path = entry.device_path.as_bytes();
            let path_len =
                u32::try_from(path.len()).map_err(|_| TrackMetadataListError::TooLarge)?;
            bytes.extend_from_slice(&path_len.to_le_bytes());
            bytes.extend_from_slice(path);
            bytes.extend_from_slice(&entry.rating.to_le_bytes());
            bytes.extend_from_slice(&entry.play_count.to_le_bytes());
        }
        let segment_count =
            u32::try_from(self.segments.len()).map_err(|_| TrackMetadataListError::TooLarge)?;
        bytes.extend_from_slice(&segment_count.to_le_bytes());
        for entry in &self.segments {
            let path = entry.device_path.as_bytes();
            let path_len =
                u32::try_from(path.len()).map_err(|_| TrackMetadataListError::TooLarge)?;
            bytes.extend_from_slice(&path_len.to_le_bytes());
            bytes.extend_from_slice(path);
            bytes.extend_from_slice(&entry.segment_start_ms.to_le_bytes());
            bytes.extend_from_slice(&entry.rating.to_le_bytes());
            bytes.extend_from_slice(&entry.play_count.to_le_bytes());
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, TrackMetadataListError> {
        let mut reader = Reader::new(bytes);
        if reader.take(MAGIC.len())? != MAGIC {
            return Err(TrackMetadataListError::InvalidMagic);
        }
        let version = reader.u16()?;
        if version != FORMAT_VERSION {
            return Err(TrackMetadataListError::UnsupportedVersion(version));
        }
        let count = reader.u32()? as usize;
        let mut entries = Vec::with_capacity(count.min(4_096));
        for _ in 0..count {
            entries.push(TrackMetadataEntry {
                device_path: reader.path()?,
                rating: reader.i32()?,
                play_count: reader.i64()?,
            });
        }
        let segment_count = reader.u32()? as usize;
        let mut segments = Vec::with_capacity(segment_count.min(4_096));
        for _ in 0..segment_count {
            segments.push(SegmentMetadataEntry {
                device_path: reader.path()?,
                segment_start_ms: reader.i64()?,
                rating: reader.i32()?,
                play_count: reader.i64()?,
            });
        }
        if !reader.is_empty() {
            return Err(TrackMetadataListError::TrailingBytes);
        }
        Ok(Self::new(entries).with_segments(segments))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TrackMetadataListError {
    #[error("track metadata list has the wrong magic")]
    InvalidMagic,
    #[error("track metadata list version {0} is not supported")]
    UnsupportedVersion(u16),
    #[error("track metadata list ended before its declared data")]
    UnexpectedEnd,
    #[error("track metadata list contains an invalid UTF-8 identity")]
    InvalidUtf8,
    #[error("track metadata list has trailing bytes")]
    TrailingBytes,
    #[error("track metadata list is too large")]
    TooLarge,
}

struct Reader<'a> {
    remaining: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], TrackMetadataListError> {
        let Some((head, tail)) = self.remaining.split_at_checked(count) else {
            return Err(TrackMetadataListError::UnexpectedEnd);
        };
        self.remaining = tail;
        Ok(head)
    }

    fn path(&mut self) -> Result<String, TrackMetadataListError> {
        let path_len = self.u32()? as usize;
        std::str::from_utf8(self.take(path_len)?)
            .map(str::to_owned)
            .map_err(|_| TrackMetadataListError::InvalidUtf8)
    }

    fn u16(&mut self) -> Result<u16, TrackMetadataListError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two bytes were requested"),
        ))
    }

    fn u32(&mut self) -> Result<u32, TrackMetadataListError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes were requested"),
        ))
    }

    fn i32(&mut self) -> Result<i32, TrackMetadataListError> {
        Ok(i32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes were requested"),
        ))
    }

    fn i64(&mut self) -> Result<i64, TrackMetadataListError> {
        Ok(i64::from_le_bytes(
            self.take(8)?
                .try_into()
                .expect("eight bytes were requested"),
        ))
    }

    fn is_empty(&self) -> bool {
        self.remaining.is_empty()
    }
}

#[cfg(test)]
#[path = "track_metadata_list_tests.rs"]
mod tests;
