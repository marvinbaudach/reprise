//! Display regression for a catalog delete that shrinks the list below the
//! viewport's old absolute scroll value.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

use super::{capture_catalog_delete_reload, reload_after_catalog_delete};
use crate::ui::track_list::TrackList;

const ROWS: i64 = 2_276;
const SCROLL_POSITION: u32 = 1_900;
const DELETE_THROUGH: i64 = 1_500;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(8);
const SETTLE_AFTER_DELETE: Duration = Duration::from_millis(600);
const MIN_SAMPLES: usize = 20;

fn large_track_list() -> (Rc<TrackList>, gtk4::Window) {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=ROWS {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/synthetic/{id:04}.flac"),
                format!("Track {id:04}"),
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let track_list = Rc::new(TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    ));
    *track_list.shared.sort.borrow_mut() = crate::ui::track_list_sort::SortState {
        field: "title".into(),
        dir: "asc".into(),
    };
    crate::ui::track_list::reload(&track_list.shared);
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        track_list
            .shared
            .column_view
            .vadjustment()
            .is_some_and(|adjustment| adjustment.upper() > adjustment.page_size())
    });
    (track_list, window)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
// Counterprobe after the geometry-service track provides the switch:
// scratchpad/probe-any.sh large-delete-no-preseed \
//   ui::delete_tracks::large_block_display_tests::browse_11_large_block_delete_keeps_the_deep_viewport_off_the_top \
//   REPRISE_NO_PRESEED=1
// The same test must fail: suppressing the pre-seed must make the sampled
// viewport jump observable again. Verify that the runner reports one test,
// because an incomplete path silently filters the proof out.
fn browse_11_large_block_delete_keeps_the_deep_viewport_off_the_top() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = large_track_list();
    let shared = &track_list.shared;
    let adjustment = shared.column_view.vadjustment().unwrap();

    shared
        .column_view
        .scroll_to(SCROLL_POSITION, None, gtk4::ListScrollFlags::NONE, None);
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > adjustment.page_size() * 2.0
    });
    let before_scroll = adjustment.value();
    // Range-derived height checks the shrinkage precondition; it is not the target oracle.
    let old_row_height = adjustment.upper() / f64::from(shared.model.n_items());
    let expected_remaining = ROWS - DELETE_THROUGH;
    let projected_new_height = expected_remaining as f64 * old_row_height;
    assert!(
        projected_new_height < before_scroll,
        "precondition: the shrunken list must be shorter than the old scroll value: \
         projected new height={projected_new_height}, old scroll={before_scroll}"
    );

    let reload_state = capture_catalog_delete_reload(shared);
    let captured_anchor = reload_state
        .anchor
        .anchor
        .expect("a deep viewport must capture a stable track anchor");
    let before_ids = shared.current_view_ids();
    let removed_ids = before_ids
        .iter()
        .copied()
        .filter(|id| *id <= DELETE_THROUGH)
        .collect::<Vec<_>>();
    assert_eq!(removed_ids.len(), DELETE_THROUGH as usize);

    let samples: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let sampler = {
        let samples = samples.clone();
        let adjustment = adjustment.clone();
        gtk4::glib::timeout_add_local(SAMPLE_INTERVAL, move || {
            samples.borrow_mut().push(adjustment.value());
            gtk4::glib::ControlFlow::Continue
        })
    };

    crate::test_db::connection(&shared.conn)
        .execute("DELETE FROM tracks WHERE id <= ?1", [DELETE_THROUGH])
        .unwrap();
    reload_after_catalog_delete(shared, &removed_ids, reload_state);
    crate::ui::test_settle::settle_for(SETTLE_AFTER_DELETE);
    sampler.remove();

    assert_eq!(shared.model.n_items(), expected_remaining as u32);
    let after_ids = shared.current_view_ids();
    let anchor_position = after_ids
        .iter()
        .position(|id| *id == captured_anchor.0)
        .expect("the captured anchor must survive the leading block delete");
    // Range-derived height only bounds surviving-anchor error; it is not the target oracle.
    let row_height = adjustment.upper() / after_ids.len() as f64;
    let expected = row_height * anchor_position as f64 + captured_anchor.1;
    let samples = samples.borrow();
    let first = samples.first().copied();
    let minimum = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let sample_report = format!(
        "samples(n={} first={first:?} min={minimum} max={maximum})",
        samples.len()
    );
    assert!(
        samples.len() >= MIN_SAMPLES,
        "the sampler did not cover the large-delete handover; {sample_report}"
    );
    assert!(
        minimum > expected - row_height * 2.0,
        "the large delete exposed the top before restoring its surviving anchor: \
         expected={expected}, row height={row_height}; {sample_report}"
    );
    assert!(
        (adjustment.value() - expected).abs() < row_height,
        "the large delete did not settle on its surviving anchor: actual={}, \
         expected={expected}, row height={row_height}; {sample_report}",
        adjustment.value()
    );

    window.close();
}

