//! The tag editor's pending-session model: a snapshot of the tracks being
//! edited plus per-track pending changes that survive navigation between
//! tracks (TAG-4). All GUI-facing computation the editor needs — mixed-value
//! placeholders (TAG-2), the review summary and per-field diff lines
//! (TAG-5), the MusicBrainz uniform-artist/album check, and the final
//! effective write batch — lives here as pure Rust so the GTK layer only
//! ever binds to already-computed data (Beschluss #7: "GUI bindet nur").
//!
//! ## Effective value = original, overridden by pending
//!
//! Every read in this module goes through `TagEditSession::effective_value`:
//! a track's pending patch (if any) wins field-by-field over its original
//! snapshot value. This is the single definition of "what the user would see
//! right now", reused by the mixed-placeholder calculation, the review
//! lines, the MusicBrainz uniformity check, and the final write batch — so
//! all of them agree with each other by construction rather than by
//! parallel maintenance.
//!
//! ## Effective diff = exact comparison, no trim/case-folding (TAG-5)
//!
//! A field only counts as "really changed" when its effective value is not
//! `==` to its original value, byte for byte. `"Rock"` vs `"Rock "` or
//! `"rock"` vs `"Rock"` are both real changes, never silently normalized
//! away — matching the write path's own no-op check in `tag_edit_write`.

use std::collections::HashMap;
use std::collections::HashSet;
use std::path::PathBuf;

use super::tag_edit::{EditableTags, TagPatch, TrackEditPatch};
use super::tag_edit_write::TrackWrite;

/// A field's placeholder text for an empty string / absent number, used
/// consistently in mixed-value and review-line formatting (TAG-2/TAG-5).
const EMPTY_LABEL: &str = "empty";

/// Above this many distinct values, a mixed placeholder / review line shows
/// only the count ("8 different values") rather than listing them all out
/// (TAG-2).
const MAX_LISTED_DISTINCT_VALUES: usize = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TagField {
    Title,
    Artist,
    Album,
    AlbumArtist,
    Genre,
    Year,
    TrackNo,
    Rating,
}

const ALL_TAG_FIELDS: [TagField; 7] = [
    TagField::Title,
    TagField::Artist,
    TagField::Album,
    TagField::AlbumArtist,
    TagField::Genre,
    TagField::Year,
    TagField::TrackNo,
];

const ALL_FIELDS: [TagField; 8] = [
    TagField::Title,
    TagField::Artist,
    TagField::Album,
    TagField::AlbumArtist,
    TagField::Genre,
    TagField::Year,
    TagField::TrackNo,
    TagField::Rating,
];

/// Which track(s) a [`TagEditSession::set_pending`]/`revert` call targets.
/// `AllTracks` is the Multi-mode bulk-edit scope (TAG-2); `CurrentTrack` is
/// the SingleNav browsing scope (TAG-4) — the caller picks based on
/// [`SessionMode`], the session itself enforces nothing beyond "which track
/// ids does this resolve to".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingScope {
    AllTracks,
    CurrentTrack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionMode {
    Multi,
    SingleNav,
}

/// One track's editable snapshot as the session first saw it — never
/// mutated after construction; all edits live in the session's separate
/// pending map so the original is always available for diffing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionTrack {
    pub id: i64,
    pub path: PathBuf,
    pub tags: EditableTags,
    pub rating: i32,
}

/// A typed value for [`TagEditSession::set_pending`] — the variant must
/// match the target [`TagField`]'s data type (`Text` for the five string
/// fields, `Number` for Year/TrackNo, `Rating` for Rating). A mismatched
/// pairing is a caller bug; it is logged and ignored rather than panicking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Text(String),
    Number(Option<u32>),
    Rating(i32),
}

