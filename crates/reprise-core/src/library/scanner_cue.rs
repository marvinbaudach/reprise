//! Finds the CUE sheet that sits beside an audio file and says which of the
//! sheet's tracks belong to that file.
//!
//! The walk delivers a directory's entries in no useful order, so a sheet can
//! arrive after the audio it describes. Nothing here relies on the walk: before
//! a batch is classified, the directories of its audio files are listed once and
//! their sheets read, and every later file in a directory reuses the answer for
//! the rest of the scan.
//!
//! All source I/O happens in those two steps and in [`CueDirectories::ensure_parsed`],
//! never while the scan holds the database writer. The classification that
//! follows only looks things up.
//!
//! A sheet the scan cannot see is not a sheet that is gone. When a directory
//! cannot be listed, or a sheet in it cannot be probed or read, every audio file
//! there is [`Cover::Unknown`] for the rest of the scan, and the catalog keeps
//! what it has for them: deleting the tracks a sheet cut, with their ratings
//! and playlists, takes proof that the sheet changed.
//!
//! A sheet an earlier scan already applied, and that has not changed since, is
//! recognised from the catalog (`cue_path`, `cue_mtime` and `cue_size` on its tracks, loaded
//! once when the scan starts) and not read again. Only a new, changed or broken
//! sheet is read and parsed up front.

use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use rusqlite::Transaction;

use crate::cue::{self, CueError, CueFile, CueSheet};
use crate::library::import_errors;
use crate::library::source::{
    LibraryDirectoryEntry, LibraryLinkMode, LibraryPathPresence, LibrarySource,
};
use crate::models::ImportErrorKind;

use super::{is_audio_file, now_unix, scanner_file_metadata, ScanError};

/// The sheet that governs an audio file, as far as the catalog needs to know
/// to decide whether the file is up to date.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct SheetRef {
    pub(super) path: PathBuf,
    pub(super) mtime: i64,
    pub(super) size: i64,
}

impl SheetRef {
    pub(super) fn path_text(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

/// A sheet read and resolved against the audio files of its directory.
struct ParsedSheet {
    sheet: CueSheet,
    /// For each `FILE` block that holds audio tracks, the file it resolves to.
    resolved: Vec<(usize, PathBuf)>,
}

impl ParsedSheet {
    fn covers(&self, audio: &Path) -> bool {
        self.resolved.iter().any(|(_, path)| path == audio)
    }

    /// The sheet reduced to the `FILE` blocks that describe `audio`.
    fn sub_sheet(&self, audio: &Path) -> CueSheet {
        let files: Vec<CueFile> = self
            .resolved
            .iter()
            .filter(|(_, path)| path == audio)
            .map(|(index, _)| self.sheet.files[*index].clone())
            .collect();
        CueSheet {
            files,
            ..self.sheet.clone()
        }
    }
}

/// What the scan knows about a sheet it found.
enum SheetState {
    /// An earlier scan applied this very version of the sheet to these files.
    /// Its text has not been read, and is read only if one of them changed.
    Settled(HashSet<PathBuf>),
    Parsed(Rc<ParsedSheet>),
    /// The sheet cannot be applied; the text says why.
    Invalid(String),
    /// The sheet could not be probed or read, so nothing is known about it.
    Unknown,
}

/// Which sheet governs an audio file, as far as the scan can tell.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Cover {
    /// No sheet beside the file describes it.
    Plain,
    Sheet(SheetRef),
    /// The directory or one of its sheets could not be seen.
    Unknown,
}

/// What a governing sheet says about one of its files.
pub(super) enum SheetFit {
    /// The part of the sheet that describes the file.
    Part(CueSheet),
    /// The sheet cannot be applied to the file after all; its issue is recorded.
    Unfit,
    /// The sheet could not be read.
    Unknown,
}

struct DirectorySheet {
    reference: SheetRef,
    state: SheetState,
    /// Whether the catalog has heard about this state: an issue for an invalid
    /// sheet, the end of one for a sheet that is valid again.
    reported: bool,
}

#[derive(Default)]
struct DirectoryCues {
    audio: Vec<PathBuf>,
    sheets: Vec<DirectorySheet>,
    /// The directory could not be listed.
    unlisted: bool,
}

impl DirectoryCues {
    fn is_unknown(&self) -> bool {
        self.unlisted
            || self
                .sheets
                .iter()
                .any(|sheet| matches!(sheet.state, SheetState::Unknown))
    }
}

/// What the catalog says an earlier scan applied: the sheet's mtime and size
/// then, and the files it cut.
struct AppliedSheet {
    version: Option<(i64, i64)>,
    files: HashSet<PathBuf>,
}

/// The sheets of every directory this scan has met, listed once each.
#[derive(Default)]
pub(super) struct CueDirectories {
    /// Sheet path to the version it was last applied at and the files it was applied to.
    applied: HashMap<String, AppliedSheet>,
    directories: HashMap<PathBuf, DirectoryCues>,
}

impl CueDirectories {
    /// Loads which sheets earlier scans applied to which files under `root`.
    pub(super) fn load_applied(
        &mut self,
        conn: &rusqlite::Connection,
        root: &Path,
    ) -> Result<(), rusqlite::Error> {
        let pattern = format!(
            "{}/%",
            crate::library::playlists::escape_like(root.to_string_lossy().trim_end_matches('/'))
        );
        let mut statement = conn.prepare(
            "SELECT DISTINCT cue_path, cue_mtime, cue_size, path FROM tracks \
             WHERE cue_path IS NOT NULL AND cue_mtime IS NOT NULL AND cue_size IS NOT NULL \
               AND path LIKE ?1 ESCAPE '\\'",
        )?;
        let rows = statement.query_map([pattern], |row| {
            Ok((
                row.get::<_, String>(0)?,
                (row.get::<_, i64>(1)?, row.get::<_, i64>(2)?),
                row.get::<_, String>(3)?,
            ))
        })?;
        self.applied.clear();
        for row in rows {
            let (sheet, version, path) = row?;
            let entry = self.applied.entry(sheet).or_insert(AppliedSheet {
                version: Some(version),
                files: HashSet::new(),
            });
            // Rows of one sheet that disagree on its version were not all applied
            // under the current text; none of them settles it.
            if entry.version == Some(version) {
                entry.files.insert(PathBuf::from(path));
            } else {
                entry.version = None;
            }
        }
        Ok(())
    }

