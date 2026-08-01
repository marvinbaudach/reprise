use std::path::Path;

use reprise_core::models::Track;
use reprise_core::view_source::ViewSource;

pub fn target_for_import(playlist_id: i64) -> ViewSource {
    ViewSource::Playlist(playlist_id)
}

/// Derives the imported playlist name from the M3U file stem, falling back
/// to the already-rendered generic name when the stem is missing or blank.
pub fn playlist_name_from_file(file_path: &Path, rendered_fallback: &str) -> String {
    file_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|stem| !stem.is_empty())
        .unwrap_or(rendered_fallback)
        .to_string()
}

/// Returns `"Artist - Title"`, or just the title when the artist is blank.
pub fn display_name(track: &Track) -> String {
    if track.artist.trim().is_empty() {
        track.title.clone()
    } else {
        format!("{} - {}", track.artist, track.title)
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use reprise_core::models::Track;
    use reprise_core::view_source::ViewSource;

    #[test]
    fn successful_import_selects_the_created_playlist_source() {
        assert_eq!(super::target_for_import(42), ViewSource::Playlist(42));
    }

    #[test]
    fn imported_playlist_name_uses_the_stem_or_the_rendered_fallback() {
        assert_eq!(
            super::playlist_name_from_file(Path::new("/x/Road Trip.m3u"), "Imported playlist"),
            "Road Trip"
        );
        assert_eq!(
            super::playlist_name_from_file(Path::new("/"), "Imported playlist"),
            "Imported playlist"
        );
    }

    #[test]
    fn exported_track_name_uses_artist_and_title_or_title_alone() {
        let mut track = sample_track();
        track.artist = "Some Artist".to_owned();
        track.title = "Some Title".to_owned();
        assert_eq!(super::display_name(&track), "Some Artist - Some Title");

        track.artist = "  ".to_owned();
        assert_eq!(super::display_name(&track), "Some Title");
    }

    fn sample_track() -> Track {
        Track {
            id: 1,
            path: "/x/a.flac".to_owned(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            album_artist: String::new(),
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
            missing_since: None,
            missing_reason: None,
            untagged: false,
            file_size: 0,
            device: None,
            inode: None,
            playlist_position: None,
            is_ai: false,
        }
    }
}
