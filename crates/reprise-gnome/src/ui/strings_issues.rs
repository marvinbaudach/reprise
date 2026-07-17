macro_rules! N_ {
    ($message:literal) => {
        $message
    };
}

use super::plural;

pub const ISSUE_SHOW_ONE_MORE: &str = N_!("Show 1 more");
pub const ISSUE_SHOW_MORE: &str = N_!("Show {count} more");

pub fn issue_show_more(count: u32) -> String {
    let count_text = count.to_string();
    let count = usize::try_from(count).unwrap_or(usize::MAX);
    plural(
        ISSUE_SHOW_ONE_MORE,
        ISSUE_SHOW_MORE,
        count,
        &[("count", &count_text)],
    )
}
