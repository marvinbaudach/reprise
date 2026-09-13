//! Signal shapes for SQL-backed track-list query replacements.

use gtk4::gio::prelude::*;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;

use super::diagnostic_trail::{self, Event};
use super::track_list_model::TrackListModel;
use super::track_list_model_change::{ModelChange, ModelChangeKind};

/// Emits a span change while avoiding GTK's full-range identity search.
///
/// `gtk_list_item_manager_model_items_changed_cb` identity-searches every
/// added item when one signal both removes a tracked row and adds replacements.
/// A non-empty whole-model replacement therefore removes first and inserts
/// second, so neither signal can enter that search. Both emissions remain in
/// the same synchronous call, with the GListModel count valid after each one.
pub(super) fn emit_span_change(
    model: &TrackListModel,
    change: ModelChange,
    old_total: u32,
    new_total: u32,
) -> ModelChange {
    let full_replacement = change.kind == ModelChangeKind::Span
        && change.position == 0
        && change.removed == old_total
        && change.added == new_total
        && old_total > 0
        && new_total > 0;
    if !full_replacement {
        record_and_emit(model, change.position, change.removed, change.added);
        return change;
    }

    model.imp().state.borrow_mut().total = 0;
    record_and_emit(model, 0, old_total, 0);
    model.imp().state.borrow_mut().total = new_total;
    record_and_emit(model, 0, 0, new_total);
    ModelChange {
        position: 0,
        removed: 0,
        added: new_total,
        before_total: 0,
        after_total: new_total,
        ..change
    }
}

fn record_and_emit(model: &TrackListModel, position: u32, removed: u32, added: u32) {
    diagnostic_trail::record(Event::ItemsChanged {
        position,
        removed,
        added,
    });
    model.items_changed(position, removed, added);
}

/// A narrowed query delta only makes GTK reconsider sections intersecting its
/// item range. Every non-Queue query is one whole-model section, so a partial
/// cardinality change must explicitly invalidate that surviving section's new
/// end boundary. Full-range item invalidations already cover it.
pub(super) fn query_section_change(change: ModelChange) -> Option<(u32, u32)> {
    let total_changed = change.before_total != change.after_total;
    let covers_every_survivor = change.position == 0 && change.added >= change.after_total;
    (total_changed && change.after_total > 0 && !covers_every_survivor)
        .then_some((0, change.after_total))
}