/// [`TagEditSession::mixed_placeholder`]'s result for a field whose
/// effective values differ across the session's tracks (TAG-2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MixedPlaceholder {
    /// Ready-to-display placeholder text, e.g. `"Mixed — Ambient, empty"`
    /// or `"Mixed — 8 different values"`.
    pub label: String,
    /// Number of distinct effective values — the field's counter annotation
    /// (e.g. `"2 values"`).
    pub distinct_count: usize,
}

/// [`TagEditSession::summary`]'s result: the review footer's summary line
/// (TAG-5, e.g. "2 fields · 30 tracks affected").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewSummary {
    pub fields: usize,
    pub tracks_affected: usize,
}

/// One row of [`TagEditSession::review_lines`] — a single field's aggregate
/// diff across every track it effectively changed on (TAG-5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewLine {
    pub field: TagField,
    pub old_display: String,
    pub new_display: String,
    pub tracks_affected: usize,
}

/// The pending-edit session: an immutable snapshot of tracks plus a mutable
/// per-track pending-patch map. Reused as the single source of truth by
/// every review/diff/write computation the tag editor needs.
#[derive(Debug, Clone)]
pub struct TagEditSession {
    tracks: Vec<SessionTrack>,
    mode: SessionMode,
    current_id: i64,
    pending: HashMap<i64, TrackEditPatch>,
}

impl TagEditSession {
    /// `tracks` must be non-empty in practice (the editor never opens on an
    /// empty selection); an empty slice degrades to a session with no
    /// resolvable "current" track rather than panicking.
    pub fn new(tracks: Vec<SessionTrack>, mode: SessionMode) -> Self {
        let current_id = tracks.first().map(|track| track.id).unwrap_or_default();
        Self {
            tracks,
            mode,
            current_id,
            pending: HashMap::new(),
        }
    }

    pub fn mode(&self) -> SessionMode {
        self.mode
    }

    pub fn track_count(&self) -> usize {
        self.tracks.len()
    }

    pub fn current_track_id(&self) -> i64 {
        self.current_id
    }

    /// Switches the "current track" for [`PendingScope::CurrentTrack`]
    /// (TAG-4 browsing). A `track_id` outside this session's snapshot is
    /// ignored — the previous current track stays current.
    pub fn set_current_track(&mut self, track_id: i64) {
        if self.tracks.iter().any(|track| track.id == track_id) {
            self.current_id = track_id;
        }
    }

    fn scope_ids(&self, scope: PendingScope) -> Vec<i64> {
        match scope {
            PendingScope::AllTracks => self.tracks.iter().map(|track| track.id).collect(),
            PendingScope::CurrentTrack => vec![self.current_id],
        }
    }

    /// Arms `field` with `value` for every track `scope` resolves to
    /// (TAG-2's "first keystroke arms the field", TAG-4's per-track
    /// pending). Persists per track id, so switching the current track
    /// (TAG-4) never loses it.
    pub fn set_pending(&mut self, scope: PendingScope, field: TagField, value: &FieldValue) {
        for id in self.scope_ids(scope) {
            let patch = self.pending.entry(id).or_default();
            apply_pending_value(patch, field, value.clone());
        }
    }

    /// Clears a pending value for `field` on every track `scope` resolves
    /// to, reverting to that track's original value (the field's ↺).
    pub fn revert(&mut self, scope: PendingScope, field: TagField) {
        for id in self.scope_ids(scope) {
            if let Some(patch) = self.pending.get_mut(&id) {
                clear_pending_value(patch, field);
                if patch.is_empty() {
                    self.pending.remove(&id);
                }
            }
        }
    }

    fn track(&self, track_id: i64) -> Option<&SessionTrack> {
        self.tracks.iter().find(|track| track.id == track_id)
    }

    fn original_value(&self, track: &SessionTrack, field: TagField) -> FieldValue {
        match field {
            TagField::Title => FieldValue::Text(track.tags.title.clone()),
            TagField::Artist => FieldValue::Text(track.tags.artist.clone()),
            TagField::Album => FieldValue::Text(track.tags.album.clone()),
            TagField::AlbumArtist => FieldValue::Text(track.tags.album_artist.clone()),
            TagField::Genre => FieldValue::Text(track.tags.genre.clone()),
            TagField::Year => FieldValue::Number(track.tags.year),
            TagField::TrackNo => FieldValue::Number(track.tags.track_no),
            TagField::Rating => FieldValue::Rating(track.rating),
        }
    }

