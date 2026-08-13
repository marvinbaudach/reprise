//! The most-played band's hero card — the double-width leader of the bands
//! row. Its runners-up are separate tiles (`stats_band_tile.rs`), composed
//! beside it by `stats_bands_row.rs`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::library::stats_screen::RankedGroup;
use reprise_core::library::stats_snapshot::SortBy;

use super::stats_artist_image::{ArtistImageRequest, StatsArtistImage};
use crate::ui::strings;

type StringCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;

#[derive(Clone)]
pub(in crate::ui) struct StatsBandCard {
    root: gtk4::Overlay,
    #[cfg_attr(not(test), allow(dead_code))]
    card_click: gtk4::GestureClick,
    picture: gtk4::Picture,
    fallback: gtk4::Label,
    pub(super) name_button: gtk4::Button,
    summary: gtk4::Label,
    unify_hint: gtk4::Button,
    current_artist: Rc<RefCell<String>>,
    current_key: Rc<RefCell<String>>,
    artist_image: Rc<RefCell<Option<Rc<StatsArtistImage>>>>,
    current_candidates: Rc<RefCell<Vec<String>>>,
    cover_generation: Rc<Cell<u64>>,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) image_loaded: Rc<Cell<Option<bool>>>,
    on_open_artist: StringCallback,
    on_unify: StringCallback,
}

impl StatsBandCard {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Overlay::new();
        root.add_css_class("stats-band-card");
        // The row's grid hands out the width; only the height is the card's
        // own business, and it matches the runner-up tiles beside it.
        root.set_size_request(-1, 250);
        root.set_hexpand(true);
        root.set_valign(gtk4::Align::Start);
        root.set_overflow(gtk4::Overflow::Hidden);
        // The card is one activation target (BTN-1): pointer cursor, like the
        // song rows below it. GTK4 CSS has no `cursor`, so it is set here.
        crate::ui::style::buttons::arm_cursor(&root);

        let picture = gtk4::Picture::new();
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_can_shrink(true);
        root.set_child(Some(&picture));

        let fallback = gtk4::Label::new(Some("?"));
        fallback.add_css_class("stats-band-initials");
        fallback.set_hexpand(true);
        fallback.set_vexpand(true);
        root.add_overlay(&fallback);

        let fade = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        fade.add_css_class("stats-band-fade");
        fade.set_can_target(false);
        fade.set_hexpand(true);
        fade.set_vexpand(true);
        root.add_overlay(&fade);

        // STATS-21: the whole card activates, so the whole card lights up —
        // and it has to do that *over* the artwork, which covers the card's
        // own background. Added before the content so the wash never dims the
        // text, and untargetable so it never eats the click it advertises.
        let hover = hover_wash();
        root.add_overlay(&hover);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        content.add_css_class("stats-band-content");
        content.set_valign(gtk4::Align::End);
        content.set_hexpand(true);
        content.set_vexpand(true);
        let kicker = gtk4::Label::new(Some("MOST PLAYED BAND"));
        kicker.add_css_class("stats-eyebrow");
        kicker.set_xalign(0.0);
        content.append(&kicker);

        let name_button = gtk4::Button::new();
        name_button.add_css_class("flat");
        name_button.add_css_class("stats-band-name");
        name_button.set_halign(gtk4::Align::Start);
        let name = gtk4::Label::new(None);
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        name_button.set_child(Some(&name));
        content.append(&name_button);

        let summary = gtk4::Label::new(None);
        summary.add_css_class("stats-item-subtitle");
        summary.set_xalign(0.0);
        summary.set_wrap(true);
        content.append(&summary);

        let unify_hint = gtk4::Button::with_label("Tag spellings");
        unify_hint.add_css_class("flat");
        unify_hint.add_css_class("stats-unify-hint");
        unify_hint.set_halign(gtk4::Align::Start);
        unify_hint.set_visible(false);
        content.append(&unify_hint);
        root.add_overlay(&content);

