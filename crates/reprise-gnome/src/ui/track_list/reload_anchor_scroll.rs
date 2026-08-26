//! Precise, generation-guarded viewport restoration after a model reload.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib::prelude::ObjectExt;
use gtk4::prelude::{AdjustmentExt, ScrollableExt, WidgetExtManual};

use super::{reload_restore, Shared};
use crate::ui::adjustment_hold::AdjustmentHold;
use crate::ui::list_geometry::{ListGeometry, RowHeight};
use crate::ui::list_geometry_layout::{ListLayout, CONTENT_HEIGHT_EPSILON};

const SCROLL_TO_ADOPTION_WINDOW: Duration = Duration::from_millis(250);

#[derive(Clone, Copy)]
enum RestorePath {
    Initial,
    PageSize,
    ItemsChanged,
    Idle,
}

#[derive(Clone, Copy)]
enum AnchorPlacement {
    PreserveOffset,
    Center,
}

#[derive(Clone, Copy)]
struct RestoreAttempt {
    path: RestorePath,
    placement: AnchorPlacement,
}

enum ApplyResult {
    Applied(ListLayout),
    Pending,
    StoodDown,
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
    placement: AnchorPlacement,
}

#[derive(Clone)]
struct ScrollAdoptionGeometry {
    guard_position: u32,
    row_count: usize,
    layout: Rc<ListLayout>,
    before: f64,
}

impl ScrollAdoptionGeometry {
    fn new(
        guard_position: u32,
        row_count: usize,
        expected_section_count: usize,
        layout: Rc<ListLayout>,
        before: f64,
    ) -> Option<Self> {
        if row_count == 0
            || expected_section_count == 0
            || layout.section_count() != expected_section_count
            || layout.headers_above(guard_position) > expected_section_count
            || guard_position as usize >= row_count
        {
            return None;
        }
        Some(Self {
            guard_position,
            row_count,
            layout,
            before,
        })
    }

    fn matches(&self, candidate: f64, lower: f64, upper: f64, page_size: f64) -> bool {
        if !candidate.is_finite()
            || !self.before.is_finite()
            || !lower.is_finite()
            || !upper.is_finite()
            || !page_size.is_finite()
            || upper < lower
            || page_size < 0.0
        {
            return false;
        }

        let Some(layout) = self
            .layout
            .infer_section_header_from_observed_upper(self.row_count, upper)
        else {
            return false;
        };
        let guard_top = layout.row_top(self.guard_position);
        let requested = guard_top.clamp(lower, (upper - page_size).max(lower));
        let candidate_error = (candidate - requested).abs();
        let before_error = (self.before - requested).abs();
        candidate_error <= CONTENT_HEIGHT_EPSILON && candidate_error < before_error
    }
}

pub(super) fn schedule(
    shared: &Rc<Shared>,
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &[i64],
    hold: Option<&AdjustmentHold>,
) {
    schedule_with_placement(
        shared,
        anchor,
        captured_row_height,
        current_ids,
        hold,
        AnchorPlacement::PreserveOffset,
    );
}

pub(super) fn schedule_centered(
    shared: &Rc<Shared>,
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &[i64],
    hold: Option<&AdjustmentHold>,
) {
    schedule_with_placement(
        shared,
        anchor,
        captured_row_height,
        current_ids,
        hold,
        AnchorPlacement::Center,
    );
}

