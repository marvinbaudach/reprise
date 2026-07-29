//! Pure search-result projection shared by every surface that lets a user
//! discover a new podcast or YouTube channel — the GNOME add dialog and the
//! MCP discovery tool alike. Keeping this in `reprise-core` means both
//! surfaces filter "already subscribed" results by the exact same identity
//! rules; a copy in only one crate would let the two silently drift apart.

use rusqlite::Connection;

use super::{query, store, PodcastKind};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub kind: PodcastKind,
    pub title: String,
    pub subtitle: String,
    pub author: Option<String>,
    pub image_url: Option<String>,
    pub url: String,
    pub identity_guids: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveSourceKey {
    kind: PodcastKind,
    url: String,
    identity_guids: Vec<String>,
}

/// Loads the identity of every currently-subscribed podcast/YouTube source,
/// for filtering fresh search results. Never fails outright — a lookup error
/// is logged and treated as "nothing subscribed yet" so a transient DB issue
/// degrades to showing everything rather than hiding search results.
pub fn active_source_keys(conn: &Connection) -> Vec<ActiveSourceKey> {
    store::active_subscriptions(conn).map_or_else(
        |error| {
            tracing::warn!(%error, "could not load subscribed sources for search filtering");
            Vec::new()
        },
        |rows| {
            rows.into_iter()
                .map(|row| {
                    let identity_guids = query::episodes_for_subscription(conn, row.id)
                        .map_or_else(
                            |error| {
                                tracing::warn!(
                                    %error,
                                    subscription_id = row.id,
                                    "could not load source identity episodes"
                                );
                                Vec::new()
                            },
                            |episodes| episodes.into_iter().map(|episode| episode.guid).collect(),
                        );
                    ActiveSourceKey {
                        kind: row.kind,
                        url: row.feed_url,
                        identity_guids,
                    }
                })
                .collect()
        },
    )
}

/// `SRC-5`: drop any search candidate that already matches a subscribed
/// source by stable identity.
pub fn filter_unsubscribed(
    candidates: Vec<Candidate>,
    subscribed: &[ActiveSourceKey],
) -> Vec<Candidate> {
    candidates
        .into_iter()
        .filter(|candidate| {
            !source_is_subscribed(
                candidate.kind,
                &candidate.url,
                &candidate.identity_guids,
                subscribed,
            )
        })
        .collect()
}

pub fn source_is_subscribed(
    kind: PodcastKind,
    url: &str,
    identity_guids: &[String],
    subscribed: &[ActiveSourceKey],
) -> bool {
    let candidate_url = normalized_source_url(kind, url);
    subscribed.iter().any(|subscribed| {
        if subscribed.kind != kind {
            return false;
        }
        let subscribed_url = normalized_source_url(kind, &subscribed.url);
        subscribed_url == candidate_url
            || (kind == PodcastKind::Youtube
                && youtube_handle_channel_pair(&subscribed_url, &candidate_url)
                && identity_guids
                    .iter()
                    .any(|guid| subscribed.identity_guids.contains(guid)))
    })
}

fn youtube_handle_channel_pair(left: &str, right: &str) -> bool {
    (left.starts_with("youtube:handle:") && right.starts_with("youtube:channel:"))
        || (left.starts_with("youtube:channel:") && right.starts_with("youtube:handle:"))
}

/// `SRC-6`: a search is bound to exactly one provider — the one the caller
/// asked for. There is no mixed result list and no shared search.
pub const fn dialog_provider(opened_from: PodcastKind) -> PodcastKind {
    opened_from
}

fn normalized_source_url(kind: PodcastKind, value: &str) -> String {
    let value = value.trim();
    if kind != PodcastKind::Youtube {
        return if value.contains(['?', '#']) {
            value.to_owned()
        } else {
            value.trim_end_matches('/').to_owned()
        };
    }
    let value = value
        .split(['#', '?'])
        .next()
        .unwrap_or(value)
        .trim_end_matches('/');
    let without_scheme = value
        .strip_prefix("https://")
        .or_else(|| value.strip_prefix("http://"))
        .unwrap_or(value);
    let without_host = ["www.youtube.com/", "m.youtube.com/", "youtube.com/"]
        .into_iter()
        .find_map(|prefix| without_scheme.strip_prefix(prefix))
        .unwrap_or(without_scheme);
    if let Some(channel_id) = without_host.strip_prefix("channel/") {
        return format!("youtube:channel:{channel_id}");
    }
    if without_host.starts_with('@') {
        return format!("youtube:handle:{}", without_host.to_ascii_lowercase());
    }
    format!("youtube:{without_host}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_5_search_hides_already_subscribed_sources_by_stable_identity() {
        let candidates = vec![
            candidate(
                PodcastKind::Rss,
                "Existing show",
                "https://feeds.test/show/",
            ),
            candidate(
                PodcastKind::Youtube,
                "Existing channel",
                "https://www.youtube.com/channel/UC-existing",
            ),
            candidate(
                PodcastKind::Youtube,
                "New channel",
                "https://www.youtube.com/channel/UC-new",
            ),
        ];

        let visible = filter_unsubscribed(
            candidates,
            &[
                active(PodcastKind::Rss, "https://feeds.test/show", &[]),
                active(
                    PodcastKind::Youtube,
                    "https://m.youtube.com/channel/UC-existing/",
                    &[],
                ),
            ],
        );

        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].title, "New channel");
    }

    #[test]
    fn src_6_each_dialog_searches_only_its_own_provider() {
        assert_eq!(dialog_provider(PodcastKind::Rss), PodcastKind::Rss);
        assert_eq!(dialog_provider(PodcastKind::Youtube), PodcastKind::Youtube);
    }

    #[test]
    fn src_5_rss_identity_preserves_meaningful_query_parameters() {
        assert_ne!(
            normalized_source_url(PodcastKind::Rss, "https://feeds.test/show?token=one"),
            normalized_source_url(PodcastKind::Rss, "https://feeds.test/show?token=two")
        );
    }

    #[test]
    fn src_5_historical_youtube_handles_match_by_stable_episode_identity() {
        assert!(source_is_subscribed(
            PodcastKind::Youtube,
            "https://www.youtube.com/channel/UC-existing",
            &["video-1".into()],
            &[active(
                PodcastKind::Youtube,
                "https://www.youtube.com/@existing",
                &["video-1"],
            )]
        ));
    }

    fn candidate(kind: PodcastKind, title: &str, url: &str) -> Candidate {
        Candidate {
            kind,
            title: title.into(),
            subtitle: String::new(),
            author: None,
            image_url: Some("https://img.test/source.jpg".into()),
            url: url.into(),
            identity_guids: Vec::new(),
        }
    }

    fn active(kind: PodcastKind, url: &str, guids: &[&str]) -> ActiveSourceKey {
        ActiveSourceKey {
            kind,
            url: url.into(),
            identity_guids: guids.iter().map(|guid| (*guid).to_owned()).collect(),
        }
    }
}
