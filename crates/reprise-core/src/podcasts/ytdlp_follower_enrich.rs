//! Optional subscriber-count enrichment for discovered YouTube channels.

use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::{PodcastError, YtDlp, YtDlpChannel};

pub const FOLLOWER_ENRICHMENT_WORKERS: usize = 4;
pub const FOLLOWER_ENRICHMENT_BUDGET: Duration = Duration::from_secs(20);
pub const FOLLOWER_ENRICHMENT_MAX_CHANNELS: usize = 20;

impl YtDlp {
    pub fn channel_follower_count(&self, url: &str) -> Result<Option<u64>, PodcastError> {
        self.channel_follower_count_with_timeout(url, self.timeouts.channel_head)
    }

    fn channel_follower_count_with_timeout(
        &self,
        url: &str,
        timeout: Duration,
    ) -> Result<Option<u64>, PodcastError> {
        let mut arguments = vec![
            OsString::from("--no-warnings"),
            OsString::from("--flat-playlist"),
            OsString::from("-I"),
            OsString::from("0"),
        ];
        self.append_metadata_language(&mut arguments);
        arguments.extend([OsString::from("-J"), OsString::from(url)]);
        let output = self.run("channel_follower_count", arguments, timeout)?;
        let value = serde_json::from_str(&output)
            .map_err(|_| super::response_error("channel_follower_count"))?;
        Ok(super::super::ytdlp_search::entry_follower_count(&value))
    }

    pub fn enrich_follower_counts(&self, channels: &mut [YtDlpChannel], cancelled: &AtomicBool) {
        self.enrich_follower_counts_with_budget(channels, cancelled, FOLLOWER_ENRICHMENT_BUDGET);
    }

    pub(super) fn enrich_follower_counts_with_budget(
        &self,
        channels: &mut [YtDlpChannel],
        cancelled: &AtomicBool,
        budget: Duration,
    ) {
        let tasks = channels
            .iter()
            .enumerate()
            .filter(|(_, channel)| channel.follower_count.is_none())
            .filter(|(_, channel)| {
                let Ok(url) = url::Url::parse(&channel.url) else {
                    return false;
                };
                matches!(url.scheme(), "http" | "https")
                    && super::super::url_detect::is_youtube_url(&url)
            })
            .take(FOLLOWER_ENRICHMENT_MAX_CHANNELS)
            .map(|(index, channel)| (index, channel.url.clone()))
            .collect::<Vec<_>>();
        let worker_count = tasks.len().min(FOLLOWER_ENRICHMENT_WORKERS);
        let next = AtomicUsize::new(0);
        let deadline = Instant::now() + budget;
        let (sender, receiver) = mpsc::channel();

        let completed = std::thread::scope(|scope| {
            for _ in 0..worker_count {
                let sender = sender.clone();
                let next = &next;
                let tasks = &tasks;
                scope.spawn(move || loop {
                    if cancelled.load(Ordering::Acquire) {
                        break;
                    }
                    let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                        break;
                    };
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some((channel_index, url)) = tasks.get(index) else {
                        break;
                    };
                    // Catch here rather than at the scope boundary: a worker
                    // panic resumed by `scope` would discard the successful
                    // counts already returned by the other channels. One bad
                    // provider response must cost one optional count only.
                    let follower_count =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            self.channel_follower_count_with_timeout(
                                url,
                                self.timeouts.channel_head.min(remaining),
                            )
                        }))
                        .ok()
                        .and_then(Result::ok)
                        .flatten();
                    if sender.send((*channel_index, follower_count)).is_err() {
                        break;
                    }
                });
            }
            drop(sender);
            receiver.into_iter().collect::<Vec<_>>()
        });

        for (index, follower_count) in completed {
            channels[index].follower_count = follower_count;
        }
    }
}
