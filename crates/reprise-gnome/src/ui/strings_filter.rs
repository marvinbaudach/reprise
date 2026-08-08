//! Filter and end-of-results copy, split from the size-limited main catalog.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use crate::ui::end_of_results::ResultsUnit;

fn formatted_count(count: usize) -> String {
    reprise_core::format::format_thousands(count as i64)
}

fn result_count(unit: ResultsUnit, count: usize) -> String {
    let count_text = formatted_count(count);
    let values = [("count", count_text.as_str())];
    match unit {
        ResultsUnit::Tracks => super::plural("{count} track", "{count} tracks", count, &values),
        ResultsUnit::Episodes => {
            super::plural("{count} episode", "{count} episodes", count, &values)
        }
        ResultsUnit::Videos => super::plural("{count} video", "{count} videos", count, &values),
        ResultsUnit::Gaps => super::plural("{count} gap", "{count} gaps", count, &values),
        ResultsUnit::Stations => {
            super::plural("{count} station", "{count} stations", count, &values)
        }
        ResultsUnit::Concerts => {
            super::plural("{count} concert", "{count} concerts", count, &values)
        }
    }
}

pub fn end_of_results_hidden_by_search(unit: ResultsUnit, hidden: usize, query: &str) -> String {
    super::formatted(
        N_!("End of results — {items} hidden by search “{query}”"),
        &[("items", &result_count(unit, hidden)), ("query", query)],
    )
}

pub fn end_of_results_hidden_by_filters(unit: ResultsUnit, hidden: usize) -> String {
    super::formatted(
        N_!("End of results — {items} hidden by active filters"),
        &[("items", &result_count(unit, hidden))],
    )
}

pub fn end_of_results_hidden_by_both(unit: ResultsUnit, hidden: usize) -> String {
    super::formatted(
        N_!("End of results — {items} hidden by search and filters"),
        &[("items", &result_count(unit, hidden))],
    )
}

pub fn end_of_results_show_all(unit: ResultsUnit, total: usize) -> String {
    super::formatted(
        N_!("Show all {items}"),
        &[("items", &result_count(unit, total))],
    )
}

pub fn show_all_tracks_label(total: &str) -> String {
    super::formatted(N_!("Show all {total} tracks"), &[("total", total)])
}

#[cfg(test)]
mod tests {
    use super::*;

    // UX FIL-3a: the copy counts the hidden rows and names the search in the
    // list's own unit.
    #[test]
    fn fil_3a_hidden_copy_counts_names_search_and_unit() {
        assert_eq!(
            end_of_results_hidden_by_search(ResultsUnit::Tracks, 1_649, "falling"),
            "End of results — 1,649 tracks hidden by search “falling”"
        );
        assert_eq!(
            end_of_results_show_all(ResultsUnit::Tracks, 1_664),
            "Show all 1,664 tracks"
        );
        assert_eq!(
            end_of_results_hidden_by_search(ResultsUnit::Videos, 1, "afd"),
            "End of results — 1 video hidden by search “afd”"
        );
    }
}
