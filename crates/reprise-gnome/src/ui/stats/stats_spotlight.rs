//! Artist spotlight for the editorial My Stats page.

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
pub(in crate::ui) struct StatsSpotlight {
    root: gtk4::Box,
    cover: gtk4::Image,
    #[cfg_attr(not(test), allow(dead_code))]
    rank_badge: gtk4::Label,
    name: gtk4::Label,
    summary: gtk4::Label,
    chips: gtk4::Box,
    also: gtk4::Box,
    unify_hint: gtk4::Button,
    current_artist: Rc<RefCell<String>>,
    current_key: Rc<RefCell<String>>,
    cover_loader: Rc<RefCell<Option<Rc<CoverLoader>>>>,
    cover_generation: Rc<Cell<u64>>,
    on_play: StringCallback,
    on_go_to_artist: StringCallback,
    on_unify: StringCallback,
}

impl StatsSpotlight {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        root.add_css_class("stats-spotlight");

        let eyebrow = gtk4::Label::new(Some("YOUR #1 ARTIST"));
        eyebrow.add_css_class("stats-eyebrow");
        eyebrow.set_xalign(0.0);
        root.append(&eyebrow);

        let body = gtk4::Box::new(gtk4::Orientation::Horizontal, 20);
        let cover = gtk4::Image::builder()
            .icon_name("audio-x-generic-symbolic")
            .pixel_size(150)
            .width_request(150)
            .height_request(150)
            .build();
        cover.add_css_class("stats-spotlight-cover");
        CoverLoader::set_placeholder(&cover);
        body.append(&cover);

        let text = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        text.set_hexpand(true);
        let rank_badge = gtk4::Label::new(Some("#1"));
        rank_badge.add_css_class("stats-rank-badge");
        rank_badge.set_xalign(0.0);
        text.append(&rank_badge);
        let name = gtk4::Label::new(None);
        name.add_css_class("stats-spotlight-name");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        text.append(&name);
        let summary = gtk4::Label::new(None);
        summary.add_css_class("stats-item-subtitle");
        summary.set_xalign(0.0);
        text.append(&summary);
        let chips = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        chips.add_css_class("stats-track-chips");
        text.append(&chips);

        let current_artist = Rc::new(RefCell::new(String::new()));
        let current_key = Rc::new(RefCell::new(String::new()));
        let on_play: StringCallback = Rc::new(RefCell::new(None));
        let on_go_to_artist: StringCallback = Rc::new(RefCell::new(None));
        let on_unify: StringCallback = Rc::new(RefCell::new(None));
        let actions = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        let play = gtk4::Button::with_label("Play");
        play.add_css_class("suggested-action");
        play.connect_clicked({
            let current_key = current_key.clone();
            let on_play = on_play.clone();
            move |_| invoke(&on_play, current_key.borrow().clone())
        });
        actions.append(&play);
        let go_to_artist = gtk4::Button::with_label("Go to artist");
        go_to_artist.connect_clicked({
            let current_artist = current_artist.clone();
            let on_go_to_artist = on_go_to_artist.clone();
            move |_| invoke(&on_go_to_artist, current_artist.borrow().clone())
        });
        actions.append(&go_to_artist);
        text.append(&actions);

        let unify_hint = gtk4::Button::with_label("Tag spellings");
        unify_hint.add_css_class("flat");
        unify_hint.add_css_class("stats-unify-hint");
        unify_hint.set_halign(gtk4::Align::Start);
        unify_hint.set_visible(false);
        unify_hint.connect_clicked({
            let current_key = current_key.clone();
            let on_unify = on_unify.clone();
            move |_| invoke(&on_unify, current_key.borrow().clone())
        });
        text.append(&unify_hint);
        body.append(&text);
        root.append(&body);

        let also = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        also.add_css_class("stats-also");
        root.append(&also);

