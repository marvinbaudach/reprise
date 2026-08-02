use std::rc::Weak;

use reprise_core::up_next::QueueItem;

use crate::ui::playback::external_media_state::{ExternalSession, PodcastOrigin};
use crate::ui::playback::preview::PlaybackMode;
use crate::ui::player_controller::PlayerController;
use crate::ui::track_list::queue_sections::ContextWindow;

/// Resolves the context tail through live GTK playback state while keeping
/// the shared view model data-only and closure-free.
#[derive(Clone, Default)]
pub(in crate::ui) struct QueueContextWindow {
    player: Weak<PlayerController>,
}

impl QueueContextWindow {
    pub(in crate::ui) fn from_player(player: Weak<PlayerController>) -> Self {
        Self { player }
    }
}

impl ContextWindow for QueueContextWindow {
    fn rows(&self, offset: usize, limit: usize) -> Vec<QueueItem> {
        self.player.upgrade().map_or_else(Vec::new, |player| {
            if player.playback_mode() == PlaybackMode::Podcast {
                let external = player.external.borrow();
                let Some(ExternalSession::Podcast(session)) = external.session.as_ref() else {
                    return Vec::new();
                };
                if session.origin != PodcastOrigin::Direct {
                    return Vec::new();
                }
                return session
                    .neighbours
                    .as_ref()
                    .map_or_else(Vec::new, |neighbours| {
                        neighbours
                            .upcoming()
                            .iter()
                            .skip(offset)
                            .take(limit)
                            .copied()
                            .collect()
                    });
            }
            player
                .queue
                .borrow()
                .remaining_window(offset, limit)
                .into_iter()
                .map(QueueItem::Track)
                .collect()
        })
    }
}
