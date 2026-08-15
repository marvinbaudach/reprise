macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, plural};

pub const DOCTOR_CLEAR_SEARCH: &str = N_!("Clear Search");

pub fn doctor_review_query_scope(query: &str) -> String {
    formatted(N_!("search “{query}”"), &[("query", query)])
}

pub fn doctor_review_query_and_category_scope(category: &str, query: &str) -> String {
    formatted(
        N_!("{category} and search “{query}”"),
        &[("category", category), ("query", query)],
    )
}

pub fn doctor_no_match_title(query: &str) -> String {
    formatted(N_!("No matches for “{query}”"), &[("query", query)])
}

pub fn doctor_no_match_description(count: usize) -> String {
    plural(
        "{count} fix is hidden by this search.",
        "{count} fixes are hidden by this search.",
        count,
        &[("count", &count.to_string())],
    )
}
