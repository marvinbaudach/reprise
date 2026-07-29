//! Reading a queue that is longer than the snapshot.
//!
//! The snapshot carries 200 rows per section, which is enough for what a user
//! can see and not enough for what they can scroll to. A view with a virtual
//! tail asks for arbitrary windows, and the moment it can name a position
//! past the two-hundredth, the revision has to follow changes there too —
//! otherwise a client holding row 4,000 sends it back after a reorder that
//! never moved the counter, and the runtime applies the command to whatever
//! is at 4,000 now.

use reprise_runtime_protocol::queue::QueueCommand;

use crate::runtime::Command;

use super::{full_client, harness};

/// Ids 1..=n, all resolvable in the fake library only up to 3 — irrelevant
/// here, since paging never resolves anything.
fn many(n: i64) -> Vec<i64> {
    (1..=n).collect()
}

#[test]
fn a_page_past_the_snapshot_window_is_readable() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: many(500),
                start_index: 0,
            },
        )
        .unwrap();

    let page = harness.runtime.queue_page("context", 300, 5).unwrap();

    assert_eq!(
        page.track_ids,
        vec![302, 303, 304, 305, 306],
        "the context window starts after the cursor, so offset 300 is the \
         301st still-pending row"
    );
    assert_eq!(
        page.total, 499,
        "and the view can size its scrollbar without asking for every page"
    );
}

#[test]
fn a_page_carries_the_revision_it_was_read_at() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: many(500),
                start_index: 0,
            },
        )
        .unwrap();

    let page = harness.runtime.queue_page("context", 0, 3).unwrap();
    let snapshot = harness.runtime.snapshot().unwrap().queue;

    assert_eq!(
        page.revision, snapshot.revision,
        "a page and a snapshot read at the same moment describe the same \
         queue; two counters would let a positional command be checked \
         against the wrong one"
    );
}

#[test]
fn a_reorder_beyond_the_window_moves_the_revision() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: many(500),
                start_index: 0,
            },
        )
        .unwrap();
    let before = harness.runtime.snapshot().unwrap().queue.revision;

    // Row 400 is far outside the 200-entry snapshot window, and only
    // reachable at all because paging exists.
    harness
        .runtime
        .command(
            client,
            &Command::Queue(QueueCommand::RemoveContextAt {
                positions: vec![400],
                expected_revision: before,
            }),
        )
        .unwrap();

    assert_ne!(
        harness.runtime.snapshot().unwrap().queue.revision,
        before,
        "this renumbers every row after it. While nothing could name those \
         rows the counter could ignore them; a paged read hands them out, so \
         a client holding row 4,000 would otherwise send back a revision \
         that still looks current and have its command land on a different \
         track"
    );
}

#[test]
fn the_snapshot_window_is_unchanged_by_a_deep_reorder() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: many(500),
                start_index: 0,
            },
        )
        .unwrap();
    let before = harness.runtime.snapshot().unwrap().queue;

    harness
        .runtime
        .command(
            client,
            &Command::Queue(QueueCommand::RemoveContextAt {
                positions: vec![400],
                expected_revision: before.revision,
            }),
        )
        .unwrap();

    let after = harness.runtime.snapshot().unwrap().queue;
    assert_eq!(
        after.context_track_ids, before.context_track_ids,
        "the first 200 rows really are identical — which is exactly why \
         comparing the snapshot could not have noticed this, and why the \
         comparison had to move to the whole queue"
    );
    assert_ne!(after.revision, before.revision);
}

#[test]
fn an_over_long_page_is_capped_rather_than_served() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: many(500),
                start_index: 0,
            },
        )
        .unwrap();

    let page = harness.runtime.queue_page("context", 0, 100_000).unwrap();

    assert_eq!(
        page.track_ids.len(),
        200,
        "one client asking for every id would build that reply while every \
         other client waits — the runtime is single-threaded by design (§9.1)"
    );
}

#[test]
fn a_page_past_the_end_is_empty_rather_than_an_error() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: many(10),
                start_index: 0,
            },
        )
        .unwrap();

    let page = harness.runtime.queue_page("context", 9_000, 10).unwrap();

    assert!(
        page.track_ids.is_empty(),
        "a view whose viewport outran a shrinking queue asks for a window \
         that is no longer there; that is a race it recovers from by reading \
         the total, not a mistake to report"
    );
    assert_eq!(page.total, 9);
}

#[test]
fn the_explicit_queue_pages_separately_from_the_context() {
    let mut harness = harness();
    let client = full_client(&mut harness.runtime);
    harness
        .runtime
        .command(
            client,
            &Command::PlayTracks {
                track_ids: many(10),
                start_index: 0,
            },
        )
        .unwrap();
    harness
        .runtime
        .command(
            client,
            &Command::Queue(QueueCommand::AddNext(vec![7, 8, 9])),
        )
        .unwrap();

    let page = harness.runtime.queue_page("play_next", 1, 2).unwrap();

    assert_eq!(page.track_ids, vec![8, 9]);
    assert_eq!(page.section, "play_next");
    assert_eq!(page.total, 3);
}

#[test]
fn an_unknown_section_is_rejected_rather_than_guessed() {
    let mut harness = harness();
    let _ = full_client(&mut harness.runtime);

    let error = harness
        .runtime
        .queue_page("histoy", 0, 10)
        .expect_err("there are two sections and that is neither");

    assert_eq!(error.kind(), "rejected:no_such_queue_section");
}
