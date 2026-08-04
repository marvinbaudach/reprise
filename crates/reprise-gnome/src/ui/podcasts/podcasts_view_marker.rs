//! Which episode the player holds, and how that reaches the rows.
//!
//! Split out of `podcasts_view.rs` to keep it under the file-size gate.

use super::*;
use crate::ui::podcasts::podcasts_playback::episode_mark_requires_render;
use crate::ui::podcasts::podcasts_reveal;
use crate::ui::source_reveal::{self, LoadedItemChange, RevealPolicy};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RevealRequest {
    Episode(i64),
    #[allow(dead_code)] // Constructed by the source-view entry point in AP7.
    Channel(i64),
}

impl PodcastsView {
    pub(in crate::ui) fn set_playing_episode(&self, mark: Option<EpisodeMark>, restored: bool) {
        let previous = self.playing_episode.replace(mark);
        if !episode_mark_requires_render(previous, mark) {
            if previous != mark {
                self.restyle_playing_episode(mark);
            }
            return;
        }
        self.render();
        let change = if restored {
            LoadedItemChange::SessionRestore
        } else if self.activating_here.get() {
            LoadedItemChange::ActivatedHere
        } else {
            LoadedItemChange::ChangedElsewhere
        };
        self.reveal_loaded_episode(change);
    }

    pub(super) fn install_reveal_tracking(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.scroller.vadjustment().connect_value_changed(move |_| {
            if let Some(view) = weak.upgrade() {
                view.last_scroll_activity
                    .set(Some(std::time::Instant::now()));
            }
        });

        let weak = Rc::downgrade(self);
        self.root.connect_map(move |_| {
            if let Some(view) = weak.upgrade() {
                view.reveal_loaded_episode(LoadedItemChange::ViewEntered);
            }
        });
    }

    /// `SRC-13`: expands and centers the loaded episode without changing
    /// focus or selection. `START-3` is the single cold-start exception: it
    /// restores this episode as the sole selection before centering it.
    fn reveal_loaded_episode(&self, change: LoadedItemChange) {
        let Some(mark) = self.playing_episode.get() else {
            return;
        };
        self.reveal(RevealRequest::Episode(mark.id), change);
    }

    pub(super) fn reveal(&self, request: RevealRequest, change: LoadedItemChange) {
        let user_scrolling = source_reveal::is_user_scrolling(self.last_scroll_activity.get());
        if source_reveal::reveal_policy(change, user_scrolling) == RevealPolicy::MarkerOnly {
            return;
        }
        let groups = self.groups.borrow().clone();
        let download_states = self.download_states.borrow().clone();
        let rendered_groups =
            rendered_source_groups(&groups, &self.filter_bar.filter(), &download_states);
        let rendered_groups = rendered_groups
            .into_iter()
            .map(|rendered| rendered.group)
            .collect::<Vec<_>>();
        let target = match request {
            RevealRequest::Episode(episode_id) => {
                let window_expanded = {
                    let expanded = self.expanded_episode_sources.borrow();
                    let Some(target) =
                        podcasts_reveal::reveal_target(&rendered_groups, episode_id, false)
                    else {
                        return;
                    };
                    expanded.contains(&target.subscription_id)
                };
                podcasts_reveal::reveal_target(&rendered_groups, episode_id, window_expanded)
            }
            RevealRequest::Channel(subscription_id) => {
                podcasts_reveal::channel_reveal_target(&rendered_groups, subscription_id)
            }
        };
        let Some(target) = target else {
            return;
        };
        let mut structure_changed = self
            .expanded_sources
            .borrow_mut()
            .insert(target.subscription_id);
        if target.needs_full_window {
            structure_changed |= self
                .expanded_episode_sources
                .borrow_mut()
                .insert(target.subscription_id);
        }
        if structure_changed {
            self.render();
        }
        if change == LoadedItemChange::SessionRestore {
            if let RevealRequest::Episode(episode_id) = request {
                self.select_row(episode_id, SelectMode::Only);
            }
        }
        let row = match request {
            RevealRequest::Episode(episode_id) => self
                .download_widgets
                .borrow()
                .get(&episode_id)
                .map(|widgets| widgets.root.clone().upcast::<gtk4::Widget>()),
            RevealRequest::Channel(subscription_id) => self
                .channel_widgets
                .borrow()
                .get(&subscription_id)
                .map(|widgets| widgets.header.clone()),
        };
        let Some(row) = row else {
            return;
        };
        podcasts_reveal::center_row(&self.scroller, &row, &self.reveal_animation);
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
