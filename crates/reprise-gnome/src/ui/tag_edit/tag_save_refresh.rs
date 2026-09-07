//! Chooses whether a successful Tag Editor save can refresh realised rating
//! cells in place or must re-run the current track query.

use std::collections::HashMap;

use reprise_core::library::tag_edit::TrackWrite;
use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

use crate::ui::track_list::tag_mutation_refresh::ReloadMetrics;
use crate::ui::track_list::track_list_model_change::{changed_range, ModelChange, ModelChangeKind};
use crate::ui::track_list::Shared;

#[derive(Clone, Copy)]
pub(super) struct BatchCompletion {
    pub(super) write_ms: u128,
    pub(super) tracks: usize,
    pub(super) reload_ms: u128,
    pub(super) reload_metrics: Option<ReloadMetrics>,
    pub(super) delta: bool,
    pub(super) updated: usize,
    pub(super) failed: usize,
    pub(super) has_pre_save_view: bool,
    pub(super) before_len: usize,
    pub(super) after_len: usize,
    pub(super) first_mismatch: i64,
}

pub(super) fn log_batch_completed(completion: &BatchCompletion) {
    let BatchCompletion {
        write_ms,
        tracks,
        reload_ms,
        reload_metrics,
        delta,
        updated,
        failed,
        has_pre_save_view,
        before_len,
        after_len,
        first_mismatch,
    } = *completion;
    if let Some(metrics) = reload_metrics {
        tracing::info!(
            write_ms,
            tracks,
            reload_ms,
            idle_wait_ms = metrics.idle_wait_ms,
            reload_work_ms = metrics.reload_work_ms,
            emit = metrics.emit.as_str(),
            delta,
            updated,
            failed,
            has_pre_save_view,
            before_len,
            after_len,
            first_mismatch,
            "tag-edit batch completed"
        );
    } else {
        tracing::info!(
            write_ms,
            tracks,
            reload_ms,
            delta,
            updated,
            failed,
            has_pre_save_view,
            before_len,
            after_len,
            first_mismatch,
            "tag-edit batch completed"
        );
    }
}

pub(super) fn after_deferred_reload(action: impl FnOnce() + 'static) {
    gtk4::glib::idle_add_local_once(action);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TagSaveRefresh {
    InPlaceRatings(Vec<(i64, i32)>),
    Reload,
}

pub(in crate::ui) fn tag_save_model_change(
    before: &[i64],
    after: &[i64],
    written: &[i64],
    generation: u64,
) -> Option<ModelChange> {
    let change = changed_range(before, after, written, generation)?;
    (before == after || matches!(change.kind, ModelChangeKind::BlockMove { .. })).then_some(change)
}

pub(super) fn first_view_mismatch(before: &[i64], after: &[i64]) -> i64 {
    before
        .iter()
        .zip(after)
        .position(|(before_id, after_id)| before_id != after_id)
        .or_else(|| (before.len() != after.len()).then(|| before.len().min(after.len())))
        .map_or(-1, |index| i64::try_from(index).unwrap_or(i64::MAX))
}

pub(super) fn tag_changed_ids(writes: &[TrackWrite], updated_ids: &[i64]) -> Vec<i64> {
    writes
        .iter()
        .filter(|write| !write.patch.tags.is_empty() && updated_ids.contains(&write.id))
        .map(|write| write.id)
        .collect()
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

    fn seeded_five_tracks() -> reprise_core::db::Db {
        let db = crate::test_db::open().unwrap();
        let conn = crate::test_db::connection(&db);
        for id in 1_i64..=5 {
            conn.execute(
                "INSERT INTO tracks (id,path,title,artist,added_at) VALUES (?1,?2,?3,?4,0)",
                rusqlite::params![id, format!("/{id}.flac"), format!("Track {id}"), "Artist"],
            )
            .unwrap();
        }
        db
    }

    fn artist_sorted_ids(db: &reprise_core::db::Db) -> Vec<i64> {
        crate::test_db::connection(db)
            .prepare("SELECT id FROM tracks ORDER BY artist, title, id")
            .unwrap()
            .query_map([], |row| row.get::<_, i64>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    }

    #[test]
    fn tag_save_with_unchanged_order_reloads_by_delta() {
        let db = seeded_five_tracks();
        let before = artist_sorted_ids(&db);
        // Comment is a file tag, not a view-order column. A successful
        // comment-only write therefore leaves this seeded DB order intact.
        let after = artist_sorted_ids(&db);

        assert_eq!(
            tag_save_model_change(&before, &after, &[2, 3], 9),
            Some(
                crate::ui::track_list::track_list_model_change::ModelChange {
                    kind: crate::ui::track_list::track_list_model_change::ModelChangeKind::Span,
                    position: 1,
                    removed: 2,
                    added: 2,
                    before_total: 5,
                    after_total: 5,
                    generation: 9,
                }
            )
        );
    }

    #[test]
    fn tag_save_that_moves_one_contiguous_block_requests_the_move() {
        let db = seeded_five_tracks();
        let conn = crate::test_db::connection(&db);
        let before = artist_sorted_ids(&db);
        conn.execute("UPDATE tracks SET artist='Zulu' WHERE id=2", [])
            .unwrap();
        let after = artist_sorted_ids(&db);

        assert_eq!(
            tag_save_model_change(&before, &after, &[2], 9),
            Some(
                crate::ui::track_list::track_list_model_change::ModelChange {
                    kind:
                        crate::ui::track_list::track_list_model_change::ModelChangeKind::BlockMove {
                            from: 1,
                            to: 4,
                            len: 1,
                        },
                    position: 1,
                    removed: 4,
                    added: 4,
                    before_total: 5,
                    after_total: 5,
                    generation: 9,
                },
            )
        );
    }

    #[test]
    fn tag_save_with_a_scattered_reorder_still_requests_a_full_reload() {
        assert_eq!(
            tag_save_model_change(&[1, 2, 3, 4], &[2, 1, 4, 3], &[1, 3], 9),
            None
        );
    }

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
    fn mixed_tag_and_rating_save_builds_the_delta_from_tag_writes_only() {
        let writes = [rating_write(1, 4), tag_write(2)];

        let tag_ids = tag_changed_ids(&writes, &[1, 2]);
        let change = tag_save_model_change(&[1, 2, 3], &[1, 2, 3], &tag_ids, 11)
            .expect("the tag write must request a one-row delta");

        assert_eq!(tag_ids, vec![2]);
        assert_eq!(change.position, 1);
        assert_eq!(change.removed, 1);
        assert_eq!(change.added, 1);
    }

    #[test]
    #[ignore = "uses the global GLib main context; run alone"]
    fn batch_telemetry_runs_after_an_already_scheduled_reload() {
        use std::cell::RefCell;
        use std::rc::Rc;

        let _main_context = crate::ui::test_main_context::lock_main_context();
        let calls = Rc::new(RefCell::new(Vec::new()));
        let reload_calls = calls.clone();
        gtk4::glib::idle_add_local_once(move || reload_calls.borrow_mut().push("reload"));
        let telemetry_calls = calls.clone();
        after_deferred_reload(move || telemetry_calls.borrow_mut().push("telemetry"));

        assert!(calls.borrow().is_empty());
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert_eq!(&*calls.borrow(), &["reload", "telemetry"]);
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
