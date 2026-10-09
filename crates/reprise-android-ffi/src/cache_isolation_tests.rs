//! The binding's tests reach `reprise-core`'s default cache directories (the
//! migration probes them, the artwork path falls back to them). They must
//! resolve a test-private directory, never the user's `~/.cache/reprise`.

#[test]
fn the_core_cache_root_is_the_test_private_directory() {
    assert!(
        reprise_core::cache_root::is_isolated(),
        "reprise-core must be built with its `test-cache-root` feature for this crate's tests"
    );
    let cover_cache = reprise_core::cover::cache_dir();
    assert!(
        cover_cache.starts_with(std::env::temp_dir()),
        "{cover_cache:?}"
    );
}
