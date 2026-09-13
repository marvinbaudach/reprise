//! Live, non-persistent width policy for the default music table.

use std::rc::Rc;

use gtk4::prelude::*;

const COLLAPSE_ORDER: [&str; 4] = ["rating", "year", "duration_ms", "album"];

pub(super) struct FittedColumn {
    pub(super) id: &'static str,
    pub(super) column: gtk4::ColumnViewColumn,
    preferred_visible: bool,
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
        column.column.set_visible(column.preferred_visible);
        if let Some(width) = width(column.id) {
            column.column.set_fixed_width(width);
        }
        column.column.set_expand(column.id == "album");
    }
    let mut used: i32 = columns
        .iter()
        .filter(|column| column.column.is_visible())
        .filter_map(|column| width(column.id))
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
        used -= width(id).unwrap_or_default();
    }
}

pub(super) fn install(view: &gtk4::ColumnView) {
    let view = view.clone();
    view.connect_map(move |view| {
        let columns = fitted_columns(view);
        fit(&columns, view.width());
        let columns = Rc::new(columns);
        view.connect_notify_local(Some("width"), move |view, _| fit(&columns, view.width()));
    });
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
                preferred_visible: column.is_visible(),
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
            preferred_visible: true,
        }
    })
    .collect()
}
