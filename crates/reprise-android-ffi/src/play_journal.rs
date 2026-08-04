//! File-backed pending plays for the Android playback-session writer.
//!
//! ## One writer, held open rather than assumed
//!
//! Two journals over one file would each hold their own view of the pending
//! queue, read the same applied high-water mark, hand out the same sequence
//! numbers, and — the moment either one removes an entry — write its view over
//! the other's lines. Nothing in the app arranges that today (service and
//! activity share one process, and the session is built once), but nothing in
//! the app forbids it either: one `android:process` attribute would be enough.
//!
//! So it is not assumed. [`PlayJournal::open`] takes an exclusive advisory lock
//! and a second writer is refused with an error rather than quietly allowed to
//! destroy plays it cannot see. The lock sits in its own file because
//! [`rewrite`] replaces the journal by rename: a lock on that inode would be
//! left holding an unlinked file as soon as the first entry left.
//!
//! ## What "durable" means here
//!
//! Every append and every rewrite is followed through to the platter:
//! `sync_data` for the contents and an `fsync` of the parent directory for the
//! name that makes them findable. So the promise is the strong one — a counted
//! play survives a power cut and a kernel panic, not only the process being
//! killed. The cost is affordable precisely because of how rare the event is: a
//! play arrives a few times an hour, on a writer thread that never blocks
//! playback, and SQLite is about to fsync its own commit for the same play
//! anyway.
//!
//! ## Unexpected is not one thing
//!
//! A line this build cannot read at all, a line in a format version it does not
//! know, and a well-formed line whose sequence collides are three different
//! events with three different right answers, and only the first of them is
//! damage. Deleting is reserved for the case where there is provably nothing
//! left to save; see [`parse_entries`].

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::play_recorder::RecordedPlay;

pub(super) const FILE_NAME: &str = "android-play-count.journal";
const FORMAT_VERSION: &str = "v1";
const LOCK_FILE_NAME: &str = ".android-play-count.journal.lock";
/// Named beyond this module so a test can occupy the path and make every
/// rewrite fail, rather than hard-coding the name and drifting from it.
pub(super) const TEMP_FILE_NAME: &str = ".android-play-count.journal.tmp";

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

#[derive(Debug)]
pub(super) struct PlayJournal {
    path: PathBuf,
    /// The exclusive claim on this journal, held for as long as the journal is.
    /// Never read; dropping it — or the process dying — is what releases it.
    _lock: File,
    entries: VecDeque<JournalEntry>,
    /// Lines a failed [`Self::remove_front`] left in the file that are no
    /// longer in `entries`. They are harmless to replay, but they are still
    /// lines, so they keep occupying slots until a later rewrite clears them.
    stale_lines: usize,
    next_sequence: i64,
}

