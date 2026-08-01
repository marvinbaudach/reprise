//! The most-played band's hero card — the double-width leader of the bands
//! row. Its runners-up are separate tiles (`stats_band_tile.rs`), composed
//! beside it by `stats_bands_row.rs`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::format::format_thousands;
use reprise_core::library::stats_snapshot::SpotlightSection;

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
    cover_generation: Rc<Cell<u64>>,
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
            cover_generation: Rc::new(Cell::new(0)),
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
        self.load_cover(&section.artist.representative_track_path);
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

    fn load_cover(&self, path: &str) {
        let token = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(token);
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);
        let Some(loader) = self.cover_loader.borrow().clone() else {
            return;
        };
        let picture = self.picture.clone();
        let fallback = self.fallback.clone();
        let cover_generation = self.cover_generation.clone();
        loader.load_into_picture(
            &self.picture,
            path,
            ThumbnailSize::Portrait,
            token,
            &self.cover_generation,
            move |loaded| {
                if cover_generation.get() != token {
                    return;
                }
                picture.set_visible(loaded);
                fallback.set_visible(!loaded);
            },
        );
    }

    pub(in crate::ui) fn set_cover_loader(&self, loader: Rc<CoverLoader>) {
        *self.cover_loader.borrow_mut() = Some(loader);
    }

    pub(in crate::ui) fn clear_data(&self) {
        self.cover_generation
            .set(self.cover_generation.get().wrapping_add(1));
        self.picture.set_paintable(gtk4::gdk::Paintable::NONE);
        self.picture.set_visible(false);
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
    use reprise_core::library::group_key::Group;
    use reprise_core::library::stats_screen::{RankedGroup, TopTrack};

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
