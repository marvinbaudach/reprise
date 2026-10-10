//! The binary this suite spawns is built with the core's test isolation, so
//! no default path it resolves can be the user's `~/.local/share/reprise`.

#[test]
fn the_core_data_root_is_the_test_private_directory() {
    assert!(
        reprise_core::data_root::is_isolated(),
        "reprise-core must be built with its `test-private-dirs` feature for this crate's tests"
    );
    let database = reprise_core::db::default_path();
    assert!(database.starts_with(std::env::temp_dir()), "{database:?}");
}