impl PlayJournal {
    pub(super) fn open(database_path: &Path, applied_sequence: i64) -> io::Result<Self> {
        let directory = database_path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "database path has no parent")
        })?;
        let lock = claim(&directory.join(LOCK_FILE_NAME))?;
        let path = directory.join(FILE_NAME);
        let entries = if path.exists() {
            let (entries, needs_repair) = parse_entries(&fs::read(&path)?, applied_sequence)?;
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
            _lock: lock,
            entries,
            stale_lines: 0,
            next_sequence,
        })
    }

    /// Returns `false` only when the bounded journal explicitly refuses the
    /// newest play. I/O failures remain errors because no durable promise was
    /// made in that case either.
    pub(super) fn append(&mut self, play: RecordedPlay) -> io::Result<bool> {
        let pending = self.entries.len() + self.stale_lines;
        if pending >= MAX_PENDING_PLAYS {
            tracing::warn!(
                track_id = play.track_id,
                pending,
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
        file.sync_data()?;
        sync_directory_of(&self.path)?;
        self.entries.push_back(entry);
        self.next_sequence = following_sequence;
        Ok(true)
    }

    pub(super) fn front(&self) -> Option<JournalEntry> {
        self.entries.front().copied()
    }

    /// Drops the oldest entry, whose play Core has already counted.
    ///
    /// It leaves the queue whether or not the file can be rewritten. A rewrite
    /// that keeps failing — a full disk, say — would otherwise park every later
    /// play behind an entry that is already counted and can never be counted
    /// again, which is a worse answer than a stale line: the line's sequence is
    /// at or below the applied mark, so the next drain recognises it instead of
    /// counting it twice, and the next rewrite that does succeed clears every
    /// one of them at once. Until then it still costs a slot, so the caller's
    /// cap keeps counting it.
    pub(super) fn remove_front(&mut self) -> io::Result<()> {
        let rewritten = rewrite(&self.path, self.entries.iter().copied().skip(1));
        self.entries.pop_front();
        match rewritten {
            Ok(()) => {
                self.stale_lines = 0;
                Ok(())
            }
            Err(error) => {
                self.stale_lines += 1;
                Err(error)
            }
        }
    }
}

/// Takes the journal's exclusive advisory lock, or says who has it.
///
/// A refusal is a real error, not a silent fallback to sharing: the second
/// writer's first rewrite would write its own view of the queue over entries
/// the first writer appended and it never saw.
///
/// It can afford to be an immediate refusal rather than a wait, because a
/// session's own restart cannot collide with itself. `ReprisePlaybackService`
/// builds exactly one session, in `onCreate`, and releases it in `onDestroy`
/// via `close()` — which drops `PlayRecorder`, which joins the writer thread,
/// which is what drops this lock. Waiting would only ever help a *second*
/// writer, and a second writer is the thing being refused.
fn claim(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(TryLockError::WouldBlock) => Err(io::Error::new(
            io::ErrorKind::ResourceBusy,
            "another writer already holds the Android play journal",
        )),
        Err(TryLockError::Error(error)) => Err(error),
    }
}

/// Reads the file into the pending queue, reporting whether it has to be
/// written back.
///
/// Each way a line can fail to be what was expected gets its own answer, and
/// only one of them deletes:
///
/// * **Unreadable** — the bytes are not a record at all. There is nothing to
///   save, so it goes, and so does a half-written final line.
/// * **A version this build does not know** — the line reads as a record in a
///   dialect that arrived from somewhere newer. Rewriting the file would drop
///   it, so nothing is rewritten: the whole journal is refused instead, and the
///   file is left exactly as found.
/// * **A colliding sequence** — the line reads perfectly and describes a play
///   that happened; two overlapping writers merely numbered their plays alike.
///   Nothing about it is damage, so it is renumbered rather than deleted.
///
/// Renumbering has to land *above* the applied mark as well as above the
/// previous entry: a number at or below the mark is one Core has already
/// committed, and the rescued play would be swallowed as a replay of it.
fn parse_entries(
    contents: &[u8],
    applied_sequence: i64,
) -> io::Result<(VecDeque<JournalEntry>, bool)> {
    let mut entries: VecDeque<JournalEntry> = VecDeque::new();
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
        let entry = match read_entry(bytes) {
            Ok(entry) => entry,
            Err(UnreadableLine::Damaged) => {
                tracing::warn!(
                    line = line_number,
                    "discarded an unreadable Android play-journal entry",
                );
                needs_repair = true;
                continue;
            }
            Err(UnreadableLine::ForeignVersion(version)) => {
                tracing::warn!(
                    line = line_number,
                    %version,
                    "refused an Android play journal written in an unknown format",
                );
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    format!("unknown Android play-journal format version {version:?}"),
                ));
            }
        };
        let last_kept = entries.back().map_or(0, |previous| previous.sequence);
        let entry = if entry.sequence <= last_kept {
            let renumbered = last_kept
                .max(applied_sequence)
                .checked_add(1)
                .ok_or_else(|| io::Error::other("Android play-journal sequence exhausted"))?;
            tracing::warn!(
                line = line_number,
                sequence = entry.sequence,
                renumbered,
                track_id = entry.play.track_id,
                "renumbered a colliding Android play-journal entry",
            );
            needs_repair = true;
            JournalEntry {
                sequence: renumbered,
                ..entry
            }
        } else {
            entry
        };
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
    Ok((entries, needs_repair))
}

