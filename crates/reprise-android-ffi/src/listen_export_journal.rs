//! Durable phone actions waiting for the desktop to acknowledge them.
//!
//! This is deliberately not `play_journal`: that journal's high-water mark
//! answers whether the phone database counted a play. This journal's sequence
//! and mark answer whether the desktop received a play or rating. Sharing either
//! number would let one delivery path erase work still owed to the other.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use reprise_core::device_sync::listen_report::{
    ListenEntry, ListenReport, ListenReportAcknowledgement, RatingEntry,
};

pub(super) const FILE_NAME: &str = "android-listens-back-export.journal";
const TEMP_FILE_NAME: &str = ".android-listens-back-export.journal.tmp";
const MAGIC: &[u8; 8] = b"RPT-JRNL";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = MAGIC.len() + 2 + 8 + 1 + 8;

/// Activity ratings and service playback share one process but use different
/// native objects and worker threads. Serialize their read-modify-replace cycle
/// here; neither object is allowed to own a second view of the same file.
static JOURNAL_ACCESS: Mutex<()> = Mutex::new(());

#[derive(Debug)]
struct JournalState {
    next_sequence: u64,
    acknowledged_sequence: Option<u64>,
    report: ListenReport,
}

impl Default for JournalState {
    fn default() -> Self {
        Self {
            next_sequence: 1,
            acknowledged_sequence: None,
            report: ListenReport::default(),
        }
    }
}

pub(super) fn record_listen(
    database_path: &Path,
    device_path: &str,
    played_at: i64,
    ms_played: u64,
) -> io::Result<u64> {
    update(database_path, |state| {
        let sequence = take_sequence(state)?;
        state.report.listens.push(ListenEntry {
            sequence,
            device_path: device_path.to_owned(),
            played_at,
            ms_played,
        });
        Ok(sequence)
    })
}

pub(super) fn record_rating(
    database_path: &Path,
    device_path: &str,
    rating: i32,
    rated_at: i64,
) -> io::Result<u64> {
    update(database_path, |state| {
        let sequence = take_sequence(state)?;
        state.report.ratings.push(RatingEntry {
            sequence,
            device_path: device_path.to_owned(),
            rating,
            rated_at,
        });
        Ok(sequence)
    })
}

pub(super) fn prepare_report(
    database_path: &Path,
    acknowledgement: Option<&[u8]>,
) -> io::Result<Vec<u8>> {
    let _access = JOURNAL_ACCESS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let path = journal_path(database_path);
    let mut state = load(&path)?;
    let mut changed = false;
    if let Some(bytes) = acknowledgement {
        match ListenReportAcknowledgement::decode(bytes) {
            Ok(acknowledgement)
                if acknowledgement.applied_sequence < u64::MAX
                    && state.next_sequence == 1
                    && state.acknowledged_sequence.is_none()
                    && state.report.listens.is_empty()
                    && state.report.ratings.is_empty() =>
            {
                // The selected sync folder survives an Android reinstall while
                // this app-private journal does not. Adopt the desktop's last
                // sequence before issuing a new one, or the fresh journal would
                // reuse an already acknowledged identity and erase the event.
                state.next_sequence = acknowledgement
                    .applied_sequence
                    .checked_add(1)
                    .ok_or_else(|| io::Error::other("Android export-journal sequence exhausted"))?;
                state.acknowledged_sequence = Some(acknowledgement.applied_sequence);
                changed = true;
            }
            Ok(acknowledgement)
                if acknowledgement.applied_sequence < state.next_sequence
                    && state
                        .acknowledged_sequence
                        .is_none_or(|previous| acknowledgement.applied_sequence > previous) =>
            {
                let applied = acknowledgement.applied_sequence;
                state
                    .report
                    .listens
                    .retain(|entry| entry.sequence > applied);
                state
                    .report
                    .ratings
                    .retain(|entry| entry.sequence > applied);
                state.acknowledged_sequence = Some(applied);
                changed = true;
            }
            Ok(acknowledgement) if acknowledgement.applied_sequence >= state.next_sequence => {
                tracing::warn!(
                    acknowledged = acknowledgement.applied_sequence,
                    next_sequence = state.next_sequence,
                    "ignored an Android listen-report acknowledgement for an unissued sequence",
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "ignored an unreadable Android listen-report acknowledgement");
            }
        }
    }
    if changed {
        rewrite(&path, &state)?;
    }
    state.report.encode().map_err(invalid_data)
}

