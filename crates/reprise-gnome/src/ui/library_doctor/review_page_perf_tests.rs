//! Generated selection-refresh probes for the Library Doctor review page.

use std::cell::Cell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Instant;

use libadwaita as adw;
use reprise_core::library_doctor::{DoctorField, DoctorReviewRowId, DoctorScan, DoctorValue};

use super::super::review_row::contract_tests::{conflict_scan, scan};
use super::*;

const TRACKS_PER_ALBUM: usize = 12;
const CHURN_FIXTURE_ALBUMS: usize = 16;
const WALL_CLOCK_TOGGLES: usize = 9;
const MAX_PERF_ALBUMS: usize = 1_000;

/// Measured on 57ff0bfc74 with the 16 x 12 fixture: 386 items changed before
/// the fix. Measured on this incremental path: 24 items for the same toggle.
const MAX_TOGGLE_CHURN: u32 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AlbumHeaderCounts {
    selected: usize,
    selectable: usize,
    changes: usize,
}

fn generated_scan(album_count: usize) -> DoctorScan {
    let template = scan();
    let track_template = template.tracks[0].clone();
    let proposal_template = template.proposals[0].clone();
    let mut generated = template;
    generated.track_ids.clear();
    generated.tracks.clear();
    generated.proposals.clear();
    generated.unresolved_groups = conflict_scan().unresolved_groups;

    for album in 0..album_count {
        for track in 0..TRACKS_PER_ALBUM {
            let index = album * TRACKS_PER_ALBUM + track;
            let track_id = i64::try_from(index + 1).expect("generated track id fits i64");
            let mut snapshot = track_template.clone();
            snapshot.reference.track_id = track_id;
            snapshot.reference.path = PathBuf::from(format!(
                "/synthetic/doctor-review/album-{album:04}/track-{track:02}.flac"
            ));
            let tags = snapshot.tags.as_mut().expect("fixture track has tags");
            tags.title = format!("Track {track:02}");
            tags.artist = format!("Artist {album:04}");
            tags.album = format!("Album {album:04}");
            tags.album_artist = format!("Artist {album:04}");
            tags.track_no = Some(u32::try_from(track + 1).expect("fixture track number fits u32"));

            let mut proposal = proposal_template.clone();
            proposal.track_id = track_id;
            proposal.field = DoctorField::Title;
            proposal.current = DoctorValue::Text(format!("Track {track:02}"));
            proposal.proposed = DoctorValue::Text(format!("Track {track:02} corrected"));

            generated.track_ids.push(track_id);
            generated.tracks.push(snapshot);
            generated.proposals.push(proposal);
        }
    }
    generated.checked_tracks = album_count * TRACKS_PER_ALBUM;
    generated
}

fn page_for(scan: &DoctorScan) -> Rc<LibraryDoctorReviewPage> {
    gtk4::init().expect("the Library Doctor performance probe requires a display");
    let conn = Rc::new(crate::test_db::open().unwrap());
    let parent = adw::ApplicationWindow::builder().build();
    let on_edit = Rc::new(|_: &[i64]| {}) as Rc<dyn Fn(&[i64])>;
    LibraryDoctorReviewPage::new(
        &conn,
        &parent,
        scan,
        Rc::new(|_| {}),
        Rc::new(|| {}),
        &on_edit,
    )
}

fn album_header_counts(rows: &[ReviewRowModel]) -> HashMap<String, AlbumHeaderCounts> {
    let mut counts = HashMap::<String, AlbumHeaderCounts>::new();
    for row in rows {
        let album = counts
            .entry(row.album_key.clone())
            .or_insert(AlbumHeaderCounts {
                selected: 0,
                selectable: 0,
                changes: 0,
            });
        album.selected += row.selected_change_count;
        album.selectable += row.selectable_row_ids.len();
        album.changes += row.row_ids.len();
    }
    counts
}

