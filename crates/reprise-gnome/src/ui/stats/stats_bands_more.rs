//! Ranks six through twenty of the artist ranking (STATS-23).

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::format::format_thousands;
use reprise_core::library::stats_screen::RankedGroup;
use reprise_core::library::stats_snapshot::SortBy;

use super::stats_artist_image::{ArtistImageRequest, StatsArtistImage};
use crate::ui::strings;

const AVATAR_SIZE: i32 = 32;

pub(super) struct ContinuationRow {
    pub(super) root: gtk4::Button,
}

pub(super) fn build_row(
    rank: usize,
    artist: &RankedGroup,
    leader_metric: i64,
    sort_by: SortBy,
    image: &Rc<StatsArtistImage>,
    generation: &Rc<Cell<u64>>,
    on_open_artist: Rc<dyn Fn(String)>,
) -> ContinuationRow {
    let root = gtk4::Button::new();
    root.add_css_class("flat");
    root.add_css_class("stats-artist-row");
    root.update_property(&[gtk4::accessible::Property::Label(&artist.group.label)]);
    crate::ui::style::buttons::arm_cursor(&root);

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
    let metric = artist_metric(artist, sort_by);
    bar.set_value(share_of_leader(metric, leader_metric));
    bar.set_size_request(90, -1);
    line.append(&bar);

    let value = gtk4::Label::new(Some(&metric_text(artist, sort_by)));
    value.add_css_class("stats-ghost-rank");
    value.set_xalign(1.0);
    line.append(&value);

    root.set_child(Some(&line));
    let artist_name = artist.group.label.clone();
    root.connect_clicked(move |_| on_open_artist(artist_name.clone()));

    let token = generation.get();
    let picture = gtk4::Picture::new();
    let avatar_for_image = avatar.clone();
    let picture_for_image = picture.clone();
    image.load(
        &picture,
        ArtistImageRequest {
            artist: artist.group.label.clone(),
            candidates: artist.cover_candidates.clone(),
            size: reprise_core::cover::ThumbnailSize::List,
            token,
            generation: generation.clone(),
            on_loaded: Rc::new(move |loaded| {
                let paintable = loaded.then(|| picture_for_image.paintable()).flatten();
                avatar_for_image.set_custom_image(paintable.as_ref());
            }),
        },
    );

    ContinuationRow { root }
}

fn artist_metric(artist: &RankedGroup, sort_by: SortBy) -> i64 {
    match sort_by {
        SortBy::Plays => artist.group.plays,
        SortBy::Time => artist.group.ms,
    }
}

fn metric_text(artist: &RankedGroup, sort_by: SortBy) -> String {
    match sort_by {
        SortBy::Plays => format!("{} plays", format_thousands(artist.group.plays)),
        SortBy::Time => strings::stats_duration(artist.group.ms),
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
