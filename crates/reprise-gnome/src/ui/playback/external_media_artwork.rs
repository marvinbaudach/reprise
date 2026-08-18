//! External source artwork projection for the player bar and compact player.

use crate::ui::player_controller::PlayerController;

use super::external_media::ExternalPlaybackSnapshot;

impl PlayerController {
    pub(super) fn sync_external_artwork(&self, snapshot: Option<&ExternalPlaybackSnapshot>) {
        let Some(snapshot) = snapshot else {
            return;
        };
        let images_allowed = reprise_core::online_sources::network_allowed(
            &self.conn,
            &reprise_core::modules::ARTWORK_MODULE,
        )
        .unwrap_or(false);
        let startup_timing = if snapshot.restored {
            crate::ui::podcasts::source_image::StartupTiming::AfterQuiet
        } else {
            crate::ui::podcasts::source_image::StartupTiming::Immediate
        };

        let bar_generation = self.bar_cover_generation.get().wrapping_add(1);
        self.bar_cover_generation.set(bar_generation);
        let bar_size = self.bar.cover_image().pixel_size().max(1);
        crate::ui::podcasts::source_image::load_into_image(
            self.bar.cover_image(),
            crate::ui::podcasts::source_image::ArtworkRequest::new(
                snapshot.art_url.as_deref(),
                snapshot.fallback_art_url.as_deref(),
                (bar_size, bar_size),
                images_allowed,
                reprise_core::remote_image::CacheScope::Persistent,
                startup_timing,
            ),
            bar_generation,
            &self.bar_cover_generation,
        );

        let compact_generation = self.compact_cover_generation.get().wrapping_add(1);
        self.compact_cover_generation.set(compact_generation);
        let compact_cover = self.compact_player.cover_image();
        let previous_compact_paintable = compact_cover.paintable();
        let compact_size = compact_cover.pixel_size().max(1);
        crate::ui::podcasts::source_image::load_into_image(
            compact_cover,
            crate::ui::podcasts::source_image::ArtworkRequest::new(
                snapshot.art_url.as_deref(),
                snapshot.fallback_art_url.as_deref(),
                (compact_size, compact_size),
                images_allowed,
                reprise_core::remote_image::CacheScope::Persistent,
                startup_timing,
            ),
            compact_generation,
            &self.compact_cover_generation,
        );
        // The shared source-image loader clears its target before an async
        // cache miss. Compact mode has no crossfade, so keep the current cover
        // visible until the replacement arrives. A synchronous cache hit has
        // already installed its paintable and must not be overwritten here.
        if compact_cover.paintable().is_none() {
            if let Some(previous_compact_paintable) = previous_compact_paintable {
                compact_cover.set_paintable(Some(&previous_compact_paintable));
            }
        }
    }
}