    fn effective_value(&self, track: &SessionTrack, field: TagField) -> FieldValue {
        let pending = self.pending.get(&track.id);
        match field {
            TagField::Title => FieldValue::Text(
                pending
                    .and_then(|patch| patch.tags.title.clone())
                    .unwrap_or_else(|| track.tags.title.clone()),
            ),
            TagField::Artist => FieldValue::Text(
                pending
                    .and_then(|patch| patch.tags.artist.clone())
                    .unwrap_or_else(|| track.tags.artist.clone()),
            ),
            TagField::Album => FieldValue::Text(
                pending
                    .and_then(|patch| patch.tags.album.clone())
                    .unwrap_or_else(|| track.tags.album.clone()),
            ),
            TagField::AlbumArtist => FieldValue::Text(
                pending
                    .and_then(|patch| patch.tags.album_artist.clone())
                    .unwrap_or_else(|| track.tags.album_artist.clone()),
            ),
            TagField::Genre => FieldValue::Text(
                pending
                    .and_then(|patch| patch.tags.genre.clone())
                    .unwrap_or_else(|| track.tags.genre.clone()),
            ),
            TagField::Year => FieldValue::Number(
                pending
                    .and_then(|patch| patch.tags.year)
                    .unwrap_or(track.tags.year),
            ),
            TagField::TrackNo => FieldValue::Number(
                pending
                    .and_then(|patch| patch.tags.track_no)
                    .unwrap_or(track.tags.track_no),
            ),
            TagField::Rating => FieldValue::Rating(
                pending
                    .and_then(|patch| patch.rating)
                    .unwrap_or(track.rating),
            ),
        }
    }

    /// The effective (pending-overridden) value of `field` for `track_id`,
    /// formatted for display. `None` if `track_id` is not in this session.
    pub fn effective_display(&self, track_id: i64, field: TagField) -> Option<String> {
        self.track(track_id)
            .map(|track| display_value(&self.effective_value(track, field)))
    }

    /// TAG-2's mixed-value placeholder: `None` when every track's effective
    /// value for `field` is identical (nothing to show but the value
    /// itself); `Some` with a formatted label and counter otherwise.
    pub fn mixed_placeholder(&self, field: TagField) -> Option<MixedPlaceholder> {
        let mut distinct = Vec::new();
        for track in &self.tracks {
            let display = display_value(&self.effective_value(track, field));
            if !distinct.contains(&display) {
                distinct.push(display);
            }
        }
        if distinct.len() <= 1 {
            return None;
        }
        Some(MixedPlaceholder {
            label: format!("Mixed — {}", format_distinct_list(&distinct)),
            distinct_count: distinct.len(),
        })
    }

    /// TAG-5's per-field "was: …" line for the given scope: `None` when the
    /// effective value equals the original everywhere in `scope` (nothing
    /// to show — the reserved line stays empty, P-4).
    pub fn old_value_line(&self, scope: PendingScope, field: TagField) -> Option<String> {
        let mut old_values = Vec::new();
        let mut changed = false;
        for id in self.scope_ids(scope) {
            let track = self.track(id)?;
            let old = self.original_value(track, field);
            let new = self.effective_value(track, field);
            if old != new {
                changed = true;
            }
            let display = display_value(&old);
            if !old_values.contains(&display) {
                old_values.push(display);
            }
        }
        if !changed {
            return None;
        }
        Some(format_distinct_list(&old_values))
    }

    fn track_has_effective_change(&self, track: &SessionTrack) -> bool {
        ALL_FIELDS
            .iter()
            .any(|&field| self.original_value(track, field) != self.effective_value(track, field))
    }