        Self {
            root,
            cover,
            rank_badge,
            name,
            summary,
            chips,
            also,
            unify_hint,
            current_artist,
            current_key,
            cover_loader: Rc::new(RefCell::new(None)),
            cover_generation: Rc::new(Cell::new(0)),
            on_play,
            on_go_to_artist,
            on_unify,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_data(&self, section: &SpotlightSection) {
        let artist = &section.artist.group;
        *self.current_artist.borrow_mut() = artist.label.clone();
        *self.current_key.borrow_mut() = artist.key.clone();
        self.name.set_label(&artist.label);
        let token = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(token);
        CoverLoader::set_placeholder(&self.cover);
        if let Some(loader) = self.cover_loader.borrow().clone() {
            loader.load_into(
                &self.cover,
                &section.artist.representative_track_path,
                ThumbnailSize::Portrait,
                token,
                &self.cover_generation,
            );
        }
        // The share divides by the time that carries an artist at all, not by
        // every play, so it must not claim to be a share of "your listening".
        self.summary.set_label(&format!(
            "{} plays \u{00b7} {} \u{00b7} {}% of your artist listening",
            format_thousands(artist.plays),
            format_duration(artist.ms),
            section.share_percent
        ));
        clear(&self.chips);
        for track in &section.top_tracks {
            let chip = gtk4::Label::new(Some(&track.title));
            chip.add_css_class("stats-track-chip");
            chip.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            self.chips.append(&chip);
        }
        clear(&self.also);
        for (index, group) in section.also.iter().enumerate() {
            let label = gtk4::Label::new(Some(&format!("#{} {}", index + 2, group.group.label)));
            label.add_css_class("stats-ghost-rank");
            self.also.append(&label);
        }
        let variants = artist.variant_count;
        self.unify_hint.set_visible(variants >= 2);
        self.unify_hint.set_tooltip_text(
            (variants >= 2)
                .then(|| strings::spellings_merged_hint(variants))
                .as_deref(),
        );
    }

    pub(in crate::ui) fn set_on_play(&self, callback: impl Fn(String) + 'static) {
        *self.on_play.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_cover_loader(&self, loader: Rc<CoverLoader>) {
        *self.cover_loader.borrow_mut() = Some(loader);
    }

    pub(in crate::ui) fn set_on_go_to_artist(&self, callback: impl Fn(String) + 'static) {
        *self.on_go_to_artist.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_unify(&self, callback: impl Fn(String) + 'static) {
        *self.on_unify.borrow_mut() = Some(Rc::new(callback));
    }
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

fn format_duration(milliseconds: i64) -> String {
    let minutes = milliseconds.max(0) / 60_000;
    format!("{} h", minutes / 60)
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;
    use reprise_core::library::group_key::Group;
    use reprise_core::library::stats_screen::{RankedGroup, TopTrack};
    use reprise_core::library::stats_snapshot::SpotlightSection;

    use super::*;

    fn fixture(variant_count: usize) -> SpotlightSection {
        SpotlightSection {
            artist: RankedGroup {
                group: Group {
                    label: "Lorna Shore".to_string(),
                    key: "lorna shore".to_string(),
                    plays: 9,
                    ms: 540_000,
                    variant_count,
                },
                representative_track_path: "/music/a.flac".to_string(),
            },
            share_percent: 60,
            top_tracks: (1..=3)
                .map(|id| TopTrack {
                    track_id: id,
                    title: format!("Track {id}"),
                    artist: "Lorna Shore".to_string(),
                    album: "Album".to_string(),
                    play_count: 3,
                    total_ms: 180_000,
                    track_path: format!("/music/{id}.flac"),
                })
                .collect(),
            also: Vec::new(),
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn spotlight_shows_rank_badge_name_and_three_chips() {
        gtk4::init().unwrap();
        let spotlight = StatsSpotlight::new();
        spotlight.set_data(&fixture(1));

        assert_eq!(spotlight.rank_badge.text(), "#1");
        assert_eq!(spotlight.name.text(), "Lorna Shore");
        assert_eq!(spotlight.chips.observe_children().n_items(), 3);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn unify_hint_appears_only_for_multi_variant_groups() {
        gtk4::init().unwrap();
        let spotlight = StatsSpotlight::new();
        spotlight.set_data(&fixture(1));
        assert!(!spotlight.unify_hint.is_visible());
        assert!(spotlight.unify_hint.tooltip_text().is_none());

        spotlight.set_data(&fixture(3));
        assert!(spotlight.unify_hint.is_visible());
        assert!(spotlight.unify_hint.tooltip_text().is_some());
    }
}
