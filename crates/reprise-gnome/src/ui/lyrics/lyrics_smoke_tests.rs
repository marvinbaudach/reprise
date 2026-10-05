use reprise_core::modules::{self, ONLINE_LYRICS_MODULE};
use reprise_core::online_sources;

use super::*;

#[test]
fn the_isolated_gate_allows_the_lyrics_module() {
    let db = crate::test_db::open_fresh().unwrap();
    assert_eq!(
        online_sources::network_allowed(&db, &ONLINE_LYRICS_MODULE).ok(),
        Some(false)
    );

    open_isolated_lyrics_gate(&db).unwrap();

    assert_eq!(
        online_sources::network_allowed(&db, &ONLINE_LYRICS_MODULE).ok(),
        Some(true)
    );
    assert_eq!(online_sources::is_enabled(&db).ok(), Some(true));
    assert_eq!(
        modules::is_enabled(&db, &ONLINE_LYRICS_MODULE).ok(),
        Some(true)
    );
}

#[test]
fn the_module_alone_does_not_allow_the_network() {
    let db = crate::test_db::open_fresh().unwrap();
    modules::set_enabled(&db, &ONLINE_LYRICS_MODULE, true).unwrap();

    assert_eq!(
        modules::is_enabled(&db, &ONLINE_LYRICS_MODULE).ok(),
        Some(true)
    );
    assert_eq!(
        online_sources::network_allowed(&db, &ONLINE_LYRICS_MODULE).ok(),
        Some(false)
    );
}
