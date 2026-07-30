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
    device_sync: &Rc<crate::ui::device_sync_runtime::DeviceSyncRuntime>,
) -> SourceViews {
    let callbacks = crate::ui::podcasts::PodcastsCallbacks::new(
        {
            let player = player.map(Rc::downgrade);
            move |episode| {
                if let Some(player) = player.as_ref().and_then(std::rc::Weak::upgrade) {
                    player.play_podcast_episode(&episode);
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
    podcasts.bind_device_sync(device_sync);
    let radio = Rc::new(crate::ui::radio::install(conn.clone(), player));
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
        let podcasts = Rc::downgrade(&podcasts);
        let youtube = Rc::downgrade(&youtube);
        player.add_on_external_changed(move |snapshot| {
            let episode_id = snapshot.and_then(|snapshot| match snapshot.media {
                crate::ui::playback::external_media::ExternalMedia::Podcast {
                    episode_id, ..
                } => Some(episode_id),
                crate::ui::playback::external_media::ExternalMedia::Radio { .. } => None,
            });
            if let Some(view) = podcasts.upgrade() {
                view.set_playing_episode(episode_id);
            }
            if let Some(view) = youtube.upgrade() {
                view.set_playing_episode(episode_id);
            }
        });
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
