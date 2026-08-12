//! The most-played band's hero card — the double-width leader of the bands
//! row. Its runners-up are separate tiles (`stats_band_tile.rs`), composed
//! beside it by `stats_bands_row.rs`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::format::format_thousands;
use reprise_core::library::stats_snapshot::SpotlightSection;

use super::stats_artwork::{StatsArtworkRequest, StatsArtworkSource};
use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;
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
    cover_loader: Rc<RefCell<Option<Rc<CoverLoader>>>>,
    artist_portrait: Rc<RefCell<Option<Rc<ArtistPortraitRuntime>>>>,
    cover_generation: Rc<Cell<u64>>,
    pub(super) artwork_source: Rc<Cell<StatsArtworkSource>>,
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
            cover_loader: Rc::new(RefCell::new(None)),
            artist_portrait: Rc::new(RefCell::new(None)),
            cover_generation: Rc::new(Cell::new(0)),
            artwork_source: Rc::new(Cell::new(StatsArtworkSource::Initials)),
            on_open_artist,
            on_unify,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Overlay {
        &self.root
    }

    pub(in crate::ui) fn set_data(&self, section: &SpotlightSection) {
        let leader = &section.artist.group;
        *self.current_artist.borrow_mut() = leader.label.clone();
        *self.current_key.borrow_mut() = leader.key.clone();
        self.name_button
            .child()
            .and_downcast::<gtk4::Label>()
            .expect("band name button owns a label")
            .set_label(&leader.label);
        self.fallback.set_label(&initials(&leader.label));
        self.fallback.set_visible(true);
        self.picture.set_visible(false);
        self.summary.set_label(&format!(
            "{} plays · {} · {}% of your artist listening",
            format_thousands(leader.plays),
            strings::stats_duration(leader.ms),
            section.share_percent
        ));
        self.set_unify_hint(leader.variant_count);
        self.load_artwork(&leader.label, &section.artist.representative_track_path);
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

    fn load_artwork(&self, artist: &str, path: &str) {
        let token = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(token);
        super::stats_artwork::load(StatsArtworkRequest {
            picture: &self.picture,
            fallback: &self.fallback,
            artist,
            track_path: path,
            token,
            current: &self.cover_generation,
            portrait: self.artist_portrait.borrow().clone(),
            cover: self.cover_loader.borrow().clone(),
            source: self.artwork_source.clone(),
        });
    }

    pub(in crate::ui) fn set_cover_loader(&self, loader: Rc<CoverLoader>) {
        *self.cover_loader.borrow_mut() = Some(loader);
    }

    pub(in crate::ui) fn set_artist_portrait_runtime(&self, runtime: Rc<ArtistPortraitRuntime>) {
        *self.artist_portrait.borrow_mut() = Some(runtime);
    }

    pub(in crate::ui) fn clear_data(&self) {
        self.cover_generation
            .set(self.cover_generation.get().wrapping_add(1));
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);
        self.picture.set_visible(false);
        self.artwork_source.set(StatsArtworkSource::Initials);
        self.fallback.set_label("");
        self.name_button.set_label("");
        self.summary.set_label("");
        self.set_unify_hint(0);
        self.current_artist.borrow_mut().clear();
        self.current_key.borrow_mut().clear();
    }

    #[cfg(test)]
    pub(super) fn emit_unify(&self, key: &str) {
        invoke(&self.on_unify, key.to_string());
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
    use reprise_core::library::stats_screen::{RankedGroup, TopTrack};

    use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;

    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90,
        0x77, 0x53, 0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xA8,
        0xAF, 0xAF, 0x07, 0x00, 0x02, 0xFE, 0x01, 0x7E, 0xBA, 0x25, 0x70, 0x25, 0x00, 0x00, 0x00,
        0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn fixture(variant_count: usize) -> SpotlightSection {
        let ranked = |label: &str, ms: i64, variant_count: usize| RankedGroup {
            group: Group {
                label: label.into(),
                key: label.to_lowercase(),
                plays: ms / 60_000,
                ms,
                variant_count,
            },
            representative_track_path: format!("/music/{label}.flac"),
        };
        SpotlightSection {
            artist: RankedGroup {
                group: Group {
                    label: "Lorna Shore".into(),
                    key: "lorna shore".into(),
                    plays: 10,
                    ms: 600_000,
                    variant_count,
                },
                representative_track_path: "/missing/cover.flac".into(),
            },
            share_percent: 60,
            top_tracks: Vec::<TopTrack>::new(),
            also: vec![
                ranked("Alpha", 300_000, 1),
                ranked("Beta", 150_000, 1),
                ranked("Gamma", 60_000, 2),
                ranked("Delta", 30_000, 1),
            ],
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

    fn pump_until(condition: impl Fn() -> bool) {
        let context = gtk4::glib::MainContext::default();
        for _ in 0..10_000 {
            if condition() {
                return;
            }
            while context.pending() {
                context.iteration(false);
            }
            std::thread::yield_now();
        }
        panic!("timed out waiting for stats artwork");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn artist_portrait_is_shown_before_the_album_cover() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cache = tempfile::tempdir().unwrap();
        cache_portrait(cache.path(), "Lorna Shore");
        let (runtime, requests) = portrait_runtime(true, cache.path());
        let card = StatsBandCard::new();
        card.set_artist_portrait_runtime(runtime);

        card.set_data(&fixture(1));
        pump_until(|| card.artwork_source.get() == StatsArtworkSource::Portrait);

        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(card.picture.is_visible());
        assert!(!card.fallback.is_visible());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn missing_portrait_falls_back_to_the_album_cover() {
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
        card.set_artist_portrait_runtime(runtime);
        card.set_cover_loader(loader);
        let mut data = fixture(1);
        data.artist.representative_track_path = track.to_string_lossy().into_owned();

        card.set_data(&data);
        pump_until(|| card.artwork_source.get() == StatsArtworkSource::Cover);

        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(card.picture.is_visible());
        assert!(!card.fallback.is_visible());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn missing_portrait_and_cover_fall_back_to_initials() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cache = tempfile::tempdir().unwrap();
        let (runtime, requests) = portrait_runtime(true, cache.path());
        let card = StatsBandCard::new();
        card.set_artist_portrait_runtime(runtime);

        card.set_data(&fixture(1));
        pump_until(|| card.artwork_source.get() == StatsArtworkSource::Initials);

        assert_eq!(requests.load(Ordering::SeqCst), 1);
        assert!(!card.picture.is_visible());
        assert!(card.fallback.is_visible());
        assert_eq!(card.fallback.label(), "LS");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn disabled_artwork_module_neither_shows_nor_requests_a_portrait() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let cache = tempfile::tempdir().unwrap();
        cache_portrait(cache.path(), "Lorna Shore");
        let (runtime, requests) = portrait_runtime(false, cache.path());
        let card = StatsBandCard::new();
        card.set_artist_portrait_runtime(runtime);

        card.set_data(&fixture(1));

        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert_eq!(card.artwork_source.get(), StatsArtworkSource::Initials);
        assert!(!card.picture.is_visible());
        assert!(card.fallback.is_visible());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_13_missing_cover_falls_back_to_initials() {
        gtk4::init().unwrap();
        let card = StatsBandCard::new();
        card.set_data(&fixture(1));

        assert!(card.fallback.is_visible());
        assert_eq!(card.fallback.label(), "LS");
        assert!(!card.picture.is_visible());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn unify_hint_survives_on_the_band_card() {
        gtk4::init().unwrap();
        let card = StatsBandCard::new();
        card.set_data(&fixture(3));

        assert!(card.unify_hint.is_visible());
        assert!(card.unify_hint.tooltip_text().is_some());
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_13_band_card_click_opens_the_artist() {
        gtk4::init().unwrap();
        let card = StatsBandCard::new();
        let opened = Rc::new(RefCell::new(None));
        let outer: StringCallback = Rc::new(RefCell::new(Some(Rc::new({
            let opened = opened.clone();
            move |artist: String| *opened.borrow_mut() = Some(artist)
        }))));
        card.forward_callbacks(&outer, &Rc::new(RefCell::new(None)));
        card.set_data(&fixture(1));

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
