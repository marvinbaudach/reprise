use super::*;
use crate::ui::track_list::reload_restore;
use std::io::Write;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
struct CapturedLogs(Arc<Mutex<Vec<u8>>>);

struct CapturedLogWriter(Arc<Mutex<Vec<u8>>>);

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for CapturedLogs {
    type Writer = CapturedLogWriter;

    fn make_writer(&'a self) -> Self::Writer {
        CapturedLogWriter(Arc::clone(&self.0))
    }
}

impl Write for CapturedLogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn capture_info(operation: impl FnOnce()) -> String {
    let captured = CapturedLogs::default();
    let subscriber = tracing_subscriber::fmt()
        .without_time()
        .with_ansi(false)
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .with_writer(captured.clone())
        .finish();
    tracing::subscriber::with_default(subscriber, operation);
    let bytes = captured.0.lock().unwrap().clone();
    String::from_utf8(bytes).unwrap()
}

#[test]
fn completion_log_distinguishes_no_reload_from_a_deferred_reload() {
    let no_reload = capture_info(|| {
        tag_save_refresh::log_batch_completed(&tag_save_refresh::BatchCompletion {
            write_ms: 0,
            tracks: 1,
            reload_ms: 0,
            reload_metrics: None,
            delta: false,
            updated: 1,
            failed: 0,
            has_pre_save_view: false,
            before_len: 0,
            after_len: 0,
            first_mismatch: -1,
        });
    });
    assert!(
        no_reload.contains("tag-edit batch completed"),
        "{no_reload}"
    );
    assert!(!no_reload.contains("idle_wait_ms="), "{no_reload}");
    assert!(!no_reload.contains("reload_work_ms="), "{no_reload}");
    assert!(!no_reload.contains("emit="), "{no_reload}");

    let deferred = capture_info(|| {
        tag_save_refresh::log_batch_completed(&tag_save_refresh::BatchCompletion {
            write_ms: 0,
            tracks: 1,
            reload_ms: 0,
            reload_metrics: Some(crate::ui::track_list::tag_mutation_refresh::ReloadMetrics {
                idle_wait_ms: 2,
                reload_work_ms: 3,
                emit: crate::ui::track_list::tag_mutation_refresh::ReloadEmit::Move,
            }),
            delta: true,
            updated: 1,
            failed: 0,
            has_pre_save_view: true,
            before_len: 8,
            after_len: 8,
            first_mismatch: 0,
        });
    });
    assert!(deferred.contains("idle_wait_ms=2"), "{deferred}");
    assert!(deferred.contains("reload_work_ms=3"), "{deferred}");
    assert!(deferred.contains("emit=\"move\""), "{deferred}");
}

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

#[test]
fn tag_edit_view_diagnostics_report_the_first_difference() {
    assert_eq!(
        tag_save_refresh::first_view_mismatch(&[11, 13, 17], &[11, 19, 17]),
        1
    );
    assert_eq!(
        tag_save_refresh::first_view_mismatch(&[11, 13], &[11, 13, 17]),
        2
    );
    assert_eq!(
        tag_save_refresh::first_view_mismatch(&[11, 13], &[11, 13]),
        -1
    );
}

#[test]
fn tag_edit_reload_state_keeps_the_complete_view_when_browsing_is_capped() {
    let current_view_ids = (1_i64..=1_929).collect::<Vec<_>>();
    let browse = tag_editor::BrowseSnapshot {
        tracks: current_view_ids
            .iter()
            .take(500)
            .map(|id| SessionTrack {
                id: *id,
                path: PathBuf::from(format!("/{id}.flac")),
                tags: EditableTags::default(),
                rating: 0,
            })
            .collect(),
        bitrates: vec![None; 500],
    };

    assert_eq!(browse.ids().len(), 500);
    let reload_view_ids = reload_view_ids_at_open(&current_view_ids, Some(&browse));
    let opened = OpenedReloadState::at_open(
        reload_restore::capture(Vec::new(), Some((1_501, 0.0))),
        reload_view_ids,
    );

    assert_eq!(opened.view_ids, current_view_ids);
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
    let restored = post_save_reload_anchor(opened, &[61], &[], "artist", &[61], Some(&layout));

    assert_eq!(restored.selected_ids, vec![61]);
    assert_eq!(
        restored.anchor,
        Some((61, 7.5)),
        "the async save must reuse the viewport captured before the dialog opened"
    );
}
