//! The shape one audio file takes in the catalog: a single whole-file track, or
//! the tracks of the CUE sheet that describes it.
//!
//! A file is listed either way, never both. Switching from one shape to the
//! other happens inside the entry's transaction, so a reader never sees the
//! file as a track and as its tracks at once.

use std::path::Path;

use crate::cue::{self, CueSegment};
use crate::library::exclusions;
use crate::library::import_errors;
use crate::library::loudness::ReplayGainTags;
use crate::models::ImportErrorKind;

use super::cue_sheets::{self, SheetFit, SheetRef};
use super::entry::{EntryScan, FileFacts, ImportedTrack};
use super::{now_unix, track_meta::TrackMeta, ScanError};

#[path = "scanner_segment_match.rs"]
mod segment_match;

/// How a file is to be written.
pub(super) enum Layout {
    /// One ordinary track. `rejected_by` is the sheet that was found beside the
    /// file and could not be applied; the row remembers it, so the next scan
    /// does not try the same sheet again.
    Whole { rejected_by: Option<SheetRef> },
    /// The tracks of a sheet. `sheet` is `None` for a sheet embedded in the file.
    Segments {
        segments: Vec<CueSegment>,
        sheet: Option<SheetRef>,
    },
}

impl Layout {
    /// A plain whole-file track with no sheet in the picture.
    pub(super) fn is_plain_whole(&self) -> bool {
        matches!(self, Self::Whole { rejected_by: None })
    }
}

/// What the sheets have to say about a file: the layout it takes, and the issue
/// a sheet that could not be applied raises.
pub(super) struct Plan {
    pub(super) layout: Layout,
    pub(super) issue: Option<SheetIssue>,
}

impl Plan {
    fn whole(rejected_by: Option<SheetRef>, issue: Option<SheetIssue>) -> Self {
        Self {
            layout: Layout::Whole { rejected_by },
            issue,
        }
    }
}

/// Why a sheet was not applied, and so where its issue goes.
pub(super) enum SheetIssue {
    /// The sheet beside the file parses but does not fit it; the issue names
    /// the sheet.
    Rejected { sheet: SheetRef, reason: String },
    /// The sheet embedded in the file cannot be applied; the issue names the
    /// file, since an embedded sheet has no path of its own.
    Embedded(String),
}

/// Where a row sits inside its file and which sheet put it there.
pub(super) struct Placement<'a> {
    pub(super) segment_index: i64,
    pub(super) start_ms: Option<i64>,
    pub(super) end_ms: Option<i64>,
    pub(super) sheet: Option<&'a SheetRef>,
}

/// Decides the shape of `path`: the sheet beside it wins, then one embedded in
/// the file, and a sheet that does not fit the audio leaves the file whole.
/// `None` when the sheet beside the file could not be read: then nothing is
/// known about the file's shape, and the caller must leave its rows alone.
///
/// Writes nothing about the file itself, so the caller can still back out.
pub(super) fn plan_layout(
    scan: &mut EntryScan<'_, '_, '_>,
    path: &Path,
    governing: Option<&SheetRef>,
    meta: &TrackMeta,
) -> Result<Option<Plan>, ScanError> {
    let own_file = |_: &cue::CueFile| Some((path.to_path_buf(), meta.duration_ms));
    if let Some(sheet) = governing {
        let sub_sheet = match scan.cues.sub_sheet(scan.source, scan.tx, sheet, path)? {
            SheetFit::Part(sub_sheet) => sub_sheet,
            SheetFit::Unfit => return Ok(Some(Plan::whole(Some(sheet.clone()), None))),
            SheetFit::Unknown => return Ok(None),
        };
        return Ok(Some(match cue::segments(&sub_sheet, own_file) {
            Ok(segments) => Plan {
                layout: Layout::Segments {
                    segments,
                    sheet: Some(sheet.clone()),
                },
                issue: None,
            },
            Err(error) => Plan::whole(
                Some(sheet.clone()),
                Some(SheetIssue::Rejected {
                    sheet: sheet.clone(),
                    reason: error.to_string(),
                }),
            ),
        }));
    }
    let Some(text) = &meta.embedded_cuesheet else {
        return Ok(Some(Plan::whole(None, None)));
    };
    Ok(Some(
        match cue::parse(text.as_bytes()).and_then(|sheet| cue::segments(&sheet, own_file)) {
            Ok(segments) => Plan {
                layout: Layout::Segments {
                    segments,
                    sheet: None,
                },
                issue: None,
            },
            Err(error) => Plan::whole(None, Some(SheetIssue::Embedded(error.to_string()))),
        },
    ))
}