    /// TAG-5's summary line: how many distinct fields have at least one
    /// real (effective) change anywhere, and how many tracks are affected
    /// by at least one such change — the same "tracks" currency the
    /// progress/toast/save-button all share.
    pub fn summary(&self) -> ReviewSummary {
        let fields = self.review_lines().len();
        let mut affected: HashSet<i64> = HashSet::new();
        for track in &self.tracks {
            if self.track_has_effective_change(track) {
                affected.insert(track.id);
            }
        }
        ReviewSummary {
            fields,
            tracks_affected: affected.len(),
        }
    }

    /// TAG-5's review expander rows: one line per field with at least one
    /// real change, aggregating the distinct old/new values and the count
    /// of tracks that field actually changed on. Fields with zero effective
    /// change across every track are omitted entirely.
    pub fn review_lines(&self) -> Vec<ReviewLine> {
        let mut lines = Vec::new();
        for &field in &ALL_FIELDS {
            let mut old_values = Vec::new();
            let mut new_values = Vec::new();
            let mut affected = 0usize;
            for track in &self.tracks {
                let old = self.original_value(track, field);
                let new = self.effective_value(track, field);
                if old == new {
                    continue;
                }
                affected += 1;
                let old_display = display_value(&old);
                let new_display = display_value(&new);
                if !old_values.contains(&old_display) {
                    old_values.push(old_display);
                }
                if !new_values.contains(&new_display) {
                    new_values.push(new_display);
                }
            }
            if affected == 0 {
                continue;
            }
            lines.push(ReviewLine {
                field,
                old_display: format_distinct_list(&old_values),
                new_display: format_distinct_list(&new_values),
                tracks_affected: affected,
            });
        }
        lines
    }

    fn effective_patch_for_track(&self, track: &SessionTrack) -> TrackEditPatch {
        let mut patch = TrackEditPatch::default();
        for &field in &ALL_TAG_FIELDS {
            let old = self.original_value(track, field);
            let new = self.effective_value(track, field);
            if old == new {
                continue;
            }
            set_tag_patch_value(&mut patch.tags, field, new);
        }
        let old_rating = self.original_value(track, TagField::Rating);
        let new_rating = self.effective_value(track, TagField::Rating);
        if old_rating != new_rating {
            if let FieldValue::Rating(rating) = new_rating {
                patch.rating = Some(rating);
            }
        }
        patch
    }

    /// The final write batch (TAG-5): one [`TrackWrite`] per track with at
    /// least one effective change, carrying only the fields that actually
    /// differ. Tracks with zero effective change are excluded entirely —
    /// an all-pending-but-zero-effective session yields an empty `Vec`.
    pub fn write_batch(&self) -> Vec<TrackWrite> {
        self.tracks
            .iter()
            .filter_map(|track| {
                let patch = self.effective_patch_for_track(track);
                if patch.is_empty() {
                    None
                } else {
                    Some(TrackWrite {
                        id: track.id,
                        path: track.path.clone(),
                        patch,
                    })
                }
            })
            .collect()
    }

    /// How many tracks currently have at least one effective pending
    /// change — the same count as `write_batch().len()`, exposed without
    /// materializing the batch so the UI can cheaply decide when to show
    /// the review expander (F1: "Single sobald `pending_track_count() > 1`").
    pub fn pending_track_count(&self) -> usize {
        self.tracks
            .iter()
            .filter(|track| self.track_has_effective_change(track))
            .count()
    }

