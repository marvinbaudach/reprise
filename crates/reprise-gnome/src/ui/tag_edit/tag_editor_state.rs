//! Pure patch parsing, navigation, and field identity for the tag editor.

use crate::ui::strings;

// ── Pure-logic helpers (unchanged from v1, exercised by the tests below) ─────

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("expected a positive whole number")]
pub struct ParseFieldError;

pub(super) const RATING_MAX: i32 = 5;

pub(crate) fn string_patch(dirty: bool, text: &str) -> Option<String> {
    dirty.then(|| text.to_string())
}

pub(crate) fn number_patch(
    dirty: bool,
    text: &str,
) -> Result<Option<Option<u32>>, ParseFieldError> {
    if !dirty {
        return Ok(None);
    }
    let text = text.trim();
    if text.is_empty() {
        return Ok(Some(None));
    }
    let value = text.parse::<u32>().map_err(|_| ParseFieldError)?;
    if value == 0 {
        return Err(ParseFieldError);
    }
    Ok(Some(Some(value)))
}

// ── Navigation direction ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavigateDirection {
    Previous,
    Next,
}

// ── Field identity for dirty tracking ────────────────────────────────────────

/// Indices into the `dirty` flags vector.
pub(super) const FIELD_TITLE: usize = 0;
pub(super) const FIELD_ARTIST: usize = 1;
pub(super) const FIELD_ALBUM: usize = 2;
pub(super) const FIELD_ALBUM_ARTIST: usize = 3;
pub(super) const FIELD_YEAR: usize = 4;
pub(super) const FIELD_TRACK_NO: usize = 5;
pub(super) const FIELD_GENRE: usize = 6;
pub(super) const FIELD_RATING: usize = 7;
pub(super) const FIELD_COUNT: usize = 8;

/// Human-readable names for the pending-change bar, indexed by `FIELD_*`.
/// Uses the same i18n constants as the form field labels.
pub(super) fn field_name(index: usize) -> String {
    use strings::*;
    match index {
        FIELD_TITLE => text(TAG_TITLE),
        FIELD_ARTIST => text(TAG_ARTIST),
        FIELD_ALBUM => text(TAG_ALBUM),
        FIELD_ALBUM_ARTIST => text(TAG_ALBUM_ARTIST),
        FIELD_YEAR => text(TAG_YEAR),
        FIELD_TRACK_NO => text(TAG_TRACK_NUMBER),
        FIELD_GENRE => text(TAG_GENRE),
        FIELD_RATING => text(RATING),
        _ => String::new(),
    }
}