fn schedule_with_placement(
    shared: &Rc<Shared>,
    anchor: Option<(i64, f64)>,
    captured_row_height: Option<RowHeight>,
    current_ids: &[i64],
    hold: Option<&AdjustmentHold>,
    placement: AnchorPlacement,
) {
    if crate::ui::scroll_probe::restore_after_allocation_enabled()
        && !has_allocated_viewport(shared)
    {
        arm_refinement(
            shared,
            anchor,
            captured_row_height,
            current_ids,
            hold,
            placement,
        );
        return;
    }
    let Some(anchor_position) = reload_restore::prepaint_position(anchor, current_ids) else {
        return;
    };
    let applied_layout = match apply(
        shared,
        anchor,
        captured_row_height,
        current_ids,
        hold,
        RestoreAttempt {
            path: RestorePath::Initial,
            placement,
        },
    ) {
        ApplyResult::Applied(layout) => Some(layout),
        ApplyResult::Pending => None,
        ApplyResult::StoodDown => return,
    };
    if applied_layout.is_none() {
        arm_refinement(
            shared,
            anchor,
            captured_row_height,
            current_ids,
            hold,
            placement,
        );
        if !has_allocated_viewport(shared) {
            return;
        }
    }

    // A centered reveal can name the row edge GTK must adopt even while the
    // live range is still settling. `apply` intentionally returns `None` in
    // that case so no provisional pixel target becomes authoritative; the
    // same provisional layout is nevertheless sufficient to choose the row
    // whose realized edge will be nearest the arithmetic centre.
    let centered_layout = (applied_layout.is_none()
        && matches!(placement, AnchorPlacement::Center))
    .then(|| super::track_list_geometry::layout(shared, captured_row_height, current_ids.len()))
    .flatten();
    let guard_layout = applied_layout.as_ref().or(centered_layout.as_ref());
    if matches!(placement, AnchorPlacement::Center) && guard_layout.is_none() {
        return;
    }

    scroll_to_anchor(
        shared,
        ScrollRequest {
            anchor,
            captured_row_height,
            current_ids,
            anchor_position,
            placement,
        },
        guard_layout,
        RestorePath::Initial,
        hold,
    );
}