    /// Lists `directory` and reads whichever of its sheets need reading. Done
    /// once per directory and scan, before any of its files is classified.
    pub(super) fn discover(&mut self, source: &dyn LibrarySource, directory: &Path) {
        if self.directories.contains_key(directory) {
            return;
        }
        let cues = list_directory(source, &self.applied, directory);
        self.directories.insert(directory.to_path_buf(), cues);
    }

    /// The sheet that governs `audio`. The first sheet by path wins when two
    /// describe the same file. Records, on the way, what the scan has learned
    /// about the sheets of its directory.
    pub(super) fn covering(
        &mut self,
        source: &dyn LibrarySource,
        tx: &Transaction<'_>,
        audio: &Path,
    ) -> Result<Cover, ScanError> {
        let Some(cues) = self.directory_of(source, audio) else {
            return Ok(Cover::Plain);
        };
        report_unreported(tx, cues)?;
        if cues.is_unknown() {
            return Ok(Cover::Unknown);
        }
        Ok(cues
            .sheets
            .iter()
            .find(|sheet| sheet_covers(sheet, audio))
            .map_or(Cover::Plain, |sheet| Cover::Sheet(sheet.reference.clone())))
    }

    /// Reads a sheet that was only recognised from the catalog, because one of
    /// the files it governs changed and so needs the sheet's text.
    pub(super) fn ensure_parsed(
        &mut self,
        source: &dyn LibrarySource,
        sheet: &SheetRef,
        audio: &Path,
    ) {
        let Some(directory) = source.parent_of(audio) else {
            return;
        };
        let Some(cues) = self.directories.get_mut(&directory) else {
            return;
        };
        let audio_files = cues.audio.clone();
        let Some(entry) = cues
            .sheets
            .iter_mut()
            .find(|candidate| candidate.reference == *sheet)
        else {
            return;
        };
        if matches!(entry.state, SheetState::Settled(_)) {
            entry.state = read_state(source, &entry.reference, &audio_files);
            entry.reported = false;
        }
    }

