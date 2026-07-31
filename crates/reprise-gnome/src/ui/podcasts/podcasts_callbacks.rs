//! Callback boundary between the Podcasts surface and its window-owned
//! playback/sidebar collaborators.

use std::rc::Rc;

use reprise_core::podcasts::EpisodeRow;
use reprise_core::up_next::QueueItem;

type OnEpisodeActivated = Rc<dyn Fn(EpisodeRow, Vec<i64>)>;
type OnPlayPause = Rc<dyn Fn()>;
type OnSubscriptionRemoved = Rc<dyn Fn(i64)>;
type OnSidebarRefresh = Rc<dyn Fn()>;
type OnQueueItems = Rc<dyn Fn(&[QueueItem]) -> bool>;

#[derive(Clone)]
pub(in crate::ui) struct PodcastsCallbacks {
    pub(super) on_episode_activated: OnEpisodeActivated,
    pub(super) on_play_pause: OnPlayPause,
    pub(super) on_subscription_removed: OnSubscriptionRemoved,
    pub(super) on_sidebar_refresh: OnSidebarRefresh,
    pub(super) on_play_next: OnQueueItems,
    pub(super) on_add_to_queue: OnQueueItems,
}

impl Default for PodcastsCallbacks {
    fn default() -> Self {
        Self {
            on_episode_activated: Rc::new(|_, _| {}),
            on_play_pause: Rc::new(|| {}),
            on_subscription_removed: Rc::new(|_| {}),
            on_sidebar_refresh: Rc::new(|| {}),
            on_play_next: Rc::new(|_| false),
            on_add_to_queue: Rc::new(|_| false),
        }
    }
}

impl PodcastsCallbacks {
    pub(in crate::ui) fn new(
        on_episode_activated: impl Fn(EpisodeRow, Vec<i64>) + 'static,
        on_play_pause: impl Fn() + 'static,
        on_subscription_removed: impl Fn(i64) + 'static,
        on_sidebar_refresh: impl Fn() + 'static,
        on_play_next: impl Fn(&[QueueItem]) -> bool + 'static,
        on_add_to_queue: impl Fn(&[QueueItem]) -> bool + 'static,
    ) -> Self {
        Self {
            on_episode_activated: Rc::new(on_episode_activated),
            on_play_pause: Rc::new(on_play_pause),
            on_subscription_removed: Rc::new(on_subscription_removed),
            on_sidebar_refresh: Rc::new(on_sidebar_refresh),
            on_play_next: Rc::new(on_play_next),
            on_add_to_queue: Rc::new(on_add_to_queue),
        }
    }
}
