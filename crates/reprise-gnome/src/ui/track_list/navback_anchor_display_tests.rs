//! Probe: Back out of a filtered artist view must land on the row the
//! library was anchored to when the artist link was clicked.
//!
//! The two views have very different lengths, which is the point: the restore
//! path derives its row height from `adjustment.upper() / current_ids.len()`,
//! and `upper` still belongs to the view being left at the moment the first
//! write happens.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use reprise_core::browser::{
    ArtistKey, BrowserPlace, LibraryScope, SortDirection, TrackCollection, TrackFocus, TrackSort,
    TrackViewState,
};
use reprise_core::view_source::ViewSource;

use crate::ui::track_list::TrackList;

const PAST_THE_SCROLL_HOLD: Duration = Duration::from_millis(600);
/// The 8ms sampler yields ~70 values over the window above. A run starved of
/// main-loop turns collects far fewer, and a handful of samples cannot show
/// that the viewport never visited the top — so demand a floor well below the
/// healthy count but far above "it ran at all".
const MIN_SAMPLES: usize = 20;
const FILTER_ARTIST: &str = "Filter Artist";
const ROWS: i64 = 2_276;
/// Mirrors a real library: ~2300 tracks, an artist holding ~23 of them —
/// long enough to scroll in a 320px window, far shorter than the library.
const FILTER_EVERY: i64 = 100;
const ANCHOR_POSITION: f64 = 1_100.0;

/// Which of the real journey's traits this run reproduces. Each is a thing
/// the plain probe left out, isolated so a red run names its own cause.
#[derive(Clone, Copy)]
struct Journey {
    /// The artist link builds its destination from `TrackViewState::default()`
    /// (see `navigation::metadata_target_state`), so coming back re-applies
    /// the library's own sort — a sort change on top of the source change.
    sort_differs: bool,
    /// The user clicked the link *in the table*, so the table has focus and
    /// the captured place carries `TrackFocus::Track`, which makes the
    /// restore call `grab_focus()` on a `ColumnView` whose focus row the
    /// model swap just reset.
    focus_in_table: bool,
}

struct Outcome {
    actual: f64,
    expected: f64,
    row_height: f64,
    top_id: Option<i64>,
    expected_top_id: i64,
    samples: Vec<f64>,
}

