//! Precise, generation-guarded viewport restoration after a model reload.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib::prelude::ObjectExt;
use gtk4::prelude::{AdjustmentExt, ScrollableExt};

use super::{reload_restore, Shared};
use crate::ui::adjustment_hold::AdjustmentHold;
use crate::ui::list_geometry::{ListGeometry, RowHeight};

const SCROLL_TO_ADOPTION_WINDOW: Duration = Duration::from_millis(250);

#[derive(Clone, Copy)]
enum RestorePath {
    Initial,
    PageSize,
    ItemsChanged,
    Idle,
}

impl RestorePath {
    const fn apply_probe(self) -> &'static str {
        match self {
            Self::Initial => "anchor.initial.apply",
            Self::PageSize => "anchor.page_size.apply",
            Self::ItemsChanged => "anchor.items_changed.apply",
            Self::Idle => "anchor.idle.apply",
        }
    }

    const fn hold_probe(self) -> &'static str {
        match self {
            Self::Initial => "anchor.initial.hold_target",
            Self::PageSize => "anchor.page_size.hold_target",
            Self::ItemsChanged => "anchor.items_changed.hold_target",
            Self::Idle => "anchor.idle.hold_target",
        }
    }

    const fn scroll_probe(self) -> &'static str {
        match self {
            Self::Initial => "anchor.initial.scroll_to",
            Self::PageSize => "anchor.page_size.scroll_to",
            Self::ItemsChanged => "anchor.items_changed.scroll_to",
            Self::Idle => "anchor.idle.scroll_to",
        }
    }
}

#[derive(Clone, Copy)]
struct ScrollRequest<'a> {
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &'a [i64],
    anchor_position: u32,
}

pub(super) fn schedule(
    shared: &Rc<Shared>,
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &[i64],
    hold: Option<&AdjustmentHold>,
) {
    if crate::ui::scroll_probe::restore_after_allocation_enabled()
        && !has_allocated_viewport(shared)
    {
        arm_refinement(shared, anchor, captured_row_height, current_ids, hold);
        return;
    }
    let Some(anchor_position) = reload_restore::prepaint_position(anchor, current_ids) else {
        return;
    };
    let applied = apply(
        shared,
        anchor,
        captured_row_height,
        current_ids,
        hold,
        RestorePath::Initial,
    );
    if !applied {
        arm_refinement(shared, anchor, captured_row_height, current_ids, hold);
        if !has_allocated_viewport(shared) {
            return;
        }
    }

    scroll_to_anchor(
        shared,
        ScrollRequest {
            anchor,
            captured_row_height,
            current_ids,
            anchor_position,
        },
        applied,
        RestorePath::Initial,
        hold,
    );
}

fn scroll_to_anchor(
    shared: &Shared,
    request: ScrollRequest<'_>,
    applied: bool,
    path: RestorePath,
    hold: Option<&AdjustmentHold>,
) {
    let guard_position = if applied {
        let page = shared
            .column_view
            .vadjustment()
            .map(|value| value.page_size());
        request
            .captured_row_height
            .zip(page)
            .and_then(|(height, page)| {
                reload_restore::prepaint_guard_position(
                    request.anchor,
                    request.current_ids,
                    height.pixels(),
                    page,
                )
            })
            .unwrap_or(request.anchor_position)
    } else {
        request.anchor_position
    };
    let scroll = gtk4::ScrollInfo::new();
    scroll.set_enable_vertical(true);
    let adopt_scroll_value = !applied && !shared.queue_sections.borrow().is_empty();
    let adoption = shared.column_view.vadjustment().and_then(|adjustment| {
        crate::ui::scroll_probe::probe_scroll_to(path.scroll_probe(), &adjustment, guard_position);
        if !adopt_scroll_value {
            return None;
        }
        let before = adjustment.value();
        let hold = hold.cloned()?;
        let handler = Rc::new(RefCell::new(None));
        let callback_handler = handler.clone();
        let writer = path.scroll_probe();
        let id = adjustment.connect_value_changed(move |changed| {
            let handler = callback_handler.borrow_mut().take();
            if let Some(handler) = handler {
                changed.disconnect(handler);
            }
            crate::ui::scroll_probe::probe_value_change(writer, changed, before);
            // `scroll_to` is the final restore writer. Its GTK-computed value
            // includes section geometry that a provisional row-only target
            // cannot know while the rebuilt list is still settling.
            hold.set_target(changed.value());
        });
        handler.borrow_mut().replace(id);
        Some((adjustment, handler))
    });
    shared.column_view.scroll_to(
        guard_position,
        None,
        gtk4::ListScrollFlags::NONE,
        Some(scroll),
    );
    if let Some((adjustment, handler)) = adoption {
        gtk4::glib::timeout_add_local_once(SCROLL_TO_ADOPTION_WINDOW, move || {
            let handler = handler.borrow_mut().take();
            if let Some(handler) = handler {
                adjustment.disconnect(handler);
            }
        });
    }
}

fn has_allocated_viewport(shared: &Shared) -> bool {
    shared
        .column_view
        .vadjustment()
        .is_some_and(|adjustment| adjustment.page_size() > 0.0)
}