/// Records the issue a plan raised, unless the user dismissed this very
/// version of what it names: the sheet beside the file, or the file itself for
/// a sheet embedded in it.
pub(super) fn report_issue(
    scan: &EntryScan<'_, '_, '_>,
    path_str: &str,
    facts: &FileFacts,
    issue: &SheetIssue,
) -> Result<(), ScanError> {
    match issue {
        SheetIssue::Rejected { sheet, reason } => {
            cue_sheets::report_rejected(scan.tx, sheet, reason)
        }
        SheetIssue::Embedded(reason) => {
            let now = now_unix();
            if import_errors::check_dismissed(scan.tx, path_str, facts.mtime, facts.file_size, now)?
            {
                return Ok(());
            }
            tracing::warn!(path = %path_str, %reason, "embedded CUE sheet ignored");
            import_errors::record_error(
                scan.tx,
                path_str,
                ImportErrorKind::InvalidCueSheet,
                reason,
                now,
            )?;
            Ok(())
        }
    }
}

/// Writes `layout` and returns how many tracks the file holds afterwards.
pub(super) fn write_layout(
    scan: &EntryScan<'_, '_, '_>,
    path: &Path,
    path_str: &str,
    facts: &FileFacts,
    imported: &ImportedTrack<'_>,
    layout: &Layout,
) -> Result<u32, ScanError> {
    match layout {
        // The new rows go in before the old ones come out. A row id is the highest
        // free one, so the replacement gets an id no row of the shape it replaces
        // ever had, and nothing that kept one of those ids can meet it again.
        Layout::Whole { rejected_by } => {
            let placement = Placement {
                segment_index: 0,
                start_ms: None,
                end_ms: None,
                sheet: rejected_by.as_ref(),
            };
            super::entry::upsert_track(scan, path_str, facts, imported, &placement)?;
            scan.tx.execute(
                "DELETE FROM tracks WHERE path = ?1 AND segment_index > 0",
                [path_str],
            )?;
            Ok(1)
        }
        Layout::Segments { segments, sheet } => {
            let kept = write_segments(scan, path, path_str, facts, imported, segments, sheet)?;
            scan.tx.execute(
                "DELETE FROM tracks WHERE path = ?1 AND segment_index = 0",
                [path_str],
            )?;
            remove_unkept_segments(scan, path_str)?;
            Ok(u32::try_from(kept).unwrap_or(u32::MAX))
        }
    }
}

/// Writes the tracks of `segments` that are not excluded, each on the row of the
/// same song where the file already has one (see [`segment_match`]), and
/// returns how many it wrote. The rows no track kept are left parked at a
/// negative position for [`remove_unkept_segments`].
fn write_segments(
    scan: &EntryScan<'_, '_, '_>,
    path: &Path,
    path_str: &str,
    facts: &FileFacts,
    imported: &ImportedTrack<'_>,
    segments: &[CueSegment],
    sheet: &Option<SheetRef>,
) -> Result<usize, ScanError> {
    let hidden = hide_excluded(scan, path, facts, segments, sheet)?;
    let included: Vec<&CueSegment> = segments
        .iter()
        .zip(hidden)
        .filter_map(|(segment, hidden)| (!hidden).then_some(segment))
        .collect();
    let titles: Vec<String> = included
        .iter()
        .map(|segment| segment_title(segment))
        .collect();
    let wanted: Vec<segment_match::WantedSegment<'_>> = included
        .iter()
        .zip(&titles)
        .map(|(segment, title)| segment_match::WantedSegment {
            index: segment.segment_index,
            start_ms: segment.start_ms,
            title,
        })
        .collect();
    let keeps = segment_match::match_rows(&known_segments(scan, path_str)?, &wanted);
    // Every row of the file steps aside to a position no track has, so each
    // track can take its row whatever position that row held before.
    scan.tx.execute(
        "UPDATE tracks SET segment_index = -id WHERE path = ?1 AND segment_index > 0",
        [path_str],
    )?;
    for ((segment, title), keep) in included.iter().zip(&titles).zip(keeps) {
        if let Some(id) = keep {
            scan.tx.execute(
                "UPDATE tracks SET segment_index = ?2 WHERE id = ?1",
                rusqlite::params![id, segment.segment_index],
            )?;
        }
        let meta = segment_meta(imported.meta, segment);
        let track = ImportedTrack {
            title,
            meta: &meta,
            untagged: imported.untagged,
            mount_point: imported.mount_point.clone(),
        };
        let placement = Placement {
            segment_index: segment.segment_index,
            start_ms: Some(segment.start_ms),
            end_ms: Some(segment.end_ms),
            sheet: sheet.as_ref(),
        };
        super::entry::upsert_track(scan, path_str, facts, &track, &placement)?;
    }
    Ok(included.len())
}