/// Why a line did not become an entry.
enum UnreadableLine {
    /// The bytes are not a record in any dialect: wrong shape, wrong field
    /// count, unparsable numbers, or a sequence outside the format's range.
    Damaged,
    /// The bytes *are* a record, in a format version this build cannot
    /// interpret. Carries the version so the refusal can name it.
    ForeignVersion(String),
}

/// Reads one line, keeping "not a record" apart from "a record I do not speak".
///
/// The shape is checked first and the version last on purpose: only a line that
/// is otherwise exactly a record counts as a foreign dialect, so random damage
/// that happens to start with an unfamiliar tag cannot wedge the journal.
fn read_entry(bytes: &[u8]) -> Result<JournalEntry, UnreadableLine> {
    let line = std::str::from_utf8(bytes).map_err(|_| UnreadableLine::Damaged)?;
    let mut fields = line.split('\t');
    let Some(version) = fields.next() else {
        return Err(UnreadableLine::Damaged);
    };
    let sequence = parse_number(fields.next())?;
    let track_id = parse_number(fields.next())?;
    let at_unix = parse_number(fields.next())?;
    if fields.next().is_some() || sequence <= 0 {
        return Err(UnreadableLine::Damaged);
    }
    if version != FORMAT_VERSION {
        return Err(UnreadableLine::ForeignVersion(version.to_owned()));
    }
    Ok(JournalEntry {
        sequence,
        play: RecordedPlay { track_id, at_unix },
    })
}

fn parse_number(value: Option<&str>) -> Result<i64, UnreadableLine> {
    value
        .ok_or(UnreadableLine::Damaged)?
        .parse()
        .map_err(|_| UnreadableLine::Damaged)
}

fn rewrite(path: &Path, entries: impl Iterator<Item = JournalEntry>) -> io::Result<()> {
    let temporary = path.with_file_name(TEMP_FILE_NAME);
    let mut file = fs::File::create(&temporary)?;
    for entry in entries {
        write_entry(&mut file, entry)?;
    }
    file.sync_data()?;
    drop(file);
    fs::rename(temporary, path)?;
    sync_directory_of(path)
}

