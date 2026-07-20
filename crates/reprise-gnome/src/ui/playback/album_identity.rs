//! Borrow-safe loaded-album identity queries shared by artist navigation and GRID-5.

use super::player_controller::PlayerController;

impl PlayerController {
    /// The effective album artist of the loaded track, or `None` when no
    /// track is loaded or the resolved artist is blank. The Artists view
    /// groups by this value rather than by the display track artist.
    pub fn current_track_album_artist(&self) -> Option<String> {
        let id = self.now_playing.borrow().as_ref().map(|track| track.id)?;
        let artist = {
            let conn = self.conn.borrow();
            reprise_core::queries::query_track_album_artist(&conn, id)
                .inspect_err(|error| {
                    tracing::warn!(%error, id, "album-artist deep-link lookup failed");
                })
                .ok()
                .flatten()?
        };
        let trimmed = artist.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    }

    /// Clone-out album identity for album-card toggle and reveal decisions.
    pub fn current_album_identity(&self) -> Option<(String, String)> {
        let (album, track_artist) = {
            let now_playing = self.now_playing.borrow();
            let track = now_playing.as_ref()?;
            (track.album.clone(), track.artist.clone())
        };
        // A track removed from the database can still be loaded. Preserve an
        // identity in that case so GRID-5 attempts the lookup and reaches its
        // required NAV-9b fallback instead of silently becoming a no-op.
        let album_artist = self.current_track_album_artist().unwrap_or(track_artist);
        Some((album, album_artist))
    }
}
