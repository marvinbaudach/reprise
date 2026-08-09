//! Per-table fixed-width and header-order persistence.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio::prelude::*;
use gtk4::glib;
use reprise_core::library::settings;
use reprise_view::columns::{ColumnKey, layout};

use super::registry::ColumnRegistry;

/// Coalescing delay for width writes: header drags fire many `fixed-width`
/// notifications, so we persist only after the drag settles.
const WIDTH_SAVE_DEBOUNCE_MS: u64 = 500;

pub(in crate::ui) fn wire<K: ColumnKey>(
    registry: &Rc<ColumnRegistry<K>>,
    label: impl Fn(K) -> String + 'static,
    width: impl Fn(K) -> i32 + 'static,
    filler: K,
) {
    registry.configure(Rc::new(label), Rc::new(width), filler);
    registry.syncing_width.set(true);
    if let Some(width) = registry.width_policy() {
        for (key, column) in &registry.columns {
            column.set_fixed_width(width(*key));
        }
    }
    restore_stored_widths(registry);
    registry.syncing_width.set(false);
    wire_width_persistence(registry);
    wire_order_persistence(registry);

    // GTK's native header drag-reorder is broken in 4.22 because the title
    // widget's own click gesture claims the press. The shared custom header
    // drag owns this interaction, so native reordering must stay disabled.
    registry.view.set_reorderable(false);
}

fn restore_stored_widths<K: ColumnKey>(registry: &ColumnRegistry<K>) {
    let stored = settings::get_setting(&registry.conn, registry.keys.widths)
        .map_err(|error| tracing::warn!(%error, key = registry.keys.widths, "could not load stored column widths"))
        .ok()
        .flatten();
    let Some(stored) = stored else {
        return;
    };
    for (key, width) in reprise_view::column_widths::parse_widths::<K>(&stored) {
        if key.pin().is_none() {
            if let Some(column) = registry.columns.get(&key) {
                // A stored width means the user took manual control. Lock a
                // still-filling column before listeners are installed.
                if column.expands() {
                    column.set_expand(false);
                }
                column.set_fixed_width(width);
            }
        }
    }
}

fn save_widths_now<K: ColumnKey>(
    registry: &ColumnRegistry<K>,
    columns: &[(K, gtk4::ColumnViewColumn)],
) {
    let widths: Vec<(K, i32)> = columns
        .iter()
        .filter(|(_, column)| !column.expands())
        .map(|(key, column)| (*key, column.fixed_width()))
        .collect();
    let serialized = reprise_view::column_widths::serialize_widths(&widths);
    if let Err(error) = settings::set_setting(&registry.conn, registry.keys.widths, &serialized) {
        tracing::warn!(%error, key = registry.keys.widths, "could not persist column widths");
    }
}

/// Wires a debounced listener after policy and stored widths are applied, so
/// setup cannot self-trigger. A first manual resize of an expanding filler
/// locks that column to the fixed width GTK just reported.
fn wire_width_persistence<K: ColumnKey>(registry: &Rc<ColumnRegistry<K>>) {
    let snapshot: Rc<Vec<(K, gtk4::ColumnViewColumn)>> = Rc::new(
        registry
            .columns
            .iter()
            .filter(|(key, _)| key.pin().is_none())
            .map(|(key, column)| (*key, column.clone()))
            .collect(),
    );
    let debounce: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    for (_, column) in snapshot.iter() {
        let registry_weak = Rc::downgrade(registry);
        let snapshot = snapshot.clone();
        let debounce = debounce.clone();
        column.connect_fixed_width_notify(move |column| {
            let Some(registry) = registry_weak.upgrade() else {
                return;
            };
            if !registry.syncing_width.get() && column.expands() {
                column.set_expand(false);
            }
            if let Some(pending) = debounce.borrow_mut().take() {
                pending.remove();
            }
            let registry_weak = Rc::downgrade(&registry);
            let snapshot = snapshot.clone();
            let debounce_inner = debounce.clone();
            let handle = glib::timeout_add_local_once(
                Duration::from_millis(WIDTH_SAVE_DEBOUNCE_MS),
                move || {
                    *debounce_inner.borrow_mut() = None;
                    if let Some(registry) = registry_weak.upgrade() {
                        save_widths_now(&registry, &snapshot);
                    }
                },
            );
            *debounce.borrow_mut() = Some(handle);
        });
    }
}

/// Persists a completed custom header reorder. `syncing_order` mutes the
/// programmatic remove/re-append in `apply`; a short model during mutation is
/// ignored so only the final complete order can reach settings.
fn wire_order_persistence<K: ColumnKey>(registry: &Rc<ColumnRegistry<K>>) {
    let registry_weak = Rc::downgrade(registry);
    registry
        .view
        .columns()
        .connect_items_changed(move |model, _, _, _| {
            let Some(registry) = registry_weak.upgrade() else {
                return;
            };
            if registry.syncing_order.get() {
                return;
            }
            let order: Vec<K> = (0..model.n_items())
                .filter_map(|index| {
                    let column = model
                        .item(index)?
                        .downcast::<gtk4::ColumnViewColumn>()
                        .ok()?;
                    registry
                        .columns
                        .iter()
                        .find(|(_, candidate)| **candidate == column)
                        .map(|(key, _)| *key)
                })
                .collect();
            if order.len() != registry.columns.len() {
                return;
            }
            let visible = order
                .iter()
                .copied()
                .filter(|key| {
                    registry
                        .columns
                        .get(key)
                        .is_some_and(gtk4::ColumnViewColumn::is_visible)
                })
                .collect();
            let serialized = layout::serialize(&layout::normalize(order, visible));
            registry.persist_value(&serialized);
            tracing::debug!(layout = %serialized, "column order persisted after header drag");
        });
}
