//! Isolated display tests for the conversion/staging view — run only via
//! `scripts/check-display-tests.sh` (one test = one process). They prove the
//! widget reflects the view-model rules INST-2 and INST-4.

use std::cell::RefCell;
use std::rc::Rc;

use reprise_core::ai_jobs;
use reprise_core::ai_staging::StagingStore;
use rusqlite::Connection;

use super::ConversionView;

const WORKER: i64 = 99;
const NOW: i64 = 100;
const LEASE: i64 = 1_000;
const MODEL: &str = "model@1";

fn seeded_conn(track_ids: &[i64]) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    for id in track_ids {
        conn.execute(
            "INSERT INTO tracks (id, path, title, artist, album, added_at) \
             VALUES (?1, ?2, 'Song', 'Artist', 'Album', 0)",
            rusqlite::params![id, format!("/music/{id}.flac")],
        )
        .unwrap();
    }
    conn
}

fn enqueue(conn: &Connection, staging: &StagingStore, source: i64) -> i64 {
    ai_jobs::enqueue_instrumental(conn, staging, source, MODEL, NOW)
        .unwrap()
        .job_id()
}

fn claim(conn: &Connection) -> i64 {
    ai_jobs::claim_next(conn, WORKER, NOW, LEASE)
        .unwrap()
        .expect("a queued job to claim")
        .id
}

// UX INST-2: the single aggregate bar reflects the job rows — done/total and
// the mean completion, the same figures the CLI/MCP report.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn inst_2_aggregate_bar_reflects_the_job_rows() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let conn = seeded_conn(&[1, 2, 3, 4]);
    let staging = StagingStore::new(dir.path().join("staging"));

    // Two done (1000), one running at 500, one queued (0): mean permille 625.
    let j1 = enqueue(&conn, &staging, 1);
    claim(&conn);
    ai_jobs::mark_done(&conn, j1, WORKER, NOW).unwrap();
    let j2 = enqueue(&conn, &staging, 2);
    claim(&conn);
    ai_jobs::mark_done(&conn, j2, WORKER, NOW).unwrap();
    let j3 = enqueue(&conn, &staging, 3);
    claim(&conn);
    ai_jobs::set_progress(&conn, j3, WORKER, 500).unwrap();
    enqueue(&conn, &staging, 4); // stays queued

    let view = ConversionView::new(Rc::new(RefCell::new(conn)), staging);

    assert_eq!(view.row_count(), 4);
    assert!(
        (view.aggregate_fraction() - 0.625).abs() < 0.01,
        "fraction was {}",
        view.aggregate_fraction()
    );
    let text = view.aggregate_text();
    assert!(text.contains("2 of 4"), "aggregate text was {text:?}");
    assert!(text.contains("63%"), "aggregate text was {text:?}");
}

// UX INST-4a: the view marks a finished, staged render as playable while a
// sibling still processes (not playable, it shows progress).
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn inst_4a_done_row_marked_playable_while_a_sibling_processes() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let conn = seeded_conn(&[1, 2]);
    let staging = StagingStore::new(dir.path().join("staging"));
    staging.ensure_dir().unwrap();

    // j1: done with a staging render present -> playable.
    let j1 = enqueue(&conn, &staging, 1);
    claim(&conn);
    ai_jobs::mark_done(&conn, j1, WORKER, NOW).unwrap();
    std::fs::write(staging.path_for_job(j1), b"render").unwrap();
    // j2: still processing.
    let j2 = enqueue(&conn, &staging, 2);
    claim(&conn);
    ai_jobs::set_progress(&conn, j2, WORKER, 400).unwrap();

    let view = ConversionView::new(Rc::new(RefCell::new(conn)), staging);

    assert_eq!(
        view.row_play_enabled(j1),
        Some(true),
        "a finished, staged render is playable"
    );
    assert_eq!(view.row_is_processing(j1), Some(false));
    assert_eq!(
        view.row_play_enabled(j2),
        Some(false),
        "a processing sibling is not playable (wait-with-progress, INST-5)"
    );
    assert_eq!(view.row_is_processing(j2), Some(true));
}

// UX INST-6: per-row Save/Discard are offered only on a finished, undecided
// render; a processing row offers neither, and "Save all" is enabled only while
// an undecided render exists.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn inst_6_save_and_discard_are_offered_only_on_undecided_renders() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let conn = seeded_conn(&[1, 2]);
    let staging = StagingStore::new(dir.path().join("staging"));
    staging.ensure_dir().unwrap();

    // j1: finished, staged, unsaved -> undecided (offers Save + Discard).
    let j1 = enqueue(&conn, &staging, 1);
    claim(&conn);
    ai_jobs::mark_done(&conn, j1, WORKER, NOW).unwrap();
    std::fs::write(staging.path_for_job(j1), b"render").unwrap();
    // j2: still processing (offers neither).
    let j2 = enqueue(&conn, &staging, 2);
    claim(&conn);
    ai_jobs::set_progress(&conn, j2, WORKER, 400).unwrap();

    let view = ConversionView::new(Rc::new(RefCell::new(conn)), staging);

    assert_eq!(
        view.row_save_visible(j1),
        Some(true),
        "undecided offers Save"
    );
    assert_eq!(
        view.row_discard_visible(j1),
        Some(true),
        "undecided offers Discard"
    );
    assert_eq!(
        view.row_save_visible(j2),
        Some(false),
        "a processing row offers no Save"
    );
    assert_eq!(
        view.row_discard_visible(j2),
        Some(false),
        "a processing row offers no Discard"
    );
    assert!(
        view.save_all_sensitive(),
        "Save all is enabled while an undecided render exists"
    );
}

// UX INST-8: an undecided render's disk cost is visible in the view — both the
// per-row size AND the aggregate total ("Größe je Zeile / Summe"), so hours of
// kept renders are never an invisible cost.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn inst_8_undecided_render_shows_its_disk_cost() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let conn = seeded_conn(&[1, 2]);
    let staging = StagingStore::new(dir.path().join("staging"));
    staging.ensure_dir().unwrap();

    // Two kept (undecided) renders of different sizes, in binary units (MiB,
    // like the rest of the app's size readouts — FIX-5).
    let j1 = enqueue(&conn, &staging, 1);
    claim(&conn);
    ai_jobs::mark_done(&conn, j1, WORKER, NOW).unwrap();
    std::fs::write(staging.path_for_job(j1), vec![0u8; 2 * 1024 * 1024]).unwrap(); // 2 MiB
    let j2 = enqueue(&conn, &staging, 2);
    claim(&conn);
    ai_jobs::mark_done(&conn, j2, WORKER, NOW).unwrap();
    std::fs::write(staging.path_for_job(j2), vec![0u8; 3 * 1024 * 1024]).unwrap(); // 3 MiB

    let view = ConversionView::new(Rc::new(RefCell::new(conn)), staging);

    // Per row: each undecided render shows its own size.
    let c1 = view.row_state_text(j1).expect("row exists");
    assert!(
        c1.contains("2.0 MiB"),
        "the first row shows its disk cost: {c1:?}"
    );
    let c2 = view.row_state_text(j2).expect("row exists");
    assert!(
        c2.contains("3.0 MiB"),
        "the second row shows its disk cost: {c2:?}"
    );

    // Total ("Summe"): the aggregate is the real sum of the kept renders, not a
    // copy of any single row.
    assert!(
        view.disk_total_visible(),
        "the kept-render total is shown while undecided renders exist"
    );
    let total = view.disk_total_text();
    assert!(
        total.contains("5.0 MiB"),
        "the total sums both kept renders (2.0 + 3.0 MiB): {total:?}"
    );
}
