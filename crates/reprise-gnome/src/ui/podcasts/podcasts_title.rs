//! Pure title-tail presentation for grouped YouTube episodes.

use std::collections::{BTreeMap, BTreeSet};

const SEPARATORS: [char; 4] = ['|', '–', '-', '•'];
const MIN_MATCHING_TITLES: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TitleParts {
    pub distinct: String,
    pub dimmed: Option<String>,
}

/// Splits a title only when at least three titles in the group share the
/// longest exact suffix that starts at a supported separator.
pub(super) fn split_repeated_suffix(titles: &[&str], title: &str) -> TitleParts {
    let mut occurrences = BTreeMap::<String, BTreeSet<usize>>::new();
    for (title_index, candidate_title) in titles.iter().enumerate() {
        for (byte_index, character) in candidate_title.char_indices() {
            if !SEPARATORS.contains(&character) {
                continue;
            }
            let suffix = candidate_title[byte_index..].trim();
            if suffix.chars().count() <= 1 {
                continue;
            }
            occurrences
                .entry(suffix.to_owned())
                .or_default()
                .insert(title_index);
        }
    }

    let common = occurrences
        .into_iter()
        .filter(|(_, titles)| titles.len() >= MIN_MATCHING_TITLES)
        .map(|(suffix, _)| suffix)
        .max_by_key(|suffix| suffix.chars().count());
    let Some(common) = common else {
        return TitleParts {
            distinct: title.to_owned(),
            dimmed: None,
        };
    };
    let Some(prefix) = title.trim_end().strip_suffix(&common) else {
        return TitleParts {
            distinct: title.to_owned(),
            dimmed: None,
        };
    };
    let distinct = prefix.trim_end();
    if distinct.is_empty() {
        return TitleParts {
            distinct: title.to_owned(),
            dimmed: None,
        };
    }
    TitleParts {
        distinct: distinct.to_owned(),
        dimmed: Some(format!(" {common}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_tail_is_split_only_after_three_matching_titles() {
        let titles = [
            "First subject | The Channel",
            "Second subject | The Channel",
            "Third subject | The Channel",
        ];

        assert_eq!(
            split_repeated_suffix(&titles, titles[0]),
            TitleParts {
                distinct: "First subject".to_owned(),
                dimmed: Some(" | The Channel".to_owned()),
            }
        );
    }

    #[test]
    fn a_pair_or_a_single_title_is_never_split() {
        let pair = ["First – The Channel", "Second – The Channel"];

        assert_eq!(
            split_repeated_suffix(&pair, pair[0]),
            TitleParts {
                distinct: pair[0].to_owned(),
                dimmed: None,
            }
        );
        assert_eq!(
            split_repeated_suffix(&[pair[0]], pair[0]),
            TitleParts {
                distinct: pair[0].to_owned(),
                dimmed: None,
            }
        );
    }

    #[test]
    fn longest_shared_suffix_starts_at_a_supported_separator() {
        let titles = [
            "One - Weekly • The Channel",
            "Two - Weekly • The Channel",
            "Three - Weekly • The Channel",
        ];

        assert_eq!(
            split_repeated_suffix(&titles, titles[1]),
            TitleParts {
                distinct: "Two".to_owned(),
                dimmed: Some(" - Weekly • The Channel".to_owned()),
            }
        );
    }

    #[test]
    fn unrelated_or_empty_tails_leave_the_title_intact() {
        let titles = ["One | Alpha", "Two | Beta", "Three | Gamma"];

        assert_eq!(
            split_repeated_suffix(&titles, titles[0]),
            TitleParts {
                distinct: titles[0].to_owned(),
                dimmed: None,
            }
        );
        assert_eq!(
            split_repeated_suffix(&["One |", "Two |", "Three |"], "One |"),
            TitleParts {
                distinct: "One |".to_owned(),
                dimmed: None,
            }
        );
    }
}