fn arm_refinement(
    shared: &Rc<Shared>,
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &[i64],
    hold: Option<&AdjustmentHold>,
) {
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return;
    };
    let generation = shared.model.generation();

    if adjustment.page_size() <= 0.0 {
        let weak_shared = Rc::downgrade(shared);
        let allocated_ids = current_ids.to_owned();
        let allocated_hold = hold.cloned();
        crate::ui::list_geometry_changed::after_first_positive_page_size(&adjustment, move || {
            let Some(shared) = weak_shared.upgrade() else {
                return;
            };
            if !refinement_is_current(&shared, generation) {
                return;
            }
            super::track_list_geometry::remember_after_layout(&shared, allocated_ids.len());
            refine_once(
                &shared,
                generation,
                anchor,
                captured_row_height,
                &allocated_ids,
                allocated_hold.as_ref(),
                RestorePath::PageSize,
            );
        });
        return;
    }

    let restored = Rc::new(Cell::new(false));

    let weak_shared = Rc::downgrade(shared);
    let changed_ids = current_ids.to_owned();
    let changed_hold = hold.cloned();
    let changed_restored = restored.clone();
    crate::ui::list_geometry_changed::after_changed_once(&adjustment, move || {
        if changed_restored.get() {
            return;
        }
        let Some(shared) = weak_shared.upgrade() else {
            return;
        };
        if !refinement_is_current(&shared, generation) {
            return;
        }
        super::track_list_geometry::remember_after_layout(&shared, changed_ids.len());
        if refine_once(
            &shared,
            generation,
            anchor,
            captured_row_height,
            &changed_ids,
            changed_hold.as_ref(),
            RestorePath::ItemsChanged,
        ) {
            changed_restored.set(true);
        }
    });

    // A stable range emits no `changed`; its rows still settle in the pending
    // redraw, so make one post-redraw attempt as the complementary path.
    let weak_shared = Rc::downgrade(shared);
    let idle_ids = current_ids.to_owned();
    let idle_hold = hold.cloned();
    gtk4::glib::idle_add_local_once(move || {
        if restored.get() {
            return;
        }
        let Some(shared) = weak_shared.upgrade() else {
            return;
        };
        if refine_once(
            &shared,
            generation,
            anchor,
            captured_row_height,
            &idle_ids,
            idle_hold.as_ref(),
            RestorePath::Idle,
        ) {
            restored.set(true);
        }
    });
}

fn refine_once(
    shared: &Rc<Shared>,
    generation: u64,
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &[i64],
    hold: Option<&AdjustmentHold>,
    path: RestorePath,
) -> bool {
    if !refinement_is_current(shared, generation) {
        return false;
    }
    if !apply(shared, anchor, captured_row_height, current_ids, hold, path) {
        return false;
    }
    let Some(anchor_position) = reload_restore::prepaint_position(anchor, current_ids) else {
        return false;
    };
    scroll_to_anchor(
        shared,
        ScrollRequest {
            anchor,
            captured_row_height,
            current_ids,
            anchor_position,
        },
        true,
        path,
        hold,
    );
    true
}

fn refinement_is_current(shared: &Shared, generation: u64) -> bool {
    shared.model.generation() == generation && has_allocated_viewport(shared)
}

fn apply(
    shared: &Shared,
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &[i64],
    hold: Option<&AdjustmentHold>,
    path: RestorePath,
) -> bool {
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return false;
    };
    crate::ui::scroll_probe::probe_rows("apply_scroll_anchor", &shared.column_view);
    if current_ids.is_empty() {
        return false;
    }
    if adjustment.page_size() <= 0.0 {
        return false;
    }
    let n_sections = shared.queue_sections.borrow().len();
    let geometry = ListGeometry::for_view(&shared.column_view);
    let Some(height) = captured_row_height
        .or_else(|| {
            geometry.observed_row_height(
                &shared.conn,
                &shared.list_geometry_cache,
                current_ids.len(),
                n_sections,
            )
        })
        .map(RowHeight::pixels)
    else {
        return false;
    };
    let Some(target) =
        reload_restore::scroll_target(anchor, current_ids, height, adjustment.page_size())
    else {
        return false;
    };
    if std::env::var_os("REPRISE_SCROLL_PROBE").is_some() {
        eprintln!(
            "SCROLLMODEL path={} anchor={anchor:?} position={:?} row_height={height:.1} \
             sections={:?} target={target:.1}",
            path.apply_probe(),
            anchor.and_then(|(track_id, _)| current_ids.iter().position(|id| *id == track_id)),
            shared
                .queue_sections
                .borrow()
                .iter()
                .map(|section| (section.start, section.len))
                .collect::<Vec<_>>()
        );
    }
    let provisional_sectioned_refinement = !matches!(path, RestorePath::Initial) && n_sections > 0;
    if !provisional_sectioned_refinement {
        set_hold_target(hold, &adjustment, target, path);
    }
    if !crate::ui::scroll_probe::preseed_suppressed() {
        geometry.configure(
            &adjustment,
            target,
            &shared.conn,
            &shared.list_geometry_cache,
            current_ids.len(),
            n_sections,
        );
    }
    if !geometry.is_settled(adjustment.upper(), current_ids.len(), n_sections) {
        return false;
    }
    if provisional_sectioned_refinement {
        // A refinement that cannot prove settled geometry must not replace the
        // value adopted from `scroll_to`; that stale replacement caused the
        // one-row Queue settle. Settled refinements remain authoritative.
        set_hold_target(hold, &adjustment, target, path);
    }
    geometry.remember_if_settled(
        &shared.conn,
        &shared.list_geometry_cache,
        adjustment.upper(),
        current_ids.len(),
        n_sections,
    );
    crate::ui::scroll_probe::probe(path.apply_probe(), &adjustment, target);
    debug_assert!(
        !crate::ui::list_geometry_changed::in_changed_emission(),
        "scroll anchor written from inside a changed emission"
    );
    adjustment.set_value(target);
    true
}

fn set_hold_target(
    hold: Option<&AdjustmentHold>,
    adjustment: &gtk4::Adjustment,
    target: f64,
    path: RestorePath,
) {
    if let Some(hold) = hold {
        crate::ui::scroll_probe::probe(path.hold_probe(), adjustment, target);
        hold.set_target(target);
    }
}