    /// The Beschluss-#3 MusicBrainz gate: `Some((artist, album))` only when
    /// every track's *effective* (original + pending) artist and album are
    /// both non-empty and identical across the whole session.
    pub fn mb_uniform_artist_album(&self) -> Option<(String, String)> {
        let mut uniform: Option<(String, String)> = None;
        for track in &self.tracks {
            let artist = match self.effective_value(track, TagField::Artist) {
                FieldValue::Text(value) => value,
                _ => unreachable!("Artist is always FieldValue::Text"),
            };
            let album = match self.effective_value(track, TagField::Album) {
                FieldValue::Text(value) => value,
                _ => unreachable!("Album is always FieldValue::Text"),
            };
            if artist.is_empty() || album.is_empty() {
                return None;
            }
            match &uniform {
                None => uniform = Some((artist, album)),
                Some((existing_artist, existing_album)) => {
                    if *existing_artist != artist || *existing_album != album {
                        return None;
                    }
                }
            }
        }
        uniform
    }
}

fn apply_pending_value(patch: &mut TrackEditPatch, field: TagField, value: FieldValue) {
    match (field, value) {
        (TagField::Title, FieldValue::Text(value)) => patch.tags.title = Some(value),
        (TagField::Artist, FieldValue::Text(value)) => patch.tags.artist = Some(value),
        (TagField::Album, FieldValue::Text(value)) => patch.tags.album = Some(value),
        (TagField::AlbumArtist, FieldValue::Text(value)) => patch.tags.album_artist = Some(value),
        (TagField::Genre, FieldValue::Text(value)) => patch.tags.genre = Some(value),
        (TagField::Year, FieldValue::Number(value)) => patch.tags.year = Some(value),
        (TagField::TrackNo, FieldValue::Number(value)) => patch.tags.track_no = Some(value),
        (TagField::Rating, FieldValue::Rating(value)) => patch.rating = Some(value),
        (field, value) => {
            tracing::warn!(
                ?field,
                ?value,
                "tag edit session: ignoring mismatched field/value pairing"
            );
        }
    }
}

fn clear_pending_value(patch: &mut TrackEditPatch, field: TagField) {
    match field {
        TagField::Title => patch.tags.title = None,
        TagField::Artist => patch.tags.artist = None,
        TagField::Album => patch.tags.album = None,
        TagField::AlbumArtist => patch.tags.album_artist = None,
        TagField::Genre => patch.tags.genre = None,
        TagField::Year => patch.tags.year = None,
        TagField::TrackNo => patch.tags.track_no = None,
        TagField::Rating => patch.rating = None,
    }
}

fn set_tag_patch_value(tags: &mut TagPatch, field: TagField, value: FieldValue) {
    match (field, value) {
        (TagField::Title, FieldValue::Text(value)) => tags.title = Some(value),
        (TagField::Artist, FieldValue::Text(value)) => tags.artist = Some(value),
        (TagField::Album, FieldValue::Text(value)) => tags.album = Some(value),
        (TagField::AlbumArtist, FieldValue::Text(value)) => tags.album_artist = Some(value),
        (TagField::Genre, FieldValue::Text(value)) => tags.genre = Some(value),
        (TagField::Year, FieldValue::Number(value)) => tags.year = Some(value),
        (TagField::TrackNo, FieldValue::Number(value)) => tags.track_no = Some(value),
        (field, value) => {
            unreachable!("field {field:?} paired with mismatched value {value:?}")
        }
    }
}

fn display_value(value: &FieldValue) -> String {
    match value {
        FieldValue::Text(text) if text.is_empty() => EMPTY_LABEL.to_string(),
        FieldValue::Text(text) => text.clone(),
        FieldValue::Number(Some(number)) => number.to_string(),
        FieldValue::Number(None) => EMPTY_LABEL.to_string(),
        FieldValue::Rating(rating) => rating.to_string(),
    }
}

/// Shared TAG-2/TAG-5 formatting rule: up to [`MAX_LISTED_DISTINCT_VALUES`]
/// distinct values are listed verbatim; beyond that, only the count is
/// shown.
fn format_distinct_list(values: &[String]) -> String {
    if values.len() <= MAX_LISTED_DISTINCT_VALUES {
        values.join(", ")
    } else {
        format!("{} different values", values.len())
    }
}

#[cfg(test)]
#[path = "tag_edit_session_tests.rs"]
mod tests;
