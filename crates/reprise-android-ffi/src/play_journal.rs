//! File-backed pending plays for the Android playback-session writer.

use std::collections::VecDeque;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::play_recorder::RecordedPlay;

pub(super) const FILE_NAME: &str = "android-play-count.journal";
const FORMAT_VERSION: &str = "v1";
const TEMP_FILE_NAME: &str = ".android-play-count.journal.tmp";

/// About 65 KiB at the format's longest possible integer widths, and more
/// than two days of uninterrupted three-minute tracks. Once full, the journal
/// refuses the newest play and names it in the log: evicting an older entry
/// would break a durability promise already made, while silently accepting
/// more would turn a permanently wedged database into unbounded file growth.
pub(super) const MAX_PENDING_PLAYS: usize = 1_024;

#[derive(Clone, Copy, Debug)]
pub(super) struct JournalEntry {
    pub(super) sequence: i64,
    pub(super) play: RecordedPlay,
}

pub(super) struct PlayJournal {
    path: PathBuf,
    entries: VecDeque<JournalEntry>,
    next_sequence: i64,
}

impl PlayJournal {
    pub(super) fn open(database_path: &Path, applied_sequence: i64) -> io::Result<Self> {
        let directory = database_path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "database path has no parent")
        })?;
        let path = directory.join(FILE_NAME);
        let entries = if path.exists() {
            let (entries, needs_repair) = parse_entries(&fs::read(&path)?);
            if needs_repair {
                rewrite(&path, entries.iter().copied())?;
            }
            entries
        } else {
            VecDeque::new()
        };
        let last_pending = entries.back().map_or(0, |entry| entry.sequence);
        let next_sequence = applied_sequence
            .max(last_pending)
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Android play-journal sequence exhausted"))?;
        Ok(Self {
            path,
            entries,
            next_sequence,
        })
    }

    /// Returns `false` only when the bounded journal explicitly refuses the
    /// newest play. I/O failures remain errors because no durable promise was
    /// made in that case either.
    pub(super) fn append(&mut self, play: RecordedPlay) -> io::Result<bool> {
        if self.entries.len() >= MAX_PENDING_PLAYS {
            tracing::warn!(
                track_id = play.track_id,
                pending = self.entries.len(),
                "refused the newest Android play count: the play journal is full",
            );
            return Ok(false);
        }
        let following_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| io::Error::other("Android play-journal sequence exhausted"))?;
        let entry = JournalEntry {
            sequence: self.next_sequence,
            play,
        };
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        write_entry(&mut file, entry)?;
        file.flush()?;
        self.entries.push_back(entry);
        self.next_sequence = following_sequence;
        Ok(true)
    }

    pub(super) fn front(&self) -> Option<JournalEntry> {
        self.entries.front().copied()
    }

    pub(super) fn remove_front(&mut self) -> io::Result<()> {
        let remaining = self.entries.iter().copied().skip(1);
        rewrite(&self.path, remaining)?;
        self.entries.pop_front();
        Ok(())
    }
}

fn parse_entries(contents: &[u8]) -> (VecDeque<JournalEntry>, bool) {
    let mut entries = VecDeque::new();
    let mut needs_repair = false;
    let complete_file = contents.last().is_none_or(|byte| *byte == b'\n');
    let lines: Vec<_> = contents.split(|byte| *byte == b'\n').collect();
    for (index, bytes) in lines.iter().enumerate() {
        if bytes.is_empty() && index + 1 == lines.len() {
            continue;
        }
        let line_number = index + 1;
        if index + 1 == lines.len() && !complete_file {
            tracing::warn!(
                line = line_number,
                "discarded a truncated Android play-journal tail",
            );
            needs_repair = true;
            continue;
        }
        let entry = std::str::from_utf8(bytes)
            .ok()
            .and_then(|line| parse_entry(line).ok());
        let Some(entry) = entry else {
            tracing::warn!(
                line = line_number,
                "discarded a corrupt Android play-journal entry",
            );
            needs_repair = true;
            continue;
        };
        if entries
            .back()
            .is_some_and(|previous: &JournalEntry| entry.sequence <= previous.sequence)
        {
            tracing::warn!(
                line = line_number,
                sequence = entry.sequence,
                "discarded an out-of-order Android play-journal entry",
            );
            needs_repair = true;
            continue;
        }
        if entries.len() >= MAX_PENDING_PLAYS {
            tracing::warn!(
                line = line_number,
                track_id = entry.play.track_id,
                "discarded a newest Android play-journal entry beyond the journal cap",
            );
            needs_repair = true;
            continue;
        }
        entries.push_back(entry);
    }
    (entries, needs_repair)
}

fn parse_entry(line: &str) -> io::Result<JournalEntry> {
    let mut fields = line.split('\t');
    let version = fields.next();
    let sequence = parse_number(fields.next(), "sequence")?;
    let track_id = parse_number(fields.next(), "track id")?;
    let at_unix = parse_number(fields.next(), "timestamp")?;
    if version != Some(FORMAT_VERSION) || fields.next().is_some() || sequence <= 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid Android play-journal entry",
        ));
    }
    Ok(JournalEntry {
        sequence,
        play: RecordedPlay { track_id, at_unix },
    })
}

fn parse_number(value: Option<&str>, name: &str) -> io::Result<i64> {
    value
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, format!("missing {name}")))?
        .parse()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn rewrite(path: &Path, entries: impl Iterator<Item = JournalEntry>) -> io::Result<()> {
    let temporary = path.with_file_name(TEMP_FILE_NAME);
    let mut file = fs::File::create(&temporary)?;
    for entry in entries {
        write_entry(&mut file, entry)?;
    }
    file.flush()?;
    fs::rename(temporary, path)
}

fn write_entry(writer: &mut impl Write, entry: JournalEntry) -> io::Result<()> {
    writeln!(
        writer,
        "{FORMAT_VERSION}\t{}\t{}\t{}",
        entry.sequence, entry.play.track_id, entry.play.at_unix,
    )
}

#[cfg(test)]
mod tests {
    use super::{PlayJournal, MAX_PENDING_PLAYS};
    use crate::log_capture::{CapturedLogs, LogCapture};
    use crate::play_recorder::RecordedPlay;

    #[test]
    fn a_full_journal_refuses_and_names_the_newest_play() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("reprise.db");
        let mut journal = PlayJournal::open(&database_path, 0).unwrap();
        for track_id in 1..=MAX_PENDING_PLAYS as i64 {
            assert!(journal
                .append(RecordedPlay {
                    track_id,
                    at_unix: 1_700_000_000,
                })
                .unwrap());
        }

        let logs = CapturedLogs::default();
        let accepted = tracing::subscriber::with_default(LogCapture(logs.clone()), || {
            journal
                .append(RecordedPlay {
                    track_id: 9_999,
                    at_unix: 1_700_000_001,
                })
                .unwrap()
        });

        assert!(!accepted);
        let logged = logs.joined();
        assert!(
            logged.contains("refused the newest Android play count"),
            "the bounded loss must be explicit, got {logged}",
        );
        assert!(
            logged.contains("track_id=9999"),
            "the refused track must be named, got {logged}",
        );
        assert_eq!(
            std::fs::read_to_string(directory.path().join(super::FILE_NAME))
                .unwrap()
                .lines()
                .count(),
            MAX_PENDING_PLAYS,
        );
    }
}
