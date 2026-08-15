macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::{formatted, plural};

pub const DOCTOR_CLEAR_FILTERS: &str = N_!("Clear Filters");

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
        N_!("{count} fix is hidden by this search."),
        N_!("{count} fixes are hidden by this search."),
        count,
        &[("count", &count.to_string())],
    )
}

pub fn doctor_category_no_match_title(category: &str) -> String {
    formatted(N_!("No matches in {category}"), &[("category", category)])
}

pub fn doctor_category_no_match_description(count: usize, category: &str) -> String {
    plural(
        N_!("{count} fix is hidden by the {category} filter."),
        N_!("{count} fixes are hidden by the {category} filter."),
        count,
        &[("count", &count.to_string()), ("category", category)],
    )
}

pub fn doctor_query_and_category_no_match_title(query: &str, category: &str) -> String {
    formatted(
        N_!("No matches for “{query}” in {category}"),
        &[("query", query), ("category", category)],
    )
}

pub fn doctor_query_and_category_no_match_description(
    count: usize,
    query: &str,
    category: &str,
) -> String {
    plural(
        N_!("{count} fix is hidden by search “{query}” and the {category} filter."),
        N_!("{count} fixes are hidden by search “{query}” and the {category} filter."),
        count,
        &[
            ("count", &count.to_string()),
            ("query", query),
            ("category", category),
        ],
    )
}
