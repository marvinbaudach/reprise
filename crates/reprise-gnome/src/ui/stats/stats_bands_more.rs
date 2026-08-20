//! Ranks six through twenty of the artist ranking (STATS-23).

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::stats_screen::RankedGroup;
use reprise_core::library::stats_snapshot::SortBy;

use super::stats_artist_image::{ArtistImageRequest, StatsArtistImage};
use crate::ui::strings;

const AVATAR_SIZE: i32 = 32;

pub(super) struct ContinuationRow {
    pub(super) root: gtk4::Box,
    avatar: adw::Avatar,
    picture: gtk4::Picture,
    image: Rc<StatsArtistImage>,
    candidates: Vec<String>,
    generation: Rc<Cell<u64>>,
    token: u64,
    image_started: Cell<bool>,
    #[cfg(test)]
    pub(super) open_button: gtk4::Button,
    #[cfg(test)]
    pub(super) unify_button: gtk4::Button,
    #[cfg(test)]
    pub(super) bar: gtk4::LevelBar,
    artist: String,
}

pub(super) struct ContinuationCallbacks {
    pub(super) open_artist: Rc<dyn Fn(String)>,
    pub(super) unify: Rc<dyn Fn(String)>,
}

pub(super) fn build_row(
    rank: usize,
    artist: &RankedGroup,
    leader_metric: i64,
    sort_by: SortBy,
    image: &Rc<StatsArtistImage>,
    generation: &Rc<Cell<u64>>,
    callbacks: ContinuationCallbacks,
) -> ContinuationRow {
    let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    root.add_css_class("stats-artist-row");

    let open_button = gtk4::Button::new();
    open_button.add_css_class("flat");
    open_button.set_hexpand(true);
    open_button.update_property(&[gtk4::accessible::Property::Label(&artist.group.label)]);
    crate::ui::style::buttons::arm(&open_button, crate::ui::style::buttons::TERTIARY_CLASS);

    let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    let rank_label = gtk4::Label::new(Some(&rank.to_string()));
    rank_label.add_css_class("stats-ghost-rank");
    rank_label.set_size_request(24, -1);
    rank_label.set_xalign(1.0);
    line.append(&rank_label);

    let avatar = adw::Avatar::new(AVATAR_SIZE, Some(&artist.group.label), true);
    line.append(&avatar);

    let name = gtk4::Label::new(Some(&artist.group.label));
    name.add_css_class("stats-item-title");
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    name.set_hexpand(true);
    line.append(&name);

    let bar = gtk4::LevelBar::new();
    bar.add_css_class("stats-song-bar");
    bar.set_min_value(0.0);
    bar.set_max_value(1.0);
    let metric = super::stats_bands_row::artist_metric(artist, sort_by);
    bar.set_value(share_of_leader(metric, leader_metric));
    bar.set_size_request(90, 8);
    bar.set_valign(gtk4::Align::Center);
    line.append(&bar);

    let value = gtk4::Label::new(Some(&metric_text(artist, sort_by)));
    value.add_css_class("stats-ghost-rank");
    value.set_xalign(1.0);
    line.append(&value);

    open_button.set_child(Some(&line));
    let artist_name = artist.group.label.clone();
    open_button.connect_clicked(move |_| (callbacks.open_artist)(artist_name.clone()));
    root.append(&open_button);

    let unify_button = gtk4::Button::from_icon_name("document-edit-symbolic");
    unify_button.add_css_class("flat");
    crate::ui::style::buttons::arm(&unify_button, crate::ui::style::buttons::ICON_CLASS);
    let unify_hint = (artist.group.variant_count >= 2)
        .then(|| strings::spellings_merged_hint(artist.group.variant_count));
    unify_button.set_tooltip_text(unify_hint.as_deref());
    if let Some(hint) = &unify_hint {
        unify_button.update_property(&[gtk4::accessible::Property::Label(hint)]);
    }
    unify_button.set_visible(artist.group.variant_count >= 2);
    let artist_key = artist.group.key.clone();
    unify_button.connect_clicked(move |_| (callbacks.unify)(artist_key.clone()));
    root.append(&unify_button);

    let picture = gtk4::Picture::new();

    ContinuationRow {
        root,
        avatar,
        picture,
        image: image.clone(),
        candidates: artist.cover_candidates.clone(),
        generation: generation.clone(),
        token: generation.get(),
        image_started: Cell::new(false),
        #[cfg(test)]
        open_button,
        #[cfg(test)]
        unify_button,
        #[cfg(test)]
        bar,
        artist: artist.group.label.clone(),
    }
}

fn metric_text(artist: &RankedGroup, sort_by: SortBy) -> String {
    match sort_by {
        SortBy::Plays => strings::stats_artist_plays(artist.group.plays),
        SortBy::Time => strings::stats_duration(artist.group.ms),
    }
}

impl ContinuationRow {
    pub(super) fn load_image(&self) {
        if self.image_started.replace(true) {
            return;
        }
        let avatar = self.avatar.clone();
        let picture = self.picture.clone();
        self.image.load(
            &self.picture,
            ArtistImageRequest {
                artist: self.artist.clone(),
                candidates: self.candidates.clone(),
                size: reprise_core::cover::ThumbnailSize::List,
                token: self.token,
                generation: self.generation.clone(),
                on_loaded: Rc::new(move |loaded| {
                    let paintable = loaded.then(|| picture.paintable()).flatten();
                    avatar.set_custom_image(paintable.as_ref());
                }),
            },
        );
    }

    #[cfg(test)]
    pub(super) fn artist_label(&self) -> String {
        self.artist.clone()
    }
}

fn share_of_leader(metric: i64, leader: i64) -> f64 {
    if leader <= 0 {
        return 0.0;
    }
    (metric.max(0) as f64 / leader as f64).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::share_of_leader;

    #[test]
    fn zero_leader_produces_an_empty_bar() {
        assert_eq!(share_of_leader(10, 0), 0.0);
    }

    #[test]
    fn artist_bar_is_clamped_to_the_leader() {
        assert_eq!(share_of_leader(150, 100), 1.0);
    }
}
