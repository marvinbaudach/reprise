//! Composition helper for the deferred Podcasts and Radio source views.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::db::Db;

use super::player_controller::PlayerController;
use super::sidebar::Sidebar;

pub(in crate::ui) struct SourceViews {
    pub(in crate::ui) podcasts:
        super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    pub(in crate::ui) youtube:
        super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
    pub(in crate::ui) radio: super::content_stack::DeferredPage<crate::ui::radio::RadioView>,
}

impl SourceViews {
    /// SRC-1a: no sidebar refresh here. The source counters name how many
    /// shows and channels are subscribed, and playing an episode moves
    /// neither — only subscribing does, and that path refreshes on its own.
    pub(in crate::ui) fn wire_episode_played(&self, player: &Rc<PlayerController>) {
        let views = [self.podcasts.clone(), self.youtube.clone()];
        player.add_on_episode_played(move |episode_id| {
            for page in &views {
                page.if_materialized(|view| view.update_played_state(episode_id));
            }
        });
    }

    pub(in crate::ui) fn wire_episode_position(&self, player: &Rc<PlayerController>) {
        let views = [self.podcasts.clone(), self.youtube.clone()];
        player.add_on_episode_position(move |episode_id, position_ms| {
            for page in &views {
                page.if_materialized(|view| view.update_position_state(episode_id, position_ms));
            }
        });
    }

    pub(in crate::ui) fn set_toast_overlay(&self, overlay: &libadwaita::ToastOverlay) {
        for page in [&self.podcasts, &self.youtube] {
            let overlay = overlay.downgrade();
            page.on_materialized(move |view| {
                if let Some(overlay) = overlay.upgrade() {
                    view.set_toast_overlay(&overlay);
                }
            });
        }
        let overlay = overlay.downgrade();
        self.radio.on_materialized(move |view| {
            if let Some(overlay) = overlay.upgrade() {
                view.set_toast_overlay(&overlay);
            }
        });
    }

    pub(in crate::ui) fn into_parts(
        self,
    ) -> (
        super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
        super::content_stack::DeferredPage<crate::ui::podcasts::PodcastsView>,
        super::content_stack::DeferredPage<crate::ui::radio::RadioView>,
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
    let podcasts = super::content_stack::DeferredPage::install(content_stack, "podcasts", {
        let conn = conn.clone();
        let podcasts_runtime = podcasts_runtime.clone();
        let callbacks = callbacks.clone();
        move || {
            let _measurement = super::startup_report::measure("view.podcasts.construct");
            let view = crate::ui::podcasts::install(
                conn,
                podcasts_runtime,
                callbacks,
                reprise_core::podcasts::PodcastKind::Rss,
            );
            let root = view.root().clone();
            (view, root)
        }
    });
    let youtube = super::content_stack::DeferredPage::install(content_stack, "youtube", {
        let conn = conn.clone();
        let podcasts_runtime = podcasts_runtime.clone();
        move || {
            let _measurement = super::startup_report::measure("view.youtube.construct");
            let view = crate::ui::podcasts::install(
                conn,
                podcasts_runtime,
                callbacks,
                reprise_core::podcasts::PodcastKind::Youtube,
            );
            let root = view.root().clone();
            (view, root)
        }
    });
    let radio = super::content_stack::DeferredPage::install(content_stack, "radio", {
        let conn = conn.clone();
        let player = player.cloned();
        let location_broadcast = location_broadcast.clone();
        move || {
            let _measurement = super::startup_report::measure("view.radio.construct");
            let view = Rc::new(crate::ui::radio::install(
                conn,
                player.as_ref(),
                &location_broadcast,
            ));
            let root = view.root().clone();
            (view, root)
        }
    });

    {
        let sidebar = Rc::downgrade(sidebar);
        radio.on_materialized(move |radio| {
            radio.set_on_mutated(move || {
                if let Some(sidebar) = sidebar.upgrade() {
                    sidebar.refresh("radio favorites changed");
                }
            });
        });
    }
    if let Some(player) = player {
        let latest_snapshot = Rc::new(RefCell::new(None));
        let podcasts_marker = podcasts.clone();
        let youtube_marker = youtube.clone();
        for page in [&podcasts, &youtube] {
            let latest_snapshot = latest_snapshot.clone();
            page.on_materialized(move |view| {
                let snapshot = latest_snapshot.borrow().clone();
                apply_episode_snapshot(view, snapshot.as_ref());
            });
        }
        player.add_on_external_changed(move |snapshot| {
            latest_snapshot.replace(snapshot.clone());
            podcasts_marker.if_materialized(|view| apply_episode_snapshot(view, snapshot.as_ref()));
            youtube_marker.if_materialized(|view| apply_episode_snapshot(view, snapshot.as_ref()));
            let episode_mark = crate::ui::podcasts::episode_mark_from_snapshot(snapshot.as_ref());
            tracing::debug!(
                episode_id = ?episode_mark.map(|mark| mark.id),
                phase = ?snapshot.as_ref().and_then(|snapshot| snapshot.podcast_phase),
                can_go_previous =
                    snapshot.as_ref().is_some_and(|snapshot| snapshot.can_go_previous),
                can_go_next = snapshot.as_ref().is_some_and(|snapshot| snapshot.can_go_next),
                "external session changed"
            );
        });

        let podcasts = podcasts.clone();
        let youtube = youtube.clone();
        player.add_on_episode_download_state(move |episode_id, state| {
            podcasts.if_materialized(|view| view.set_download_state(episode_id, &state));
            youtube.if_materialized(|view| view.set_download_state(episode_id, &state));
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

fn apply_episode_snapshot(
    view: &crate::ui::podcasts::PodcastsView,
    snapshot: Option<&crate::ui::playback::external_media::ExternalPlaybackSnapshot>,
) {
    let episode_mark = crate::ui::podcasts::episode_mark_from_snapshot(snapshot);
    let restored = snapshot.is_some_and(|snapshot| snapshot.restored);
    let unavailable_episode = snapshot.and_then(|snapshot| {
        (snapshot.podcast_phase == Some(crate::ui::playback::external_media::PodcastPhase::Failed))
            .then(|| episode_mark.map(|mark| mark.id))
            .flatten()
    });
    view.set_playing_episode(episode_mark, restored);
    view.set_unavailable_episode(unavailable_episode);
}

pub(in crate::ui) fn wire_update_sidebar_refresh(
    concerts: &super::content_stack::DeferredPage<crate::ui::concerts::ConcertsView>,
    releases: &Rc<crate::ui::releases::ReleasesView>,
    sidebar: &Rc<Sidebar>,
) {
    {
        let sidebar = Rc::downgrade(sidebar);
        concerts.on_materialized(move |concerts| {
            concerts.set_on_refreshed(move || {
                if let Some(sidebar) = sidebar.upgrade() {
                    sidebar.refresh("concerts view refreshed");
                }
            });
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
