//! Which episode the player holds, and how that reaches the rows.
//!
//! Split out of `podcasts_view.rs` to keep it under the file-size gate.

use super::*;
use crate::ui::podcasts::podcasts_playback::episode_mark_requires_render;

impl PodcastsView {
    pub(in crate::ui) fn set_playing_episode(&self, mark: Option<EpisodeMark>) {
        let previous = self.playing_episode.replace(mark);
        if episode_mark_requires_render(previous, mark) {
            self.render();
        } else if previous != mark {
            self.restyle_playing_episode(mark);
        }
    }

    /// A pause or resume of the episode already on screen: only the marker
    /// and its glyph change, so the rows are restyled in place. Rebuilding
    /// here would throw away scroll position and expander state on every
    /// tap of the pause button.
    fn restyle_playing_episode(&self, mark: Option<EpisodeMark>) {
        let Some(mark) = mark else { return };
        let widgets = self.download_widgets.borrow().get(&mark.id).cloned();
        if let Some(widgets) = widgets {
            podcasts_groups::update_playback_state(&widgets, mark.playing);
        }
        self.youtube_detail.update_playback_state(mark);
    }

    pub(in crate::ui) fn set_unavailable_episode(&self, episode_id: Option<i64>) {
        if self.unavailable_episode.replace(episode_id) != episode_id {
            self.render();
        }
    }
}
