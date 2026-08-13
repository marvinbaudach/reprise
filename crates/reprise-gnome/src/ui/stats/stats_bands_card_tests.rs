use super::*;

use chrono::Utc;
use reprise_core::library::stats_period::StatsPeriod;
use reprise_core::library::stats_snapshot::{self, StatsSnapshot};

use crate::ui::cover_loader::CoverLoader;
use crate::ui::stats::stats_artist_image::StatsArtistImage;

/// The continuation continues the ranking: five surfaces on screen, fifteen
/// more behind the button, no rank shown twice.
#[test]
fn stats_23_the_continuation_starts_at_rank_six() {
    assert_eq!(RUNNER_UP_COUNT, 4);
    assert_eq!(ARTIST_ROW_EXTRA, 15);
    assert_eq!(first_continuation_rank(), 6);
}

/// The button is offered only when it would open onto something.
#[test]
fn stats_23_the_expander_is_offered_only_past_the_five_on_screen() {
    assert!(!has_continuation(5));
    assert!(has_continuation(6));
    assert!(has_continuation(150));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_23_the_toggle_reorders_the_whole_row() {
    gtk4::init().unwrap();
    let (card, snapshot) = card_and_snapshot();

    card.set_data(&snapshot);
    let by_time = card.leader_label();
    card.sort_toggle.set_active_name(Some("plays"));
    let by_plays = card.leader_label();

    assert_eq!(by_time, "Marathon");
    assert_eq!(by_plays, "Sprinter");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_23_show_more_reveals_the_continuation_rows() {
    gtk4::init().unwrap();
    let (card, snapshot) = card_and_snapshot_with(9);

    card.set_data(&snapshot);
    assert!(!card.revealer.reveals_child());
    assert_eq!(card.reveal_button.label().unwrap(), "Show more top artists");

    card.reveal_button.emit_clicked();

    assert!(card.revealer.reveals_child());
    assert_eq!(card.reveal_button.label().unwrap(), "Hide more top artists");
    assert_eq!(card.continuation_rows(), 4, "ranks 6 to 9");
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_23_a_continuation_row_opens_its_artist() {
    gtk4::init().unwrap();
    let (card, snapshot) = card_and_snapshot_with(6);
    let opened = Rc::new(RefCell::new(Vec::new()));
    card.set_on_open_artist({
        let opened = opened.clone();
        move |artist| opened.borrow_mut().push(artist)
    });

    card.set_data(&snapshot);
    card.reveal_button.emit_clicked();
    card.state.rows.borrow()[0].root.emit_clicked();

    assert_eq!(&*opened.borrow(), &["Artist 06"]);
}

fn card_and_snapshot() -> (StatsBandsCard, StatsSnapshot) {
    let conn = crate::test_db::open().unwrap();
    insert_artist(&conn, 1, "Sprinter", 60_000, 6);
    insert_artist(&conn, 2, "Marathon", 600_000, 2);
    snapshot_card(&conn)
}

fn card_and_snapshot_with(artists: i64) -> (StatsBandsCard, StatsSnapshot) {
    let conn = crate::test_db::open().unwrap();
    for id in 1..=artists {
        insert_artist(
            &conn,
            id,
            &format!("Artist {id:02}"),
            60_000,
            usize::try_from(artists - id + 1).unwrap(),
        );
    }
    snapshot_card(&conn)
}

fn insert_artist(
    conn: &reprise_core::db::Db,
    id: i64,
    artist: &str,
    duration_ms: i64,
    plays: usize,
) {
    crate::test_db::connection(conn)
        .execute(
            "INSERT INTO tracks \
             (id, path, title, artist, album, album_artist, genre, duration_ms, added_at) \
             VALUES (?1, ?2, 'Track', ?3, ?4, '', 'Rock', ?5, 0)",
            rusqlite::params![
                id,
                format!("/music/{id}.flac"),
                artist,
                format!("Album {id}"),
                duration_ms,
            ],
        )
        .unwrap();
    for play in 0..plays {
        crate::test_db::connection(conn)
            .execute(
                "INSERT INTO listen_events (track_id, played_at, ms_played) \
                 VALUES (?1, ?2, ?3)",
                rusqlite::params![id, 1_000 + i64::try_from(play).unwrap(), duration_ms],
            )
            .unwrap();
    }
}

fn snapshot_card(conn: &reprise_core::db::Db) -> (StatsBandsCard, StatsSnapshot) {
    let snapshot = stats_snapshot::compute(conn, StatsPeriod::AllTime, 10_000, &Utc).unwrap();
    let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    let image = StatsArtistImage::for_test(loader, |_| None);
    (StatsBandsCard::new(image), snapshot)
}
