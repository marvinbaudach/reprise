//! One runner-up band tile (#2–#5) of the most-played-bands row.
//!
//! The leader's hero card lives in `stats_band_card.rs`; this is its small
//! sibling: image on top fading into the card ground, then rank, name, figures
//! and a bar relative to the leader.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::format::format_thousands;
use reprise_core::library::stats_screen::RankedGroup;

use super::stats_artwork::{StatsArtworkRequest, StatsArtworkSource};
use super::stats_view_widgets::label;
use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

type StringCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;

/// Image height of a runner-up tile. Tall enough to read as artwork, short
/// enough that four of them stay subordinate to the leader beside them.
const IMAGE_HEIGHT: i32 = 130;

#[derive(Clone)]
pub(super) struct StatsBandTile {
    root: gtk4::Button,
    picture: gtk4::Picture,
    initials: gtk4::Label,
    rank: gtk4::Label,
    name: gtk4::Label,
    figures: gtk4::Label,
    bar: gtk4::LevelBar,
    unify: gtk4::Button,
    current_artist: Rc<RefCell<String>>,
    current_key: Rc<RefCell<String>>,
    cover_loader: Rc<RefCell<Option<Rc<CoverLoader>>>>,
    artist_portrait: Rc<RefCell<Option<Rc<ArtistPortraitRuntime>>>>,
    cover_generation: Rc<Cell<u64>>,
    pub(super) artwork_source: Rc<Cell<StatsArtworkSource>>,
}

