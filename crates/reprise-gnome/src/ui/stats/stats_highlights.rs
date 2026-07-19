//! Four editorial listening highlights and the Smart Mix call to action.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::library::stats_snapshot::{GenreSegment, HighlightsSection};

/// The genre the mix CTA acts on. It carries the group **key**, because the
/// label is only the most common raw spelling of the group (STATS-9) and a
/// name equality on it would select a different set of tracks than the screen
/// shows.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::ui) struct TopGenre {
    pub key: String,
    pub label: String,
}

impl TopGenre {
    pub(in crate::ui) fn from_segment(segment: &GenreSegment) -> Self {
        Self {
            key: segment.key.clone(),
            label: segment.label.clone(),
        }
    }
}

type GenreCallback = Rc<RefCell<Option<Rc<dyn Fn(TopGenre)>>>>;

#[derive(Clone)]
pub(in crate::ui) struct StatsHighlights {
    root: gtk4::Box,
    #[cfg_attr(not(test), allow(dead_code))]
    grid: gtk4::Grid,
    values: [gtk4::Label; 4],
    create: gtk4::Button,
    top_genre: Rc<RefCell<Option<TopGenre>>>,
    on_create_mix: GenreCallback,
}

impl StatsHighlights {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        root.add_css_class("stats-highlights");
        let grid = gtk4::Grid::builder()
            .column_spacing(8)
            .row_spacing(8)
            .column_homogeneous(true)
            .build();
        let values = [
            tile(&grid, 0, 0, "STREAK"),
            tile(&grid, 1, 0, "DISCOVERED"),
            tile(&grid, 0, 1, "BUSIEST DAY"),
            tile(&grid, 1, 1, "ON REPEAT"),
        ];
        root.append(&grid);
        // One genre, not "top genres": the rule engine behind the mix joins
        // its rules with AND and knows no alternation, so a mix can only ever
        // be built from a single genre group.
        let create = gtk4::Button::with_label(&mix_label(None));
        create.add_css_class("flat");
        create.set_halign(gtk4::Align::Start);
        create.set_visible(false);
        let top_genre = Rc::new(RefCell::new(None::<TopGenre>));
        let on_create_mix: GenreCallback = Rc::new(RefCell::new(None));
        create.connect_clicked({
            let top_genre = top_genre.clone();
            let on_create_mix = on_create_mix.clone();
            move |_| {
                let genre = top_genre.borrow().clone();
                let callback = on_create_mix.borrow().clone();
                if let (Some(genre), Some(callback)) = (genre, callback) {
                    callback(genre);
                }
            }
        });
        root.append(&create);
        Self {
            root,
            grid,
            values,
            create,
            top_genre,
            on_create_mix,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_data(&self, section: &HighlightsSection) {
        self.values[0].set_label(&format!("{} days", section.streak_days));
        self.values[1].set_label(&format!("{} tracks", section.discovered_tracks));
        self.values[2].set_label(&section.busiest_day.as_ref().map_or_else(
            || "\u{2014}".to_string(),
            |day| day.day.format("%b %-d").to_string(),
        ));
        self.values[3].set_label(
            &section
                .on_repeat
                .as_ref()
                .map_or_else(|| "\u{2014}".to_string(), |track| track.title.clone()),
        );
    }

    /// Names the genre the CTA would mix, or hides it when the period has no
    /// genre to offer.
    pub(in crate::ui) fn set_top_genre(&self, genre: Option<TopGenre>) {
        self.create.set_label(&mix_label(genre.as_ref()));
        self.create.set_visible(genre.is_some());
        *self.top_genre.borrow_mut() = genre;
    }

    pub(in crate::ui) fn set_on_create_mix(&self, callback: impl Fn(TopGenre) + 'static) {
        *self.on_create_mix.borrow_mut() = Some(Rc::new(callback));
    }
}

fn mix_label(genre: Option<&TopGenre>) -> String {
    match genre {
        Some(genre) => format!("Mix from {} \u{00b7} Create", genre.label),
        None => "Mix from your top genre \u{00b7} Create".to_string(),
    }
}

fn tile(grid: &gtk4::Grid, column: i32, row: i32, title: &str) -> gtk4::Label {
    let tile = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    tile.add_css_class("stats-highlight-tile");
    let title = gtk4::Label::new(Some(title));
    title.add_css_class("stats-eyebrow");
    title.set_xalign(0.0);
    tile.append(&title);
    let value = gtk4::Label::new(None);
    value.add_css_class("stats-highlight-value");
    value.set_xalign(0.0);
    value.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    tile.append(&value);
    grid.attach(&tile, column, row, 1, 1);
    value
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use gtk4::prelude::*;
    use reprise_core::library::stats_screen::TopTrack;
    use reprise_core::library::stats_snapshot::{BusiestDay, HighlightsSection};

    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn highlights_grid_renders_four_tiles() {
        gtk4::init().unwrap();
        let highlights = StatsHighlights::new();
        highlights.set_data(&HighlightsSection {
            streak_days: 5,
            discovered_tracks: 3,
            busiest_day: Some(BusiestDay {
                day: NaiveDate::from_ymd_opt(2026, 7, 19).unwrap(),
                total_ms: 3_600_000,
            }),
            on_repeat: Some(TopTrack {
                track_id: 1,
                title: "Repeat".to_string(),
                artist: "Artist".to_string(),
                album: "Album".to_string(),
                play_count: 8,
                total_ms: 800,
                track_path: "/music/repeat.flac".to_string(),
            }),
        });

        assert_eq!(highlights.grid.observe_children().n_items(), 4);
    }
}
