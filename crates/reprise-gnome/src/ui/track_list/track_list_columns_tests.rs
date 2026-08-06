use reprise_core::models::MissingReason;

use super::*;

#[test]
fn ac_24_the_running_row_reads_neither_bass_signal() {
    // The EQ bars and the row colour are the whole marker (MOT-5, NAV-10b).
    for source in [
        include_str!("track_list_columns.rs"),
        include_str!("track_list_title_column.rs"),
    ] {
        assert!(!source.contains("row_wash"));
        assert!(!source.contains("kick"));
    }
}

#[test]
fn rating_sort_requires_query_reload_but_other_sorts_need_one_row() {
    assert_eq!(rating_refresh_for_sort("rating"), RatingRefresh::Query);
    assert_eq!(rating_refresh_for_sort("title"), RatingRefresh::Row);
}

#[test]
fn missing_track_explanation_distinguishes_unavailable_drive_from_missing_file() {
    assert_eq!(
        missing_track_explanation(Some(1_000_000_000), Some(MissingReason::Unmounted)),
        Some("On unavailable drive — returns when mounted".into())
    );
    for reason in [MissingReason::Deleted, MissingReason::Unknown] {
        assert_eq!(
            missing_track_explanation(Some(1_000_000_000), Some(reason)),
            Some("File missing since 2001-09-09 01:46".into())
        );
    }
    assert_eq!(missing_track_explanation(None, None), None);
}

#[test]
fn missing_title_css_uses_half_opacity() {
    let css = crate::ui::track_list_row_interaction::css();
    assert!(css.contains(".missing-track-title"));
    assert!(css.contains("opacity: 0.5"));
}

// UX INST-10: the AI badge renders for AI-manipulated tracks and never on a
// plain one. The provenance flag is the only input — no gate sits in front
// of it, so a track the CLI/MCP frontends produced is always marked.
#[test]
fn inst_10_ai_badge_shows_only_for_ai_tracks() {
    assert!(ai_badge_visible(true), "an AI track shows the badge");
    assert!(!ai_badge_visible(false), "a plain track shows no badge");
}
