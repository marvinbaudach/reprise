//! Toolkit-free quick-open matching, ranking, grouping, and session recents.

use std::collections::{BTreeMap, VecDeque};

const MAX_ROWS_PER_GROUP: usize = 5;
const MAX_RECENTS: usize = 8;

/// Result kinds in their stable presentation order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum QuickOpenKind {
    Track,
    Album,
    Artist,
    Playlist,
    Podcast,
    Radio,
}

impl QuickOpenKind {
    #[must_use]
    pub const fn singular_label(self) -> &'static str {
        match self {
            Self::Track => "Track",
            Self::Album => "Album",
            Self::Artist => "Artist",
            Self::Playlist => "Playlist",
            Self::Podcast => "Podcast show",
            Self::Radio => "Radio station",
        }
    }

    #[must_use]
    pub const fn section_label(self) -> &'static str {
        match self {
            Self::Track => "Tracks",
            Self::Album => "Albums",
            Self::Artist => "Artists",
            Self::Playlist => "Playlists",
            Self::Podcast => "Podcast shows",
            Self::Radio => "Radio stations",
        }
    }
}

/// Semantic action carried by a result; GTK only decides how to dispatch it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickOpenAction {
    PlayTrack {
        track_id: i64,
        album: Option<String>,
        album_artist: Option<String>,
        artist: Option<String>,
    },
    PlayStation {
        station_id: i64,
        name: String,
        stream_url: String,
        uuid: Option<String>,
    },
    NavigateAlbum {
        album: String,
        album_artist: String,
    },
    NavigateArtist {
        artist: String,
    },
    NavigatePlaylist {
        playlist_id: i64,
        smart: bool,
    },
    NavigatePodcast {
        subscription_id: i64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickOpenCandidate {
    pub kind: QuickOpenKind,
    pub title: String,
    pub subtitle: String,
    pub play_count: i64,
    pub action: QuickOpenAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickOpenRow {
    Item(QuickOpenCandidate),
    ShowAll {
        kind: QuickOpenKind,
        count: usize,
        query: String,
    },
}

impl QuickOpenRow {
    #[must_use]
    pub const fn item(&self) -> Option<&QuickOpenCandidate> {
        match self {
            Self::Item(item) => Some(item),
            Self::ShowAll { .. } => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuickOpenGroup {
    pub kind: QuickOpenKind,
    pub rows: Vec<QuickOpenRow>,
}

/// Filters candidates with the quick-open boundary rule, then applies the
/// contract's rank and per-group cap.
#[must_use]
pub fn rank_and_group(candidates: Vec<QuickOpenCandidate>, query: &str) -> Vec<QuickOpenGroup> {
    let query = normalize(query);
    if query.is_empty() {
        return Vec::new();
    }
    let mut grouped = BTreeMap::<QuickOpenKind, Vec<(u8, QuickOpenCandidate)>>::new();
    for candidate in candidates {
        let Some(rank) = match_rank(&candidate, &query) else {
            continue;
        };
        grouped
            .entry(candidate.kind)
            .or_default()
            .push((rank, candidate));
    }
    grouped
        .into_iter()
        .map(|(kind, mut candidates)| {
            candidates.sort_by(|(left_rank, left), (right_rank, right)| {
                right_rank
                    .cmp(left_rank)
                    .then_with(|| right.play_count.cmp(&left.play_count))
                    .then_with(|| normalize(&left.title).cmp(&normalize(&right.title)))
                    .then_with(|| left.subtitle.cmp(&right.subtitle))
            });
            let total = candidates.len();
            let mut rows = candidates
                .into_iter()
                .take(MAX_ROWS_PER_GROUP)
                .map(|(_, candidate)| QuickOpenRow::Item(candidate))
                .collect::<Vec<_>>();
            if total > MAX_ROWS_PER_GROUP {
                rows.push(QuickOpenRow::ShowAll {
                    kind,
                    count: total,
                    query: query.clone(),
                });
            }
            QuickOpenGroup { kind, rows }
        })
        .collect()
}

fn match_rank(candidate: &QuickOpenCandidate, query: &str) -> Option<u8> {
    let title = normalize(&candidate.title);
    let subtitle = normalize(&candidate.subtitle);
    if title == query {
        return Some(3);
    }
    if title.starts_with(query) {
        return Some(2);
    }
    (word_prefix(&title, query) || word_prefix(&subtitle, query)).then_some(1)
}

fn word_prefix(value: &str, query: &str) -> bool {
    value.match_indices(query).any(|(index, _)| {
        index == 0
            || value[..index]
                .chars()
                .next_back()
                .is_none_or(|character| !character.is_alphanumeric())
    })
}

fn normalize(value: &str) -> String {
    reprise_core::library::group_key::normalize_group_key(value)
}

/// Most-recent-first, de-duplicated session history.
#[derive(Default)]
pub struct QuickOpenRecents {
    items: VecDeque<QuickOpenCandidate>,
}

impl QuickOpenRecents {
    pub fn remember(&mut self, item: QuickOpenCandidate) {
        self.items.retain(|existing| existing.action != item.action);
        self.items.push_front(item);
        self.items.truncate(MAX_RECENTS);
    }

    #[must_use]
    pub fn items(&self) -> Vec<QuickOpenCandidate> {
        self.items.iter().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(kind: QuickOpenKind, title: &str, plays: i64, id: i64) -> QuickOpenCandidate {
        QuickOpenCandidate {
            kind,
            title: title.into(),
            subtitle: "Fixture artist".into(),
            play_count: plays,
            action: QuickOpenAction::PlayTrack {
                track_id: id,
                album: None,
                album_artist: None,
                artist: None,
            },
        }
    }

    #[test]
    fn search_16_matching_folds_case_and_diacritics_at_word_boundaries() {
        let candidates = vec![
            candidate(QuickOpenKind::Track, "Beyonce Halo", 0, 1),
            candidate(QuickOpenKind::Track, "Halo Beyonce", 0, 2),
            candidate(QuickOpenKind::Track, "Debeyonce", 0, 3),
        ];

        let groups = rank_and_group(candidates, "BEYONCÉ");
        let ids = groups[0]
            .rows
            .iter()
            .filter_map(QuickOpenRow::item)
            .filter_map(|item| match item.action {
                QuickOpenAction::PlayTrack { track_id, .. } => Some(track_id),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![1, 2]);

        let punctuation = vec![candidate(QuickOpenKind::Track, "AC/DC", 0, 4)];
        assert_eq!(
            rank_and_group(punctuation, "dc")[0].rows[0]
                .item()
                .map(|item| item.title.as_str()),
            Some("AC/DC")
        );
    }

    #[test]
    fn search_16_ranks_exact_then_prefix_then_word_prefix_then_play_count() {
        let candidates = vec![
            candidate(QuickOpenKind::Track, "Blue", 1, 1),
            candidate(QuickOpenKind::Track, "Blue Monday", 2, 2),
            candidate(QuickOpenKind::Track, "Monday Blue", 50, 3),
            candidate(QuickOpenKind::Track, "Blue Moon", 20, 4),
        ];

        let rows = rank_and_group(candidates, "blue").remove(0).rows;
        let ids = rows
            .iter()
            .filter_map(QuickOpenRow::item)
            .filter_map(|item| match item.action {
                QuickOpenAction::PlayTrack { track_id, .. } => Some(track_id),
                _ => None,
            })
            .collect::<Vec<_>>();

        assert_eq!(ids, vec![1, 4, 2, 3]);
    }

    #[test]
    fn search_16_groups_in_contract_order_and_caps_each_group() {
        let mut candidates = (0..7)
            .map(|id| candidate(QuickOpenKind::Track, &format!("Match {id}"), id, id))
            .collect::<Vec<_>>();
        candidates.push(QuickOpenCandidate {
            kind: QuickOpenKind::Album,
            title: "Match Album".into(),
            subtitle: "Artist".into(),
            play_count: 0,
            action: QuickOpenAction::NavigateAlbum {
                album: "Match Album".into(),
                album_artist: "Artist".into(),
            },
        });

        let groups = rank_and_group(candidates, "match");

        assert_eq!(
            groups.iter().map(|group| group.kind).collect::<Vec<_>>(),
            vec![QuickOpenKind::Track, QuickOpenKind::Album]
        );
        assert_eq!(groups[0].rows.len(), 6);
        assert!(matches!(
            groups[0].rows[5],
            QuickOpenRow::ShowAll { count: 7, .. }
        ));
    }

    #[test]
    fn search_16_recents_keep_the_last_eight_unique_opened_items() {
        let mut recents = QuickOpenRecents::default();
        for id in 0..10 {
            recents.remember(candidate(
                QuickOpenKind::Track,
                &format!("Track {id}"),
                0,
                id,
            ));
        }
        recents.remember(candidate(QuickOpenKind::Track, "Track 4", 0, 4));

        let items = recents.items();
        assert_eq!(items.len(), 8);
        assert_eq!(items[0].title, "Track 4");
        assert_eq!(items[7].title, "Track 2");
    }
}
