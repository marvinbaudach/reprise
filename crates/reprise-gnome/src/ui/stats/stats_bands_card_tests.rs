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
    let (card, snapshot) = card_and_full_ranking_snapshot();

    card.set_data(&snapshot);
    card.reveal_button.emit_clicked();
    let by_time = (
        card.leader_label(),
        card.runner_up_labels()[0].clone(),
        card.continuation_labels()[0].clone(),
        card.leader_summary(),
    );
    card.sort_toggle.set_active_name(Some("plays"));
    let by_plays = (
        card.leader_label(),
        card.runner_up_labels()[0].clone(),
        card.continuation_labels()[0].clone(),
        card.leader_summary(),
    );

    assert_eq!(
        by_time,
        (
            "Marathon".to_string(),
            "Mid".to_string(),
            "Other Six".to_string(),
            "2 plays · 20 min · 30% of your artist listening".to_string(),
        )
    );
    assert_eq!(
        by_plays,
        (
            "Sprinter".to_string(),
            "Play Runner".to_string(),
            "Other Five".to_string(),
            "10 plays · 10 min · 15% of your artist listening".to_string(),
        )
    );
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
    card.state.rows.borrow()[0].open_button.emit_clicked();

    assert_eq!(&*opened.borrow(), &["Artist 06"]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_23_a_continuation_row_retains_the_unification_hint() {
    gtk4::init().unwrap();
    let (card, mut snapshot) = card_and_snapshot_with(6);
    let artist = snapshot
        .top_artists
        .iter_mut()
        .find(|artist| artist.group.label == "Artist 06")
        .unwrap();
    artist.group.variant_count = 2;
    let unified = Rc::new(RefCell::new(Vec::new()));
    card.set_on_unify({
        let unified = unified.clone();
        move |key| unified.borrow_mut().push(key)
    });

    card.set_data(&snapshot);
    card.reveal_button.emit_clicked();
    let rows = card.state.rows.borrow();
    assert!(rows[0].unify_button.is_visible());
    assert_eq!(
        rows[0].unify_button.tooltip_text().as_deref(),
        Some("2 spellings merged — unify them in the tag editor?")
    );
    assert!(gtk4::test_accessible_has_property(
        &rows[0].unify_button,
        gtk4::AccessibleProperty::Label
    ));
    rows[0].unify_button.emit_clicked();

    assert_eq!(&*unified.borrow(), &["name:artist 06"]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_23_continuation_controls_use_shared_states_and_accessibility() {
    gtk4::init().unwrap();
    let (card, snapshot) = card_and_snapshot_with(6);

    card.set_data(&snapshot);
    card.reveal_button.emit_clicked();

    assert!(card.state.rows.borrow()[0]
        .open_button
        .has_css_class(crate::ui::style::buttons::TERTIARY_CLASS));
    assert!(gtk4::test_accessible_has_state(
        &card.reveal_button,
        gtk4::AccessibleState::Expanded
    ));
    assert!(gtk4::test_accessible_has_relation(
        &card.reveal_button,
        gtk4::AccessibleRelation::Controls
    ));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_23_continuation_bar_matches_the_song_ranking_geometry() {
    gtk4::init().unwrap();
    let (card, snapshot) = card_and_snapshot_with(6);

    card.set_data(&snapshot);
    card.reveal_button.emit_clicked();
    let rows = card.state.rows.borrow();

    assert_eq!(rows[0].bar.height_request(), 8);
    assert_eq!(rows[0].bar.valign(), gtk4::Align::Center);
}

fn card_and_full_ranking_snapshot() -> (StatsBandsCard, StatsSnapshot) {
    let conn = crate::test_db::open().unwrap();
    for (id, artist, duration_ms, plays) in [
        (1, "Sprinter", 60_000, 10),
        (2, "Play Runner", 50_000, 9),
        (3, "Mid", 80_000, 8),
        (4, "Other Seven", 70_000, 7),
        (5, "Other Six", 60_000, 6),
        (6, "Other Five", 60_000, 5),
        (7, "Marathon", 600_000, 2),
    ] {
        insert_artist(&conn, id, artist, duration_ms, plays);
    }
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
