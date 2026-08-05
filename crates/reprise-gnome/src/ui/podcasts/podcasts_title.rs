//! Pure title-tail presentation for grouped YouTube episodes.

use std::collections::{BTreeMap, BTreeSet};

const SEPARATORS: [char; 4] = ['|', '–', '-', '•'];
const MIN_MATCHING_TITLES: usize = 3;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TitleParts {
    pub distinct: String,
    pub dimmed: Option<String>,
}

pub(super) fn for_group(titles: &[&str], title: &str, split_tail: bool) -> TitleParts {
    if split_tail {
        split_repeated_suffix(titles, title)
    } else {
        TitleParts {
            distinct: title.to_owned(),
            dimmed: None,
        }
    }
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

pub(super) fn markup(parts: &TitleParts) -> String {
    markup_matching(parts, "", None)
}

/// `POD-25` / FIL-5: the same title, with the section's query accented
/// inside it — the promise the chip makes ("in episode titles") made
/// visible in every row that survived it. Highlighting reuses the track
/// list's own helper, so a hit reads the same wherever the user finds it.
/// The dimmed channel tail is never highlighted: it is the part the row
/// plays *down*, and accenting it would fight that.
///
/// Known gap: the filter folds case with full Unicode `to_lowercase`, this
/// accent with `to_ascii_lowercase` (the track table's rule, kept so a hit
/// reads the same in both places). A title that matches only under
/// non-ASCII case folding is therefore listed but not accented — the same
/// shape of accepted gap FIL-5 already names for hidden columns, never a
/// wrong row.
pub(super) fn markup_matching(parts: &TitleParts, query: &str, accent: Option<&str>) -> String {
    let distinct =
        crate::ui::track_list::match_highlight::highlight_markup(&parts.distinct, query, accent)
            .unwrap_or_else(|| gtk4::glib::markup_escape_text(&parts.distinct).to_string());
    let Some(dimmed) = parts.dimmed.as_deref() else {
        return distinct;
    };
    let dimmed = gtk4::glib::markup_escape_text(dimmed);
    format!("{distinct}<span alpha=\"55%\">{dimmed}</span>")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// UX POD-25 / FIL-5: the query is accented inside the episode title it
    /// matched — case-insensitively, mid-word, every occurrence — and the
    /// dimmed channel tail is left alone. Without a query the markup is
    /// exactly what it always was.
    #[test]
    fn pod_25_matching_titles_accent_the_query_and_leave_the_tail_dimmed() {
        let parts = TitleParts {
            distinct: "Antwerpen: Wie ein Hafen wirklich funktioniert".into(),
            dimmed: Some(" | Werkbank".into()),
        };

        assert_eq!(
            markup_matching(&parts, "wer", None),
            "Ant<b>wer</b>pen: Wie ein Hafen wirklich funktioniert\
             <span alpha=\"55%\"> | Werkbank</span>"
        );
        // Every occurrence, not only the first — and the dimmed tail's own
        // "Werkbank" stays untouched.
        assert_eq!(
            markup_matching(
                &TitleParts {
                    distinct: "Werkzeuge und Auswertung".into(),
                    dimmed: Some(" | Werkbank".into()),
                },
                "wer",
                None
            ),
            "<b>Wer</b>kzeuge und Aus<b>wer</b>tung\
             <span alpha=\"55%\"> | Werkbank</span>"
        );
        assert_eq!(markup_matching(&parts, "  ", None), markup(&parts));
        assert_eq!(
            markup_matching(&parts, "nothing here", None),
            markup(&parts),
            "a title that does not match is rendered unchanged"
        );
    }

    /// UX POD-25 / FIL-5: markup in a title is escaped, highlighted or not —
    /// an episode called `Rock & <Roll>` must never become markup.
    #[test]
    fn pod_25_highlighting_still_escapes_the_title() {
        let parts = TitleParts {
            distinct: "Rock & <Roll>".into(),
            dimmed: None,
        };

        assert_eq!(
            markup_matching(&parts, "rock", None),
            "<b>Rock</b> &amp; &lt;Roll&gt;"
        );
        assert_eq!(markup(&parts), "Rock &amp; &lt;Roll&gt;");
    }

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

    #[test]
    fn markup_escapes_titles_and_dims_only_the_repeated_tail() {
        assert_eq!(
            markup(&TitleParts {
                distinct: "Fish & Chips".to_owned(),
                dimmed: Some(" | <Channel>".to_owned()),
            }),
            "Fish &amp; Chips<span alpha=\"55%\"> | &lt;Channel&gt;</span>"
        );
    }
}
