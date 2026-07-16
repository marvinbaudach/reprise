macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::plural;
pub const APP_NAME: &str = N_!("Reprise");
pub const LIBRARY_VIEW_TRACKS: &str = N_!("Tracks");
pub const LIBRARY_VIEW_ALBUMS: &str = N_!("Albums");
pub const LIBRARY_VIEW_ARTISTS: &str = N_!("Artists");
pub const ALBUMS_EMPTY_TITLE: &str = N_!("No Albums Yet");
pub const ALBUMS_EMPTY_DESCRIPTION: &str = N_!("Scan a music folder to see album covers here.");
pub const ARTISTS_EMPTY_TITLE: &str = N_!("No Artists Yet");
pub const ARTISTS_EMPTY_DESCRIPTION: &str = N_!("Scan a music folder to see artists here.");
pub const UNKNOWN_ARTIST: &str = N_!("Unknown Artist");

pub fn artist_counts(album_count: i64, track_count: i64) -> String {
    let album_count = usize::try_from(album_count).unwrap_or(usize::MAX);
    let track_count = usize::try_from(track_count).unwrap_or(usize::MAX);
    let albums = plural(
        "{count} album",
        "{count} albums",
        album_count,
        &[("count", &album_count.to_string())],
    );
    let tracks = plural(
        "{count} track",
        "{count} tracks",
        track_count,
        &[("count", &track_count.to_string())],
    );
    format!("{albums} · {tracks}")
}
pub const ARTIST_SORT_ALPHABETICAL: &str = N_!("A–Z");
pub const ARTIST_SORT_MOST_PLAYED: &str = N_!("Most played");
pub const ARTIST_SORT_RECENTLY_PLAYED: &str = N_!("Recently played");

/// The Artists master-list header count, e.g. "42 artists".
#[allow(dead_code)] // consumed by the Artists master/detail view wiring (later task)
pub fn artist_master_count(count: usize) -> String {
    plural(
        "{count} artist",
        "{count} artists",
        count,
        &[("count", &count.to_string())],
    )
}

// Artists detail pane (src/ui/library_views/artist_detail_pane.rs).
#[allow(dead_code)] // consumed by the Artists master/detail view wiring (later task)
pub const ARTIST_DETAIL_EYEBROW: &str = N_!("ARTIST");
#[allow(dead_code)]
pub const ARTIST_DETAIL_PLAY_ALL: &str = N_!("Play all");
#[allow(dead_code)]
pub const ARTIST_DETAIL_MENU: &str = N_!("More artist actions");
#[allow(dead_code)]
pub const ARTIST_DETAIL_ADD_TO_QUEUE: &str = N_!("Add to queue");
#[allow(dead_code)]
pub const ARTIST_DETAIL_EDIT_TAGS: &str = N_!("Edit tags for all");
#[allow(dead_code)]
pub const ARTIST_DETAIL_GO_TO_FOLDER: &str = N_!("Go to folder");
#[allow(dead_code)]
pub const ARTIST_DETAIL_ALBUMS: &str = N_!("Albums");
#[allow(dead_code)]
pub const ARTIST_DETAIL_TOP_TRACKS: &str = N_!("Top tracks");
#[allow(dead_code)]
pub const ARTIST_DETAIL_SHOW_ALL: &str = N_!("Show all");
#[allow(dead_code)]
pub const ARTIST_DETAIL_SHOW_LESS: &str = N_!("Show less");
#[allow(dead_code)]
pub const ARTIST_DETAIL_NO_ALBUMS: &str = N_!("No albums for this artist yet.");

/// The hero meta line, e.g. "3 albums · 12 tracks · 5 hours · 1 play this year".
#[allow(dead_code)] // consumed by the Artists master/detail view wiring (later task)
pub fn artist_detail_meta(
    album_count: i64,
    track_count: i64,
    catalog_ms: i64,
    plays_this_year: i64,
) -> String {
    let album_count = usize::try_from(album_count).unwrap_or(usize::MAX);
    let track_count = usize::try_from(track_count).unwrap_or(usize::MAX);
    let hours = usize::try_from(catalog_ms.max(0) / 3_600_000).unwrap_or(usize::MAX);
    let plays = usize::try_from(plays_this_year.max(0)).unwrap_or(usize::MAX);
    let albums = plural(
        "{count} album",
        "{count} albums",
        album_count,
        &[("count", &album_count.to_string())],
    );
    let tracks = plural(
        "{count} track",
        "{count} tracks",
        track_count,
        &[("count", &track_count.to_string())],
    );
    let hours = plural(
        "{count} hour",
        "{count} hours",
        hours,
        &[("count", &hours.to_string())],
    );
    let plays = plural(
        "{count} play this year",
        "{count} plays this year",
        plays,
        &[("count", &plays.to_string())],
    );
    format!("{albums} · {tracks} · {hours} · {plays}")
}

/// An album card's meta line, e.g. "2020 · 12 tracks" (drops the year when 0).
#[allow(dead_code)] // consumed by the Artists master/detail view wiring (later task)
pub fn artist_album_meta(year: i64, track_count: i64) -> String {
    let track_count = usize::try_from(track_count).unwrap_or(usize::MAX);
    let tracks = plural(
        "{count} track",
        "{count} tracks",
        track_count,
        &[("count", &track_count.to_string())],
    );
    if year > 0 {
        format!("{year} · {tracks}")
    } else {
        tracks
    }
}

/// A top-track row's play count, e.g. "1 play" / "12 plays".
#[allow(dead_code)] // consumed by the Artists master/detail view wiring (later task)
pub fn artist_counts_plays(play_count: i64) -> String {
    let play_count = usize::try_from(play_count.max(0)).unwrap_or(usize::MAX);
    plural(
        "{count} play",
        "{count} plays",
        play_count,
        &[("count", &play_count.to_string())],
    )
}

/// The "Show all N tracks ›" button under the top-tracks list.
#[allow(dead_code)] // consumed by the Artists master/detail view wiring (later task)
pub fn artist_detail_show_all_tracks(track_count: i64) -> String {
    let track_count = usize::try_from(track_count).unwrap_or(usize::MAX);
    plural(
        "Show all {count} track \u{203a}",
        "Show all {count} tracks \u{203a}",
        track_count,
        &[("count", &track_count.to_string())],
    )
}
