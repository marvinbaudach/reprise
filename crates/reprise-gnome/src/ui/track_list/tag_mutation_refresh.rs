//! Shared UI invalidation after successful tag writes.

use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use reprise_core::queries::BrowseFilter;
use reprise_core::view_source::ViewSource;

use super::reload_restore::ReloadAnchor;
use super::track_list_model_change::{changed_range, ModelChange, ModelChangeKind};
use super::track_list_reload::{
    capture_reload_anchor, reload_with_anchor_and_viewport, ReloadViewport,
};
use super::Shared;

#[derive(Clone, PartialEq)]
struct ReloadQueryKey {
    source: ViewSource,
    sort_field: String,
    sort_dir: String,
    filter: String,
    browse: BrowseFilter,
    exclude_ai: bool,
}

struct ReloadChange {
    model: ModelChange,
    current_ids: Vec<i64>,
    query: ReloadQueryKey,
    metadata_only: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) enum ReloadEmit {
    Metadata,
    Span,
    Move,
    Full,
}

impl ReloadEmit {
    pub(in crate::ui) const fn as_str(self) -> &'static str {
        match self {
            Self::Metadata => "metadata",
            Self::Span => "span",
            Self::Move => "move",
            Self::Full => "full",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::ui) struct ReloadMetrics {
    pub(in crate::ui) idle_wait_ms: u128,
    pub(in crate::ui) reload_work_ms: u128,
    pub(in crate::ui) emit: ReloadEmit,
}

pub(in crate::ui) type ReloadReceipt = Rc<Cell<Option<ReloadMetrics>>>;

fn schedule_measured_reload(action: impl FnOnce() -> ReloadEmit + 'static) -> ReloadReceipt {
    let scheduled = Instant::now();
    let receipt = Rc::new(Cell::new(None));
    let receipt_for_idle = receipt.clone();
    gtk4::glib::idle_add_local_once(move || {
        let fired = Instant::now();
        let emit = action();
        receipt_for_idle.set(Some(ReloadMetrics {
            idle_wait_ms: fired.duration_since(scheduled).as_millis(),
            reload_work_ms: fired.elapsed().as_millis(),
            emit,
        }));
    });
    receipt
}

fn reload_query_key(shared: &Shared) -> ReloadQueryKey {
    let source = shared.source.borrow().clone();
    let sort = shared.sort.borrow().clone();
    ReloadQueryKey {
        exclude_ai: shared.browse_bar.exclude_ai() && matches!(source, ViewSource::Library),
        source,
        sort_field: sort.field,
        sort_dir: sort.dir,
        filter: shared.filter.borrow().clone(),
        browse: shared.browse_filter.borrow().clone(),
    }
}

pub(in crate::ui) fn refresh_after_tag_mutation(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
) -> ReloadReceipt {
    let anchor = capture_reload_anchor(shared);
    refresh_after_tag_mutation_with_anchor(shared, ids, paths, anchor)
}

pub(in crate::ui) fn refresh_after_tag_mutation_with_anchor(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
    anchor: ReloadAnchor,
) -> ReloadReceipt {
    refresh_with_reload_change(
        shared,
        ids,
        paths,
        anchor,
        ReloadViewport::PreserveAnchor,
        None,
    )
}

pub(in crate::ui) fn refresh_after_tag_mutation_with_save_anchor(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
    anchor: ReloadAnchor,
    sort_field_changed: bool,
) -> ReloadReceipt {
    let viewport = if sort_field_changed {
        ReloadViewport::PostSaveSortAnchor
    } else {
        ReloadViewport::PreserveAnchor
    };
    refresh_with_reload_change(shared, ids, paths, anchor, viewport, None)
}

pub(in crate::ui) fn refresh_after_tag_mutation_with_view_ids(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
    anchor: ReloadAnchor,
    before_ids: &[i64],
    after_ids: Vec<i64>,
) -> ReloadReceipt {
    let generation = shared.model.generation();
    let metadata_only = before_ids == after_ids;
    let reload_change = changed_range(before_ids, &after_ids, ids, generation);
    refresh_after_tag_mutation_with_model_change(
        shared,
        ids,
        paths,
        anchor,
        ReloadViewport::PreserveAnchor,
        reload_change,
        after_ids,
        metadata_only,
    )
}

#[allow(clippy::too_many_arguments)]
pub(in crate::ui) fn refresh_after_tag_mutation_with_model_change(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
    anchor: ReloadAnchor,
    viewport: ReloadViewport,
    model_change: Option<ModelChange>,
    current_ids: Vec<i64>,
    metadata_only: bool,
) -> ReloadReceipt {
    let reload_change = model_change.map(|model| ReloadChange {
        model,
        current_ids,
        query: reload_query_key(shared),
        metadata_only,
    });
    refresh_with_reload_change(shared, ids, paths, anchor, viewport, reload_change)
}

pub(in crate::ui) fn refresh_after_tag_mutation_with_save_change(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
    anchor: ReloadAnchor,
    before_ids: &[i64],
    after_ids: Vec<i64>,
    model_change: ModelChange,
) -> ReloadReceipt {
    match model_change.kind {
        ModelChangeKind::BlockMove { .. } => refresh_after_tag_mutation_with_model_change(
            shared,
            ids,
            paths,
            anchor,
            ReloadViewport::PostSaveSortAnchor,
            Some(model_change),
            after_ids,
            false,
        ),
        ModelChangeKind::Span => refresh_after_tag_mutation_with_view_ids(
            shared, ids, paths, anchor, before_ids, after_ids,
        ),
    }
}

fn refresh_with_reload_change(
    shared: &Rc<Shared>,
    ids: &[i64],
    paths: &[PathBuf],
    anchor: ReloadAnchor,
    viewport: ReloadViewport,
    reload_change: Option<ReloadChange>,
) -> ReloadReceipt {
    shared.cover_loader.invalidate_paths(paths);
    shared.browse_bar.refresh();
    if let Some(player) = shared.player.borrow().upgrade() {
        player.refresh_edited_metadata(ids);
    }
    // One reload, on idle. The tag editor is a dialog whose save completes on
    // the main loop just as the dialog is animating shut, so the `ColumnView`
    // behind it can still be obscured / not yet re-mapped: GTK then skips
    // rebinding the not-yet-visible rows and the live view keeps showing the
    // PRE-EDIT tags until the next manual reload (a header click / new
    // search). Deferring past the current main-loop turn is what makes the
    // edited rows rebind.
    //
    // This used to run *twice* — once synchronously here and once on idle —
    // and the synchronous one is the one that cannot be trusted to rebind. It
    // was not free either: every `reload` is a sorted full-table id query plus
    // an `items_changed(0, old, new)` that collapses selection and scroll for
    // `track_list_reload`'s restore to put back again.
    let receipt = {
        let shared = shared.clone();
        schedule_measured_reload(move || match reload_change {
            Some(change) if change.metadata_only && change.query == reload_query_key(&shared) => {
                shared
                    .model
                    .invalidate_cached_metadata(change.model.position, change.model.added);
                shared.reapply_now_playing_markers();
                ReloadEmit::Metadata
            }
            Some(change) if change.query == reload_query_key(&shared) => {
                let emit = match change.model.kind {
                    ModelChangeKind::Span => ReloadEmit::Span,
                    ModelChangeKind::BlockMove { .. } => ReloadEmit::Move,
                };
                reload_with_anchor_and_viewport(
                    &shared,
                    &anchor,
                    viewport,
                    Some(change.model),
                    Some(change.current_ids),
                );
                emit
            }
            None | Some(_) => {
                reload_with_anchor_and_viewport(&shared, &anchor, viewport, None, None);
                ReloadEmit::Full
            }
        })
    };
    let callback = shared.on_tags_mutated.borrow().clone();
    if let Some(callback) = callback {
        callback(paths);
    }
    receipt
}

#[cfg(test)]
#[path = "tag_mutation_refresh_block_move_display_tests.rs"]
mod block_move_display_tests;
#[cfg(test)]
#[path = "tag_mutation_refresh_display_tests.rs"]
mod display_tests;
#[cfg(test)]
#[path = "tag_mutation_refresh_marker_display_tests.rs"]
mod marker_display_tests;

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::*;

    #[test]
    fn deferred_reload_reports_wait_work_and_emit_shape() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        let ran = Rc::new(Cell::new(false));
        let ran_in_idle = ran.clone();
        let receipt = schedule_measured_reload(move || {
            ran_in_idle.set(true);
            ReloadEmit::Span
        });

        assert!(receipt.get().is_none());
        while gtk4::glib::MainContext::default().iteration(false) {}

        let metrics = receipt
            .get()
            .expect("the idle must publish its measurements");
        assert!(ran.get());
        assert_eq!(metrics.emit, ReloadEmit::Span);
        assert!(metrics.idle_wait_ms < 10_000);
        assert!(metrics.reload_work_ms < 10_000);
    }
}
