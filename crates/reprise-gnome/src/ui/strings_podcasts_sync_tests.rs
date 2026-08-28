//! Initial-sync copy tests split from the main podcast string inventory.

use super::*;

#[test]
fn pod_26_sync_progress_names_the_source_and_live_episode_count() {
    assert_eq!(PODCAST_SYNC_ADDED, "Podcast added");
    assert_eq!(YOUTUBE_SYNC_ADDED, "Channel added");
    assert_eq!(podcast_sync_reading(0), "Reading feed");
    assert_eq!(podcast_sync_reading(1), "Reading feed — 1 episode");
    assert_eq!(podcast_sync_reading(23), "Reading feed — 23 episodes");
    assert_eq!(PODCAST_SYNC_ARTWORK, "Fetching artwork");
    assert_eq!(PODCAST_SYNC_FAILED, "Couldn't read feed");
    assert_eq!(PODCAST_SYNC_RETRY, "Retry");
}
