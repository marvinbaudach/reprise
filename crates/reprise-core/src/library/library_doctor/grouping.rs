use std::collections::{HashMap, HashSet};

use crate::library::group_key::normalize_group_key;

use super::{
    DoctorField, DoctorReviewRow, DoctorReviewRowId, DoctorReviewRowState, DoctorReviewSession,
    DoctorScan, DoctorValue,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReviewAlbum {
    pub key: String,
    pub title: String,
    pub artist: String,
    pub track_count: usize,
    pub change_count: usize,
    pub rows: Vec<DoctorReviewDisplayRow>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorReviewDisplayRow {
    Track {
        row_id: DoctorReviewRowId,
        track_id: i64,
    },
    AllTracks {
        row_ids: Vec<DoctorReviewRowId>,
        track_count: usize,
    },
}

struct AlbumSeed {
    key: String,
    title: String,
    artist: String,
    first_position: usize,
    track_ids: HashSet<i64>,
}

struct MatchingRows<'a> {
    field: DoctorField,
    current: &'a DoctorValue,
    proposed: &'a DoctorValue,
    rows: Vec<&'a DoctorReviewRow>,
}

pub fn group_review_rows(
    scan: &DoctorScan,
    session: &DoctorReviewSession,
) -> Vec<DoctorReviewAlbum> {
    let mut seeds = Vec::<AlbumSeed>::new();
    let mut track_keys = HashMap::<i64, String>::new();
    let snapshots = scan
        .tracks
        .iter()
        .map(|track| (track.reference.track_id, track))
        .collect::<HashMap<_, _>>();

    for (position, track_id) in scan.track_ids.iter().copied().enumerate() {
        let tags = snapshots
            .get(&track_id)
            .and_then(|track| track.tags.as_ref());
        let album = tags.map(|tags| tags.album.as_str()).unwrap_or_default();
        let artist = tags
            .map(|tags| {
                if tags.album_artist.trim().is_empty() {
                    tags.artist.as_str()
                } else {
                    tags.album_artist.as_str()
                }
            })
            .unwrap_or_default();
        let key = if album.trim().is_empty() {
            String::new()
        } else {
            format!(
                "{}\u{1}{}",
                normalize_group_key(artist),
                normalize_group_key(album)
            )
        };
        track_keys.insert(track_id, key.clone());
        if let Some(seed) = seeds.iter_mut().find(|seed| seed.key == key) {
            seed.track_ids.insert(track_id);
        } else {
            seeds.push(AlbumSeed {
                key,
                title: album.to_owned(),
                artist: artist.to_owned(),
                first_position: position,
                track_ids: HashSet::from([track_id]),
            });
        }
    }

    seeds.sort_by_key(|seed| (seed.key.is_empty(), seed.first_position));
    seeds
        .into_iter()
        .filter_map(|seed| album_from_seed(seed, &track_keys, session))
        .collect()
}

fn album_from_seed(
    seed: AlbumSeed,
    track_keys: &HashMap<i64, String>,
    session: &DoctorReviewSession,
) -> Option<DoctorReviewAlbum> {
    let album_rows = session
        .rows()
        .iter()
        .filter(|row| session.category_filter_matches(row.problem_class))
        .filter(|row| track_keys.get(&row.track_id) == Some(&seed.key))
        .collect::<Vec<_>>();
    if album_rows.is_empty() {
        return None;
    }
    let mut matching = Vec::<MatchingRows<'_>>::new();
    for row in &album_rows {
        if let Some(group) = matching.iter_mut().find(|group| {
            group.field == row.field
                && group.current == &row.current
                && group.proposed == &row.proposed
        }) {
            group.rows.push(row);
        } else {
            matching.push(MatchingRows {
                field: row.field,
                current: &row.current,
                proposed: &row.proposed,
                rows: vec![row],
            });
        }
    }
    let collapsed = matching
        .iter()
        .filter(|group| {
            group
                .rows
                .iter()
                .map(|row| row.track_id)
                .collect::<HashSet<_>>()
                == seed.track_ids
        })
        .map(|group| (group.field, group.current.clone(), group.proposed.clone()))
        .collect::<Vec<_>>();
    let mut emitted = Vec::<(DoctorField, DoctorValue, DoctorValue)>::new();
    let mut rows = Vec::new();
    for row in &album_rows {
        let identity = (row.field, row.current.clone(), row.proposed.clone());
        if collapsed.contains(&identity) {
            if emitted.contains(&identity) {
                continue;
            }
            emitted.push(identity.clone());
            let row_ids = matching
                .iter()
                .find(|group| {
                    group.field == identity.0
                        && group.current == &identity.1
                        && group.proposed == &identity.2
                })
                .expect("collapsed identity came from a matching group")
                .rows
                .iter()
                .map(|row| row.id)
                .collect();
            rows.push(DoctorReviewDisplayRow::AllTracks {
                row_ids,
                track_count: seed.track_ids.len(),
            });
        } else {
            rows.push(DoctorReviewDisplayRow::Track {
                row_id: row.id,
                track_id: row.track_id,
            });
        }
    }
    let change_count = album_rows
        .iter()
        .filter(|row| row.selected && row.state == DoctorReviewRowState::Ready)
        .count();
    Some(DoctorReviewAlbum {
        key: seed.key,
        title: seed.title,
        artist: seed.artist,
        track_count: seed.track_ids.len(),
        change_count,
        rows,
    })
}
