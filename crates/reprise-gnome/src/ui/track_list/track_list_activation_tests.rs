use std::rc::Rc;

use super::*;
use crate::ui::track_list::TrackList;

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn activation_ids_are_reused_until_the_track_model_generation_changes() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    // Distinct artists give the default artist-ascending sort a unique order
    // to settle on. Tied artists have no tiebreaker in `SORT_WHITELIST`
    // (`queries::clauses`), so their relative order falls out of whichever
    // plan the query planner happens to pick — not what this test means to
    // exercise, which is activation-id caching across model generations.
    crate::test_db::connection(&conn)
        .execute_batch(
            "INSERT INTO tracks (path, title, artist, added_at) VALUES
             ('/music/one.flac', 'One', 'Charlie', 0),
             ('/music/two.flac', 'Two', 'Alice', 0);",
        )
        .unwrap();
    let track_list = TrackList::new(
        conn.clone(),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let generation = track_list.shared.model.generation();

    let first = queue_ids_for_activation(&track_list.shared, 0, 1).0;
    assert_eq!(first, vec![2, 1]);

    crate::test_db::connection(&conn)
        .execute(
            "INSERT INTO tracks (path, title, artist, added_at)
             VALUES ('/music/three.flac', 'Three', 'Bob', 0)",
            [],
        )
        .unwrap();
    let without_reload = queue_ids_for_activation(&track_list.shared, 0, 1).0;
    assert_eq!(
        without_reload, first,
        "the same rendered generation must reuse its activation ids"
    );
    assert_eq!(track_list.shared.model.generation(), generation);

    track_list.reload();
    assert_ne!(track_list.shared.model.generation(), generation);
    let after_reload = queue_ids_for_activation(&track_list.shared, 0, 1).0;
    assert_eq!(after_reload, vec![2, 3, 1]);
}
