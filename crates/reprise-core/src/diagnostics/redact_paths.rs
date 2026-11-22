//! Paths and URIs in free text.
//!
//! A path name may hold spaces, commas, quotes and brackets (`Earth, Wind &
//! Fire`, `Livin' on a Prayer`, `12" Version`), in any script, and a log line
//! cannot tell where the name ends and the sentence goes on. So nothing here
//! tries to: once a path is recognised, it takes the rest of its line. Over-
//! redaction is the accepted failure; a leaked name is not.
//!
//! Recognising the start is the whole job, and it is done by shape, not by
//! the folders a path happens to name: [`first_trigger`] finds an absolute
//! path under any root, one glued to a `key=`, a URI, a `~`, `$VAR` or
//! `%VAR%` root, a drive or UNC path and a remote root; [`relative_trigger`]
//! finds a relative path with spaces in it. The music and home folders of the
//! context are found by their text. The earliest start on the line wins.

use super::model::RedactionContext;
use super::redact_markers::ELLIPSIS;
use super::redact_path_relative::relative_trigger;
use super::redact_path_triggers::first_trigger;

/// What replaces a path under the music folder and under the home folder.
const MUSIC_MARKER: &str = "$XDG_MUSIC_DIR/…";
const HOME_MARKER: &str = "$HOME/…";

/// Where a path starts on its line and what stands in its place.
pub(super) struct Cut {
    pub(super) begin: usize,
    pub(super) replacement: String,
}

impl Cut {
    pub(super) fn path(begin: usize) -> Self {
        Self {
            begin,
            replacement: ELLIPSIS.to_owned(),
        }
    }
}

/// Replaces the earliest path on every line, and the rest of that line.
pub(super) fn redact_paths(input: &str, context: &RedactionContext) -> String {
    let folders = [
        (context.music_dir.as_deref(), MUSIC_MARKER),
        (context.home_dir.as_deref(), HOME_MARKER),
    ];
    let mut output = String::with_capacity(input.len());
    for segment in input.split_inclusive(['\n', '\r']) {
        let body = segment.trim_end_matches(['\n', '\r']);
        let line_break = &segment[body.len()..];
        match earliest_cut(body, &folders) {
            Some(cut) => {
                output.push_str(&body[..cut.begin]);
                output.push_str(&cut.replacement);
            }
            None => output.push_str(body),
        }
        output.push_str(line_break);
    }
    output
}

fn earliest_cut(line: &str, folders: &[(Option<&str>, &str)]) -> Option<Cut> {
    // The folders come first, so that on a tie the more specific marker wins.
    let by_text = folders
        .iter()
        .filter_map(|(folder, marker)| folder_cut(line, folder, marker))
        .chain(first_trigger(line))
        .reduce(|kept, candidate| {
            if candidate.begin < kept.begin {
                candidate
            } else {
                kept
            }
        });
    // A word inside a path that starts earlier belongs to that path.
    let limit = by_text.as_ref().map_or(line.len(), |cut| cut.begin);
    match relative_trigger(line, limit) {
        Some(relative) if relative.begin < limit => Some(relative),
        _ => by_text,
    }
}

/// The first place the text of a configured folder stands on the line.
fn folder_cut(line: &str, folder: &Option<&str>, marker: &str) -> Option<Cut> {
    let folder = folder.filter(|folder| !folder.trim_matches('/').is_empty())?;
    line.find(folder).map(|begin| Cut {
        begin,
        replacement: marker.to_owned(),
    })
}
