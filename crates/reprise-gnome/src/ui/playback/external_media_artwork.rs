//! External source artwork projection for the full-width player bar.

use crate::ui::player_controller::PlayerController;

use super::external_media::ExternalPlaybackSnapshot;

impl PlayerController {
    pub(super) fn sync_external_bar_artwork(&self, snapshot: Option<&ExternalPlaybackSnapshot>) {
        let Some(snapshot) = snapshot else {
            return;
        };
        let generation = self.bar_cover_generation.get().wrapping_add(1);
        self.bar_cover_generation.set(generation);
        let size = self.bar.cover_image().pixel_size().max(1);
        let images_allowed = reprise_core::online_sources::network_allowed(
            &self.conn,
            &reprise_core::modules::ARTWORK_MODULE,
        )
        .unwrap_or(false);
        crate::ui::podcasts::source_image::load_into_image(
            self.bar.cover_image(),
            snapshot.art_url.as_deref(),
            snapshot.fallback_art_url.as_deref(),
            (size, size),
            images_allowed,
            reprise_core::remote_image::CacheScope::Persistent,
            snapshot.restored,
            generation,
            &self.bar_cover_generation,
        );
    }
}
