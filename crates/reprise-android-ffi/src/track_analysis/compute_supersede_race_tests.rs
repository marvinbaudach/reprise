//! The edges of a supersede: a caller that returns to a track after its decode
//! was superseded, a foreground caller waiting on the backfill's decode, a
//! supersede that lands right after a caller joined, one that lands before the
//! decode is registered, one that lands after the whole stream is decoded, and
//! one that lands while the decode is being stored.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::track_analysis::AndroidAnalysisOutcome;

use super::progress_tests::{
    blocking_decoder, expected_frames, foreground_import, gate, pcm_frames,
};
use super::supersede_tests::{has_render_data, library_with_two_tracks};
use super::tests::{set_flag, wait_flag, wait_for_in_flight_waiter, ClosureDecoder};
use super::{Claim, CurrentDecodeSlot, Join, Waiter};

const RELEASED_WAITER_TIMEOUT: Duration = Duration::from_secs(10);

/// What a joined caller was told, in a form a test can compare.
#[derive(Debug, PartialEq, Eq)]
enum Answer {
    Done(AndroidAnalysisOutcome),
    Stale,
    Mine,
}

fn answer(claim: &Claim) -> Answer {
    match claim {
        Claim::Done(outcome) => Answer::Done(*outcome),
        Claim::Stale => Answer::Stale,
        Claim::Mine(_) => Answer::Mine,
    }
}

fn joined(library: &crate::MusicLibrary, track_id: i64) -> Waiter {
    match library.analysis_in_flight.join(track_id) {
        Join::Waiting(waiter) => waiter,
        Join::Mine(_) => panic!("the track was not being decoded"),
    }
}

/// Waits on a separate thread, so a waiter that is never let go fails the
/// test instead of hanging it.
fn foreground_answer(waiter: Waiter) -> Answer {
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(answer(&waiter.wait(false)));
    });
    receiver
        .recv_timeout(RELEASED_WAITER_TIMEOUT)
        .expect("the foreground waiter was never let go")
}

#[test]
fn nav_15e_returning_to_a_superseded_track_restarts_its_analysis() {
    let (_directory, library, a, b) = library_with_two_tracks();
    let total = expected_frames(&library, a);
    let calls = Arc::new(AtomicUsize::new(0));
    let (first_pushed, release_first) = (gate(), gate());
    let (first_pushed_in, release_first_in) =
        (Arc::clone(&first_pushed), Arc::clone(&release_first));
    let calls_in = Arc::clone(&calls);
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |_uri, sink| {
            if calls_in.fetch_add(1, Ordering::SeqCst) == 0 {
                // The first decode of A is still unwinding when the listener returns.
                assert!(sink.push_pcm_i16(pcm_frames(total / 2), 32_000, 1));
                set_flag(&first_pushed_in);
                wait_flag(&release_first_in);
                let _ = sink.push_pcm_i16(pcm_frames(total - total / 2), 32_000, 1);
            } else {
                assert!(sink.push_pcm_i16(pcm_frames(total), 32_000, 1));
            }
            Ok(())
        },
    )));
    let first = foreground_import(&library, a);
    wait_flag(&first_pushed);

    library.supersede_foreground_track_analysis(Some(b));
    // A plays again: its new request arrives while the superseded decode unwinds.
    let returned = foreground_import(&library, a);
    wait_for_in_flight_waiter(&library, a);
    set_flag(&release_first);

    assert_eq!(
        first.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Superseded
    );
    assert_eq!(
        returned.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Computed,
        "a request made after the supersede must not inherit it"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert!(has_render_data(&library, a));
}

#[test]
fn nav_15e_superseding_frees_a_foreground_waiter_on_the_backfill_decode() {
    let (_directory, library, a, b) = library_with_two_tracks();
    let total = expected_frames(&library, a);
    let (pushed, release) = (gate(), gate());
    library.register_track_pcm_decoder(Box::new(blocking_decoder(
        total / 2,
        total - total / 2,
        Arc::clone(&pushed),
        Arc::clone(&release),
    )));
    let slot: Arc<CurrentDecodeSlot> = Arc::new(Mutex::new(None));
    let library_in_thread = Arc::clone(&library);
    let slot_in_thread = Arc::clone(&slot);
    let backfill = std::thread::spawn(move || {
        library_in_thread
            .analysis_context()
            .compute(a, true, None, Some(&slot_in_thread))
    });
    wait_flag(&pushed);
    let (sender, receiver) = mpsc::channel();
    let library_in_waiter = Arc::clone(&library);
    std::thread::spawn(move || {
        let _ = sender.send(library_in_waiter.import_track_analysis(a));
    });
    wait_for_in_flight_waiter(&library, a);

    library.supersede_foreground_track_analysis(Some(b));

    let released = receiver
        .recv_timeout(RELEASED_WAITER_TIMEOUT)
        .expect("the foreground waiter kept blocking on the backfill's decode");
    assert_eq!(released.unwrap(), AndroidAnalysisOutcome::Superseded);
    set_flag(&release);
    assert_eq!(
        backfill.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Computed,
        "the backfill's own decode carries on"
    );
    assert!(has_render_data(&library, a));
}

#[test]
fn nav_15e_a_supersede_before_the_decode_registers_still_stops_it() {
    let (_directory, library, a, b) = library_with_two_tracks();
    let total = expected_frames(&library, a);
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |_uri, sink| {
            let _ = sink.push_pcm_i16(pcm_frames(total), 32_000, 1);
            Ok(())
        },
    )));
    // Holding the decoder handle parks the request after its claim and before
    // its decode is registered.
    let decoder_guard = library.pcm_decoder.lock().unwrap();
    let import = foreground_import(&library, a);
    let deadline = std::time::Instant::now() + RELEASED_WAITER_TIMEOUT;
    while !library.analysis_in_flight.contains(a) {
        assert!(std::time::Instant::now() < deadline, "A was never claimed");
        std::thread::yield_now();
    }

    library.supersede_foreground_track_analysis(Some(b));
    drop(decoder_guard);

    assert_eq!(
        import.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Superseded
    );
    assert!(!has_render_data(&library, a));
}