fn mixed_track_list() -> (TrackList, gtk4::Window) {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=ROWS {
        let artist = if id % FILTER_EVERY == 0 {
            FILTER_ARTIST
        } else {
            "Bulk Artist"
        };
        tx.execute(
            "INSERT INTO tracks (id, path, title, album, artist, added_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
                format!("Album {:03}", ROWS - id),
                artist,
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let track_list = TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
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

fn sorted_state(field: &str) -> TrackViewState {
    TrackViewState {
        sort: TrackSort::new(field, SortDirection::Ascending),
        ..TrackViewState::default()
    }
}

/// The row the viewport's top edge is sitting on right now, as a track id.
fn top_row_track_id(track_list: &TrackList) -> Option<i64> {
    let adjustment = track_list.shared.column_view.vadjustment()?;
    let total = track_list.shared.model.n_items();
    if total == 0 || adjustment.upper() <= 0.0 {
        return None;
    }
    let height = adjustment.upper() / f64::from(total);
    let index = (adjustment.value() / height).floor().max(0.0) as u32;
    track_list
        .shared
        .model
        .track_at(index)
        .map(|track| track.id)
}

/// Library → artist link → play → Back, routed the way
/// `library_shell::route_to_place`'s non-scoped branch routes it: the sidebar
/// re-selects the source first (a `reload()` preserving the *filtered* view's
/// viewport), and only then does the router restore the saved place.
fn run_journey(track_list: &TrackList, journey: Journey) -> Outcome {
    // The library the user is browsing.
    let library_sort = if journey.sort_differs {
        "album"
    } else {
        "title"
    };
    assert!(track_list.restore_browser_place(&BrowserPlace::tracks(
        TrackCollection::Library(LibraryScope::All),
        sorted_state(library_sort),
    )));
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    let library_ids = track_list.shared.current_view_ids();
    eprintln!(
        "PROBE ids: current_view_ids={} model_n_items={} rows_inserted={ROWS} upper={}",
        library_ids.len(),
        track_list.shared.model.n_items(),
        adjustment.upper(),
    );
    let row_height = adjustment.upper() / library_ids.len() as f64;

    // Scrolled deep into the library, where the artist link gets clicked.
    adjustment.set_value(row_height * ANCHOR_POSITION);
    crate::ui::test_settle::settle_for(Duration::from_millis(100));
    assert!(
        adjustment.value() > adjustment.page_size() * 2.0,
        "precondition: the user is deep in the library, not near the top"
    );

    let mut captured = track_list.browser_place();
    let captured_anchor = captured
        .track_state()
        .expect("the library place must carry track view state")
        .anchor
        .expect("a scrolled library must capture an anchor");
    let expected_top_id = captured_anchor.track_id;
    if journey.focus_in_table {
        // `capture` only records this when the ColumnView reports focus, which
        // it cannot in a headless test — the user's click gave it focus.
        let BrowserPlace::Tracks(place) = &mut captured else {
            unreachable!("the library place is a track place")
        };
        place.state.focus = TrackFocus::Track(expected_top_id);
        place.state.selected_ids = vec![expected_top_id];
    }

    // Follow the artist link. The real intent builds its state from
    // `TrackViewState::default()`, so the filtered view carries the default
    // sort rather than the library's.
    let filtered_state = if journey.sort_differs {
        TrackViewState::default()
    } else {
        sorted_state("title")
    };
    assert!(track_list.restore_browser_place(&BrowserPlace::tracks(
        TrackCollection::Library(LibraryScope::Artist(ArtistKey::new(FILTER_ARTIST))),
        filtered_state,
    )));
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);
    assert_eq!(
        track_list.shared.current_view_ids().len(),
        (ROWS / FILTER_EVERY) as usize,
        "precondition: the filtered view is much shorter than the library"
    );
    assert!(
        adjustment.upper() > adjustment.page_size(),
        "precondition: the filtered view itself scrolls, so the restore path \
         does not bail out on `upper <= page`"
    );

    // Play something in the filtered view, which also moves its viewport.
    let playing_id = track_list
        .shared
        .model
        .track_at(12)
        .expect("the filtered view has rows")
        .id;
    track_list.shared.playing_track_id.set(Some(playing_id));
    adjustment.set_value(adjustment.upper() - adjustment.page_size());
    crate::ui::test_settle::settle_for(Duration::from_millis(100));

    // Back. Sample every 8ms so a jump-and-settle shows up, not just the
    // resting value.
    let samples: Rc<RefCell<Vec<f64>>> = Rc::new(RefCell::new(Vec::new()));
    let sampler = {
        let samples = samples.clone();
        let adjustment = adjustment.clone();
        gtk4::glib::timeout_add_local(Duration::from_millis(8), move || {
            samples.borrow_mut().push(adjustment.value());
            gtk4::glib::ControlFlow::Continue
        })
    };
    track_list.set_source(ViewSource::Library);
    let reveal_destination = track_list
        .shared
        .scroll_glide
        .deliberate_destination()
        .expect("the source switch must reveal the playing track before Back restores history");
    assert_ne!(
        playing_id, expected_top_id,
        "the test must distinguish the playing reveal from Back's history anchor"
    );
    eprintln!(
        "PROBE Back boundary: playing_id={playing_id} anchor_id={expected_top_id} \
         reveal_destination={reveal_destination}"
    );
    assert!(track_list.restore_browser_place(&captured));
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);
    sampler.remove();

    let restored_ids = track_list.shared.current_view_ids();
    let restored_height = adjustment.upper() / restored_ids.len() as f64;
    let anchor_index = restored_ids
        .iter()
        .position(|id| *id == expected_top_id)
        .expect("the anchored track is still in the library");
    let taken = samples.borrow().clone();
    Outcome {
        actual: adjustment.value(),
        expected: restored_height * anchor_index as f64 + captured_anchor.row_offset,
        row_height: restored_height,
        top_id: top_row_track_id(track_list),
        expected_top_id,
        samples: taken,
    }
}

fn assert_landed_on_the_anchor(label: &str, outcome: &Outcome) {
    assert!(
        outcome.samples.len() >= MIN_SAMPLES,
        "the sampler must have run throughout the journey — {} samples prove \
         nothing about the frames in between",
        outcome.samples.len(),
    );
    eprintln!(
        "PROBE {label}: value={} expected={} row_height={} top={:?} wanted={} \
         samples(n={} first={:?} min={:?} max={:?})",
        outcome.actual,
        outcome.expected,
        outcome.row_height,
        outcome.top_id,
        outcome.expected_top_id,
        outcome.samples.len(),
        outcome.samples.first(),
        outcome
            .samples
            .iter()
            .cloned()
            .fold(f64::INFINITY, f64::min),
        outcome
            .samples
            .iter()
            .cloned()
            .fold(f64::NEG_INFINITY, f64::max),
    );
    // The resting value has been correct all along; what the user sees is the
    // viewport visiting the top of the list on the way there. Nothing in this
    // journey may take it above the anchored row.
    let lowest = outcome
        .samples
        .iter()
        .cloned()
        .fold(f64::INFINITY, f64::min);
    assert!(
        lowest > outcome.expected - outcome.row_height * 2.0,
        "{label}: the viewport jumped towards the top of the list on the way \
         back: lowest sampled value {lowest}, anchored row sits at {} \
         (row height {})",
        outcome.expected,
        outcome.row_height,
    );
    assert!(
        (outcome.actual - outcome.expected).abs() < outcome.row_height,
        "{label}: Back must land on the anchored row: actual {}, expected {} \
         (row height {}); top row is {:?}, wanted {}",
        outcome.actual,
        outcome.expected,
        outcome.row_height,
        outcome.top_id,
        outcome.expected_top_id,
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_back_lands_on_the_anchored_row() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = mixed_track_list();
    let outcome = run_journey(
        &track_list,
        Journey {
            sort_differs: false,
            focus_in_table: false,
        },
    );
    assert_landed_on_the_anchor("plain", &outcome);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_back_lands_on_the_anchored_row_when_the_sort_differs() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = mixed_track_list();
    let outcome = run_journey(
        &track_list,
        Journey {
            sort_differs: true,
            focus_in_table: false,
        },
    );
    assert_landed_on_the_anchor("sort-differs", &outcome);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_back_lands_on_the_anchored_row_when_the_table_had_focus() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = mixed_track_list();
    let outcome = run_journey(
        &track_list,
        Journey {
            sort_differs: false,
            focus_in_table: true,
        },
    );
    assert_landed_on_the_anchor("focus-in-table", &outcome);
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_back_lands_on_the_anchored_row_in_the_full_journey() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = mixed_track_list();
    let outcome = run_journey(
        &track_list,
        Journey {
            sort_differs: true,
            focus_in_table: true,
        },
    );
    assert_landed_on_the_anchor("full", &outcome);
    window.close();
}