// --- Diagnostic variants: the anchor row is deleted along with the block ---
//
// The shipped test above deletes the *leading* block, so the captured anchor
// survives and its target stays reachable inside the stale range — which is
// why it passes even with the pre-seed suppressed. These two variants remove
// the anchor itself, which is the case `surviving_delete_anchor` has to answer.

fn run_anchor_deleting_delete(label: &str, delete_from: i64, delete_through: i64) {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = large_track_list();
    let shared = &track_list.shared;
    let adjustment = shared.column_view.vadjustment().unwrap();

    shared
        .column_view
        .scroll_to(SCROLL_POSITION, None, gtk4::ListScrollFlags::NONE, None);
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > adjustment.page_size() * 2.0
    });
    let before_scroll = adjustment.value();
    // Range-derived height supplies diagnostic context; it is not the target oracle.
    let row_height = adjustment.upper() / f64::from(shared.model.n_items());

    let reload_state = capture_catalog_delete_reload(shared);
    let captured_anchor = reload_state.anchor.anchor;
    let removed_ids = shared
        .current_view_ids()
        .into_iter()
        .filter(|id| *id >= delete_from && *id <= delete_through)
        .collect::<Vec<_>>();
    let anchor_is_deleted = captured_anchor.is_some_and(|(id, _)| removed_ids.contains(&id));

    let samples: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let sampler = {
        let samples = samples.clone();
        let adjustment = adjustment.clone();
        gtk4::glib::timeout_add_local(SAMPLE_INTERVAL, move || {
            samples.borrow_mut().push(adjustment.value());
            gtk4::glib::ControlFlow::Continue
        })
    };

    crate::test_db::connection(&shared.conn)
        .execute(
            "DELETE FROM tracks WHERE id >= ?1 AND id <= ?2",
            [delete_from, delete_through],
        )
        .unwrap();
    reload_after_catalog_delete(shared, &removed_ids, reload_state);
    crate::ui::test_settle::settle_for(SETTLE_AFTER_DELETE);
    sampler.remove();

    let samples = samples.borrow();
    let minimum = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let maximum = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let new_upper = adjustment.upper();
    let new_max = (new_upper - adjustment.page_size()).max(0.0);
    eprintln!(
        "DELETEPROBE {label}: anchor={captured_anchor:?} deleted={anchor_is_deleted} \
         before={before_scroll:.0} row_h={row_height:.1} rows_left={} \
         new_upper={new_upper:.0} new_max={new_max:.0} final={:.0} \
         samples(n={} first={:?} min={minimum:.0} max={maximum:.0})",
        shared.model.n_items(),
        adjustment.value(),
        samples.len(),
        samples.first(),
    );
    assert!(
        samples.len() >= MIN_SAMPLES,
        "{label}: the sampler did not cover the handover (n={})",
        samples.len()
    );
    assert!(
        minimum > adjustment.page_size(),
        "{label}: the viewport visited the top of the list (min={minimum:.0})"
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn diag_delete_the_block_the_viewport_sits_in() {
    run_anchor_deleting_delete("anchor-block", 1_800, 2_100);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn diag_delete_everything_above_and_including_the_viewport() {
    run_anchor_deleting_delete("anchor-and-shrink", 1, 2_100);
}
