use super::*;
use crate::ui::track_list::reload_restore;

#[test]
fn healed_import_hint_refreshes_in_place_without_a_success_toast() {
    assert_eq!(completion_toast(ApplyOrigin::ImportHint, 1, 0), None);
    assert_eq!(
        completion_toast(ApplyOrigin::TrackList, 1, 0).as_deref(),
        Some("Updated 1 track")
    );
    assert_eq!(
        completion_toast(ApplyOrigin::ImportHint, 0, 1).as_deref(),
        Some("Updated 0 tracks; 1 failed")
    );
}

#[test]
fn smoke_tag_edit_mode_parses_open_count_and_preserves_title_save() {
    assert_eq!(
        parse_smoke_tag_edit_mode("open:2"),
        Some(SmokeTagEditMode::Open(2))
    );
    assert_eq!(parse_smoke_tag_edit_mode("open:0"), None);
    assert_eq!(parse_smoke_tag_edit_mode("open:many"), None);
    assert_eq!(
        parse_smoke_tag_edit_mode("title:Acceptance title"),
        Some(SmokeTagEditMode::SaveTitle("Acceptance title".into()))
    );
}

/// TAG-1 (G2): `select_written_tracks` composes entirely from
/// `reload_restore::positions_for_ids` (already `#[test]`-covered at
/// Task A's pure-logic level) plus real `gtk4::MultiSelection` widget
/// calls this crate's headless suite cannot construct outside the
/// display-test harness (`scripts/check-display-tests.sh`) — see this
/// package's report for why a full `Shared` fixture wasn't built for
/// this wave. This test instead pins the exact mapping the post-save
/// selection depends on: written ids win, an unrelated failed id never
/// widens the selection, and an id no longer in the (possibly
/// concurrently changed) current view drops out silently rather than
/// erroring — the same "no side effect from a vanished id" rule a plain
/// `reload()` already applies.
#[test]
fn tag_1_selection_after_save_is_written_tracks() {
    let updated_ids = vec![7_i64, 9_i64];
    let current_view = vec![11_i64, 7_i64, 9_i64];
    let positions = reload_restore::positions_for_ids(&updated_ids, &current_view);
    assert_eq!(
        positions,
        vec![1, 2],
        "selection follows the written ids, not the unrelated failed track at position 0"
    );

    let narrowed_view = vec![9_i64];
    assert_eq!(
        reload_restore::positions_for_ids(&updated_ids, &narrowed_view),
        vec![0],
        "a written id no longer in the current view drops out silently"
    );
}

#[test]
fn tag_1_query_reload_keeps_the_scroll_anchor_from_editor_open() {
    let opened = reload_restore::capture(vec![61], Some((61, 7.5)));
    let layout = crate::ui::list_geometry_layout::ListLayout::rows_only(
        crate::ui::list_geometry::RowHeight::new(20.0).unwrap(),
    );
    let restored = post_save_reload_anchor(opened, &[61], &[], "artist", &[61], &layout);

    assert_eq!(restored.selected_ids, vec![61]);
    assert_eq!(
        restored.anchor,
        Some((61, 7.5)),
        "the async save must reuse the viewport captured before the dialog opened"
    );
}
