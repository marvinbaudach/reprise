//! NAV-10b display test: deleting the running track must not cost the table
//! its follow to the next one.
//!
//! `delete_tracks::finish` advances the player first and reloads second, in
//! one main-loop turn. The advance defers a centering glide; the reload
//! preserves the viewport the user is looking at and guards it with an
//! `AdjustmentHold`. The hold's write then reads to `ScrollGlide` as a foreign
//! write (`scroll_glide.rs`'s `foreign_write`), which aborts the glide — the
//! list stays on the deleted row's place while playback has moved on.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use super::*;

/// Past `track_list_reload::SCROLL_ADJUSTMENT_HOLD` plus a glide, so the
/// assertion reads a settled viewport rather than one still in flight.
const PAST_THE_HOLD_AND_GLIDE: Duration = Duration::from_millis(900);
/// An 8ms sampler normally records well over 20 values during the settled
/// journey below. Fewer samples do not cover enough frames to rule out a
/// transient jump, so a starved run is invalid rather than inconclusive.
const MIN_SAMPLES: usize = 20;

fn synthetic_track_list(rows: i64) -> (Rc<TrackList>, gtk4::Window) {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=rows {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
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

fn centered_value_for(track_list: &TrackList, track_id: i64) -> Option<f64> {
    let position = track_list
        .shared
        .current_view_ids()
        .iter()
        .position(|id| *id == track_id)? as u32;
    crate::ui::scroll_center::centered_scroll_target(
        &track_list.shared.column_view,
        track_list.shared.model.n_items(),
        position,
    )
    .map(|(_, value)| value)
}

/// The rule underneath the test below, on its own: an automatic advance yields
/// to a user who is scrolling right now, and that must mean the user — not the
/// list moving under its own steam. Reading it off the adjustment made every
/// reload look like scrolling, and a library scan reloads in bursts, so the
/// table stopped following playback for as long as a scan ran.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_a_reload_does_not_count_as_the_user_scrolling() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = synthetic_track_list(200);

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    track_list
        .shared
        .column_view
        .scroll_to(120, None, gtk4::ListScrollFlags::NONE, None);
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > 0.0
    });
    track_list.shared.last_scroll_activity.set(None);

    // What a scan does, repeatedly.
    crate::ui::track_list::reload(&track_list.shared);
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(300));

    assert!(
        track_list.shared.last_scroll_activity.get().is_none(),
        "a reload registered as user scrolling, which suppresses the follow to the next song"
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_deleting_the_running_track_keeps_the_follow_to_the_next_one() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    gtk4::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(true);
    let (track_list, window) = synthetic_track_list(200);

    let playing_id = track_list.shared.model.track_at(100).unwrap().id;
    let next_id = track_list.shared.model.track_at(112).unwrap().id;
    track_list.update_current_track(playing_id, None, CurrentTrackChange::PlaybackStarted);

    // The running track sits centred, the way playback leaves it.
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::track_list::track_reveal::reveal_position(
        &track_list.shared,
        100,
        8,
        crate::ui::track_list::track_reveal::RevealMotion::Glide,
    );
    let playing_centre = centered_value_for(&track_list, playing_id).unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        (adjustment.value() - playing_centre).abs() < 1.0
    });
    // Nothing the user did with the wheel — an automatic advance yields to
    // recent scrolling, and this test is about the case where it must not.
    track_list.shared.last_scroll_activity.set(None);

    // `delete_tracks::finish`, in its own order: the player advances past the
    // deleted track, then the generation-guarded catalog delta runs from the
    // state captured before the dialog/worker.
    let reload_state = crate::ui::delete_tracks::capture_catalog_delete_reload(&track_list.shared);
    let samples: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let sampler = {
        let samples = samples.clone();
        let adjustment = adjustment.clone();
        gtk4::glib::timeout_add_local(Duration::from_millis(8), move || {
            samples.borrow_mut().push(adjustment.value());
            gtk4::glib::ControlFlow::Continue
        })
    };
    crate::test_db::connection(&track_list.shared.conn)
        .execute("DELETE FROM tracks WHERE id = ?1", [playing_id])
        .unwrap();
    track_list.update_current_track(next_id, None, CurrentTrackChange::AutomaticAdvance);
    crate::ui::delete_tracks::reload_after_catalog_delete(
        &track_list.shared,
        &[playing_id],
        reload_state,
    );
    // Waiting on the destination rather than on a stopwatch: under load the
    // reveal's idle and its glide can take longer than any fixed delay, and a
    // viewport that arrives late still arrived. What must not happen is
    // arriving somewhere else and staying there, which the settle below cannot
    // hide — it ends on the wrong value just as it would on no value.
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        centered_value_for(&track_list, next_id)
            .is_some_and(|centre| (adjustment.value() - centre).abs() < 1.0)
    });
    // …and then stays there: the hold that guards a reload outlives the glide,
    // so arriving is only half of it.
    crate::ui::test_settle::settle_for(PAST_THE_HOLD_AND_GLIDE);
    sampler.remove();

    let expected = centered_value_for(&track_list, next_id)
        .expect("the next track must still be in the reloaded view");
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
        "the sampler did not cover the delete/follow journey; {sample_report}"
    );
    // Range-derived height only bounds the jump and row-edge error; it is not the target oracle.
    let row_height = adjustment.upper() / f64::from(track_list.shared.model.n_items());
    assert!(
        minimum > playing_centre - row_height * 2.0,
        "the viewport jumped towards the top while following the next track: \
         deleted track centre={playing_centre}, row height={row_height}; {sample_report}"
    );
    assert!(
        (adjustment.value() - expected).abs() <= row_height,
        "the table did not follow playback past the deleted track: actual {}, expected \
         {expected}, the deleted track's place was {playing_centre}; {sample_report}\ntrail:\n{}",
        adjustment.value(),
        track_list.shared.diagnostic_trail.snapshot().join("\n")
    );

    window.close();
}
