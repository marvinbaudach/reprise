//! The text the redactor writes in place of a value: the one list every pass
//! agrees on, so a pass can never read another pass's output as data, and a
//! forged look-alike (`$marvin`, `$HOME/…And Justice`) is never taken for it.

pub(super) const ELLIPSIS: &str = "…";

/// Every marker word the redactor writes. Case-sensitive and exact: `$user`
/// and `$USERNAME` are not markers, they are a name behind a dollar sign.
pub(super) const MARKERS: [&str; 7] = [
    "$USER",
    "$HOST",
    "$IP",
    "$MAC",
    "$REDACTED",
    "$HOME",
    "$XDG_MUSIC_DIR",
];

/// Whether `ch` continues a name (`marvin_laptop`, `v2`).
pub(super) fn is_word(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// The length of the marker that `text` opens with, if it opens with one. A
/// marker is exact text, whatever follows it (`$HOST3`, `$USER_laptop`): the
/// name replacement must never read the marker it wrote as a name again.
pub(super) fn marker_len(text: &str) -> Option<usize> {
    MARKERS
        .iter()
        .filter(|marker| text.starts_with(**marker))
        .map(|marker| marker.len())
        .max()
}

/// Whether the whole token is one marker, ignoring wrapping punctuation.
pub(super) fn is_marker(token: &str) -> bool {
    let trimmed = token.trim_matches(|ch: char| !ch.is_alphanumeric() && ch != '$' && ch != '_');
    MARKERS.contains(&trimmed)
}

/// The byte ranges of the markers in `text`.
pub(super) fn marker_spans(text: &str) -> Vec<(usize, usize)> {
    text.match_indices('$')
        .filter_map(|(at, _)| marker_len(&text[at..]).map(|len| (at, at + len)))
        .collect()
}
