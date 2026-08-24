//! NR-2a release-cover to artist-portrait state transitions.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use gtk4::prelude::*;
use reprise_core::cover_download::{CoverState, ReleaseGroupCover};

use super::release_cover::{override_cover_fetch, override_cover_state, LazyReleaseCover};
use crate::ui::artist_portrait_tiles::ArtistPortraitTiles;
use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;

const FIRST_MBID: &str = "11111111-1111-1111-1111-111111111111";
const SECOND_MBID: &str = "22222222-2222-2222-2222-222222222222";

fn image_fixture() -> PathBuf {
    std::fs::canonicalize(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../showroom/public/media/showroom/android-cover-360.webp"),
    )
    .expect("the committed portrait fixture exists")
}

fn image_chain(
    cached: impl Fn(&str) -> Option<PathBuf> + Send + Sync + 'static,
    enabled: bool,
    remote: impl Fn(&str) -> Option<PathBuf> + Send + Sync + 'static,
) -> Rc<ArtistPortraitTiles> {
    let image = ArtistPortraitTiles::for_test(cached);
    image.set_sources(
        CoverLoader::new(crate::ui::cover_download_worker::setup_for_test()),
        ArtistPortraitRuntime::for_test(enabled, remote),
    );
    image
}

fn settle_until(label: &str, ready: impl FnMut() -> bool) {
    assert!(
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, ready,),
        "timed out waiting for {label}"
    );
}

fn present(cover: &LazyReleaseCover) -> gtk4::Window {
    let window = gtk4::Window::new();
    window.set_child(Some(cover.widget()));
    window.present();
    settle_until("the release tile to map", || cover.widget().is_mapped());
    window
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_2a_cached_cover_never_calls_the_portrait_resolver() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = calls.clone();
    let chain = image_chain(
        move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            None
        },
        true,
        |_| panic!("a cached release cover must not start a portrait request"),
    );
    let cover_path = image_fixture();
    let _state = override_cover_state({
        let cover_path = cover_path.clone();
        move |_| CoverState::Cached(cover_path.clone())
    });
    let cover = LazyReleaseCover::new_unbound(56);
    cover.connect_artist_portrait_tiles(chain);

    cover.set_release(FIRST_MBID, "Mental Cruelty");

    assert!(cover.shows_image());
    assert_eq!(cover.picture_source_path(), Some(cover_path));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_2a_known_missing_cover_shows_cached_portrait_at_bind_time() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let portrait = image_fixture();
    let chain = image_chain(
        {
            let portrait = portrait.clone();
            move |_| Some(portrait.clone())
        },
        true,
        |_| panic!("the cached portrait should satisfy the request"),
    );
    let fetches = Arc::new(AtomicUsize::new(0));
    let counted = fetches.clone();
    let _state = override_cover_state(|_| CoverState::KnownMissing);
    let _fetch = override_cover_fetch(move |_| {
        counted.fetch_add(1, Ordering::SeqCst);
        ReleaseGroupCover::Fallback
    });
    let cover = LazyReleaseCover::new_unbound(56);
    cover.connect_artist_portrait_tiles(chain);

    cover.set_release(FIRST_MBID, "Mental Cruelty");
    settle_until("the cached portrait", || cover.shows_image());

    assert_eq!(fetches.load(Ordering::SeqCst), 0);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_2a_unknown_cover_fallback_then_shows_cached_portrait() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let portrait = image_fixture();
    let chain = image_chain(
        {
            let portrait = portrait.clone();
            move |_| Some(portrait.clone())
        },
        true,
        |_| panic!("the cached portrait should satisfy the request"),
    );
    let _state = override_cover_state(|_| CoverState::Unknown);
    let _fetch = override_cover_fetch(|_| ReleaseGroupCover::Fallback);
    let cover = LazyReleaseCover::new_unbound(56);
    cover.connect_artist_portrait_tiles(chain);
    cover.set_release(FIRST_MBID, "Mental Cruelty");

    let window = present(&cover);
    settle_until("the portrait after cover fallback", || cover.shows_image());

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_2a_disabled_artwork_module_never_requests_a_missing_portrait() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));
    let counted = requests.clone();
    let chain = image_chain(
        |_| None,
        false,
        move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            None
        },
    );
    let _state = override_cover_state(|_| CoverState::Unknown);
    let _fetch = override_cover_fetch(|_| ReleaseGroupCover::Fallback);
    let cover = LazyReleaseCover::new_unbound(56);
    cover.connect_artist_portrait_tiles(chain);
    cover.set_release(FIRST_MBID, "Mental Cruelty");

    let window = present(&cover);
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(100));

    assert_eq!(requests.load(Ordering::SeqCst), 0);
    assert!(!cover.shows_image());
    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nr_2a_rebinding_drops_an_in_flight_portrait_result() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let started = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let portrait = image_fixture();
    let chain = image_chain(|_| None, true, {
        let started = started.clone();
        let release = release.clone();
        let portrait = portrait.clone();
        move |_| {
            started.store(true, Ordering::SeqCst);
            while !release.load(Ordering::SeqCst) {
                std::thread::yield_now();
            }
            Some(portrait.clone())
        }
    });
    let new_cover = image_fixture();
    let _state = override_cover_state({
        let new_cover = new_cover.clone();
        move |mbid| {
            if mbid == FIRST_MBID {
                CoverState::KnownMissing
            } else {
                CoverState::Cached(new_cover.clone())
            }
        }
    });
    let cover = LazyReleaseCover::new_unbound(56);
    cover.connect_artist_portrait_tiles(chain);
    cover.set_release(FIRST_MBID, "Mental Cruelty");
    let window = present(&cover);
    settle_until("the first portrait request to start", || {
        started.load(Ordering::SeqCst)
    });

    cover.set_release(SECOND_MBID, "Mental Cruelty");
    release.store(true, Ordering::SeqCst);
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(150));

    assert_eq!(cover.picture_source_path(), Some(new_cover));
    window.close();
}
