//! Pure decisions for the shared track-row context menu.
//!
//! Keeping selection summarization and action sensitivity independent of GTK
//! widgets makes every context testable without a display. Runtime
//! [`ViewSource`] values collapse onto the five menu contexts: Missing,
//! Smart, My Stats, Import Errors, and Device defensively use
//! [`MenuContext::LibraryTracks`]. Device destructive semantics remain a
//! separate product decision; this module preserves today's library-like
//! treatment until that decision is made.

use reprise_core::models::Track;
use reprise_core::view_source::ViewSource;

#[allow(dead_code)] // All variants become runtime inputs when the builder lands in Task 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum MenuContext {
    LibraryTracks,
    AlbumDetail,
    ArtistDetail,
    Playlist,
    Queue,
}

impl MenuContext {
    #[allow(dead_code)] // Used by the live adapter in Task 7.
    pub(in crate::ui) fn from_source(source: &ViewSource) -> Self {
        match source {
            ViewSource::Album { .. } => Self::AlbumDetail,
            ViewSource::Artist(_) => Self::ArtistDetail,
            ViewSource::Playlist(_) => Self::Playlist,
            ViewSource::Queue => Self::Queue,
            ViewSource::Library
            | ViewSource::Smart(_)
            | ViewSource::Missing
            | ViewSource::ImportErrors
            | ViewSource::MyStats
            | ViewSource::Device { .. } => Self::LibraryTracks,
        }
    }
}

#[allow(dead_code)] // Used by the live adapter in Task 7.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct SelectionSummary {
    pub count: usize,
    pub any_missing: bool,
    pub all_missing: bool,
    pub same_album: bool,
    pub same_artist: bool,
    pub same_folder: bool,
}

#[allow(dead_code)] // Used by the playlist submenu builder in Task 3.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::ui) struct PlaylistEntry {
    pub id: i64,
    pub name: String,
    pub is_current: bool,
}

#[allow(dead_code)] // Used by the live adapter in Task 7.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) struct ActionStates {
    pub enqueue: bool,
    pub go_to_album: bool,
    pub go_to_artist: bool,
    pub show_in_files: bool,
    pub trash: bool,
    pub edit_tags: bool,
}

#[allow(dead_code)] // Used by the live adapter in Task 7.
pub(in crate::ui) fn summarize_selection(tracks: &[Track]) -> SelectionSummary {
    let Some(first) = tracks.first() else {
        return SelectionSummary {
            count: 0,
            any_missing: false,
            all_missing: false,
            same_album: false,
            same_artist: false,
            same_folder: false,
        };
    };

    let normalize = |value: &str| value.trim().to_lowercase();
    let first_album = (normalize(&first.album), normalize(&first.album_artist));
    let first_artist = normalize(&first.album_artist);
    let first_folder = std::path::Path::new(&first.path).parent();

    SelectionSummary {
        count: tracks.len(),
        any_missing: tracks.iter().any(Track::is_missing),
        all_missing: tracks.iter().all(Track::is_missing),
        same_album: tracks
            .iter()
            .all(|track| (normalize(&track.album), normalize(&track.album_artist)) == first_album),
        same_artist: tracks
            .iter()
            .all(|track| normalize(&track.album_artist) == first_artist),
        same_folder: tracks
            .iter()
            .all(|track| std::path::Path::new(&track.path).parent() == first_folder),
    }
}

#[allow(dead_code)] // Used by the live adapter in Task 7.
pub(in crate::ui) fn action_states(
    _context: MenuContext,
    selection: &SelectionSummary,
) -> ActionStates {
    ActionStates {
        enqueue: selection.count > 0 && !selection.any_missing,
        go_to_album: selection.count > 0 && selection.same_album,
        go_to_artist: selection.count > 0 && selection.same_artist,
        show_in_files: selection.count > 0 && !selection.any_missing && selection.same_folder,
        trash: selection.count > 0 && !selection.any_missing,
        edit_tags: selection.count > 0 && !selection.all_missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: i64, album: &str, album_artist: &str, path: &str, missing: bool) -> Track {
        Track {
            id,
            path: path.into(),
            title: format!("Track {id}"),
            artist: album_artist.into(),
            album: album.into(),
            album_artist: album_artist.into(),
            year: None,
            track_no: None,
            genre: String::new(),
            duration_ms: 0,
            bitrate_kbps: None,
            rating: 0,
            play_count: 0,
            last_played_at: None,
            added_at: 0,
            file_mtime: 0,
            missing_since: missing.then_some(1),
            missing_reason: None,
            untagged: false,
            file_size: 0,
            device: None,
            inode: None,
            playlist_position: None,
        }
    }

    fn same_selection() -> SelectionSummary {
        SelectionSummary {
            count: 3,
            any_missing: false,
            all_missing: false,
            same_album: true,
            same_artist: true,
            same_folder: true,
        }
    }

    #[test]
    fn ctx_4_nav_disabled_on_mixed_selection() {
        let same = same_selection();
        let mixed = SelectionSummary {
            same_album: false,
            same_artist: false,
            same_folder: false,
            ..same
        };
        let a = action_states(MenuContext::LibraryTracks, &same);
        assert!(a.go_to_album && a.go_to_artist);
        let b = action_states(MenuContext::LibraryTracks, &mixed);
        assert!(
            !b.go_to_album && !b.go_to_artist,
            "mixed album/artist selection greys nav out, not hidden"
        );
        let empty = SelectionSummary { count: 0, ..same };
        let c = action_states(MenuContext::LibraryTracks, &empty);
        assert!(!c.go_to_album && !c.enqueue && !c.edit_tags);
    }

    #[test]
    fn ctx_10_show_in_files_same_folder_only() {
        let base = SelectionSummary {
            count: 2,
            any_missing: false,
            all_missing: false,
            same_album: false,
            same_artist: false,
            same_folder: true,
        };
        assert!(action_states(MenuContext::LibraryTracks, &base).show_in_files);
        let diff_folder = SelectionSummary {
            same_folder: false,
            ..base
        };
        assert!(!action_states(MenuContext::LibraryTracks, &diff_folder).show_in_files);
        let missing = SelectionSummary {
            any_missing: true,
            ..base
        };
        assert!(
            !action_states(MenuContext::LibraryTracks, &missing).show_in_files,
            "missing file cannot be revealed"
        );
    }

    #[test]
    fn summarize_marks_same_album_folder_and_missing() {
        let tracks = [
            track(1, "Blue", "Joni", "/m/blue/01.flac", false),
            track(2, "Blue", "joni ", "/m/blue/02.flac", false),
        ];
        let summary = summarize_selection(&tracks);
        assert!(
            summary.same_album
                && summary.same_artist
                && summary.same_folder
                && !summary.any_missing
                && summary.count == 2
        );
        let mixed = [
            tracks[0].clone(),
            track(3, "Red", "Neil", "/m/red/01.flac", true),
        ];
        let summary = summarize_selection(&mixed);
        assert!(
            !summary.same_album
                && !summary.same_folder
                && summary.any_missing
                && !summary.all_missing
        );
    }
}
