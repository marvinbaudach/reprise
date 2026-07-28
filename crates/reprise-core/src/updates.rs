//! The Updates surface: when its feeds refresh, and what their result says.
//!
//! Reprise shows Concerts and New Releases in one place, and a frontend has to
//! answer the same three questions about them however it renders: may a fetch
//! start now, is a fetch that spans both feeds finished, and what does the
//! badge say. None of that is toolkit-specific, and none of it needs a device,
//! a network or a database, so it lives here rather than in a widget.
//!
//! Fetching itself does not. `concerts::refresh` and `artist_news::refresh`
//! own that, and the platform owns the thread they run on.

/// Which of the two feeds a result belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Feed {
    NewReleases,
    Concerts,
}

/// Whether a feed may start a fetch right now.
///
/// A disabled feed never fetches. A feed that is already fetching does not
/// start a second run — the frontends used to guard this with their own
/// `Cell<bool>`, and each of them had to remember to. `due` is the caller's
/// answer from `refresh_due`, or `true` when the user asked explicitly.
pub fn fetch_allowed(enabled: bool, fetching: bool, due: bool) -> bool {
    enabled && !fetching && due
}

/// A fetch across both feeds.
///
/// Feeds finish independently and in any order, so the run has to know how
/// many answers it is still waiting for and what each one said. The frontend
/// keeps only the widgets.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedRefresh {
    pending: Vec<Feed>,
    failed: Vec<Feed>,
}

impl FeedRefresh {
    /// Starts a run over the feeds that are allowed to fetch.
    ///
    /// A run with no participants is already complete, which is how a
    /// frontend learns there is nothing to spin for.
    pub fn start(feeds: &[Feed]) -> Self {
        Self {
            pending: feeds.to_vec(),
            failed: Vec::new(),
        }
    }

    /// Records one feed's outcome.
    ///
    /// An answer from a feed that is not pending — a duplicate, or one from a
    /// run that has already finished — changes nothing.
    pub fn finish(&mut self, feed: Feed, failed: bool) {
        let Some(position) = self.pending.iter().position(|pending| *pending == feed) else {
            return;
        };
        self.pending.remove(position);
        if failed {
            self.failed.push(feed);
        }
    }

    pub fn is_complete(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn is_pending(&self, feed: Feed) -> bool {
        self.pending.contains(&feed)
    }

    /// The feeds whose fetch failed, in the order they reported.
    pub fn failed(&self) -> &[Feed] {
        &self.failed
    }

    pub fn has_failed(&self, feed: Feed) -> bool {
        self.failed.contains(&feed)
    }
}

/// What one feed contributes to the badge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedBadge {
    pub enabled: bool,
    /// Whether the feed has ever completed a fetch. A feed that has never run
    /// has nothing to report, and a count from it would be a guess.
    pub ready: bool,
    pub unseen: i64,
}

impl FeedBadge {
    fn contribution(&self) -> i64 {
        if self.enabled && self.ready {
            self.unseen.max(0)
        } else {
            0
        }
    }
}

/// The badge's text, or `None` when it should not be shown at all.
///
/// Counts above nine collapse to `9+`: the exact number stops being useful
/// long before it stops growing, and a widening badge shifts the header.
pub fn badge_text(unseen: i64) -> Option<String> {
    match unseen {
        n if n <= 0 => None,
        1..=9 => Some(unseen.to_string()),
        _ => Some("9+".to_string()),
    }
}

/// The badge across both feeds.
pub fn updates_badge(new_releases: FeedBadge, concerts: FeedBadge) -> Option<String> {
    badge_text(
        new_releases
            .contribution()
            .saturating_add(concerts.contribution()),
    )
}

#[cfg(test)]
#[path = "updates_tests.rs"]
mod updates_tests;
