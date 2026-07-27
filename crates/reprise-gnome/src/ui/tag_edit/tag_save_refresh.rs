//! Chooses whether a successful Tag Editor save can refresh realised rating
//! cells in place or must re-run the current track query.

use std::collections::HashMap;

use reprise_core::library::tag_edit::TrackWrite;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

use crate::ui::track_list::Shared;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TagSaveRefresh {
    InPlaceRatings(Vec<(i64, i32)>),
    Reload,
}

pub(super) fn plan(
    writes: &[TrackWrite],
    updated_ids: &[i64],
    source: &ViewSource,
    sort_field: &str,
    browse: &BrowseFilter,
) -> TagSaveRefresh {
    if !matches!(source, ViewSource::Library) || sort_field == "rating" || browse.rating.is_some() {
        return TagSaveRefresh::Reload;
    }

    let ratings = updated_ids
        .iter()
        .map(|updated_id| {
            writes
                .iter()
                .find(|write| write.id == *updated_id)
                .filter(|write| write.patch.tags.is_empty())
                .and_then(|write| write.patch.rating.map(|rating| (write.id, rating)))
        })
        .collect::<Option<Vec<_>>>();

    match ratings {
        Some(ratings) if !ratings.is_empty() => TagSaveRefresh::InPlaceRatings(ratings),
        _ => TagSaveRefresh::Reload,
    }
}

pub(super) fn apply_in_place(shared: &Shared, ratings: &[(i64, i32)]) {
    let current_ids = shared.current_view_ids();
    let by_id: HashMap<i64, i32> = ratings.iter().copied().collect();
    for (position, track_id) in current_ids.iter().enumerate() {
        let Some(&rating) = by_id.get(track_id) else {
            continue;
        };
        if let Ok(position) = u32::try_from(position) {
            shared.model.set_cached_rating(position, rating);
        }
    }
    shared.refresh_realised_ratings(ratings);
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use reprise_core::library::tag_edit::{TagPatch, TrackEditPatch};

    use super::*;

    fn rating_write(id: i64, rating: i32) -> TrackWrite {
        TrackWrite {
            id,
            path: PathBuf::from(format!("/synthetic/{id}.flac")),
            patch: TrackEditPatch {
                tags: TagPatch::default(),
                rating: Some(rating),
            },
        }
    }

    fn tag_write(id: i64) -> TrackWrite {
        TrackWrite {
            id,
            path: PathBuf::from(format!("/synthetic/{id}.flac")),
            patch: TrackEditPatch {
                tags: TagPatch {
                    title: Some("Moved title".into()),
                    ..TagPatch::default()
                },
                rating: None,
            },
        }
    }

    #[test]
    fn tag_1_plain_library_rating_save_is_viewport_neutral_in_place() {
        let writes = [rating_write(61, 4)];

        assert_eq!(
            plan(
                &writes,
                &[61],
                &ViewSource::Library,
                "artist",
                &BrowseFilter::default(),
            ),
            TagSaveRefresh::InPlaceRatings(vec![(61, 4)])
        );
    }

    #[test]
    fn tag_1_rating_dependent_views_and_tag_writes_still_requery() {
        let rating = [rating_write(61, 4)];
        let tag = [tag_write(61)];
        let filtered = BrowseFilter {
            rating: Some("4".into()),
            ..BrowseFilter::default()
        };

        for refresh in [
            plan(
                &rating,
                &[61],
                &ViewSource::Library,
                "rating",
                &BrowseFilter::default(),
            ),
            plan(&rating, &[61], &ViewSource::Library, "artist", &filtered),
            plan(
                &tag,
                &[61],
                &ViewSource::Library,
                "artist",
                &BrowseFilter::default(),
            ),
        ] {
            assert_eq!(refresh, TagSaveRefresh::Reload);
        }
    }
}