        let current_artist = Rc::new(RefCell::new(String::new()));
        let current_key = Rc::new(RefCell::new(String::new()));
        let on_open_artist: StringCallback = Rc::new(RefCell::new(None));
        let on_unify: StringCallback = Rc::new(RefCell::new(None));
        name_button.connect_clicked({
            let current_artist = current_artist.clone();
            let callback = on_open_artist.clone();
            move |_| {
                let artist = current_artist.borrow().clone();
                invoke(&callback, artist);
            }
        });
        // input-parity: ACC-8 keyboard=artist-name-button
        let card_click = gtk4::GestureClick::new();
        card_click.set_button(gtk4::gdk::BUTTON_PRIMARY);
        card_click.connect_released({
            let current_artist = current_artist.clone();
            let callback = on_open_artist.clone();
            move |_, _, _, _| {
                let artist = current_artist.borrow().clone();
                invoke(&callback, artist);
            }
        });
        root.add_controller(card_click.clone());
        unify_hint.connect_clicked({
            let current_key = current_key.clone();
            let callback = on_unify.clone();
            move |_| {
                let key = current_key.borrow().clone();
                invoke(&callback, key);
            }
        });

        Self {
            root,
            card_click,
            picture,
            fallback,
            name_button,
            summary,
            unify_hint,
            current_artist,
            current_key,
            artist_image: Rc::new(RefCell::new(None)),
            current_candidates: Rc::new(RefCell::new(Vec::new())),
            cover_generation: Rc::new(Cell::new(0)),
            image_loaded: Rc::new(Cell::new(None)),
            on_open_artist,
            on_unify,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Overlay {
        &self.root
    }

    #[cfg(test)]
    pub(super) fn artwork_generation_for_test(&self) -> u64 {
        self.cover_generation.get()
    }

    pub(in crate::ui) fn set_data(
        &self,
        ranked: &RankedGroup,
        share_percent: i64,
        _sort_by: SortBy,
    ) {
        let leader = &ranked.group;
        *self.current_artist.borrow_mut() = leader.label.clone();
        *self.current_key.borrow_mut() = leader.key.clone();
        *self.current_candidates.borrow_mut() = ranked.cover_candidates.clone();
        self.name_button
            .child()
            .and_downcast::<gtk4::Label>()
            .expect("band name button owns a label")
            .set_label(&leader.label);
        self.fallback.set_label(&initials(&leader.label));
        self.fallback.set_visible(true);
        self.picture.set_visible(false);
        self.summary.set_label(&strings::stats_artist_summary(
            leader.plays,
            leader.ms,
            share_percent,
        ));
        self.set_unify_hint(leader.variant_count);
        self.load_image(&leader.label, &ranked.cover_candidates);
    }

    /// Routes this card's activations into the row's shared callbacks, so the
    /// leader and the runner-up tiles reach the same navigation.
    pub(super) fn forward_callbacks(
        &self,
        on_open_artist: &StringCallback,
        on_unify: &StringCallback,
    ) {
        *self.on_open_artist.borrow_mut() = Some({
            let outer = on_open_artist.clone();
            Rc::new(move |artist| invoke(&outer, artist))
        });
        *self.on_unify.borrow_mut() = Some({
            let outer = on_unify.clone();
            Rc::new(move |key| invoke(&outer, key))
        });
    }

    fn set_unify_hint(&self, variants: usize) {
        self.unify_hint.set_visible(variants >= 2);
        self.unify_hint.set_tooltip_text(
            (variants >= 2)
                .then(|| strings::spellings_merged_hint(variants))
                .as_deref(),
        );
    }

    fn load_image(&self, artist: &str, candidates: &[String]) {
        let token = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(token);
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);
        self.picture.set_visible(false);
        self.fallback.set_visible(true);
        self.image_loaded.set(None);
        let image = self.artist_image.borrow().clone();
        let Some(image) = image else {
            return;
        };
        let picture = self.picture.clone();
        let fallback = self.fallback.clone();
        let generation = self.cover_generation.clone();
        let image_loaded = self.image_loaded.clone();
        image.load(
            &self.picture,
            ArtistImageRequest {
                artist: artist.to_string(),
                candidates: candidates.to_vec(),
                size: ThumbnailSize::Portrait,
                token,
                generation: generation.clone(),
                on_loaded: Rc::new(move |loaded| {
                    if generation.get() != token {
                        return;
                    }
                    picture.set_visible(loaded);
                    fallback.set_visible(!loaded);
                    image_loaded.set(Some(loaded));
                }),
            },
        );
    }

