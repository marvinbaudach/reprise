use std::collections::BTreeMap;
use std::fs::OpenOptions;
use std::io::BufWriter;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4::gio::prelude::ListModelExt;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use serde::Serialize;

use super::track_list::TrackList;

const REPORT_ENV: &str = "REPRISE_PERF_RUNTIME_REPORT";

#[derive(Debug, Serialize)]
struct RuntimeMetrics {
    schema_version: u32,
    model_items: u32,
    column_factories: u32,
    visible_columns: u32,
    cached_windows: usize,
    cached_tracks: usize,
    total_window_widgets: usize,
    column_view_widgets: usize,
    row_widgets: usize,
    cell_widgets: usize,
    column_view_type_counts: BTreeMap<String, usize>,
}

fn widget_type_counts(root: &gtk4::Widget) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        *counts.entry(widget.type_().name().to_string()).or_default() += 1;
        let mut child = widget.first_child();
        while let Some(widget) = child {
            child = widget.next_sibling();
            pending.push(widget);
        }
    }
    counts
}

fn count_types(counts: &BTreeMap<String, usize>, needle: &str) -> usize {
    counts
        .iter()
        .filter(|(name, _)| name.contains(needle))
        .map(|(_, count)| count)
        .sum()
}

fn collect(window: &adw::ApplicationWindow, track_list: &TrackList) -> RuntimeMetrics {
    let window_counts = widget_type_counts(window.upcast_ref());
    let column_view = &track_list.shared.column_view;
    let column_counts = widget_type_counts(column_view.upcast_ref());
    let columns = column_view.columns();
    let visible_columns = (0..columns.n_items())
        .filter(|position| {
            columns
                .item(*position)
                .and_then(|column| column.downcast::<gtk4::ColumnViewColumn>().ok())
                .is_some_and(|column| column.is_visible())
        })
        .count();
    let (cached_windows, cached_tracks) = track_list.shared.model.cache_usage();

    RuntimeMetrics {
        schema_version: 1,
        model_items: track_list.shared.model.n_items(),
        column_factories: columns.n_items(),
        visible_columns: u32::try_from(visible_columns).unwrap_or(u32::MAX),
        cached_windows,
        cached_tracks,
        total_window_widgets: window_counts.values().sum(),
        column_view_widgets: column_counts.values().sum(),
        row_widgets: count_types(&column_counts, "ColumnViewRow"),
        cell_widgets: count_types(&column_counts, "ColumnViewCell"),
        column_view_type_counts: column_counts,
    }
}

pub(in crate::ui) fn arm(window: &adw::ApplicationWindow, track_list: &Rc<TrackList>) {
    let Ok(report_path) = std::env::var(REPORT_ENV) else {
        return;
    };
    let report_path = PathBuf::from(report_path);
    let window = window.clone();
    let track_list = track_list.clone();
    glib::timeout_add_local_once(Duration::from_millis(250), move || {
        let report = collect(&window, &track_list);
        let result = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&report_path)
            .map(BufWriter::new)
            .map_err(serde_json::Error::io)
            .and_then(|writer| serde_json::to_writer(writer, &report));
        match result {
            Ok(()) => {
                tracing::info!(path = %report_path.display(), "runtime performance report written");
            }
            Err(error) => {
                tracing::error!(%error, path = %report_path.display(), "runtime performance report failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_metrics_serialize_a_stable_public_report() {
        let report = RuntimeMetrics {
            schema_version: 1,
            model_items: 100_000,
            column_factories: 9,
            visible_columns: 7,
            cached_windows: 2,
            cached_tracks: 400,
            total_window_widgets: 800,
            column_view_widgets: 300,
            row_widgets: 24,
            cell_widgets: 168,
            column_view_type_counts: BTreeMap::from([
                ("GtkColumnViewCellWidget".to_string(), 168),
                ("GtkColumnViewRowWidget".to_string(), 24),
            ]),
        };

        assert_eq!(
            serde_json::to_value(report).unwrap(),
            serde_json::json!({
                "schema_version": 1,
                "model_items": 100000,
                "column_factories": 9,
                "visible_columns": 7,
                "cached_windows": 2,
                "cached_tracks": 400,
                "total_window_widgets": 800,
                "column_view_widgets": 300,
                "row_widgets": 24,
                "cell_widgets": 168,
                "column_view_type_counts": {
                    "GtkColumnViewCellWidget": 168,
                    "GtkColumnViewRowWidget": 24
                }
            })
        );
    }
}