/// Makes a name durable, not just the bytes behind it.
///
/// `sync_data` covers a file's contents; the directory entry that makes those
/// contents findable — a fresh journal's first append, or the rename that
/// replaces it — is a separate write with its own crash window. Skipping this
/// is what would reduce the promise to "survives the process, not the battery",
/// and on a phone the battery is the ordinary case.
fn sync_directory_of(file_path: &Path) -> io::Result<()> {
    let directory = file_path.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "Android play-journal path has no parent",
        )
    })?;
    File::open(directory)?.sync_all()
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
    use std::io::ErrorKind;

    use super::{PlayJournal, MAX_PENDING_PLAYS};
    use crate::log_capture::{CapturedLogs, LogCapture};
    use crate::play_recorder::RecordedPlay;

    fn fill_to_the_cap(journal: &mut PlayJournal) {
        for track_id in 1..=MAX_PENDING_PLAYS as i64 {
            assert!(journal
                .append(RecordedPlay {
                    track_id,
                    at_unix: 1_700_000_000,
                })
                .unwrap());
        }
    }

    /// Two writers over one journal each hold their own view of the pending
    /// queue, and the first rewrite from either one writes its view over the
    /// other's entries. The second writer has to be refused, loudly, rather
    /// than allowed to destroy counted plays it cannot see.
    #[test]
    fn a_second_journal_over_the_same_library_is_refused_rather_than_sharing() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("reprise.db");
        let first = PlayJournal::open(&database_path, 0).unwrap();

        let error = PlayJournal::open(&database_path, 0).unwrap_err();

        assert_eq!(
            error.kind(),
            ErrorKind::ResourceBusy,
            "a journal another writer owns must fail loudly, got {error}",
        );
        drop(first);
        PlayJournal::open(&database_path, 0)
            .expect("the lock must be released with the journal that held it");
    }

    /// A colliding sequence is not a damaged line: both entries describe a play
    /// that happened. Draining them in file order would let the first move the
    /// applied mark past the second, so the second is renumbered above
    /// everything already committed — never below it, where it would be
    /// mistaken for an already-applied replay.
    #[test]
    fn a_colliding_entry_is_renumbered_above_the_applied_mark_rather_than_discarded() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("reprise.db");
        let journal_path = directory.path().join(super::FILE_NAME);
        std::fs::write(
            &journal_path,
            "v1\t3\t11\t1700000000\nv1\t3\t22\t1700000001\n",
        )
        .unwrap();

        let logs = CapturedLogs::default();
        let mut journal = tracing::subscriber::with_default(LogCapture(logs.clone()), || {
            PlayJournal::open(&database_path, 7).unwrap()
        });

        assert_eq!(
            std::fs::read_to_string(&journal_path).unwrap(),
            "v1\t3\t11\t1700000000\nv1\t8\t22\t1700000001\n",
            "the rescued entry must be renumbered past the applied mark on disk",
        );
        let mut queued = Vec::new();
        while let Some(entry) = journal.front() {
            queued.push((entry.sequence, entry.play.track_id));
            journal.remove_front().unwrap();
        }
        assert_eq!(queued, vec![(3, 11), (8, 22)]);
        let logged = logs.joined();
        assert!(
            logged.contains("renumbered a colliding Android play-journal entry"),
            "a collision must be named as one, got {logged}",
        );
        assert!(
            !logged.contains("unreadable"),
            "a collision must not be reported as file damage, got {logged}",
        );
    }

    /// A line this build cannot interpret is not a line this build may delete.
    /// Rewriting around records of an unknown dialect would drop them, so the
    /// journal steps back from the file entirely.
    #[test]
    fn an_unknown_format_version_refuses_the_journal_instead_of_deleting_the_line() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("reprise.db");
        let journal_path = directory.path().join(super::FILE_NAME);
        let written = "v1\t1\t11\t1700000000\nv2\t2\t22\t1700000001\n";
        std::fs::write(&journal_path, written).unwrap();

        let logs = CapturedLogs::default();
        let error = tracing::subscriber::with_default(LogCapture(logs.clone()), || {
            PlayJournal::open(&database_path, 0).unwrap_err()
        });

        assert_eq!(
            error.kind(),
            ErrorKind::Unsupported,
            "an unknown dialect is unsupported, not corrupt, got {error}",
        );
        assert_eq!(
            std::fs::read_to_string(&journal_path).unwrap(),
            written,
            "a version this build cannot read must survive it untouched",
        );
        let logged = logs.joined();
        assert!(
            logged.contains("unknown format"),
            "the refusal must name the reason, got {logged}",
        );
    }

    /// A rewrite that cannot happen must not park later plays behind an entry
    /// that is already counted — but the line it leaves behind is still a line
    /// in the file, so it keeps occupying one of the bounded slots until some
    /// later rewrite clears them all.
    #[test]
    fn a_line_left_by_a_failed_removal_still_counts_against_the_cap() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("reprise.db");
        let journal_path = directory.path().join(super::FILE_NAME);
        let mut journal = PlayJournal::open(&database_path, 0).unwrap();
        fill_to_the_cap(&mut journal);
        std::fs::create_dir(directory.path().join(super::TEMP_FILE_NAME)).unwrap();

        journal
            .remove_front()
            .expect_err("the fixture must make every rewrite fail");

        assert!(journal.front().is_some());
        assert!(
            !journal
                .append(RecordedPlay {
                    track_id: 9_999,
                    at_unix: 1_700_000_002,
                })
                .unwrap(),
            "a slot the file has not actually given back is not a free slot",
        );
        assert_eq!(
            std::fs::read_to_string(&journal_path)
                .unwrap()
                .lines()
                .count(),
            MAX_PENDING_PLAYS,
            "the journal file must never grow past its cap",
        );
    }

    #[test]
    fn a_full_journal_refuses_and_names_the_newest_play() {
        let directory = tempfile::tempdir().unwrap();
        let database_path = directory.path().join("reprise.db");
        let mut journal = PlayJournal::open(&database_path, 0).unwrap();
        fill_to_the_cap(&mut journal);

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