    pub(in crate::ui) fn set_artist_image(&self, image: Rc<StatsArtistImage>) {
        *self.artist_image.borrow_mut() = Some(image);
    }

    pub(in crate::ui) fn clear_data(&self) {
        self.cover_generation
            .set(self.cover_generation.get().wrapping_add(1));
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);
        self.picture.set_visible(false);
        self.image_loaded.set(None);
        self.fallback.set_label("");
        self.name_button.set_label("");
        self.summary.set_label("");
        self.set_unify_hint(0);
        self.current_artist.borrow_mut().clear();
        self.current_key.borrow_mut().clear();
        self.current_candidates.borrow_mut().clear();
    }

    #[cfg(test)]
    pub(super) fn emit_unify(&self, key: &str) {
        invoke(&self.on_unify, key.to_string());
    }

    #[cfg(test)]
    pub(super) fn artist_label(&self) -> String {
        self.current_artist.borrow().clone()
    }

    #[cfg(test)]
    pub(super) fn summary_text(&self) -> String {
        self.summary.text().to_string()
    }
}

fn invoke(callback: &StringCallback, value: String) {
    let callback = callback.borrow().clone();
    if !value.is_empty() {
        if let Some(callback) = callback {
            callback(value);
        }
    }
}

/// The hover surface both band surfaces wear (STATS-21). One builder, so the
/// leader and its runner-up tiles can never drift into two different hovers.
pub(super) fn hover_wash() -> gtk4::Box {
    let wash = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    wash.add_css_class("stats-band-hover");
    wash.set_can_target(false);
    wash.set_hexpand(true);
    wash.set_vexpand(true);
    wash
}