impl StatsBandTile {
    pub(super) fn new(on_open_artist: &StringCallback, on_unify: &StringCallback) -> Self {
        // A button, not a box with a gesture: the tile is one activation
        // target, so it inherits focus, Enter/Space and the platform's own
        // pressed state instead of reimplementing them.
        let root = gtk4::Button::new();
        root.add_css_class("flat");
        root.add_css_class("stats-band-tile");
        root.set_hexpand(true);
        root.set_valign(gtk4::Align::Start);
        root.set_overflow(gtk4::Overflow::Hidden);
        crate::ui::style::buttons::arm_cursor(&root);

        // STATS-21: same hover as the leader card and the song rows. The tile
        // wears it as an overlay for the same reason the leader does — its
        // artwork covers the button ground a background hover would paint.
        let stack = gtk4::Overlay::new();
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

        let image_slot = gtk4::Overlay::new();
        image_slot.set_size_request(-1, IMAGE_HEIGHT);
        image_slot.set_overflow(gtk4::Overflow::Hidden);
        let picture = gtk4::Picture::new();
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_can_shrink(true);
        picture.set_size_request(-1, IMAGE_HEIGHT);
        image_slot.set_child(Some(&picture));
        let initials = gtk4::Label::new(None);
        initials.add_css_class("stats-band-initials");
        initials.add_css_class("stats-band-tile-initials");
        initials.set_hexpand(true);
        initials.set_vexpand(true);
        image_slot.add_overlay(&initials);
        let fade = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        fade.add_css_class("stats-band-tile-fade");
        fade.set_can_target(false);
        fade.set_hexpand(true);
        fade.set_vexpand(true);
        image_slot.add_overlay(&fade);
        content.append(&image_slot);

        let body = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        body.add_css_class("stats-band-tile-body");
        let rank = label("", "stats-band-tile-rank");
        rank.set_xalign(0.0);
        body.append(&rank);
        let name = label("", "stats-band-tile-name");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        body.append(&name);
        let figures = label("", "stats-item-subtitle");
        figures.set_xalign(0.0);
        figures.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        body.append(&figures);
        let bar = gtk4::LevelBar::new();
        bar.add_css_class("stats-band-rank-bar");
        bar.set_min_value(0.0);
        bar.set_max_value(1.0);
        bar.set_height_request(3);
        bar.set_valign(gtk4::Align::Center);
        body.append(&bar);
        content.append(&body);
        stack.set_child(Some(&content));
        stack.add_overlay(&super::stats_band_card::hover_wash());
        root.set_child(Some(&stack));

        let current_artist = Rc::new(RefCell::new(String::new()));
        let current_key = Rc::new(RefCell::new(String::new()));
        root.connect_clicked({
            let current_artist = current_artist.clone();
            let callback = on_open_artist.clone();
            move |_| invoke(&callback, current_artist.borrow().clone())
        });

        let unify = gtk4::Button::from_icon_name("document-edit-symbolic");
        unify.add_css_class("flat");
        unify.add_css_class("stats-band-tile-unify");
        unify.set_halign(gtk4::Align::End);
        unify.set_valign(gtk4::Align::Start);
        unify.set_visible(false);
        unify.connect_clicked({
            let current_key = current_key.clone();
            let callback = on_unify.clone();
            move |_| invoke(&callback, current_key.borrow().clone())
        });
        image_slot.add_overlay(&unify);

        Self {
            root,
            picture,
            initials,
            rank,
            name,
            figures,
            bar,
            unify,
            current_artist,
            current_key,
            cover_loader: Rc::new(RefCell::new(None)),
            artist_portrait: Rc::new(RefCell::new(None)),
            cover_generation: Rc::new(Cell::new(0)),
            artwork_source: Rc::new(Cell::new(StatsArtworkSource::Initials)),
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Button {
        &self.root
    }

    pub(super) fn bar(&self) -> gtk4::LevelBar {
        self.bar.clone()
    }

    pub(super) fn set_cover_loader(&self, loader: Rc<CoverLoader>) {
        *self.cover_loader.borrow_mut() = Some(loader);
    }

    pub(super) fn set_artist_portrait_runtime(&self, runtime: Rc<ArtistPortraitRuntime>) {
        *self.artist_portrait.borrow_mut() = Some(runtime);
    }

    pub(super) fn set_data(&self, rank: usize, ranked: &RankedGroup, leader_ms: i64) {
        let group = &ranked.group;
        *self.current_artist.borrow_mut() = group.label.clone();
        *self.current_key.borrow_mut() = group.key.clone();
        self.root.set_visible(true);
        self.root
            .update_property(&[gtk4::accessible::Property::Label(&group.label)]);
        self.rank.set_label(&format!("#{rank}"));
        self.name.set_label(&group.label);
        self.initials
            .set_label(&super::stats_band_card::initials(&group.label));
        self.figures.set_label(&format!(
            "{} plays · {}",
            format_thousands(group.plays),
            strings::stats_duration(group.ms)
        ));
        self.bar.set_value(relative_value(group.ms, leader_ms));
        self.unify.set_visible(group.variant_count >= 2);
        self.unify.set_tooltip_text(
            (group.variant_count >= 2)
                .then(|| strings::spellings_merged_hint(group.variant_count))
                .as_deref(),
        );
        self.load_artwork(&group.label, &ranked.representative_track_path);
    }

    /// Hides the tile. The row has a fixed five slots, so a library with
    /// fewer than five artists leaves the tail empty rather than reflowing
    /// the remaining tiles into wider ones.
    pub(super) fn clear_data(&self) {
        self.cover_generation
            .set(self.cover_generation.get().wrapping_add(1));
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);
        self.artwork_source.set(StatsArtworkSource::Initials);
        self.root.set_visible(false);
        self.current_artist.borrow_mut().clear();
        self.current_key.borrow_mut().clear();
    }

    fn load_artwork(&self, artist: &str, path: &str) {
        let token = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(token);
        super::stats_artwork::load(StatsArtworkRequest {
            picture: &self.picture,
            fallback: &self.initials,
            artist,
            track_path: path,
            token,
            current: &self.cover_generation,
            portrait: self.artist_portrait.borrow().clone(),
            cover: self.cover_loader.borrow().clone(),
            source: self.artwork_source.clone(),
        });
    }
}

fn invoke(callback: &StringCallback, value: String) {
    let callback = callback.borrow().clone();
    if let Some(callback) = callback {
        callback(value);
    }
}

fn relative_value(value: i64, leader: i64) -> f64 {
    if leader <= 0 {
        0.0
    } else {
        value.max(0) as f64 / leader as f64
    }
}
