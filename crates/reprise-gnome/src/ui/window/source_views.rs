//! Composition helper for the Podcasts and Radio source views.

use std::rc::Rc;

use reprise_core::db::Db;

use super::player_controller::PlayerController;
use super::sidebar::Sidebar;

pub(in crate::ui) struct SourceViews {
    pub(in crate::ui) podcasts: Rc<crate::ui::podcasts::PodcastsView>,
    pub(in crate::ui) youtube: Rc<crate::ui::podcasts::PodcastsView>,
    pub(in crate::ui) radio: Rc<crate::ui::radio::RadioView>,
}

impl SourceViews {
    pub(in crate::ui) fn wire_episode_played(
        &self,
        player: &Rc<PlayerController>,
        sidebar: &Rc<Sidebar>,
    ) {
        let sidebar = Rc::downgrade(sidebar);
        let views = [Rc::downgrade(&self.podcasts), Rc::downgrade(&self.youtube)];
        player.add_on_episode_played(move |episode_id| {
            if let Some(sidebar) = sidebar.upgrade() {
                sidebar.refresh("episode played");
            }
            for view in views.iter().filter_map(std::rc::Weak::upgrade) {
                view.update_played_state(episode_id);
            }
        });
    }

    pub(in crate::ui) fn wire_episode_position(&self, player: &Rc<PlayerController>) {
        let views = [Rc::downgrade(&self.podcasts), Rc::downgrade(&self.youtube)];
        player.add_on_episode_position(move |episode_id, position_ms| {
            for view in views.iter().filter_map(std::rc::Weak::upgrade) {
                view.update_position_state(episode_id, position_ms);
            }
        });
    }

    pub(in crate::ui) fn set_toast_overlay(&self, overlay: &libadwaita::ToastOverlay) {
        self.podcasts.set_toast_overlay(overlay);
        self.youtube.set_toast_overlay(overlay);
        self.radio.set_toast_overlay(overlay);
    }

    pub(in crate::ui) fn into_parts(
        self,
    ) -> (
        Rc<crate::ui::podcasts::PodcastsView>,
        Rc<crate::ui::podcasts::PodcastsView>,
        Rc<crate::ui::radio::RadioView>,
    ) {
        (self.podcasts, self.youtube, self.radio)
    }
}