fn update<T>(
    database_path: &Path,
    change: impl FnOnce(&mut JournalState) -> io::Result<T>,
) -> io::Result<T> {
    let _access = JOURNAL_ACCESS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let path = journal_path(database_path);
    let mut state = load(&path)?;
    let answer = change(&mut state)?;
    rewrite(&path, &state)?;
    Ok(answer)
}

fn take_sequence(state: &mut JournalState) -> io::Result<u64> {
    let sequence = state.next_sequence;
    state.next_sequence = sequence
        .checked_add(1)
        .ok_or_else(|| io::Error::other("Android export-journal sequence exhausted"))?;
    Ok(sequence)
}

fn journal_path(database_path: &Path) -> PathBuf {
    database_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(FILE_NAME)
}

fn load(path: &Path) -> io::Result<JournalState> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(JournalState::default()),
        Err(error) => return Err(error),
    };
    if bytes.len() < HEADER_BYTES || &bytes[..MAGIC.len()] != MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Android export-journal header",
        ));
    }
    let mut at = MAGIC.len();
    let version = read_u16(&bytes, &mut at)?;
    if version != VERSION {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("unsupported Android export-journal version {version}"),
        ));
    }
    let next_sequence = read_u64(&bytes, &mut at)?;
    let acknowledged = read_u8(&bytes, &mut at)?;
    let acknowledged_sequence = match acknowledged {
        0 => {
            let _unused = read_u64(&bytes, &mut at)?;
            None
        }
        1 => Some(read_u64(&bytes, &mut at)?),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid Android export-journal acknowledgement flag",
            ));
        }
    };
    let report = ListenReport::decode(&bytes[at..]).map_err(invalid_data)?;
    let highest = report.highest_sequence().unwrap_or(0);
    if next_sequence == 0
        || highest >= next_sequence
        || acknowledged_sequence.is_some_and(|sequence| sequence >= next_sequence)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Android export-journal sequence state",
        ));
    }
    Ok(JournalState {
        next_sequence,
        acknowledged_sequence,
        report,
    })
}

fn rewrite(path: &Path, state: &JournalState) -> io::Result<()> {
    let report = state.report.encode().map_err(invalid_data)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "journal has no parent"))?;
    let temporary = parent.join(TEMP_FILE_NAME);
    let mut file = File::create(&temporary)?;
    file.write_all(MAGIC)?;
    file.write_all(&VERSION.to_le_bytes())?;
    file.write_all(&state.next_sequence.to_le_bytes())?;
    match state.acknowledged_sequence {
        Some(sequence) => {
            file.write_all(&[1])?;
            file.write_all(&sequence.to_le_bytes())?;
        }
        None => {
            file.write_all(&[0])?;
            file.write_all(&0_u64.to_le_bytes())?;
        }
    }
    file.write_all(&report)?;
    file.sync_data()?;
    fs::rename(&temporary, path)?;
    File::open(parent)?.sync_all()
}

fn read_u8(bytes: &[u8], at: &mut usize) -> io::Result<u8> {
    let value = *bytes
        .get(*at)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "truncated journal"))?;
    *at += 1;
    Ok(value)
}

fn read_u16(bytes: &[u8], at: &mut usize) -> io::Result<u16> {
    read_array(bytes, at).map(u16::from_le_bytes)
}

fn read_u64(bytes: &[u8], at: &mut usize) -> io::Result<u64> {
    read_array(bytes, at).map(u64::from_le_bytes)
}

fn read_array<const N: usize>(bytes: &[u8], at: &mut usize) -> io::Result<[u8; N]> {
    let end = at
        .checked_add(N)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "journal offset overflow"))?;
    let value = bytes
        .get(*at..end)
        .ok_or_else(|| io::Error::new(io::ErrorKind::UnexpectedEof, "truncated journal"))?
        .try_into()
        .expect("slice length was checked");
    *at = end;
    Ok(value)
}

fn invalid_data(error: impl std::fmt::Display) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error.to_string())
}