/// Which of `segments` the user removed from the library, matched to their
/// exclusions as tracks are matched to rows (see [`segment_match`]), so a song
/// stays hidden when a sheet edit moves it. Each matched exclusion then takes
/// its song's current position, start, title and sheet; one the sheet no longer
/// has is parked at a position no track has and matches by start and title only.
fn hide_excluded(
    scan: &EntryScan<'_, '_, '_>,
    path: &Path,
    facts: &FileFacts,
    segments: &[CueSegment],
    sheet: &Option<SheetRef>,
) -> Result<Vec<bool>, ScanError> {
    let path_str = path.to_string_lossy();
    let excluded = exclusions::segment_exclusions(scan.tx, &path_str, facts.device, facts.inode)?;
    if excluded.is_empty() {
        return Ok(vec![false; segments.len()]);
    }
    let known: Vec<segment_match::KnownSegment> = excluded
        .iter()
        .map(|exclusion| segment_match::KnownSegment {
            id: exclusion.id,
            index: exclusion.index,
            start_ms: exclusion.start_ms,
            title: exclusion.title.clone().unwrap_or_default(),
        })
        .collect();
    let titles: Vec<String> = segments.iter().map(segment_title).collect();
    let wanted: Vec<segment_match::WantedSegment<'_>> = segments
        .iter()
        .zip(&titles)
        .map(|(segment, title)| segment_match::WantedSegment {
            index: segment.segment_index,
            start_ms: segment.start_ms,
            title,
        })
        .collect();
    let matched = segment_match::match_rows(&known, &wanted);
    let ids: Vec<i64> = excluded.iter().map(|exclusion| exclusion.id).collect();
    exclusions::park_segment_exclusions(scan.tx, &ids)?;
    let sheet_text = sheet.as_ref().map(SheetRef::path_text);
    for ((segment, title), id) in segments.iter().zip(&titles).zip(&matched) {
        let Some(id) = id else { continue };
        let placement = exclusions::SegmentPlacement {
            index: segment.segment_index,
            start_ms: segment.start_ms,
            title,
            sheet: sheet
                .as_ref()
                .zip(sheet_text.as_deref())
                .map(|(sheet, path)| (path, sheet.mtime, sheet.size)),
            file_mtime: facts.mtime,
            file_size: facts.file_size,
        };
        exclusions::place_segment_exclusion(scan.tx, *id, &placement)?;
    }
    Ok(matched.iter().map(Option::is_some).collect())
}