    /// What `sheet` says about `audio`. It can turn out unfit after all, for
    /// instance because a file it names has since been deleted; the issue is
    /// recorded by the time this returns.
    pub(super) fn sub_sheet(
        &mut self,
        source: &dyn LibrarySource,
        tx: &Transaction<'_>,
        sheet: &SheetRef,
        audio: &Path,
    ) -> Result<SheetFit, ScanError> {
        let Some(cues) = self.directory_of(source, audio) else {
            return Ok(SheetFit::Unknown);
        };
        report_unreported(tx, cues)?;
        if cues.is_unknown() {
            return Ok(SheetFit::Unknown);
        }
        let found = cues.sheets.iter().find(|entry| entry.reference == *sheet);
        Ok(match found.map(|entry| &entry.state) {
            Some(SheetState::Parsed(parsed)) if parsed.covers(audio) => {
                SheetFit::Part(parsed.sub_sheet(audio))
            }
            _ => SheetFit::Unfit,
        })
    }

    fn directory_of(
        &mut self,
        source: &dyn LibrarySource,
        audio: &Path,
    ) -> Option<&mut DirectoryCues> {
        let directory = source.parent_of(audio)?;
        self.directories.get_mut(&directory)
    }
}

fn sheet_covers(sheet: &DirectorySheet, audio: &Path) -> bool {
    match &sheet.state {
        SheetState::Settled(files) => files.contains(audio),
        SheetState::Parsed(parsed) => parsed.covers(audio),
        SheetState::Invalid(_) | SheetState::Unknown => false,
    }
}

fn list_directory(
    source: &dyn LibrarySource,
    applied: &HashMap<String, AppliedSheet>,
    directory: &Path,
) -> DirectoryCues {
    let Some(entries) = source.read_directory(directory) else {
        tracing::warn!(directory = %directory.display(), "directory cannot be listed; its CUE sheets are unknown this scan");
        return DirectoryCues {
            unlisted: true,
            ..DirectoryCues::default()
        };
    };
    let mut audio: Vec<PathBuf> = entries
        .iter()
        .filter(|entry| is_audio_file(&entry.path))
        .map(|entry| entry.path.clone())
        .collect();
    audio.sort();
    let mut sheets: Vec<DirectorySheet> = entries
        .iter()
        .filter(|entry| is_sheet_file(entry))
        .filter_map(|entry| directory_sheet(source, applied, &audio, entry))
        .collect();
    sheets.sort_by(|left, right| left.reference.path.cmp(&right.reference.path));
    DirectoryCues {
        audio,
        sheets,
        unlisted: false,
    }
}

fn is_sheet_file(entry: &LibraryDirectoryEntry) -> bool {
    let is_cue = entry
        .path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("cue"));
    is_cue
        && entry
            .metadata
            .as_ref()
            .is_none_or(|metadata| metadata.is_file)
}

fn directory_sheet(
    source: &dyn LibrarySource,
    applied: &HashMap<String, AppliedSheet>,
    audio: &[PathBuf],
    entry: &LibraryDirectoryEntry,
) -> Option<DirectorySheet> {
    let metadata = match entry.metadata.clone() {
        Some(metadata) => metadata,
        None => match source.probe(&entry.path, LibraryLinkMode::Follow) {
            LibraryPathPresence::Present(metadata) => metadata,
            LibraryPathPresence::Absent => return None,
            LibraryPathPresence::Unknown => {
                return Some(DirectorySheet {
                    reference: SheetRef {
                        path: entry.path.clone(),
                        mtime: 0,
                        size: 0,
                    },
                    state: SheetState::Unknown,
                    reported: false,
                });
            }
        },
    };
    let (mtime, stat) = scanner_file_metadata(Some(metadata));
    let reference = SheetRef {
        path: entry.path.clone(),
        mtime,
        size: stat.map_or(0, |(size, _)| size as i64),
    };
    let state = match applied.get(&reference.path_text()) {
        Some(applied)
            if applied.version == Some((reference.mtime, reference.size))
                && !applied.files.is_empty() =>
        {
            SheetState::Settled(applied.files.clone())
        }
        _ => read_state(source, &reference, audio),
    };
    Some(DirectorySheet {
        reference,
        state,
        reported: false,
    })
}