pub(super) fn initials(label: &str) -> String {
    label
        .split_whitespace()
        .filter_map(|part| part.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect::<String>()
        .chars()
        .take(2)
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::hash::{Hash, Hasher};
    use std::sync::atomic::{AtomicUsize, Ordering};

    use reprise_core::artist_portrait::PortraitOutcome;
    use reprise_core::library::group_key::Group;
    use reprise_core::library::stats_screen::RankedGroup;

    use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
    use crate::ui::cover_loader::CoverLoader;

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xA8,
        0xAF, 0xAF, 0x07, 0x00, 0x02, 0xFE, 0x01, 0x7E, 0xBA, 0x25, 0x70, 0x25, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn fixture(variant_count: usize) -> RankedGroup {
        RankedGroup {
            group: Group {
                label: "Lorna Shore".into(),
                key: "lorna shore".into(),
                plays: 10,
                ms: 600_000,
                variant_count,
            },
            representative_track_path: "/missing/cover.flac".into(),
            cover_candidates: vec!["/missing/cover.flac".into()],
        }
    }

    fn portrait_runtime(
        enabled: bool,
        cache_dir: &std::path::Path,
    ) -> (Rc<ArtistPortraitRuntime>, std::sync::Arc<AtomicUsize>) {
        let requests = std::sync::Arc::new(AtomicUsize::new(0));
        let runtime = ArtistPortraitRuntime::for_test(enabled, {
            let cache_dir = cache_dir.to_path_buf();
            let requests = requests.clone();
            move |artist| {
                requests.fetch_add(1, Ordering::SeqCst);
                match reprise_core::artist_portrait::load_cached_from(artist, &cache_dir) {
                    PortraitOutcome::Found(path) => Some(path),
                    PortraitOutcome::NotFound => None,
                }
            }
        });
        (runtime, requests)
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

    fn artist_image(
        loader: Rc<CoverLoader>,
        runtime: Rc<ArtistPortraitRuntime>,
        cache_dir: &std::path::Path,
    ) -> Rc<StatsArtistImage> {
        let cache_dir = cache_dir.to_path_buf();
        let image = StatsArtistImage::for_test(loader, move |artist| {
            match reprise_core::artist_portrait::load_cached_from(artist, &cache_dir) {
                PortraitOutcome::Found(path) => Some(path),
                PortraitOutcome::NotFound => None,
            }
        });
        image.set_portrait_runtime(runtime);
        image
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_23_artist_portrait_is_shown_before_the_album_cover() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cache = tempfile::tempdir().unwrap();
        cache_portrait(cache.path(), "Lorna Shore");
        let (runtime, requests) = portrait_runtime(true, cache.path());
        let card = StatsBandCard::new();
        let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
        card.set_artist_image(artist_image(loader, runtime, cache.path()));

        card.set_data(&fixture(1), 60, SortBy::Time);
        assert!(
            crate::ui::test_settle::settle_until(
                crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
                || card.image_loaded.get() == Some(true),
            ),
            "timed out waiting for stats artwork"
        );

        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert!(card.picture.is_visible());
        assert!(!card.fallback.is_visible());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_23_missing_portrait_falls_back_to_the_album_cover() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let (runtime, requests) = portrait_runtime(true, cache.path());
        let album = tempfile::tempdir().unwrap();
        let track = album.path().join("untagged.mp3");
        std::fs::write(&track, b"not really an mp3").unwrap();
        std::fs::write(album.path().join("cover.png"), TINY_PNG).unwrap();
        let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
        let card = StatsBandCard::new();
        card.set_artist_image(artist_image(loader, runtime, cache.path()));
        let mut data = fixture(1);
        data.representative_track_path = track.to_string_lossy().into_owned();
        data.cover_candidates = vec![track.to_string_lossy().into_owned()];

        card.set_data(&data, 60, SortBy::Time);
        assert!(
            crate::ui::test_settle::settle_until(
                crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
                || card.image_loaded.get() == Some(true),
            ),
            "timed out waiting for stats artwork"
        );

        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(card.picture.is_visible());
        assert!(!card.fallback.is_visible());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_23_cover_walk_uses_the_next_album_with_artwork() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let (runtime, requests) = portrait_runtime(false, cache.path());
        let coverless_album = tempfile::tempdir().unwrap();
        let coverless_track = coverless_album.path().join("first.mp3");
        std::fs::write(&coverless_track, b"not really an mp3").unwrap();
        let illustrated_album = tempfile::tempdir().unwrap();
        let illustrated_track = illustrated_album.path().join("second.mp3");
        std::fs::write(&illustrated_track, b"not really an mp3").unwrap();
        std::fs::write(illustrated_album.path().join("cover.png"), TINY_PNG).unwrap();
        let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
        let card = StatsBandCard::new();
        card.set_artist_image(artist_image(loader, runtime, cache.path()));
        let mut data = fixture(1);
        data.cover_candidates = vec![
            coverless_track.to_string_lossy().into_owned(),
            illustrated_track.to_string_lossy().into_owned(),
        ];

        card.set_data(&data, 60, SortBy::Time);
        assert!(
            crate::ui::test_settle::settle_until(
                crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
                || card.image_loaded.get() == Some(true),
            ),
            "timed out waiting for the second album candidate"
        );

        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert!(card.picture.is_visible());
        assert!(!card.fallback.is_visible());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_23_missing_portrait_and_cover_fall_back_to_initials() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let (runtime, requests) = portrait_runtime(true, cache.path());
        let card = StatsBandCard::new();
        let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
        card.set_artist_image(artist_image(loader, runtime, cache.path()));

        card.set_data(&fixture(1), 60, SortBy::Time);
        assert!(
            crate::ui::test_settle::settle_until(
                crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
                || card.image_loaded.get() == Some(false),
            ),
            "timed out waiting for stats artwork"
        );

        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(!card.picture.is_visible());
        assert!(card.fallback.is_visible());
        assert_eq!(card.fallback.label(), "LS");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_23_disabled_artwork_module_skips_cached_portrait_and_uses_album_cover() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cache = tempfile::tempdir().unwrap();
        cache_portrait(cache.path(), "Lorna Shore");
        let (runtime, requests) = portrait_runtime(false, cache.path());
        let cache_reads = std::sync::Arc::new(AtomicUsize::new(0));
        let album = tempfile::tempdir().unwrap();
        let track = album.path().join("untagged.mp3");
        std::fs::write(&track, b"not really an mp3").unwrap();
        std::fs::write(album.path().join("cover.png"), TINY_PNG).unwrap();
        let card = StatsBandCard::new();
        let loader = CoverLoader::new(crate::ui::cover_download_worker::setup_for_test());
        let image = StatsArtistImage::for_test(loader, {
            let cache_dir = cache.path().to_path_buf();
            let cache_reads = cache_reads.clone();
            move |artist| {
                cache_reads.fetch_add(1, Ordering::SeqCst);
                match reprise_core::artist_portrait::load_cached_from(artist, &cache_dir) {
                    PortraitOutcome::Found(path) => Some(path),
                    PortraitOutcome::NotFound => None,
                }
            }
        });
        image.set_portrait_runtime(runtime);
        card.set_artist_image(image);
        let mut data = fixture(1);
        data.representative_track_path = track.to_string_lossy().into_owned();
        data.cover_candidates = vec![track.to_string_lossy().into_owned()];

        card.set_data(&data, 60, SortBy::Time);
        assert!(
            crate::ui::test_settle::settle_until(
                crate::ui::test_settle::DISPLAY_TEST_TIMEOUT,
                || card.image_loaded.get() == Some(true),
            ),
            "timed out waiting for album artwork"
        );

        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert_eq!(cache_reads.load(Ordering::SeqCst), 0);
        assert!(card.picture.is_visible());
        assert!(!card.fallback.is_visible());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_23_missing_cover_falls_back_to_initials() {
        gtk4::init().unwrap();
        let card = StatsBandCard::new();
        card.set_data(&fixture(1), 60, SortBy::Time);

        assert!(card.fallback.is_visible());
        assert_eq!(card.fallback.label(), "LS");
        assert!(!card.picture.is_visible());
    }

    /// STATS-23: the card asks for the artist by name and hands over every
    /// album candidate, so a coverless favourite cannot blank it.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_23_the_card_requests_the_artist_and_all_candidates() {
        gtk4::init().unwrap();
        let card = StatsBandCard::new();
        let mut ranked = fixture(1);
        ranked.cover_candidates = vec![
            "/music/first.flac".to_string(),
            "/music/second.flac".to_string(),
        ];

        card.set_data(&ranked, 11, SortBy::Time);

        assert_eq!(&*card.current_artist.borrow(), "Lorna Shore");
        assert_eq!(
            *card.current_candidates.borrow(),
            vec![
                "/music/first.flac".to_string(),
                "/music/second.flac".to_string()
            ]
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn unify_hint_survives_on_the_band_card() {
        gtk4::init().unwrap();
        let card = StatsBandCard::new();
        card.set_data(&fixture(3), 60, SortBy::Time);

        assert!(card.unify_hint.is_visible());
        assert!(card.unify_hint.tooltip_text().is_some());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_23_band_card_click_opens_the_artist() {
        gtk4::init().unwrap();
        let card = StatsBandCard::new();
        let opened = Rc::new(RefCell::new(None));
        let outer: StringCallback = Rc::new(RefCell::new(Some(Rc::new({
            let opened = opened.clone();
            move |artist: String| *opened.borrow_mut() = Some(artist)
        }))));
        card.forward_callbacks(&outer, &Rc::new(RefCell::new(None)));
        card.set_data(&fixture(1), 60, SortBy::Time);

        assert_eq!(card.card_click.button(), gtk4::gdk::BUTTON_PRIMARY);
        card.card_click
            .emit_by_name::<()>("released", &[&1_i32, &0.0_f64, &0.0_f64]);

        assert_eq!(opened.borrow().as_deref(), Some("Lorna Shore"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn band_navigation_callback_may_refresh_current_artist() {
        gtk4::init().unwrap();
        let card = StatsBandCard::new();
        *card.current_artist.borrow_mut() = "Current artist".into();
        let outer: StringCallback = Rc::new(RefCell::new(Some(Rc::new({
            let current_artist = card.current_artist.clone();
            move |_: String| *current_artist.borrow_mut() = "Refreshed artist".into()
        }))));
        card.forward_callbacks(&outer, &Rc::new(RefCell::new(None)));

        card.name_button.emit_clicked();

        assert_eq!(&*card.current_artist.borrow(), "Refreshed artist");
    }
}
