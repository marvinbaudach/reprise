use reprise_core::podcasts::PodcastKind;

use crate::ui::player_controller::PlayerController;

use super::external_media_state::{ExternalMedia, ExternalSession};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::ui) enum LoadedSourceItem {
    Episode {
        subscription_id: i64,
        episode_id: i64,
        kind: PodcastKind,
    },
    Station {
        station_id: i64,
    },
}

/// Pure projection, so the mapping is testable without a controller.
pub(super) fn loaded_source_item(session: Option<&ExternalSession>) -> Option<LoadedSourceItem> {
    match session? {
        ExternalSession::Podcast(session) => {
            // Every current GTK producer builds podcast media from an
            // `EpisodeRow`, so zero is not user-reachable today. Keep the
            // guard because `play_external` accepts an identity whose row can
            // be absent, and the unwired runtime/D-Bus path accepts opaque
            // external identities that a later cut-over must validate too.
            if session.subscription_id <= 0 {
                return None;
            }
            let ExternalMedia::Podcast { episode_id, .. } = session.media else {
                return None;
            };
            Some(LoadedSourceItem::Episode {
                subscription_id: session.subscription_id,
                episode_id,
                kind: session.kind,
            })
        }
        ExternalSession::Radio(session) => {
            let ExternalMedia::Radio { station_id, .. } = session.media else {
                return None;
            };
            Some(LoadedSourceItem::Station { station_id })
        }
    }
}

impl PlayerController {
    #[allow(dead_code)] // Wired to all player-bar surfaces in AP9.
    pub(in crate::ui) fn current_source_item(&self) -> Option<LoadedSourceItem> {
        loaded_source_item(self.external.borrow().session.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use reprise_core::podcasts::PodcastKind;

    use super::super::external_media_state::{
        ExternalMedia, ExternalSession, PodcastOrigin, PodcastPhase, PodcastSession,
        RadioPresentation, RadioSession, ResumePolicy,
    };
    use super::{loaded_source_item, LoadedSourceItem};

    fn podcast_session(
        kind: PodcastKind,
        origin: PodcastOrigin,
        subscription_id: i64,
        restored: bool,
    ) -> ExternalSession {
        ExternalSession::Podcast(PodcastSession {
            media: ExternalMedia::Podcast {
                episode_id: 7,
                title: "Episode".into(),
                show: "Show".into(),
                source: super::super::external_media_state::EpisodeSource::Url(
                    "https://example.test/episode.mp3".into(),
                ),
                resume_ms: 0,
                duration_ms: None,
            },
            neighbours: None,
            automatic_advance: None,
            subscription_id,
            kind,
            published_at: None,
            art_url: None,
            phase: PodcastPhase::Playing,
            restored,
            origin,
            resume: ResumePolicy::new(0),
            position_ms: 0,
            last_persisted_ms: 0,
            duration_known: false,
            error: None,
        })
    }

    fn radio_session() -> ExternalSession {
        ExternalSession::Radio(RadioSession {
            media: ExternalMedia::Radio {
                station_id: 9,
                name: "Station".into(),
                stream_url: "https://example.test/radio".into(),
                uuid: None,
            },
            art_url: None,
            presentation: RadioPresentation::connected(),
            retry_guard: reprise_core::radio::click::ReresolveGuard::default(),
        })
    }

    #[test]
    fn browse_4_library_playback_has_no_loaded_source_item() {
        assert_eq!(loaded_source_item(None), None);
    }

    #[test]
    fn browse_4_an_rss_session_reports_its_subscription_and_kind() {
        let session = podcast_session(PodcastKind::Rss, PodcastOrigin::Direct, 42, false);
        assert_eq!(
            loaded_source_item(Some(&session)),
            Some(LoadedSourceItem::Episode {
                subscription_id: 42,
                episode_id: 7,
                kind: PodcastKind::Rss,
            })
        );
    }

    #[test]
    fn browse_4_a_youtube_session_reports_the_youtube_kind() {
        let session = podcast_session(PodcastKind::Youtube, PodcastOrigin::Direct, 42, false);
        assert_eq!(
            loaded_source_item(Some(&session)),
            Some(LoadedSourceItem::Episode {
                subscription_id: 42,
                episode_id: 7,
                kind: PodcastKind::Youtube,
            })
        );
    }

    #[test]
    fn browse_4_a_queued_episode_is_the_same_source_item_as_a_direct_one() {
        let direct = podcast_session(PodcastKind::Rss, PodcastOrigin::Direct, 42, false);
        let queued = podcast_session(PodcastKind::Rss, PodcastOrigin::ManualQueue, 42, false);
        assert_eq!(
            loaded_source_item(Some(&direct)),
            loaded_source_item(Some(&queued))
        );
    }

    #[test]
    fn browse_4_a_radio_session_reports_its_station() {
        let session = radio_session();
        assert_eq!(
            loaded_source_item(Some(&session)),
            Some(LoadedSourceItem::Station { station_id: 9 })
        );
    }

    #[test]
    fn browse_4_a_restored_session_keeps_its_kind() {
        let session = podcast_session(PodcastKind::Youtube, PodcastOrigin::Direct, 42, true);
        assert_eq!(
            loaded_source_item(Some(&session)),
            Some(LoadedSourceItem::Episode {
                subscription_id: 42,
                episode_id: 7,
                kind: PodcastKind::Youtube,
            })
        );
    }

    #[test]
    fn browse_4_a_session_without_a_subscription_has_no_source_item() {
        let session = podcast_session(PodcastKind::Rss, PodcastOrigin::Direct, 0, false);
        assert_eq!(loaded_source_item(Some(&session)), None);
    }
}
