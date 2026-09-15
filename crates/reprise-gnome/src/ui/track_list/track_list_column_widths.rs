//! Live, non-persistent width policy for the default music table.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

const COLLAPSE_ORDER: [&str; 4] = ["rating", "year", "duration_ms", "album"];

pub(super) struct FittedColumn {
    pub(super) id: &'static str,
    pub(super) column: gtk4::ColumnViewColumn,
    preferred_visible: Cell<bool>,
    collapsed: Cell<bool>,
}

fn width(id: &str) -> Option<i32> {
    match id {
        "cover" => Some(40),
        "title" => Some(160),
        "artist" => Some(200),
        "album" => Some(220),
        "year" => Some(64),
        "duration_ms" => Some(72),
        "rating" => Some(88),
        _ => None,
    }
}

pub(super) fn fit(columns: &[FittedColumn], viewport_width: i32) {
    for column in columns {
        if let Some(preferred_visible) =
            crate::ui::table_columns::registry::preferred_visibility(&column.column)
        {
            column.preferred_visible.set(preferred_visible);
        }
        column.collapsed.set(false);
    }
    let mut used: i32 = columns
        .iter()
        .filter(|column| column.preferred_visible.get())
        .map(collapse_width)
        .sum();
    for id in COLLAPSE_ORDER {
        if used <= viewport_width {
            break;
        }
        let Some(column) = columns
            .iter()
            .find(|column| column.id == id && column.preferred_visible.get())
        else {
            continue;
        };
        column.collapsed.set(true);
        used -= collapse_width(column);
    }
    for column in columns {
        column
            .column
            .set_visible(column.preferred_visible.get() && !column.collapsed.get());
    }
}

fn collapse_width(column: &FittedColumn) -> i32 {
    let live_width = column.column.fixed_width();
    if live_width > 0 {
        live_width
    } else {
        width(column.id).unwrap_or_default()
    }
}

pub(super) fn install(view: &gtk4::ColumnView) {
    let columns = Rc::new(fitted_columns(view));
    let fitting = Rc::new(Cell::new(false));
    let pending_width = Rc::new(Cell::new(None));
    let fit_timeout = Rc::new(RefCell::new(None));
    let viewport_signal: Rc<RefCell<Option<(gtk4::Adjustment, gtk4::glib::SignalHandlerId)>>> =
        Rc::new(RefCell::new(None));
    view.connect_map({
        let columns = columns.clone();
        let fitting = fitting.clone();
        let pending_width = pending_width.clone();
        let fit_timeout = fit_timeout.clone();
        let viewport_signal = viewport_signal.clone();
        move |view| {
            let adjustment = viewport_adjustment(view);
            fit_once(
                &columns,
                &fitting,
                viewport_width(adjustment.as_ref(), view.width()),
            );
            if let Some((old_adjustment, old_signal)) = viewport_signal.borrow_mut().take() {
                old_adjustment.disconnect(old_signal);
            }
            if let Some(adjustment) = adjustment {
                let columns_for_viewport = columns.clone();
                let fitting_for_viewport = fitting.clone();
                let pending_width_for_viewport = pending_width.clone();
                let fit_timeout_for_viewport = fit_timeout.clone();
                let view_for_viewport = view.downgrade();
                let signal =
                    adjustment.connect_notify_local(Some("page-size"), move |adjustment, _| {
                        if let Some(view) = view_for_viewport.upgrade() {
                            let viewport_width = viewport_width(Some(adjustment), view.width());
                            schedule_fit(
                                &columns_for_viewport,
                                &fitting_for_viewport,
                                &pending_width_for_viewport,
                                &fit_timeout_for_viewport,
                                viewport_width,
                            );
                        }
                    });
                viewport_signal.replace(Some((adjustment, signal)));
            }
            let view = view.downgrade();
            if let Some(view) = view.upgrade() {
                schedule_fit(
                    &columns,
                    &fitting,
                    &pending_width,
                    &fit_timeout,
                    viewport_width(viewport_adjustment(&view).as_ref(), view.width()),
                );
            }
        }
    });
    view.connect_notify_local(Some("width"), {
        let columns = columns.clone();
        let fitting = fitting.clone();
        let pending_width = pending_width.clone();
        let fit_timeout = fit_timeout.clone();
        move |view, _| {
            schedule_fit(
                &columns,
                &fitting,
                &pending_width,
                &fit_timeout,
                viewport_width(viewport_adjustment(view).as_ref(), view.width()),
            );
        }
    });
}

fn viewport_adjustment(view: &gtk4::ColumnView) -> Option<gtk4::Adjustment> {
    view.parent()
        .and_then(|parent| parent.downcast::<gtk4::ScrolledWindow>().ok())
        .map(|scrolled| scrolled.hadjustment())
        .or_else(|| view.hadjustment())
}

fn viewport_width(adjustment: Option<&gtk4::Adjustment>, fallback: i32) -> i32 {
    let page_size = adjustment.map_or(0, |adjustment| adjustment.page_size().floor() as i32);
    if page_size > 0 {
        page_size
    } else {
        fallback
    }
}

fn schedule_fit(
    columns: &Rc<Vec<FittedColumn>>,
    fitting: &Rc<Cell<bool>>,
    pending_width: &Rc<Cell<Option<i32>>>,
    fit_timeout: &Rc<RefCell<Option<gtk4::glib::SourceId>>>,
    viewport_width: i32,
) {
    pending_width.set(Some(viewport_width));
    if let Some(timeout) = fit_timeout.borrow_mut().take() {
        timeout.remove();
    }
    let columns = columns.clone();
    let fitting = fitting.clone();
    let pending_width = pending_width.clone();
    let fit_timeout_for_callback = fit_timeout.clone();
    let timeout = gtk4::glib::timeout_add_local_once(Duration::from_millis(16), move || {
        fit_timeout_for_callback.borrow_mut().take();
        if let Some(viewport_width) = pending_width.take() {
            fit_once(&columns, &fitting, viewport_width);
        }
    });
    fit_timeout.replace(Some(timeout));
}

fn fit_once(columns: &[FittedColumn], fitting: &Cell<bool>, viewport_width: i32) {
    if fitting.replace(true) {
        return;
    }
    fit(columns, viewport_width);
    fitting.set(false);
}

fn fitted_columns(view: &gtk4::ColumnView) -> Vec<FittedColumn> {
    let model = view.columns();
    (0..model.n_items())
        .filter_map(|index| model.item(index))
        .filter_map(|item| item.downcast::<gtk4::ColumnViewColumn>().ok())
        .filter_map(|column| {
            let id = match column.id().as_deref()? {
                "cover" => "cover",
                "title" => "title",
                "artist" => "artist",
                "album" => "album",
                "year" => "year",
                "duration_ms" => "duration_ms",
                "rating" => "rating",
                _ => return None,
            };
            Some(FittedColumn {
                id,
                preferred_visible: Cell::new(column.is_visible()),
                collapsed: Cell::new(false),
                column,
            })
        })
        .collect()
}

#[cfg(test)]
pub(super) fn test_columns(view: &gtk4::ColumnView) -> Vec<FittedColumn> {
    [
        "cover",
        "title",
        "artist",
        "album",
        "year",
        "duration_ms",
        "rating",
    ]
    .into_iter()
    .map(|id| {
        let column = gtk4::ColumnViewColumn::new(Some(id), None::<gtk4::ListItemFactory>);
        column.set_id(Some(id));
        view.append_column(&column);
        FittedColumn {
            id,
            column,
            preferred_visible: Cell::new(true),
            collapsed: Cell::new(false),
        }
    })
    .collect()
}
