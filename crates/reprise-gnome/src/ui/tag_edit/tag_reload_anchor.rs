//! Sort-aware TAG-1 anchor choice after a Tag Editor save.

use reprise_core::library::tag_edit::TrackWrite;

use crate::ui::track_list::reload_restore::{self, ReloadAnchor};

#[derive(Clone)]
pub(super) struct OpenedReloadState {
    pub(super) anchor: ReloadAnchor,
    pub(super) view_ids: Vec<i64>,
}

fn write_patches_sort_key(write: &TrackWrite, sort_columns: &[&str]) -> bool {
    let tags = &write.patch.tags;
    (sort_columns.contains(&"title") && tags.title.is_some())
        || (sort_columns.contains(&"artist") && tags.artist.is_some())
        || (sort_columns.contains(&"album") && tags.album.is_some())
        || (sort_columns.contains(&"album_artist") && tags.album_artist.is_some())
        || (sort_columns.contains(&"year") && tags.year.is_some())
        || (sort_columns.contains(&"track_no") && tags.track_no.is_some())
        || (sort_columns.contains(&"genre") && tags.genre.is_some())
        || (sort_columns.contains(&"rating") && write.patch.rating.is_some())
}

pub(in crate::ui) fn post_save_reload_anchor(
    mut opened: ReloadAnchor,
    updated_ids: &[i64],
    writes: &[TrackWrite],
    sort_field: &str,
    old_view_ids: &[i64],
    row_height: f64,
) -> ReloadAnchor {
    opened.selected_ids = updated_ids.to_vec();
    let sort_columns = reprise_core::queries::sort_key_columns(sort_field);
    let can_resort = writes.iter().any(|write| {
        updated_ids.contains(&write.id) && write_patches_sort_key(write, sort_columns)
    });
    let Some(first_edited_id) = can_resort.then(|| updated_ids.first()).flatten().copied() else {
        return opened;
    };
    reload_restore::reanchor_on_track(opened, first_edited_id, old_view_ids, row_height)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use reprise_core::library::tag_edit::{TagPatch, TrackEditPatch};

    use super::*;

    fn tag_write(id: i64, tags: TagPatch) -> TrackWrite {
        TrackWrite {
            id,
            path: PathBuf::from(format!("/{id}.flac")),
            patch: TrackEditPatch {
                tags,
                ..Default::default()
            },
        }
    }

    #[test]
    fn tag_1_sort_changing_save_reanchors_on_the_first_edited_track() {
        let opened = reload_restore::capture(vec![40], Some((20, 4.0)));
        let writes = vec![tag_write(
            40,
            TagPatch {
                year: Some(Some(2099)),
                ..Default::default()
            },
        )];

        let restored =
            post_save_reload_anchor(opened, &[40], &writes, "artist", &[10, 20, 30, 40], 20.0);

        assert_eq!(restored.selected_ids, vec![40]);
        assert_eq!(restored.anchor, Some((40, -36.0)));
    }

    #[test]
    fn tag_1_non_sorting_save_keeps_the_original_scroll_anchor() {
        let opened = reload_restore::capture(vec![40], Some((20, 4.0)));
        let writes = vec![tag_write(
            40,
            TagPatch {
                title: Some("Renamed".into()),
                ..Default::default()
            },
        )];

        let restored =
            post_save_reload_anchor(opened, &[40], &writes, "artist", &[10, 20, 30, 40], 20.0);

        assert_eq!(restored.anchor, Some((20, 4.0)));
    }
}
