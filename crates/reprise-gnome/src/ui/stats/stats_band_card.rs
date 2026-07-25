//! Most-played-band card with a local cover hero and ranked runners-up.

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
    name_button: gtk4::Button,
    summary: gtk4::Label,
    ranks: gtk4::Box,
    rank_bars: Rc<RefCell<Vec<gtk4::LevelBar>>>,
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
        root.set_size_request(380, 420);

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

        let ranks = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        content.append(&ranks);
        root.add_overlay(&content);

        let current_artist = Rc::new(RefCell::new(String::new()));
        let current_key = Rc::new(RefCell::new(String::new()));
        let on_open_artist: StringCallback = Rc::new(RefCell::new(None));
        let on_unify: StringCallback = Rc::new(RefCell::new(None));
        name_button.connect_clicked({
            let current_artist = current_artist.clone();
            let callback = on_open_artist.clone();
            move |_| invoke(&callback, current_artist.borrow().clone())
        });
        let card_click = gtk4::GestureClick::new();
        card_click.connect_released({
            let current_artist = current_artist.clone();
            let callback = on_open_artist.clone();
            move |_, _, _, _| invoke(&callback, current_artist.borrow().clone())
        });
        root.add_controller(card_click.clone());
        unify_hint.connect_clicked({
            let current_key = current_key.clone();
            let callback = on_unify.clone();
            move |_| invoke(&callback, current_key.borrow().clone())
        });

        Self {
            root,
            card_click,
            picture,
            fallback,
            name_button,
            summary,
            ranks,
            rank_bars: Rc::new(RefCell::new(Vec::new())),
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
            format_duration(leader.ms),
            section.share_percent
        ));
        self.render_ranks(section);
        self.set_unify_hint(leader.variant_count);
        self.load_cover(&section.artist.representative_track_path);
    }

    fn render_ranks(&self, section: &SpotlightSection) {
        clear(&self.ranks);
        self.rank_bars.borrow_mut().clear();
        let leader_ms = section.artist.group.ms.max(0);
        for (index, ranked) in section.also.iter().take(4).enumerate() {
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
            row.add_css_class("stats-band-rank");
            let artist = gtk4::Button::new();
            artist.add_css_class("flat");
            artist.set_hexpand(true);
            let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            body.append(&label(
                &format!("#{} {}", index + 2, ranked.group.label),
                "stats-item-title",
            ));
            let bar = gtk4::LevelBar::new();
            bar.add_css_class("stats-band-rank-bar");
            bar.set_min_value(0.0);
            bar.set_max_value(1.0);
            bar.set_value(relative_value(ranked.group.ms, leader_ms));
            bar.set_hexpand(true);
            body.append(&bar);
            artist.set_child(Some(&body));
            artist.connect_clicked({
                let label = ranked.group.label.clone();
                let callback = self.on_open_artist.clone();
                move |_| invoke(&callback, label.clone())
            });
            row.append(&artist);
            if ranked.group.variant_count >= 2 {
                let unify = gtk4::Button::from_icon_name("document-edit-symbolic");
                unify.add_css_class("flat");
                unify.set_tooltip_text(Some(&strings::spellings_merged_hint(
                    ranked.group.variant_count,
                )));
                unify.connect_clicked({
                    let key = ranked.group.key.clone();
                    let callback = self.on_unify.clone();
                    move |_| invoke(&callback, key.clone())
                });
                row.append(&unify);
            }
            self.rank_bars.borrow_mut().push(bar);
            self.ranks.append(&row);
        }
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
        loader.load_into_picture(
            &self.picture,
            path,
            ThumbnailSize::Portrait,
            token,
            &self.cover_generation,
            move |loaded| {
                picture.set_visible(loaded);
                fallback.set_visible(!loaded);
            },
        );
    }

    pub(in crate::ui) fn set_cover_loader(&self, loader: Rc<CoverLoader>) {
        *self.cover_loader.borrow_mut() = Some(loader);
    }

    pub(in crate::ui) fn set_on_open_artist(&self, callback: impl Fn(String) + 'static) {
        *self.on_open_artist.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_unify(&self, callback: impl Fn(String) + 'static) {
        *self.on_unify.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn bars(&self) -> Vec<gtk4::LevelBar> {
        self.rank_bars.borrow().clone()
    }
}

fn label(text: &str, class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class(class);
    label.set_xalign(0.0);
    label
}

fn clear(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
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

fn initials(label: &str) -> String {
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

fn relative_value(value: i64, leader: i64) -> f64 {
    if leader <= 0 {
        0.0
    } else {
        value.max(0) as f64 / leader as f64
    }
}

fn format_duration(milliseconds: i64) -> String {
    format!("{} h", milliseconds.max(0) / 3_600_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::library::group_key::Group;
    use reprise_core::library::stats_screen::{RankedGroup, TopTrack};

    fn fixture(variant_count: usize) -> SpotlightSection {
        let ranked = |label: &str, ms: i64| RankedGroup {
            group: Group {
                label: label.into(),
                key: label.to_lowercase(),
                plays: ms / 60_000,
                ms,
                variant_count: 1,
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
                ranked("Alpha", 300_000),
                ranked("Beta", 150_000),
                ranked("Gamma", 60_000),
                ranked("Delta", 30_000),
            ],
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_13_band_card_shows_ranks_relative_to_leader() {
        gtk4::init().unwrap();
        let card = StatsBandCard::new();
        card.set_data(&fixture(1));

        let values = card
            .rank_bars
            .borrow()
            .iter()
            .map(gtk4::LevelBar::value)
            .collect::<Vec<_>>();
        assert_eq!(values, vec![0.5, 0.25, 0.1, 0.05]);
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
        card.set_on_open_artist({
            let opened = opened.clone();
            move |artist| *opened.borrow_mut() = Some(artist)
        });
        card.set_data(&fixture(1));

        card.card_click
            .emit_by_name::<()>("released", &[&1_i32, &0.0_f64, &0.0_f64]);

        assert_eq!(opened.borrow().as_deref(), Some("Lorna Shore"));
    }
}
