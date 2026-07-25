//! Genre share card with display-only segments and album-cover tile actions.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::cover::ThumbnailSize;
use reprise_core::library::stats_snapshot::{GenreSection, GenreSegment};

use super::stats_view_widgets::label;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

type StringCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;

#[derive(Clone)]
pub(in crate::ui) struct StatsGenreCard {
    root: gtk4::Box,
    segments: gtk4::Grid,
    tiles: gtk4::Grid,
    cover_loader: Rc<CoverLoader>,
    cover_generation: Rc<Cell<u64>>,
    on_unify: StringCallback,
    on_open_album_path: StringCallback,
    #[cfg_attr(not(test), allow(dead_code))]
    cover_buttons: Rc<RefCell<Vec<gtk4::Button>>>,
}

impl StatsGenreCard {
    pub(in crate::ui) fn new(cover_loader: Rc<CoverLoader>) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
        root.add_css_class("stats-genre-card");
        root.append(&label("GENRES", "stats-eyebrow"));
        let segments = gtk4::Grid::builder().column_spacing(2).build();
        segments.add_css_class("stats-genre-bar");
        segments.set_height_request(22);
        root.append(&segments);
        let tiles = gtk4::Grid::builder()
            .column_spacing(16)
            .row_spacing(10)
            .column_homogeneous(true)
            .build();
        tiles.add_css_class("stats-genre-tiles");
        root.append(&tiles);
        Self {
            root,
            segments,
            tiles,
            cover_loader,
            cover_generation: Rc::new(Cell::new(0)),
            on_unify: Rc::new(RefCell::new(None)),
            on_open_album_path: Rc::new(RefCell::new(None)),
            cover_buttons: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_data(&self, section: &GenreSection) {
        clear_grid(&self.segments);
        clear_grid(&self.tiles);
        self.cover_buttons.borrow_mut().clear();
        let token = self.cover_generation.get().wrapping_add(1);
        self.cover_generation.set(token);
        self.render_segments(section);
        for (index, segment) in section
            .segments
            .iter()
            .filter(|segment| has_genre_tile(segment))
            .take(4)
            .enumerate()
        {
            self.tiles
                .attach(&self.tile(segment, token), index as i32, 0, 1, 1);
        }
    }

    fn render_segments(&self, section: &GenreSection) {
        let mut column = 0;
        for (index, segment) in section.segments.iter().enumerate() {
            let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            bar.add_css_class("stats-genre-segment");
            bar.add_css_class(&segment_css_class(segment, index));
            bar.set_hexpand(true);
            bar.set_height_request(22);
            bar.set_tooltip_text(Some(&format!(
                "{} · {} % · {}",
                segment.label,
                segment.share_percent,
                strings::hero_listening_time(segment.total_ms)
            )));
            let width = segment.share_percent.max(1) as i32;
            self.segments.attach(&bar, column, 0, width, 1);
            column += width;
        }
    }

    fn tile(&self, segment: &GenreSegment, token: u64) -> gtk4::Box {
        let tile = gtk4::Box::new(gtk4::Orientation::Horizontal, 9);
        tile.add_css_class("stats-genre-tile");
        let cover = gtk4::Image::builder()
            .pixel_size(40)
            .width_request(40)
            .height_request(40)
            .build();
        CoverLoader::set_placeholder(&cover);
        if segment.representative_track_path.is_empty() {
            tile.append(&cover);
        } else {
            self.cover_loader.load_into(
                &cover,
                &segment.representative_track_path,
                ThumbnailSize::List,
                token,
                &self.cover_generation,
            );
            let cover_button = gtk4::Button::new();
            cover_button.add_css_class("flat");
            cover_button.add_css_class("stats-genre-cover");
            cover_button.set_tooltip_text(Some("Go to album"));
            cover_button.set_child(Some(&cover));
            cover_button.connect_clicked({
                let callback = self.on_open_album_path.clone();
                let path = segment.representative_track_path.clone();
                move |_| invoke(&callback, path.clone())
            });
            self.cover_buttons.borrow_mut().push(cover_button.clone());
            tile.append(&cover_button);
        }

        let copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        copy.set_hexpand(true);
        copy.append(&label(
            &format!("{} · {} %", segment.label, segment.share_percent),
            "stats-item-title",
        ));
        copy.append(&label(
            &format!(
                "{} · top: {}",
                strings::hero_listening_time(segment.total_ms),
                segment.top_artist.as_deref().unwrap_or("Unknown artist")
            ),
            "stats-item-subtitle",
        ));
        if segment.variant_count >= 2 {
            let hint = gtk4::Button::with_label("Tag spellings");
            hint.add_css_class("flat");
            hint.add_css_class("stats-unify-hint");
            hint.set_halign(gtk4::Align::Start);
            hint.set_tooltip_text(Some(&strings::spellings_merged_hint(segment.variant_count)));
            hint.connect_clicked({
                let callback = self.on_unify.clone();
                let key = segment.key.clone();
                move |_| invoke(&callback, key.clone())
            });
            copy.append(&hint);
        }
        tile.append(&copy);
        tile
    }

    pub(in crate::ui) fn set_on_unify(&self, callback: impl Fn(String) + 'static) {
        *self.on_unify.borrow_mut() = Some(Rc::new(callback));
    }

    #[cfg(test)]
    pub(super) fn emit_unify(&self, key: &str) {
        invoke(&self.on_unify, key.to_string());
    }

    pub(in crate::ui) fn set_on_open_album_path(&self, callback: impl Fn(String) + 'static) {
        *self.on_open_album_path.borrow_mut() = Some(Rc::new(callback));
    }
}

fn has_genre_tile(segment: &GenreSegment) -> bool {
    segment.key != "other"
}

fn segment_css_class(segment: &GenreSegment, index: usize) -> String {
    if segment.key == "other" {
        "stats-genre-segment-last".into()
    } else {
        format!("stats-genre-rank-{}", index.min(4))
    }
}

fn clear_grid(container: &gtk4::Grid) {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> GenreSection {
        GenreSection {
            segments: vec![
                GenreSegment {
                    label: "Metalcore".into(),
                    key: "metalcore".into(),
                    plays: 7,
                    total_ms: 24_600_000,
                    share_percent: 70,
                    variant_count: 2,
                    top_artist: Some("Lorna Shore".into()),
                    representative_track_path: "/music/track.flac".into(),
                },
                GenreSegment {
                    label: "Other".into(),
                    key: "other".into(),
                    plays: 3,
                    total_ms: 3_600_000,
                    share_percent: 30,
                    variant_count: 1,
                    top_artist: None,
                    representative_track_path: String::new(),
                },
            ],
            denominator_ms: 28_200_000,
        }
    }

    fn card() -> StatsGenreCard {
        StatsGenreCard::new(CoverLoader::new(
            crate::ui::cover_download_worker::setup_for_test(),
        ))
    }

    #[test]
    fn genre_style_and_tile_eligibility_follow_the_semantic_key() {
        let mut genre = fixture().segments.remove(0);
        genre.label = "Other".into();
        assert_eq!(segment_css_class(&genre, 0), "stats-genre-rank-0");
        assert!(has_genre_tile(&genre));

        genre.key = "other".into();
        genre.label = "Weitere".into();
        assert_eq!(segment_css_class(&genre, 0), "stats-genre-segment-last");
        assert!(!has_genre_tile(&genre));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_15_tiles_show_cover_share_and_top_artist() {
        gtk4::init().unwrap();
        let card = card();
        card.set_data(&fixture());

        assert_eq!(card.tiles.observe_children().n_items(), 1);
        let tile = card.tiles.first_child().unwrap();
        assert!(descendant_labels(&tile)
            .iter()
            .any(|copy| copy == "Metalcore · 70 %"));
        assert!(descendant_labels(&tile)
            .iter()
            .any(|copy| copy.contains("top: Lorna Shore")));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_15_genre_segments_fill_the_22px_bar() {
        gtk4::init().unwrap();
        let card = card();
        card.set_data(&fixture());
        let window = gtk4::Window::builder()
            .default_width(640)
            .child(card.widget())
            .build();
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let eyebrow = card.widget().first_child().unwrap();
        let segment_grid = eyebrow.next_sibling().unwrap();
        let segment = segment_grid.first_child().unwrap();

        assert_eq!(segment_grid.height(), 22);
        assert_eq!(segment.height(), 22);
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_15_segment_carries_no_click_controller() {
        gtk4::init().unwrap();
        let card = card();
        card.set_data(&fixture());
        let segment = card.segments.first_child().unwrap();
        let controllers = segment.observe_controllers();

        assert!((0..controllers.n_items()).all(|index| {
            !controllers
                .item(index)
                .is_some_and(|item| item.is::<gtk4::GestureClick>())
        }));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_15_cover_click_requests_the_representative_album() {
        gtk4::init().unwrap();
        let card = card();
        let opened = Rc::new(RefCell::new(None));
        card.set_on_open_album_path({
            let opened = opened.clone();
            move |path| *opened.borrow_mut() = Some(path)
        });
        card.set_data(&fixture());

        card.cover_buttons.borrow()[0].emit_clicked();

        assert_eq!(opened.borrow().as_deref(), Some("/music/track.flac"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn genre_without_a_representative_path_has_no_cover_action() {
        gtk4::init().unwrap();
        let card = card();
        let mut section = fixture();
        section.segments[0].representative_track_path.clear();

        card.set_data(&section);

        assert!(card.cover_buttons.borrow().is_empty());
    }

    fn descendant_labels(root: &gtk4::Widget) -> Vec<String> {
        let mut labels = Vec::new();
        let mut child = root.first_child();
        while let Some(widget) = child {
            if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
                labels.push(label.label().to_string());
            }
            labels.extend(descendant_labels(&widget));
            child = widget.next_sibling();
        }
        labels
    }
}
