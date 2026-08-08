//! Versioned phone-to-desktop listening report and acknowledgement formats.

const REPORT_MAGIC: &[u8; 8] = b"RPT-BACK";
const ACKNOWLEDGEMENT_MAGIC: &[u8; 8] = b"RPT-ACKN";
const MAX_PREALLOCATED_ENTRIES: usize = 4_096;

pub const FORMAT_VERSION: u16 = 1;
pub const REPORT_FILE_NAME: &str = "reprise-listens-back.rpl";
pub const ACKNOWLEDGEMENT_FILE_NAME: &str = "reprise-listens-back-ack.rpl";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListenEntry {
    pub sequence: u64,
    pub device_path: String,
    pub played_at: i64,
    pub ms_played: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatingEntry {
    pub sequence: u64,
    pub device_path: String,
    pub rating: i32,
    pub rated_at: i64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListenReport {
    pub listens: Vec<ListenEntry>,
    pub ratings: Vec<RatingEntry>,
}

impl ListenReport {
    pub fn new(listens: Vec<ListenEntry>, ratings: Vec<RatingEntry>) -> Self {
        Self { listens, ratings }
    }

    pub fn encode(&self) -> Result<Vec<u8>, ListenReportError> {
        let listen_count =
            u32::try_from(self.listens.len()).map_err(|_| ListenReportError::TooLarge)?;
        let rating_count =
            u32::try_from(self.ratings.len()).map_err(|_| ListenReportError::TooLarge)?;
        let mut bytes = Vec::new();
        bytes.extend_from_slice(REPORT_MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&listen_count.to_le_bytes());
        for entry in &self.listens {
            bytes.extend_from_slice(&entry.sequence.to_le_bytes());
            encode_path(&mut bytes, &entry.device_path)?;
            bytes.extend_from_slice(&entry.played_at.to_le_bytes());
            bytes.extend_from_slice(&entry.ms_played.to_le_bytes());
        }
        bytes.extend_from_slice(&rating_count.to_le_bytes());
        for entry in &self.ratings {
            bytes.extend_from_slice(&entry.sequence.to_le_bytes());
            encode_path(&mut bytes, &entry.device_path)?;
            bytes.extend_from_slice(&entry.rating.to_le_bytes());
            bytes.extend_from_slice(&entry.rated_at.to_le_bytes());
        }
        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ListenReportError> {
        let mut reader = Reader::new(bytes);
        read_header(&mut reader, REPORT_MAGIC)?;
        let listen_count = reader.u32()? as usize;
        let mut listens = Vec::with_capacity(listen_count.min(MAX_PREALLOCATED_ENTRIES));
        for _ in 0..listen_count {
            listens.push(ListenEntry {
                sequence: reader.u64()?,
                device_path: reader.path()?,
                played_at: reader.i64()?,
                ms_played: reader.u64()?,
            });
        }
        let rating_count = reader.u32()? as usize;
        let mut ratings = Vec::with_capacity(rating_count.min(MAX_PREALLOCATED_ENTRIES));
        for _ in 0..rating_count {
            ratings.push(RatingEntry {
                sequence: reader.u64()?,
                device_path: reader.path()?,
                rating: reader.i32()?,
                rated_at: reader.i64()?,
            });
        }
        if !reader.is_empty() {
            return Err(ListenReportError::TrailingBytes);
        }
        Ok(Self::new(listens, ratings))
    }

    pub fn highest_sequence(&self) -> Option<u64> {
        self.listens
            .iter()
            .map(|entry| entry.sequence)
            .chain(self.ratings.iter().map(|entry| entry.sequence))
            .max()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListenReportAcknowledgement {
    pub applied_sequence: u64,
}

impl ListenReportAcknowledgement {
    pub fn new(applied_sequence: u64) -> Self {
        Self { applied_sequence }
    }

    pub fn encode(self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(18);
        bytes.extend_from_slice(ACKNOWLEDGEMENT_MAGIC);
        bytes.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        bytes.extend_from_slice(&self.applied_sequence.to_le_bytes());
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, ListenReportError> {
        let mut reader = Reader::new(bytes);
        read_header(&mut reader, ACKNOWLEDGEMENT_MAGIC)?;
        let acknowledgement = Self::new(reader.u64()?);
        if !reader.is_empty() {
            return Err(ListenReportError::TrailingBytes);
        }
        Ok(acknowledgement)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ListenReportError {
    #[error("listen report has the wrong magic")]
    InvalidMagic,
    #[error("listen report version {0} is not supported")]
    UnsupportedVersion(u16),
    #[error("listen report ended before its declared data")]
    UnexpectedEnd,
    #[error("listen report contains an invalid UTF-8 path")]
    InvalidUtf8,
    #[error("listen report has trailing bytes")]
    TrailingBytes,
    #[error("listen report is too large")]
    TooLarge,
}

fn encode_path(bytes: &mut Vec<u8>, path: &str) -> Result<(), ListenReportError> {
    let path = path.as_bytes();
    let path_len = u32::try_from(path.len()).map_err(|_| ListenReportError::TooLarge)?;
    bytes.extend_from_slice(&path_len.to_le_bytes());
    bytes.extend_from_slice(path);
    Ok(())
}

fn read_header(reader: &mut Reader<'_>, magic: &[u8; 8]) -> Result<(), ListenReportError> {
    if reader.take(magic.len())? != magic {
        return Err(ListenReportError::InvalidMagic);
    }
    let version = reader.u16()?;
    if version != FORMAT_VERSION {
        return Err(ListenReportError::UnsupportedVersion(version));
    }
    Ok(())
}

struct Reader<'a> {
    remaining: &'a [u8],
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], ListenReportError> {
        let Some((head, tail)) = self.remaining.split_at_checked(count) else {
            return Err(ListenReportError::UnexpectedEnd);
        };
        self.remaining = tail;
        Ok(head)
    }

    fn path(&mut self) -> Result<String, ListenReportError> {
        let path_len = self.u32()? as usize;
        std::str::from_utf8(self.take(path_len)?)
            .map(str::to_owned)
            .map_err(|_| ListenReportError::InvalidUtf8)
    }

    fn u16(&mut self) -> Result<u16, ListenReportError> {
        Ok(u16::from_le_bytes(
            self.take(2)?.try_into().expect("two bytes were requested"),
        ))
    }

    fn u32(&mut self) -> Result<u32, ListenReportError> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes were requested"),
        ))
    }

    fn i32(&mut self) -> Result<i32, ListenReportError> {
        Ok(i32::from_le_bytes(
            self.take(4)?.try_into().expect("four bytes were requested"),
        ))
    }

    fn u64(&mut self) -> Result<u64, ListenReportError> {
        Ok(u64::from_le_bytes(
            self.take(8)?
                .try_into()
                .expect("eight bytes were requested"),
        ))
    }

    fn i64(&mut self) -> Result<i64, ListenReportError> {
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
#[path = "listen_report_tests.rs"]
mod tests;