fn scroll_to_anchor(
    shared: &Shared,
    request: ScrollRequest<'_>,
    applied_layout: Option<&ListLayout>,
    path: RestorePath,
    hold: Option<&AdjustmentHold>,
) {
    let section_starts = shared
        .queue_sections
        .borrow()
        .iter()
        .map(|section| section.start)
        .collect::<Vec<_>>();
    // `Some` means the layout is settled. An offset-preserving restore asks
    // GTK to keep the last visible row in view; a centered reveal instead
    // gives GTK the row edge nearest the arithmetic centre, so GTK's realized
    // row edge owns the single viewport landing.
    let guard_position = if let Some(layout) = applied_layout {
        let page = shared
            .column_view
            .vadjustment()
            .map(|value| value.page_size());
        page.and_then(|page| match request.placement {
            AnchorPlacement::PreserveOffset => reload_restore::prepaint_guard_position(
                request.anchor,
                request.current_ids,
                layout,
                page,
            ),
            AnchorPlacement::Center => {
                let position =
                    reload_restore::prepaint_position(request.anchor, request.current_ids)?;
                super::centered_scroll_restore::centered_anchor(
                    layout,
                    position,
                    request.current_ids.len(),
                    page,
                )
                .map(|(anchor, _)| anchor)
            }
        })
        .unwrap_or(request.anchor_position)
    } else {
        request.anchor_position
    };
    let section_count = section_starts.len();
    let scroll = gtk4::ScrollInfo::new();
    scroll.set_enable_vertical(true);
    let adoption_geometry = if applied_layout.is_none() {
        let row_height = request.captured_row_height.unwrap_or_else(|| {
            ListGeometry::for_view(&shared.column_view)
                .row_height(&shared.conn, &shared.list_geometry_cache)
        });
        // Adoption re-infers the header height from `upper`; only row height
        // and section starts survive into `matches`. A valid placeholder here
        // preserves every topology guard without the discarded cache/DB read.
        let layout = ListLayout::sectioned(row_height, row_height, section_starts);
        ScrollAdoptionGeometry::new(
            guard_position,
            request.current_ids.len(),
            section_count,
            Rc::new(layout),
            0.0,
        )
    } else {
        None
    };
    let adoption = shared.column_view.vadjustment().and_then(|adjustment| {
        crate::ui::scroll_probe::probe_scroll_to(path.scroll_probe(), &adjustment, guard_position);
        let before = adjustment.value();
        let hold_lifetime = Rc::new(hold.cloned()?);
        let weak_hold = Rc::downgrade(&hold_lifetime);
        let handler = Rc::new(RefCell::new(None));
        let callback_handler = handler.clone();
        let writer = path.scroll_probe();
        if matches!(request.placement, AnchorPlacement::Center) {
            let id = adjustment.connect_value_changed(move |changed| {
                crate::ui::scroll_probe::probe_value_change(writer, changed, before);
                let Some(hold) = weak_hold.upgrade() else {
                    return;
                };
                let handler = callback_handler.borrow_mut().take();
                if let Some(handler) = handler {
                    changed.disconnect(handler);
                }
                // Centering deliberately lets GTK's realized row edge own
                // the only landing. Once that edge arrives, the reload hold
                // protects it rather than the provisional arithmetic target.
                hold.set_target(changed.value());
            });
            handler.borrow_mut().replace(id);
            return Some((adjustment, handler, hold_lifetime));
        }
        let mut geometry = adoption_geometry?;
        geometry.before = before;
        let id = adjustment.connect_value_changed(move |changed| {
            crate::ui::scroll_probe::probe_value_change(writer, changed, before);
            if !geometry.matches(
                changed.value(),
                changed.lower(),
                changed.upper(),
                changed.page_size(),
            ) {
                return;
            }
            let Some(hold) = weak_hold.upgrade() else {
                return;
            };
            let handler = callback_handler.borrow_mut().take();
            if let Some(handler) = handler {
                changed.disconnect(handler);
            }
            // `scroll_to` is the final restore writer. Its GTK-computed value
            // includes section geometry that a provisional row-only target
            // cannot know while the rebuilt list is still settling.
            hold.set_target(changed.value());
        });
        handler.borrow_mut().replace(id);
        Some((adjustment, handler, hold_lifetime))
    });
    shared.column_view.scroll_to(
        guard_position,
        None,
        gtk4::ListScrollFlags::NONE,
        Some(scroll),
    );
    if let Some((adjustment, handler, hold_lifetime)) = adoption {
        gtk4::glib::timeout_add_local_once(SCROLL_TO_ADOPTION_WINDOW, move || {
            let handler = handler.borrow_mut().take();
            if let Some(handler) = handler {
                adjustment.disconnect(handler);
            }
            drop(hold_lifetime);
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
    placement: AnchorPlacement,
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
                RestoreAttempt {
                    path: RestorePath::PageSize,
                    placement,
                },
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
            RestoreAttempt {
                path: RestorePath::ItemsChanged,
                placement,
            },
        ) {
            changed_restored.set(true);
        }
    });

    // A stable range emits no `changed`; its rows still settle in the pending
    // redraw, so make one post-redraw attempt as the complementary path.
    let weak_shared = Rc::downgrade(shared);
    let idle_ids = current_ids.to_owned();
    let idle_hold = hold.cloned();
    let view = shared.column_view.clone();
    let frames_left = Cell::new(8_u8);
    view.add_tick_callback(move |_, _| {
        if restored.get() {
            return gtk4::glib::ControlFlow::Break;
        }
        let Some(shared) = weak_shared.upgrade() else {
            return gtk4::glib::ControlFlow::Break;
        };
        if refine_once(
            &shared,
            generation,
            anchor,
            captured_row_height,
            &idle_ids,
            idle_hold.as_ref(),
            RestoreAttempt {
                path: RestorePath::Idle,
                placement,
            },
        ) {
            restored.set(true);
            return gtk4::glib::ControlFlow::Break;
        }
        let Some(remaining) = frames_left.get().checked_sub(1) else {
            return gtk4::glib::ControlFlow::Break;
        };
        frames_left.set(remaining);
        if remaining == 0 {
            gtk4::glib::ControlFlow::Break
        } else {
            gtk4::glib::ControlFlow::Continue
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
    attempt: RestoreAttempt,
) -> bool {
    if !refinement_is_current(shared, generation) {
        return false;
    }
    let layout = match apply(
        shared,
        anchor,
        captured_row_height,
        current_ids,
        hold,
        attempt,
    ) {
        ApplyResult::Applied(layout) => layout,
        ApplyResult::Pending => return false,
        ApplyResult::StoodDown => return true,
    };
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
            placement: attempt.placement,
        },
        Some(&layout),
        attempt.path,
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
    attempt: RestoreAttempt,
) -> ApplyResult {
    let section_spans = shared
        .queue_sections
        .borrow()
        .iter()
        .map(|section| (section.start, section.len))
        .collect::<Vec<_>>();
    let n_sections = section_spans.len();
    let Some(adjustment) = shared.column_view.vadjustment() else {
        return ApplyResult::Pending;
    };
    crate::ui::scroll_probe::probe_rows("apply_scroll_anchor", &shared.column_view);
    if current_ids.is_empty() {
        return ApplyResult::Pending;
    }
    if adjustment.page_size() <= 0.0 {
        return ApplyResult::Pending;
    }
    let geometry = ListGeometry::for_view(&shared.column_view);
    let Some(layout) =
        super::track_list_geometry::layout(shared, captured_row_height, current_ids.len())
    else {
        return ApplyResult::Pending;
    };
    let Some(target) = scroll_target(
        anchor,
        current_ids,
        &layout,
        adjustment.page_size(),
        attempt.placement,
    ) else {
        return ApplyResult::Pending;
    };
    if super::restore_intent::deliberate_destination_outranks(
        shared,
        &adjustment,
        layout.row_height(),
        attempt.path.hold_probe(),
        target,
    ) {
        return ApplyResult::StoodDown;
    }
    if std::env::var_os("REPRISE_SCROLL_PROBE").is_some() {
        eprintln!(
            "SCROLLMODEL path={} anchor={anchor:?} position={:?} row_height={height:.1} \
             sections={:?} target={target:.1}",
            attempt.path.apply_probe(),
            anchor.and_then(|(track_id, _)| current_ids.iter().position(|id| *id == track_id)),
            section_spans,
            height = layout.row_height().pixels(),
        );
    }
    let provisional_sectioned_refinement =
        !matches!(attempt.path, RestorePath::Initial) && n_sections > 0;
    if !provisional_sectioned_refinement
        && matches!(attempt.placement, AnchorPlacement::PreserveOffset)
    {
        set_hold_target(hold, &adjustment, target, attempt.path);
    }
    if !crate::ui::scroll_probe::preseed_suppressed() {
        let seed_target = match attempt.placement {
            AnchorPlacement::PreserveOffset => target,
            AnchorPlacement::Center => adjustment.value(),
        };
        geometry.configure(
            &adjustment,
            seed_target,
            &shared.conn,
            &shared.list_geometry_cache,
            current_ids.len(),
            n_sections,
        );
    }
    // Flat-list layout no longer waits for a persistence witness, but a range
    // this service just seeded is still provisional. Once GTK authors the
    // range, its quotient is the reveal authority even if recycled widgets
    // remain mixed. Sectioned lists additionally need realized headers before
    // their guard row can be trusted.
    if !crate::ui::list_geometry::gtk_authored(
        &shared.list_geometry_cache,
        adjustment.upper(),
        current_ids.len(),
    ) || (n_sections > 0
        && !geometry.is_settled(adjustment.upper(), current_ids.len(), n_sections))
    {
        return ApplyResult::Pending;
    }
    if provisional_sectioned_refinement
        && matches!(attempt.placement, AnchorPlacement::PreserveOffset)
    {
        // The row-only target is provisional while section geometry settles.
        // Deferring this write until after the settled check makes an early
        // return leave the existing hold target alone; settled refinements
        // remain authoritative.
        set_hold_target(hold, &adjustment, target, attempt.path);
    }
    geometry.remember_if_settled(
        &shared.conn,
        &shared.list_geometry_cache,
        adjustment.upper(),
        current_ids.len(),
        n_sections,
    );
    if matches!(attempt.placement, AnchorPlacement::PreserveOffset) {
        crate::ui::scroll_probe::probe(attempt.path.apply_probe(), &adjustment, target);
        debug_assert!(
            !crate::ui::list_geometry_changed::in_changed_emission(),
            "scroll anchor written from inside a changed emission"
        );
        adjustment.set_value(target);
    }
    ApplyResult::Applied(layout)
}

fn scroll_target(
    anchor: Option<(i64, f64)>,
    current_ids: &[i64],
    layout: &ListLayout,
    page_size: f64,
    placement: AnchorPlacement,
) -> Option<f64> {
    match placement {
        AnchorPlacement::PreserveOffset => {
            reload_restore::scroll_target(anchor, current_ids, layout, page_size)
        }
        AnchorPlacement::Center => {
            let position = reload_restore::prepaint_position(anchor, current_ids)?;
            super::centered_scroll_restore::centered_anchor(
                layout,
                position,
                current_ids.len(),
                page_size,
            )
            .map(|(_, target)| target)
        }
    }
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

#[cfg(test)]
#[path = "reload_anchor_scroll_tests.rs"]
mod tests;