/// The tracks the file holds now.
fn known_segments(
    scan: &EntryScan<'_, '_, '_>,
    path_str: &str,
) -> Result<Vec<segment_match::KnownSegment>, ScanError> {
    let mut statement = scan.tx.prepare_cached(
        "SELECT id, segment_index, segment_start_ms, title FROM tracks \
         WHERE path = ?1 AND segment_index > 0 ORDER BY segment_index",
    )?;
    let rows = statement
        .query_map([path_str], |row| {
            Ok(segment_match::KnownSegment {
                id: row.get(0)?,
                index: row.get(1)?,
                start_ms: row.get(2)?,
                title: row.get(3)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

/// Drops the rows no track kept: tracks the sheet no longer has, or that were
/// excluded. They go only after the tracks are written, so a new track never
/// gets the id of a row that history may still name.
fn remove_unkept_segments(scan: &EntryScan<'_, '_, '_>, path_str: &str) -> Result<(), ScanError> {
    scan.tx.execute(
        "DELETE FROM tracks WHERE path = ?1 AND segment_index < 0",
        [path_str],
    )?;
    Ok(())
}

/// Carries every row of a moved CUE file to its new path; see
/// [`move_detect::move_segment_rows`](super::move_detect::move_segment_rows).
pub(super) fn move_segments(
    scan: &EntryScan<'_, '_, '_>,
    old_path: &str,
    new_path: &Path,
    facts: &FileFacts,
    mount_point: Option<&str>,
) -> Result<(), ScanError> {
    super::move_detect::move_segment_rows(
        scan.tx,
        old_path,
        new_path,
        &super::move_detect::FileIdentity {
            file_mtime: facts.mtime,
            file_size: facts.file_size,
            device: facts.device,
            inode: facts.inode,
            mount_point: mount_point.map(str::to_string),
        },
    )?;
    Ok(())
}

/// The album the tracks of `layout` carry, as [`segment_meta`] writes it; `None`
/// for a file kept whole.
pub(super) fn tracks_album<'a>(layout: &'a Layout, file: &'a TrackMeta) -> Option<&'a str> {
    match layout {
        Layout::Segments { segments, .. } => segments
            .first()
            .map(|segment| non_empty(&segment.album).unwrap_or(&file.album)),
        Layout::Whole { .. } => None,
    }
}

/// A track's metadata: what the sheet says, and for what it leaves out, what the
/// audio file's own tags say. A track gain measured for the whole file says
/// nothing about one track in it, so only the album values carry over.
fn segment_meta(file: &TrackMeta, segment: &CueSegment) -> TrackMeta {
    TrackMeta {
        title: segment_title(segment),
        artist: non_empty(&segment.performer)
            .unwrap_or(&file.artist)
            .to_string(),
        album: non_empty(&segment.album).unwrap_or(&file.album).to_string(),
        album_artist: non_empty(&segment.album_artist)
            .unwrap_or(&file.album_artist)
            .to_string(),
        artist_mbid: None,
        year: segment.date.as_deref().and_then(leading_year).or(file.year),
        track_no: i32::try_from(segment.track_no).ok(),
        disc_no: file.disc_no,
        genre: segment
            .genre
            .as_deref()
            .and_then(non_empty)
            .unwrap_or(&file.genre)
            .to_string(),
        duration_ms: segment.end_ms - segment.start_ms,
        bitrate_kbps: file.bitrate_kbps,
        replay_gain: ReplayGainTags {
            track_gain_db: None,
            track_peak: None,
            album_gain_db: file.replay_gain.album_gain_db,
            album_peak: file.replay_gain.album_peak,
        },
        embedded_cuesheet: None,
    }
}

fn segment_title(segment: &CueSegment) -> String {
    non_empty(&segment.title)
        .map_or_else(|| format!("Track {:02}", segment.track_no), str::to_string)
}

fn non_empty(text: &str) -> Option<&str> {
    let text = text.trim();
    (!text.is_empty()).then_some(text)
}

/// The year a `REM DATE` begins with: `1979`, `1979-05-12` and `1979/5` all say 1979.
fn leading_year(date: &str) -> Option<i32> {
    let digits: String = date
        .trim()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    (digits.len() == 4).then(|| digits.parse().ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::leading_year;

    #[test]
    fn a_date_gives_its_year_only_when_it_starts_with_one() {
        assert_eq!(leading_year("1979"), Some(1979));
        assert_eq!(leading_year(" 1979-05-12 "), Some(1979));
        assert_eq!(leading_year("1979/5"), Some(1979));
        assert_eq!(leading_year("79"), None);
        assert_eq!(leading_year("May 1979"), None);
        assert_eq!(leading_year("19790"), None);
    }
}

#[cfg(test)]
#[path = "scanner_cue_exclusion_tests.rs"]
mod exclusion_tests;
