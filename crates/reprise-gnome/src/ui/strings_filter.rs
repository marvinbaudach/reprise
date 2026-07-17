//! Filter and end-of-results copy, split from the size-limited main catalog.

macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

pub fn end_of_results_hidden_by_search(hidden: &str, query: &str) -> String {
    super::formatted(
        N_!("End of results — {hidden} tracks hidden by search “{query}”"),
        &[("hidden", hidden), ("query", query)],
    )
}

pub fn end_of_results_hidden_by_filters(hidden: &str) -> String {
    super::formatted(
        N_!("End of results — {hidden} tracks hidden by active filters"),
        &[("hidden", hidden)],
    )
}

pub fn end_of_results_hidden_by_both(hidden: &str) -> String {
    super::formatted(
        N_!("End of results — {hidden} tracks hidden by search and filters"),
        &[("hidden", hidden)],
    )
}

pub fn show_all_tracks_label(total: &str) -> String {
    super::formatted(N_!("Show all {total} tracks"), &[("total", total)])
}

#[cfg(test)]
mod tests {
    use super::*;

    // UX FIL-3: the copy counts the hidden tracks and names the search.
    #[test]
    fn fil_3_hidden_copy_counts_and_names_the_search() {
        assert_eq!(
            end_of_results_hidden_by_search("1,649", "falling"),
            "End of results — 1,649 tracks hidden by search “falling”"
        );
        assert_eq!(show_all_tracks_label("1,664"), "Show all 1,664 tracks");
    }
}
