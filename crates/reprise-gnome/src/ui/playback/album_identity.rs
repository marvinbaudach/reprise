//! Borrow-safe loaded metadata identities shared by universal navigation.

use super::player_controller::PlayerController;

impl PlayerController {
    /// Stable identity for the loaded track even while it is filtered out.
    pub fn current_track_id(&self) -> Option<i64> {
        self.now_playing.borrow().as_ref().map(|track| track.id)
    }

    /// The effective album artist of the loaded track, or `None` when no
    /// track is loaded or the resolved artist is blank. The Artists view
    /// groups by this value rather than by the display track artist.
    pub fn current_track_album_artist(&self) -> Option<String> {
        let now_playing = self.now_playing.borrow();
        let track = now_playing.as_ref()?;
        let effective = if track.album_artist.trim().is_empty() {
            &track.artist
        } else {
            &track.album_artist
        };
        (!effective.trim().is_empty()).then(|| effective.trim().to_string())
    }

    /// Clone-out album identity for album-card toggle and reveal decisions.
    pub fn current_album_identity(&self) -> Option<(String, String)> {
        let (album, track_artist) = {
            let now_playing = self.now_playing.borrow();
            let track = now_playing.as_ref()?;
            (track.album.clone(), track.artist.clone())
        };
        // A track removed from the database can still be loaded. Its complete
        // player-owned snapshot keeps album navigation deterministic.
        let album_artist = self.current_track_album_artist().unwrap_or(track_artist);
        Some((album, album_artist))
    }

    /// Effective artist identity for universal metadata navigation.
    pub fn current_artist_identity(&self) -> Option<String> {
        let display_artist = self
            .now_playing
            .borrow()
            .as_ref()
            .map(|track| track.artist.clone())?;
        Some(self.current_track_album_artist().unwrap_or(display_artist))
    }
}