pub(in crate::ui) fn install(
    conn: &Rc<Db>,
    podcasts_runtime: &Rc<crate::ui::podcasts::PodcastsRuntime>,
    player: Option<&Rc<PlayerController>>,
    sidebar: &Rc<Sidebar>,
    content_stack: &gtk4::Stack,
    location_broadcast: &Rc<crate::ui::location_broadcast::LocationBroadcast>,
) -> SourceViews {
    let callbacks = crate::ui::podcasts::PodcastsCallbacks::new(
        {
            let player = player.map(Rc::downgrade);
            move |episode, episode_ids| {
                if let Some(player) = player.as_ref().and_then(std::rc::Weak::upgrade) {
                    player.play_podcast_episode(&episode, &episode_ids);
                } else {
                    tracing::warn!(
                        episode_id = episode.id,
                        "podcast playback unavailable: player backend is not running"
                    );
                }
            }
        },
        {
            let player = player.map(Rc::downgrade);
            move || {
                if let Some(player) = player.as_ref().and_then(std::rc::Weak::upgrade) {
                    player.toggle_pause();
                } else {
                    tracing::warn!("podcast playback unavailable: player backend is not running");
                }
            }
        },
        {
            let player = player.map(Rc::downgrade);
            move |subscription_id| {
                if let Some(player) = player.as_ref().and_then(std::rc::Weak::upgrade) {
                    player.stop_podcast_subscription(subscription_id);
                }
            }
        },
        {
            let sidebar = Rc::downgrade(sidebar);
            move || {
                if let Some(sidebar) = sidebar.upgrade() {
                    sidebar.refresh("podcasts changed");
                }
            }
        },
        {
            let player = player.map(Rc::downgrade);
            move |items| {
                let Some(player) = player.as_ref().and_then(std::rc::Weak::upgrade) else {
                    return false;
                };
                player.play_next_items(items);
                true
            }
        },
        {
            let player = player.map(Rc::downgrade);
            move |items| {
                let Some(player) = player.as_ref().and_then(std::rc::Weak::upgrade) else {
                    return false;
                };
                player.append_queue_items(items) > 0
            }
        },
    );
    let podcasts = crate::ui::podcasts::install(
        conn.clone(),
        podcasts_runtime.clone(),
        callbacks.clone(),
        reprise_core::podcasts::PodcastKind::Rss,
    );
    let youtube = crate::ui::podcasts::install(
        conn.clone(),
        podcasts_runtime.clone(),
        callbacks,
        reprise_core::podcasts::PodcastKind::Youtube,
    );
    let radio = Rc::new(crate::ui::radio::install(
        conn.clone(),
        player,
        location_broadcast,
    ));
    content_stack.add_named(podcasts.root(), Some("podcasts"));
    content_stack.add_named(youtube.root(), Some("youtube"));
    content_stack.add_named(radio.root(), Some("radio"));

    {
        let sidebar = Rc::downgrade(sidebar);
        radio.set_on_mutated(move || {
            if let Some(sidebar) = sidebar.upgrade() {
                sidebar.refresh("radio favorites changed");
            }
        });
    }
    if let Some(player) = player {
        let podcasts_marker = Rc::downgrade(&podcasts);
        let youtube_marker = Rc::downgrade(&youtube);
        player.add_on_external_changed(move |snapshot| {
            let episode_mark = crate::ui::podcasts::episode_mark_from_snapshot(snapshot.as_ref());
            let restored = snapshot.as_ref().is_some_and(|snapshot| snapshot.restored);
            let unavailable_episode = snapshot.as_ref().and_then(|snapshot| {
                if snapshot.podcast_phase
                    == Some(crate::ui::playback::external_media::PodcastPhase::Failed)
                {
                    episode_mark.map(|mark| mark.id)
                } else {
                    None
                }
            });
            if let Some(view) = podcasts_marker.upgrade() {
                view.set_playing_episode(episode_mark, restored);
                view.set_unavailable_episode(unavailable_episode);
            }
            if let Some(view) = youtube_marker.upgrade() {
                view.set_playing_episode(episode_mark, restored);
                view.set_unavailable_episode(unavailable_episode);
            }
            tracing::debug!(
                episode_id = ?episode_mark.map(|mark| mark.id),
                phase = ?snapshot.as_ref().and_then(|snapshot| snapshot.podcast_phase),
                can_go_previous =
                    snapshot.as_ref().is_some_and(|snapshot| snapshot.can_go_previous),
                can_go_next = snapshot.as_ref().is_some_and(|snapshot| snapshot.can_go_next),
                "external session changed"
            );
        });

        let podcasts = Rc::downgrade(&podcasts);
        let youtube = Rc::downgrade(&youtube);
        player.add_on_episode_download_state(move |episode_id, state| {
            if let Some(view) = podcasts.upgrade() {
                view.set_download_state(episode_id, &state);
            }
            if let Some(view) = youtube.upgrade() {
                view.set_download_state(episode_id, &state);
            }
        });
    }
    super::source_views_smoke::arm_episode_play(&youtube);
    super::source_views_smoke::arm_episode_play(&podcasts);
    if let Some(player) = player {
        super::source_views_smoke::arm_transport(player);
    }

    SourceViews {
        podcasts,
        youtube,
        radio,
    }
}

pub(in crate::ui) fn wire_update_sidebar_refresh(
    concerts: &Rc<crate::ui::concerts::ConcertsView>,
    releases: &Rc<crate::ui::releases::ReleasesView>,
    sidebar: &Rc<Sidebar>,
) {
    {
        let sidebar = Rc::downgrade(sidebar);
        concerts.set_on_refreshed(move || {
            if let Some(sidebar) = sidebar.upgrade() {
                sidebar.refresh("concerts view refreshed");
            }
        });
    }
    {
        let sidebar = Rc::downgrade(sidebar);
        releases.set_on_refreshed(move || {
            if let Some(sidebar) = sidebar.upgrade() {
                sidebar.refresh("releases view refreshed");
            }
        });
    }
}
