//! Live, non-persistent width policy for the default music table.

use std::cell::Cell;
use std::rc::Rc;

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
        let expected_visible = column.preferred_visible.get() && !column.collapsed.get();
        if column.column.is_visible() != expected_visible {
            column.preferred_visible.set(column.column.is_visible());
        }
        column.collapsed.set(false);
        column.column.set_visible(column.preferred_visible.get());
    }
    let mut used: i32 = columns
        .iter()
        .filter(|column| column.column.is_visible())
        .map(collapse_width)
        .sum();
    for id in COLLAPSE_ORDER {
        if used <= viewport_width {
            break;
        }
        let Some(column) = columns
            .iter()
            .find(|column| column.id == id && column.column.is_visible())
        else {
            continue;
        };
        column.column.set_visible(false);
        column.collapsed.set(true);
        used -= collapse_width(column);
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
    view.connect_map({
        let columns = columns.clone();
        let fitting = fitting.clone();
        move |view| {
            fit_once(&columns, &fitting, view.width());
            let view = view.downgrade();
            let columns = columns.clone();
            let fitting = fitting.clone();
            gtk4::glib::idle_add_local_once(move || {
                if let Some(view) = view.upgrade() {
                    fit_once(&columns, &fitting, view.width());
                }
            });
        }
    });
    view.connect_notify_local(Some("width"), move |view, _| {
        fit_once(&columns, &fitting, view.width());
    });
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
