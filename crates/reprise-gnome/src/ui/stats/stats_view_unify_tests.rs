use super::*;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_unify_wiring_resolves_artist_and_genre_group_ids() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    let view = StatsView::new(loader);
    for (id, artist, genre) in [
        (1, "Lorna Shore", "Metal"),
        (2, "lorna shore ", "Metal"),
        (3, "Artist A", "Deathcore"),
        (4, "Artist B", "deathcore "),
    ] {
        crate::test_db::connection(&conn)
            .execute(
                "INSERT INTO tracks \
                 (id, path, title, artist, album_artist, genre, duration_ms, added_at) \
                 VALUES (?1, ?2, ?3, ?4, '', ?5, 180000, 1)",
                rusqlite::params![
                    id,
                    format!("/music/{id}.flac"),
                    format!("Track {id}"),
                    artist,
                    genre
                ],
            )
            .unwrap();
    }
    view.wire_year_selector(&conn);
    let resolved = Rc::new(RefCell::new(Vec::new()));
    view.set_on_unify_spellings({
        let resolved = resolved.clone();
        move |ids| resolved.borrow_mut().push(ids)
    });

    view.render.band_card.emit_unify("name:lorna shore");
    view.render.genres_section_data.emit_unify("name:deathcore");

    assert_eq!(*resolved.borrow(), vec![vec![1, 2], vec![3, 4]]);
}
