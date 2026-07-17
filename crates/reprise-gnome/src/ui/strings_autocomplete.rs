//! Tag editor autocomplete dropdown + inline ghost completion copy
//! (TAG-6/TAG-7).

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::formatted;

pub const TAG_AUTOCOMPLETE_SECTION_HEADER: &str = N_!("FROM YOUR LIBRARY");
pub const TAG_AUTOCOMPLETE_USE_AS_NEW_ARTIST: &str = N_!("Use “{value}” as new artist…");
pub const TAG_AUTOCOMPLETE_USE_AS_NEW_ALBUM: &str = N_!("Use “{value}” as new album…");
pub const TAG_AUTOCOMPLETE_USE_AS_NEW_ALBUM_ARTIST: &str =
    N_!("Use “{value}” as new album artist…");
pub const TAG_AUTOCOMPLETE_USE_AS_NEW_GENRE: &str = N_!("Use “{value}” as new genre…");
pub const TAG_AUTOCOMPLETE_GHOST_TAB_HINT: &str = N_!("Tab");

pub fn tag_autocomplete_use_as_new_artist(value: &str) -> String {
    formatted(TAG_AUTOCOMPLETE_USE_AS_NEW_ARTIST, &[("value", value)])
}

pub fn tag_autocomplete_use_as_new_album(value: &str) -> String {
    formatted(TAG_AUTOCOMPLETE_USE_AS_NEW_ALBUM, &[("value", value)])
}

pub fn tag_autocomplete_use_as_new_album_artist(value: &str) -> String {
    formatted(
        TAG_AUTOCOMPLETE_USE_AS_NEW_ALBUM_ARTIST,
        &[("value", value)],
    )
}

pub fn tag_autocomplete_use_as_new_genre(value: &str) -> String {
    formatted(TAG_AUTOCOMPLETE_USE_AS_NEW_GENRE, &[("value", value)])
}
