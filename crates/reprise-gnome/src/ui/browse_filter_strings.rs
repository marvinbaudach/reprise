//! Gettext-backed copy owned by the unified Library filter bar.
//!
//! This lives beside `strings.rs` because that central catalogue is already
//! at the repository's source-size limit.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub(super) const BROWSE_GENRE: &str = N_!("Genre");
pub(super) const BROWSE_ARTIST: &str = N_!("Artist");
pub(super) const BROWSE_ALBUM: &str = N_!("Album");
pub(super) const ALL_GENRES: &str = N_!("All genres");
pub(super) const ALL_ARTISTS: &str = N_!("All artists");
pub(super) const ALL_ALBUMS: &str = N_!("All albums");
pub(super) const UNKNOWN_GENRE: &str = N_!("Unknown genre");
pub(super) const UNKNOWN_ARTIST: &str = N_!("Unknown artist");
pub(super) const UNKNOWN_ALBUM: &str = N_!("Unknown album");

pub(super) fn text(message: &str) -> String {
    crate::i18n::gettext(message)
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn chip_label(facet: &str, value: &str) -> String {
    formatted(
        N_!("{facet}: {value}"),
        &[("facet", facet), ("value", value)],
    )
}

#[cfg_attr(not(test), allow(dead_code))]
pub(super) fn remove_filter_label(facet: &str, value: &str) -> String {
    formatted(
        N_!("Remove {facet} filter: {value}"),
        &[("facet", facet), ("value", value)],
    )
}

#[cfg_attr(not(test), allow(dead_code))]
fn formatted(message: &str, values: &[(&str, &str)]) -> String {
    crate::i18n::format_message(&text(message), values)
}