fn row_ids_for_album(rows: &[ReviewRowModel], album_key: &str) -> Vec<DoctorReviewRowId> {
    rows.iter()
        .filter(|row| row.album_key == album_key)
        .flat_map(|row| row.selectable_row_ids.iter().copied())
        .collect()
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn review_selection_toggle_touches_only_the_toggled_album() {
    let page = page_for(&generated_scan(CHURN_FIXTURE_ALBUMS));
    let rows_before = page.state.visible_rows();
    let counts_before = album_header_counts(&rows_before);
    assert_eq!(rows_before.len(), CHURN_FIXTURE_ALBUMS * TRACKS_PER_ALBUM);
    assert_eq!(counts_before.len(), CHURN_FIXTURE_ALBUMS);
    let sorted_items_before = page.state.sorted.n_items();
    let toggled_album = rows_before[0].album_key.clone();
    let toggled_ids = row_ids_for_album(&rows_before, &toggled_album);
    assert_eq!(toggled_ids.len(), TRACKS_PER_ALBUM);
    let churn = Rc::new(Cell::new(0_u32));
    page.state.store.connect_items_changed({
        let churn = churn.clone();
        move |_, _, removed, added| churn.set(churn.get() + removed + added)
    });

    page.state.set_selected(&toggled_ids, false);

    let observed_churn = churn.get();
    eprintln!(
        "PERFORMANCE doctor_review churn fixture_albums={CHURN_FIXTURE_ALBUMS} tracks_per_album={TRACKS_PER_ALBUM} observed_items={observed_churn}"
    );
    assert!(
        observed_churn <= MAX_TOGGLE_CHURN,
        "one album toggle changed {observed_churn} store items; budget is {MAX_TOGGLE_CHURN}"
    );
    assert_eq!(page.state.sorted.n_items(), sorted_items_before);

    let counts_after = album_header_counts(&page.state.visible_rows());
    assert_eq!(
        counts_before[&toggled_album],
        AlbumHeaderCounts {
            selected: TRACKS_PER_ALBUM,
            selectable: TRACKS_PER_ALBUM,
            changes: TRACKS_PER_ALBUM,
        }
    );
    assert_eq!(
        counts_after[&toggled_album],
        AlbumHeaderCounts {
            selected: 0,
            selectable: TRACKS_PER_ALBUM,
            changes: TRACKS_PER_ALBUM,
        }
    );
    for (album_key, before) in counts_before {
        if album_key != toggled_album {
            assert_eq!(
                counts_after[&album_key], before,
                "album {album_key} changed"
            );
        }
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn doc_12a_a_query_change_splices_no_store_items() {
    let _guard = crate::ui::test_main_context::lock_main_context();
    let page = page_for(&generated_scan(CHURN_FIXTURE_ALBUMS));
    let churn = Rc::new(Cell::new(0_u32));
    page.state.store.connect_items_changed({
        let churn = churn.clone();
        move |_, _, removed, added| churn.set(churn.get() + removed + added)
    });

    page.state.set_query("Artist 0001");
    page.state.set_query("");

    assert_eq!(churn.get(), 0, "query changes must not splice the store");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn review_selection_toggle_wall_clock_probe() {
    let Ok(album_count) = std::env::var("REPRISE_DOCTOR_PERF_ALBUMS") else {
        return;
    };
    let album_count = album_count
        .parse::<usize>()
        .expect("REPRISE_DOCTOR_PERF_ALBUMS must be an integer");
    assert!(
        (WALL_CLOCK_TOGGLES..=MAX_PERF_ALBUMS).contains(&album_count),
        "REPRISE_DOCTOR_PERF_ALBUMS must be between {WALL_CLOCK_TOGGLES} and {MAX_PERF_ALBUMS}"
    );
    let (full_median_us, full_max_us) = measure_selection_path(album_count, true);
    let (selection_median_us, selection_max_us) = measure_selection_path(album_count, false);
    std::env::remove_var("REPRISE_DOCTOR_FULL_REFRESH");
    eprintln!(
        "PERFORMANCE doctor_review path=full albums={album_count} rows={} toggles={WALL_CLOCK_TOGGLES} median_us={full_median_us} max_us={full_max_us}",
        album_count * TRACKS_PER_ALBUM
    );
    eprintln!(
        "PERFORMANCE doctor_review path=selection albums={album_count} rows={} toggles={WALL_CLOCK_TOGGLES} median_us={selection_median_us} max_us={selection_max_us}",
        album_count * TRACKS_PER_ALBUM
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn review_search_wall_clock_probe() {
    let _guard = crate::ui::test_main_context::lock_main_context();
    let Ok(album_count) = std::env::var("REPRISE_DOCTOR_PERF_ALBUMS") else {
        eprintln!("PERFORMANCE doctor_review path=search skipped=REPRISE_DOCTOR_PERF_ALBUMS-unset");
        return;
    };
    let album_count = album_count
        .parse::<usize>()
        .expect("REPRISE_DOCTOR_PERF_ALBUMS must be an integer");
    assert!(
        (1..=MAX_PERF_ALBUMS).contains(&album_count),
        "REPRISE_DOCTOR_PERF_ALBUMS must be between 1 and {MAX_PERF_ALBUMS}"
    );
    let page = page_for(&generated_scan(album_count));
    let row_count = page.state.snapshot.borrow().rows.len();
    assert!(row_count > 0, "review search fixture built zero rows");
    let queries = ["A", "Ar", "Art", "Arti", "Artis", "Artist", ""];
    let mut elapsed = Vec::with_capacity(queries.len());
    for query in queries {
        let started = Instant::now();
        page.state.set_query(query);
        elapsed.push(started.elapsed().as_micros());
        assert_eq!(
            page.state.query.borrow().as_str(),
            query,
            "review search set_query was not reached"
        );
    }
    assert!(!elapsed.is_empty(), "review search produced no timings");
    elapsed.sort_unstable();
    let median_us = elapsed[elapsed.len() / 2];
    let max_us = *elapsed.last().expect("review search produced timings");
    eprintln!(
        "PERFORMANCE doctor_review path=search albums={album_count} rows={row_count} median_us={median_us} max_us={max_us}"
    );
}

fn measure_selection_path(album_count: usize, full_refresh_only: bool) -> (u128, u128) {
    if full_refresh_only {
        std::env::set_var("REPRISE_DOCTOR_FULL_REFRESH", "1");
    } else {
        std::env::remove_var("REPRISE_DOCTOR_FULL_REFRESH");
    }
    let page = page_for(&generated_scan(album_count));
    assert_eq!(page.state.full_refresh_only, full_refresh_only);
    let rows = page.state.visible_rows();
    let mut albums = rows
        .iter()
        .map(|row| row.album_key.clone())
        .collect::<Vec<_>>();
    albums.dedup();
    let toggle_ids = albums
        .iter()
        .take(WALL_CLOCK_TOGGLES)
        .map(|album| row_ids_for_album(&rows, album))
        .collect::<Vec<_>>();
    let mut elapsed = Vec::with_capacity(WALL_CLOCK_TOGGLES);

    for row_ids in toggle_ids {
        {
            let mut session = page.state.session.borrow_mut();
            for row_id in row_ids {
                session.set_selected(row_id, false).unwrap();
            }
        }
        let started = Instant::now();
        page.state.apply_selection(true);
        elapsed.push(started.elapsed().as_micros());
    }

    elapsed.sort_unstable();
    let median_us = elapsed[WALL_CLOCK_TOGGLES / 2];
    let max_us = elapsed[WALL_CLOCK_TOGGLES - 1];
    (median_us, max_us)
}
