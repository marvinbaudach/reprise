use super::*;

// Legacy rating helpers — kept here because they have value as unit tests
// but are no longer used in the production rating widget (now star buttons).

fn rating_choice_labels(value: &MixedValue<i32>) -> Vec<String> {
    let mut labels = Vec::with_capacity(7);
    if matches!(value, MixedValue::Mixed) {
        labels.push(strings::text(strings::MULTIPLE_VALUES));
    }
    labels.push("\u{2606} \u{2014}".into());
    labels.extend((1..=RATING_MAX).map(|rating| format!("\u{2605} {rating}")));
    labels
}

fn rating_from_selection(started_mixed: bool, selected: u32) -> Option<i32> {
    let rating = if started_mixed {
        selected.checked_sub(1)?
    } else {
        selected
    };
    i32::try_from(rating)
        .ok()
        .filter(|rating| *rating <= RATING_MAX)
}

#[test]
fn string_patch_writes_only_dirty_fields_and_allows_clear() {
    assert_eq!(string_patch(false, "replacement"), None);
    assert_eq!(
        string_patch(true, "replacement"),
        Some("replacement".into())
    );
    assert_eq!(string_patch(true, ""), Some(String::new()));
}

#[test]
fn number_patch_distinguishes_unchanged_clear_set_and_invalid() {
    assert_eq!(number_patch(false, "bad"), Ok(None));
    assert_eq!(number_patch(true, ""), Ok(Some(None)));
    assert_eq!(number_patch(true, " 42 "), Ok(Some(Some(42))));
    assert!(number_patch(true, "forty-two").is_err());
    assert!(number_patch(true, "0").is_err());
}

#[test]
fn rating_choices_keep_mixed_unrated_and_five_stars_distinct() {
    assert_eq!(
        rating_choice_labels(&MixedValue::Mixed),
        vec![
            "(multiple values)",
            "\u{2606} \u{2014}",
            "\u{2605} 1",
            "\u{2605} 2",
            "\u{2605} 3",
            "\u{2605} 4",
            "\u{2605} 5"
        ]
    );
    assert_eq!(rating_from_selection(true, 0), None);
    assert_eq!(rating_from_selection(true, 1), Some(0));
    assert_eq!(rating_from_selection(true, 6), Some(5));
    assert_eq!(rating_from_selection(false, 0), Some(0));
    assert_eq!(rating_from_selection(false, 5), Some(5));
}

#[test]
fn navigate_direction_has_expected_variants() {
    let prev = NavigateDirection::Previous;
    let next = NavigateDirection::Next;
    assert_ne!(prev, next);
}

#[test]
fn field_name_covers_all_indices() {
    for i in 0..FIELD_COUNT {
        assert!(
            !field_name(i).is_empty(),
            "field_name({i}) should not be empty"
        );
    }
}
