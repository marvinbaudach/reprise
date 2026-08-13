use super::*;

use std::cell::RefCell;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use reprise_core::artist_portrait::PortraitOutcome;
use reprise_core::library::group_key::Group;
use reprise_core::library::stats_screen::{RankedGroup, TopTrack};

use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;

const TINY_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xA8, 0xAF, 0xAF, 0x07,
    0x00, 0x02, 0xFE, 0x01, 0x7E, 0xBA, 0x25, 0x70, 0x25, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E,
    0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn ranked(label: &str, ms: i64, variant_count: usize) -> RankedGroup {
    let path = format!("/music/{label}.flac");
    RankedGroup {
        group: Group {
            label: label.into(),
            key: label.to_lowercase(),
            plays: ms / 60_000,
            ms,
            variant_count,
        },
        representative_track_path: path.clone(),
        cover_candidates: vec![path],
    }
}

fn fixture(runners_up: usize) -> SpotlightSection {
    SpotlightSection {
        artist: ranked("Lorna Shore", 600_000, 1),
        share_percent: 60,
        top_tracks: Vec::<TopTrack>::new(),
        also: [
            ranked("Alpha", 300_000, 1),
            ranked("Beta", 150_000, 1),
            ranked("Gamma", 60_000, 2),
            ranked("Delta", 30_000, 1),
        ][..runners_up]
            .to_vec(),
    }
}

fn cache_portrait(cache_dir: &std::path::Path, artist: &str) {
    std::fs::create_dir_all(cache_dir).unwrap();
    let normalized = artist
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    normalized.as_bytes().hash(&mut hasher);
    std::fs::write(
        cache_dir.join(format!("{:016x}.png", hasher.finish())),
        TINY_PNG,
    )
    .unwrap();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn the_leader_and_all_four_tiles_load_artist_portraits() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let cache = tempfile::tempdir().unwrap();
    for artist in ["Lorna Shore", "Alpha", "Beta", "Gamma", "Delta"] {
        cache_portrait(cache.path(), artist);
    }
    let requests = Arc::new(AtomicUsize::new(0));
    let runtime = ArtistPortraitRuntime::for_test(true, {
        let cache_dir = cache.path().to_path_buf();
        let requests = requests.clone();
        move |artist| {
            requests.fetch_add(1, Ordering::SeqCst);
            match reprise_core::artist_portrait::load_cached_from(artist, &cache_dir) {
                PortraitOutcome::Found(path) => Some(path),
                PortraitOutcome::NotFound => None,
            }
        }
    });
    let row = StatsBandsRow::new();
    let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
    let image = StatsArtistImage::for_test(loader, |_| None);
    image.set_portrait_runtime(runtime);
    row.set_artist_image(&image);

    row.set_data(&fixture(4));
    assert!(
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            row.leader().image_loaded.get() == Some(true)
                && row
                    .tiles()
                    .iter()
                    .all(|tile| tile.image_loaded.get() == Some(true))
        },),
        "timed out waiting for stats artwork"
    );

    assert_eq!(requests.load(Ordering::SeqCst), 5);
}

#[test]
fn stats_19_the_leader_spans_two_of_six_columns() {
    // 2 : 1 : 1 : 1 : 1 expressed as a homogeneous six-column grid.
    assert_eq!(LEADER_SPAN, 2);
    assert_eq!(RUNNER_UP_COUNT, 4);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_19_runner_up_bars_are_relative_to_the_leader() {
    gtk4::init().unwrap();
    let row = StatsBandsRow::new();
    row.set_data(&fixture(4));

    let values = row
        .bars()
        .iter()
        .map(gtk4::LevelBar::value)
        .collect::<Vec<_>>();
    assert_eq!(values, vec![0.5, 0.25, 0.1, 0.05]);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_19_a_short_ranking_leaves_the_tail_empty() {
    gtk4::init().unwrap();
    let row = StatsBandsRow::new();
    row.set_data(&fixture(2));

    let visible = row
        .tiles()
        .iter()
        .map(|tile| tile.widget().is_visible())
        .collect::<Vec<_>>();
    assert_eq!(
        visible,
        vec![true, true, false, false],
        "unfilled slots hide rather than letting the filled ones widen"
    );
}

/// STATS-21: every band surface answers the pointer, and it answers the same
/// way — a wash above its artwork plus the pointer cursor. Artwork covers the
/// card ground, so a background hover alone would be invisible here.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_21_every_band_surface_carries_the_hover_wash_and_pointer() {
    gtk4::init().unwrap();
    let row = StatsBandsRow::new();
    row.set_data(&fixture(4));

    let mut surfaces: Vec<gtk4::Widget> = vec![row.leader().widget().clone().upcast()];
    surfaces.extend(
        row.tiles()
            .iter()
            .map(|tile| tile.widget().clone().upcast::<gtk4::Widget>()),
    );
    for surface in &surfaces {
        let washes = descendants(surface)
            .into_iter()
            .filter(|widget| widget.has_css_class("stats-band-hover"))
            .collect::<Vec<_>>();
        assert_eq!(washes.len(), 1, "each band surface owns exactly one wash");
        assert!(
            !washes[0].can_target(),
            "the wash must not swallow the click it advertises"
        );
        assert_eq!(
            surface.cursor().and_then(|cursor| cursor.name()).as_deref(),
            Some("pointer"),
            "a clickable band surface has to say so under the cursor"
        );
    }
}

fn descendants(widget: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut found = Vec::new();
    let mut child = widget.first_child();
    while let Some(current) = child {
        found.push(current.clone());
        found.extend(descendants(&current));
        child = current.next_sibling();
    }
    found
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn stats_19_leader_and_tiles_share_one_navigation_callback() {
    gtk4::init().unwrap();
    let row = StatsBandsRow::new();
    let opened = Rc::new(RefCell::new(Vec::new()));
    row.set_on_open_artist({
        let opened = opened.clone();
        move |artist| opened.borrow_mut().push(artist)
    });
    row.set_data(&fixture(4));

    row.leader().name_button.emit_clicked();
    row.tiles()[0].widget().emit_clicked();

    assert_eq!(
        opened.borrow().as_slice(),
        ["Lorna Shore".to_string(), "Alpha".to_string()]
    );
}
