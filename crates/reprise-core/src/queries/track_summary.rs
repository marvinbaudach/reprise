/// The subset of a track's columns the player bar and queue playback path
/// need: the file to hand `Player::play`, display metadata, and the duration
/// play-tracking's 50%-listened check requires
/// (`library::stats::should_count_play`). Deliberately narrower than the
/// full `Track` (no rating/play_count/etc. — the bar doesn't display those),
/// avoiding the cost of loading and holding the columns nothing here reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackSummary {
    pub path: String,
    pub title: String,
    pub artist: String,
    /// Stage 2 Task 6 (MPRIS): feeds `Metadata`'s `xesam:album`. Not used by
    /// the player bar (which only shows title/artist), so it went unused
    /// here until MPRIS needed it.
    pub album: String,
    /// Raw album artist tag (may be empty). Use `effective_album_artist()` to
    /// get the display value that matches `AlbumSummary::album_artist` — i.e.
    /// `album_artist` when non-empty, `artist` otherwise. Loaded alongside the
    /// other summary fields so `notify_now_playing_album_changed` can send the
    /// same effective-artist key the album grid uses for EQ-marker matching.
    pub album_artist: String,
    /// Raw genre and artist MBID are retained by the in-flight playback
    /// snapshot so local listen history remains complete after catalog
    /// deletion.
    pub genre: String,
    pub artist_mbid: Option<String>,
    /// Optional release year displayed by metadata-rich player surfaces.
    pub year: Option<i32>,
    pub duration_ms: i64,
    /// The slice of `path` this track plays, `None` for a whole-file track.
    /// Position and duration are relative to the segment, so a consumer that
    /// plays `path` has to honour it.
    pub segment: Option<crate::models::TrackSegment>,
}

impl TrackSummary {
    /// The `(start_ms, end_ms)` a player cuts out of `path`, `None` for a
    /// whole-file track. The cut ends where the track's own `duration_ms` says:
    /// for the last track of a file that is the length the analysis decoded,
    /// which may differ from the end the sheet and the file's metadata record
    /// (CUE-19). Any other track lasts exactly from its start to its end, and a
    /// track without a duration keeps the recorded end.
    pub fn playback_segment(&self) -> Option<(i64, i64)> {
        self.segment.as_ref().map(|segment| {
            let end_ms = if self.duration_ms > 0 {
                segment.start_ms + self.duration_ms
            } else {
                segment.end_ms
            };
            (segment.start_ms, end_ms)
        })
    }

    /// Returns the effective album artist: `album_artist` when non-empty
    /// (trimmed), `artist` otherwise. Mirrors the SQL expression
    /// `CASE WHEN TRIM(album_artist) <> '' THEN TRIM(album_artist) ELSE
    /// TRIM(artist) END` that `query_albums` uses for `AlbumSummary::
    /// album_artist`, so the two sources always agree on the grouping key.
    pub fn effective_album_artist(&self) -> &str {
        if self.album_artist.trim().is_empty() {
            &self.artist
        } else {
            &self.album_artist
        }
    }
}

#[cfg(test)]
mod tests {
    use super::TrackSummary;
    use crate::models::TrackSegment;

    fn summary(segment: Option<(i64, i64)>, duration_ms: i64) -> TrackSummary {
        TrackSummary {
            path: "/music/album.flac".into(),
            title: "Second".into(),
            artist: String::new(),
            album: String::new(),
            album_artist: String::new(),
            genre: String::new(),
            artist_mbid: None,
            year: None,
            duration_ms,
            segment: segment.map(|(start_ms, end_ms)| TrackSegment {
                index: 2,
                start_ms,
                end_ms,
                cue_path: None,
            }),
        }
    }

    #[test]
    fn cue_19_a_track_the_sheet_ends_where_it_says_is_cut_there() {
        assert_eq!(
            summary(Some((8_000, 14_000)), 6_000).playback_segment(),
            Some((8_000, 14_000))
        );
    }

    #[test]
    fn cue_19_a_track_longer_than_its_recorded_cut_is_cut_at_its_own_length() {
        // The analysis found 20 s in a file whose metadata says 14 s.
        assert_eq!(
            summary(Some((8_000, 14_000)), 12_000).playback_segment(),
            Some((8_000, 20_000))
        );
    }

    #[test]
    fn cue_19_a_track_shorter_than_its_recorded_cut_is_cut_at_its_own_length() {
        assert_eq!(
            summary(Some((8_000, 14_000)), 3_500).playback_segment(),
            Some((8_000, 11_500))
        );
    }

    #[test]
    fn cue_19_a_track_without_a_duration_is_cut_where_the_sheet_ends_it() {
        assert_eq!(
            summary(Some((8_000, 14_000)), 0).playback_segment(),
            Some((8_000, 14_000))
        );
    }

    #[test]
    fn cue_19_a_whole_file_track_has_no_cut() {
        assert_eq!(summary(None, 20_000).playback_segment(), None);
    }
}