#[test]
fn nav_15e_a_supersede_after_the_whole_stream_is_decoded_still_stores_it() {
    let (_directory, library, a, b) = library_with_two_tracks();
    let total = expected_frames(&library, a);
    let (pushed, release) = (gate(), gate());
    let (pushed_in, release_in) = (Arc::clone(&pushed), Arc::clone(&release));
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |_uri, sink| {
            assert!(sink.push_pcm_i16(pcm_frames(total), 32_000, 1));
            set_flag(&pushed_in);
            // End of stream reached; the decoder is only returning.
            wait_flag(&release_in);
            Ok(())
        },
    )));
    let import = foreground_import(&library, a);
    wait_flag(&pushed);

    library.supersede_foreground_track_analysis(Some(b));
    set_flag(&release);

    assert_eq!(
        import.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Computed,
        "a complete decode is valid data whose cost is already paid"
    );
    assert!(has_render_data(&library, a));
}

#[test]
fn nav_15e_a_supersede_right_after_joining_the_backfill_decode_frees_the_waiter() {
    let (_directory, library, a, b) = library_with_two_tracks();
    let total = expected_frames(&library, a);
    let (pushed, release) = (gate(), gate());
    library.register_track_pcm_decoder(Box::new(blocking_decoder(
        total / 2,
        total - total / 2,
        Arc::clone(&pushed),
        Arc::clone(&release),
    )));
    let slot: Arc<CurrentDecodeSlot> = Arc::new(Mutex::new(None));
    let library_in_thread = Arc::clone(&library);
    let slot_in_thread = Arc::clone(&slot);
    let backfill = std::thread::spawn(move || {
        library_in_thread
            .analysis_context()
            .compute(a, true, None, Some(&slot_in_thread))
    });
    wait_flag(&pushed);
    // The supersede lands after the join and before the waiter starts waiting.
    let waiter = joined(&library, a);
    library.supersede_foreground_track_analysis(Some(b));

    let released = foreground_answer(waiter);

    set_flag(&release);
    assert_eq!(
        released,
        Answer::Done(AndroidAnalysisOutcome::Superseded),
        "a supersede after the join is meant for the waiter"
    );
    assert_eq!(
        backfill.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Computed
    );
}

#[test]
fn nav_15e_a_supersede_right_after_joining_is_final_for_the_waiter() {
    let (_directory, library, a, b) = library_with_two_tracks();
    let total = expected_frames(&library, a);
    let (pushed, release) = (gate(), gate());
    library.register_track_pcm_decoder(Box::new(blocking_decoder(
        total / 2,
        total - total / 2,
        Arc::clone(&pushed),
        Arc::clone(&release),
    )));
    let first = foreground_import(&library, a);
    wait_flag(&pushed);
    let waiter = joined(&library, a);
    library.supersede_foreground_track_analysis(Some(b));
    set_flag(&release);
    assert_eq!(
        first.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Superseded
    );

    assert_eq!(
        foreground_answer(waiter),
        Answer::Done(AndroidAnalysisOutcome::Superseded),
        "a waiter the supersede reached must not re-decode the track it left"
    );
}

#[test]
fn nav_15e_a_supersede_while_the_decode_is_stored_releases_the_waiter_and_keeps_the_data() {
    let (_directory, library, a, b) = library_with_two_tracks();
    let total = expected_frames(&library, a);
    let (pushed, release) = (gate(), gate());
    let (pushed_in, release_in) = (Arc::clone(&pushed), Arc::clone(&release));
    library.register_track_pcm_decoder(Box::new(ClosureDecoder::new(
        Arc::new(AtomicUsize::new(0)),
        move |_uri, sink| {
            assert!(sink.push_pcm_i16(pcm_frames(total), 32_000, 1));
            set_flag(&pushed_in);
            wait_flag(&release_in);
            Ok(())
        },
    )));
    let owner = foreground_import(&library, a);
    wait_flag(&pushed);
    let (sender, receiver) = mpsc::channel();
    let library_in_waiter = Arc::clone(&library);
    std::thread::spawn(move || {
        let _ = sender.send(library_in_waiter.import_track_analysis(a));
    });
    wait_for_in_flight_waiter(&library, a);

    // Holding the writer parks the owner at the store, after it finished the session.
    let writer = library.writer.lock().unwrap();
    set_flag(&release);
    let deadline = std::time::Instant::now() + RELEASED_WAITER_TIMEOUT;
    loop {
        let handle = library
            .analysis_in_flight
            .decodes()
            .lookup(a)
            .expect("the decode stays registered through its store");
        if handle.sink.partial(handle.expected_frames).is_none() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the owner never reached the store"
        );
        std::thread::yield_now();
    }
    library.supersede_foreground_track_analysis(Some(b));

    let released = receiver
        .recv_timeout(RELEASED_WAITER_TIMEOUT)
        .expect("the waiter was held through the store");
    drop(writer);

    assert_eq!(released.unwrap(), AndroidAnalysisOutcome::Superseded);
    assert_eq!(
        owner.join().unwrap().unwrap(),
        AndroidAnalysisOutcome::Computed,
        "the owner of a whole stream stores it whatever the supersede"
    );
    assert!(has_render_data(&library, a));
}