fn read_state(source: &dyn LibrarySource, reference: &SheetRef, audio: &[PathBuf]) -> SheetState {
    let bytes = match read_sheet(source, &reference.path) {
        Ok(bytes) => bytes,
        Err(error) => {
            tracing::warn!(sheet = %reference.path.display(), %error, "CUE sheet cannot be read this scan");
            return SheetState::Unknown;
        }
    };
    match resolve(&bytes, reference, audio) {
        Ok(parsed) => SheetState::Parsed(Rc::new(parsed)),
        Err(reason) => SheetState::Invalid(reason),
    }
}

/// Tells the catalog what the scan found out about the sheets of a directory:
/// an issue for one that cannot be applied, and the end of an old issue for one
/// that can. Once per sheet and state.
fn report_unreported(tx: &Transaction<'_>, cues: &mut DirectoryCues) -> Result<(), ScanError> {
    for sheet in cues.sheets.iter_mut().filter(|sheet| !sheet.reported) {
        sheet.reported = true;
        match &sheet.state {
            SheetState::Invalid(reason) => report_invalid(tx, sheet, reason)?,
            SheetState::Parsed(_) => {
                import_errors::clear_error_unless_dismissed(
                    tx,
                    &sheet.reference.path_text(),
                    sheet.reference.mtime,
                    sheet.reference.size,
                )?;
            }
            SheetState::Settled(_) | SheetState::Unknown => {}
        }
    }
    Ok(())
}

fn resolve(bytes: &[u8], reference: &SheetRef, audio: &[PathBuf]) -> Result<ParsedSheet, String> {
    let parsed = cue::parse(bytes).map_err(|error| error.to_string())?;
    let directory = reference.path.parent().unwrap_or(Path::new(""));
    let mut resolved: Vec<(usize, PathBuf)> = Vec::new();
    for (index, file) in parsed.files.iter().enumerate() {
        if !file.tracks.iter().any(|track| track.is_audio) {
            continue;
        }
        let path = cue::resolve_file(directory, &file.name, audio).ok_or_else(|| {
            CueError::MissingDuration {
                file: file.name.clone(),
            }
            .to_string()
        })?;
        if resolved.iter().any(|(_, seen)| *seen == path) {
            return Err(CueError::DuplicateFile { path }.to_string());
        }
        resolved.push((index, path));
    }
    if resolved.is_empty() {
        return Err(CueError::NoAudioTracks.to_string());
    }
    Ok(ParsedSheet {
        sheet: parsed,
        resolved,
    })
}

fn read_sheet(source: &dyn LibrarySource, path: &Path) -> std::io::Result<Vec<u8>> {
    // One byte past the cap is enough for `cue::parse` to refuse the sheet.
    let mut bytes = Vec::new();
    source
        .open_read(path)?
        .take(cue::MAX_SHEET_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// Records that `sheet` was not applied, unless the user already dismissed this
/// very version of it.
fn report_invalid(
    tx: &Transaction<'_>,
    sheet: &DirectorySheet,
    reason: &str,
) -> Result<(), ScanError> {
    let path = sheet.reference.path_text();
    let now = now_unix();
    if import_errors::check_dismissed(tx, &path, sheet.reference.mtime, sheet.reference.size, now)?
    {
        return Ok(());
    }
    tracing::warn!(sheet = %path, %reason, "CUE sheet ignored; its audio stays whole");
    import_errors::record_error(tx, &path, ImportErrorKind::InvalidCueSheet, reason, now)?;
    Ok(())
}

/// Records that a sheet that did parse still cannot be applied to its file,
/// for instance because a track starts past the end of the audio, unless the
/// user already dismissed this very version of it.
pub(super) fn report_rejected(
    tx: &Transaction<'_>,
    sheet: &SheetRef,
    reason: &str,
) -> Result<(), ScanError> {
    let path = sheet.path_text();
    let now = now_unix();
    if import_errors::check_dismissed(tx, &path, sheet.mtime, sheet.size, now)? {
        return Ok(());
    }
    tracing::warn!(sheet = %path, %reason, "CUE sheet rejected; its audio stays whole");
    import_errors::record_error(tx, &path, ImportErrorKind::InvalidCueSheet, reason, now)?;
    Ok(())
}
