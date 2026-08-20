//! Subscription writes and defaults for the Podcast add dialog.

use std::rc::Rc;

use reprise_core::db::Db;
use reprise_core::podcasts::discovery::Candidate;
use reprise_core::podcasts::{self, PodcastKind};

use super::add_dialog::OnAdded;
use crate::ui::strings;

/// `NET-3` point 4: the URL path while offline — no preview fetch, the
/// subscription is created straight away with the URL itself as a
/// placeholder title, and the next successful refresh (already scheduled
/// independently of this dialog) fills in the real title and episodes.
/// This only translates [`podcasts::offline_add::offline_subscribe`]'s
/// outcome into status text and the `on_added` callback; the decision and
/// the one DB write both live in core, where they are testable without a
/// GTK dialog.
pub(super) fn subscribe_offline(
    kind: PodcastKind,
    url: &str,
    conn: &Rc<Db>,
    status: &gtk4::Label,
    on_added: &OnAdded,
) {
    let outcome = podcasts::offline_add::offline_subscribe(conn, kind, url, false);
    match outcome {
        Ok(podcasts::offline_add::OfflineSubscribeOutcome::AlreadySubscribed) => {
            status.set_text(&strings::text(strings::PODCAST_ALREADY_SUBSCRIBED));
        }
        Ok(podcasts::offline_add::OfflineSubscribeOutcome::Added { .. }) => {
            status.set_text(&strings::text(strings::PODCAST_ADDED_OFFLINE));
            // `import_latest = false`: there is nothing to import yet while
            // offline, and forcing an immediate refresh attempt now would
            // just fail loudly over the network this dialog just avoided.
            on_added(false);
        }
        Err(error) => {
            tracing::warn!(%error, "could not save offline podcast subscription");
            status.set_text(&strings::text(strings::PODCAST_SUBSCRIBE_FAILED));
        }
    }
}

pub(super) fn subscribe(
    conn: &Db,
    candidate: &Candidate,
    future_only_baseline: Option<&[String]>,
) -> Result<i64, rusqlite::Error> {
    podcasts::store::add_or_restore_with_baseline(
        conn,
        &podcasts::store::NewSubscription {
            kind: candidate.kind,
            feed_url: candidate.url.clone(),
            title: candidate.title.clone(),
            author: candidate.author.clone(),
            // A YouTube candidate's picture is a *video* thumbnail: search hits
            // are videos (`ytdlp_search::parse_search_channels`) and the URL
            // preview reads the channel dump. Persisting it would stamp an
            // episode cover onto the channel until the first refresh, which is
            // exactly the state measured in the live database on 2026-08-18.
            // It stays a preview; the refresh brings the real avatar.
            image_url: match candidate.kind {
                PodcastKind::Rss => candidate.image_url.clone(),
                PodcastKind::Youtube => None,
            },
            auto_download: false,
        },
        chrono::Utc::now().timestamp(),
        future_only_baseline,
    )
}

pub(super) fn baseline_for_import_choice(
    import: bool,
    preview_guids: &[String],
) -> Option<Vec<String>> {
    (!import).then(|| preview_guids.to_vec())
}
