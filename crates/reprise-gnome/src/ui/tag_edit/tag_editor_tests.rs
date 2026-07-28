use super::*;
use crate::ui::tag_editor_state::number_patch;

#[test]
fn number_patch_distinguishes_unchanged_clear_set_and_invalid() {
    assert_eq!(number_patch(false, "bad"), Ok(None));
    assert_eq!(number_patch(true, ""), Ok(Some(None)));
    assert_eq!(number_patch(true, " 42 "), Ok(Some(Some(42))));
    assert!(number_patch(true, "forty-two").is_err());
    assert!(number_patch(true, "0").is_err());
}

#[test]
fn navigate_direction_has_expected_variants() {
    let prev = NavigateDirection::Previous;
    let next = NavigateDirection::Next;
    assert_ne!(prev, next);
}
