//! Action toast for the next unplayed podcast episode.

use std::rc::Rc;

use libadwaita as adw;
use reprise_core::podcasts::EpisodeRow;

use crate::ui::player_controller::PlayerController;

use super::external_media::{EpisodeSource, ExternalMedia};

impl PlayerController {
    pub(super) fn show_play_next_offer(self: &Rc<Self>, episode: &EpisodeRow) {
        let Some(overlay) = self.toast_overlay.upgrade() else {
            tracing::warn!(
                episode_id = episode.id,
                "toast overlay is gone; retaining persistent play-next offer"
            );
            return;
        };
        let toast = adw::Toast::new(&crate::ui::strings::podcast_play_next(&episode.title));
        toast.set_button_label(Some(crate::ui::strings::PODCAST_PLAY));
        toast.set_timeout(10);
        toast.set_priority(adw::ToastPriority::High);
        let controller = Rc::downgrade(self);
        let media = media_from_episode(episode);
        toast.connect_button_clicked(move |_| {
            let Some(controller) = controller.upgrade() else {
                return;
            };
            if let Err(error) = controller.play_external(media.clone()) {
                controller.show_toast(&error.to_string());
            }
        });
        overlay.add_toast(toast);
    }
}

pub(super) fn media_from_episode(episode: &EpisodeRow) -> ExternalMedia {
    let source = episode.downloaded_path.clone().map_or_else(
        || EpisodeSource::Url(episode.audio_url.clone()),
        EpisodeSource::File,
    );
    ExternalMedia::Podcast {
        episode_id: episode.id,
        title: episode.title.clone(),
        show: episode.show.clone(),
        source,
        resume_ms: episode.position_ms,
        duration_ms: episode.duration_secs.map(|seconds| seconds * 1_000),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::podcasts::PodcastKind;

    #[test]
    fn play_next_prefers_a_downloaded_episode() {
        let episode = EpisodeRow {
            id: 7,
            subscription_id: 2,
            guid: "episode-7".into(),
            title: "Next".into(),
            show: "Show".into(),
            show_image_url: None,
            image_url: None,
            kind: PodcastKind::Rss,
            audio_url: "https://example.test/next.mp3".into(),
            page_url: None,
            published_at: Some(20),
            duration_secs: Some(60),
            downloaded_path: Some("/data/next.mp3".into()),
            downloaded_bytes: Some(1_024),
            played_at: None,
            position_ms: 5_000,
            first_seen_at: 10,
        };

        assert!(matches!(
            media_from_episode(&episode),
            ExternalMedia::Podcast {
                source: EpisodeSource::File(path),
                duration_ms: Some(60_000),
                ..
            } if path == "/data/next.mp3"
        ));
    }
}
