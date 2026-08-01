//! Genre share strip: one stacked bar plus a single-line legend.
//!
//! STATS-19 demotes genres to a secondary reading, so the card is a ~90px
//! strip rather than a column of cover tiles. Everything a tile used to spell
//! out — duration, leading artist — now lives in the segment's tooltip, where
//! it costs no height until asked for.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::library::stats_snapshot::{GenreSection, GenreSegment};

use super::stats_genre_bar::StatsGenreBar;
use super::stats_view_widgets::label;
use crate::ui::motion_reveal::HorizontalReveal;
use crate::ui::strings;

type StringCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;

#[derive(Clone)]
pub(in crate::ui) struct StatsGenreCard {
    root: gtk4::Box,
    segments: StatsGenreBar,
    legend: gtk4::Box,
    on_unify: StringCallback,
    on_open_genre: StringCallback,
    segment_reveals: Rc<RefCell<Vec<HorizontalReveal>>>,
    #[cfg_attr(not(test), allow(dead_code))]
    genre_buttons: Rc<RefCell<Vec<gtk4::Button>>>,
}

impl StatsGenreCard {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        root.add_css_class("stats-genre-card");
        root.append(&label("GENRES", "stats-eyebrow"));
        let segments = StatsGenreBar::new();
        root.append(&segments);
        let legend = gtk4::Box::new(gtk4::Orientation::Horizontal, 20);
        legend.add_css_class("stats-genre-legend");
        root.append(&legend);
        let segment_reveals = Rc::new(RefCell::new(Vec::new()));
        Self {
            root,
            segments,
            legend,
            on_unify: Rc::new(RefCell::new(None)),
            on_open_genre: Rc::new(RefCell::new(None)),
            segment_reveals,
            genre_buttons: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_data(&self, section: &GenreSection) {
        self.segment_reveals.borrow_mut().clear();
        self.genre_buttons.borrow_mut().clear();
        while let Some(child) = self.legend.first_child() {
            self.legend.remove(&child);
        }
        self.render_segments(section);
        for (index, segment) in section.segments.iter().enumerate() {
            self.legend
                .append(&self.legend_entry(segment, index, section));
        }
    }

    fn render_segments(&self, section: &GenreSection) {
        let last_index = section.segments.len().saturating_sub(1);
        let target_shares = section
            .segments
            .iter()
            .map(|segment| segment.share_percent.max(1) as f64)
            .collect::<Vec<_>>();
        let mut reveals = Vec::with_capacity(section.segments.len());
        for (index, segment) in section.segments.iter().enumerate() {
            let bar = gtk4::Button::new();
            bar.add_css_class("flat");
            bar.add_css_class("stats-genre-segment");
            bar.add_css_class(&segment_css_class(segment, index, index == last_index));
            bar.set_hexpand(true);
            bar.set_height_request(14);
            bar.update_property(&[gtk4::accessible::Property::Label(&format!(
                "Open {} in the Library",
                segment.label
            ))]);
            bar.set_tooltip_text(Some(&segment_tooltip(segment)));
            if has_genre_tile(segment) {
                bar.connect_clicked({
                    let callback = self.on_open_genre.clone();
                    let genre = segment.label.clone();
                    move |_| invoke(&callback, genre.clone())
                });
                self.genre_buttons.borrow_mut().push(bar.clone());
            } else {
                bar.set_can_target(false);
            }
            let reveal = HorizontalReveal::new(&bar);
            reveals.push(reveal);
        }
        self.segments.set_segments(&reveals, &target_shares);
        self.segment_reveals.replace(reveals);
    }

    /// One legend entry: a colour dot matching its segment, the label and the
    /// share. Everything else the old tile carried is in the tooltip.
    fn legend_entry(
        &self,
        segment: &GenreSegment,
        index: usize,
        section: &GenreSection,
    ) -> gtk4::Box {
        let entry = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        entry.add_css_class("stats-genre-legend-entry");

        let dot = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        dot.add_css_class("stats-genre-legend-dot");
        dot.add_css_class(&segment_css_class(
            segment,
            index,
            index == section.segments.len().saturating_sub(1),
        ));
        dot.set_size_request(7, 7);
        dot.set_valign(gtk4::Align::Center);
        dot.set_accessible_role(gtk4::AccessibleRole::Presentation);
        entry.append(&dot);

        let text = label(
            &format!("{} {} %", segment.label, segment.share_percent),
            "stats-genre-legend-label",
        );
        if has_genre_tile(segment) {
            let button = gtk4::Button::new();
            button.add_css_class("flat");
            button.add_css_class("stats-genre-legend-button");
            button.set_child(Some(&text));
            button.set_tooltip_text(Some(&segment_tooltip(segment)));
            button.update_property(&[gtk4::accessible::Property::Label(&format!(
                "Open {} in the Library",
                segment.label
            ))]);
            button.connect_clicked({
                let callback = self.on_open_genre.clone();
                let genre = segment.label.clone();
                move |_| invoke(&callback, genre.clone())
            });
            self.genre_buttons.borrow_mut().push(button.clone());
            entry.append(&button);
        } else {
            entry.append(&text);
        }

        // The spelling-merge affordance moved here with the tiles it used to
        // live in; without it a merged genre would have no way back into the
        // tag editor.
        if segment.variant_count >= 2 {
            let hint = gtk4::Button::from_icon_name("document-edit-symbolic");
            hint.add_css_class("flat");
            hint.add_css_class("stats-genre-legend-unify");
            hint.set_tooltip_text(Some(&strings::spellings_merged_hint(segment.variant_count)));
            hint.connect_clicked({
                let callback = self.on_unify.clone();
                let key = segment.key.clone();
                move |_| invoke(&callback, key.clone())
            });
            entry.append(&hint);
        }
        entry
    }

    pub(in crate::ui) fn set_on_unify(&self, callback: impl Fn(String) + 'static) {
        *self.on_unify.borrow_mut() = Some(Rc::new(callback));
    }

    #[cfg(test)]
    pub(super) fn emit_unify(&self, key: &str) {
        invoke(&self.on_unify, key.to_string());
    }

    pub(in crate::ui) fn set_on_open_genre(&self, callback: impl Fn(String) + 'static) {
        *self.on_open_genre.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn segment_reveals(&self) -> Vec<HorizontalReveal> {
        self.segment_reveals.borrow().clone()
    }

    pub(super) fn target_segment_shares(&self) -> Vec<f64> {
        self.segments.target_shares()
    }

    pub(super) fn set_segment_shares(&self, shares: &[f64]) {
        self.segments.set_shares(shares);
    }

    #[cfg(test)]
    fn segment_buttons(&self) -> Vec<gtk4::Button> {
        self.segment_reveals
            .borrow()
            .iter()
            .map(|reveal| {
                reveal
                    .first_child()
                    .and_downcast::<gtk4::Button>()
                    .expect("every genre segment reveal must wrap its button")
            })
            .collect()
    }
}

fn has_genre_tile(segment: &GenreSegment) -> bool {
    segment.key != "other"
}

fn segment_css_class(segment: &GenreSegment, index: usize, is_last: bool) -> String {
    if segment.key == "other" || is_last {
        "stats-genre-segment-last".into()
    } else {
        format!("stats-genre-rank-{}", index.min(4))
    }
}

/// Everything the removed tile spelled out, on hover instead of on screen.
fn segment_tooltip(segment: &GenreSegment) -> String {
    format!(
        "{} · {} % · {} · top: {}",
        segment.label,
        segment.share_percent,
        strings::stats_duration(segment.total_ms),
        segment.top_artist.as_deref().unwrap_or("Unknown artist")
    )
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
        StatsGenreCard::new()
    }

    #[test]
    fn segment_classes_rank_and_terminate() {
        let mut genre = fixture().segments.remove(0);
        assert_eq!(
            segment_css_class(&genre, 1, true),
            "stats-genre-segment-last",
            "the final ranked genre must stay neutral without an Other aggregate"
        );

        genre.key = "other".into();
        genre.label = "Weitere".into();
        assert_eq!(
            segment_css_class(&genre, 0, false),
            "stats-genre-segment-last"
        );
        assert!(!has_genre_tile(&genre));
    }

    #[test]
    fn genre_duration_uses_the_compact_hour_minute_format() {
        assert_eq!(strings::stats_duration(25_080_000), "6 h 58");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_15_genre_card_uses_compact_spacing_and_exact_tooltips() {
        gtk4::init().unwrap();
        let card = card();
        let mut section = fixture();
        section.segments[0].total_ms = 25_080_000;
        section.segments[0].share_percent = 55;
        card.set_data(&section);

        assert_eq!(card.widget().spacing(), 8);
        assert_eq!(
            card.segment_buttons()
                .first()
                .expect("the fixture must render a genre segment")
                .tooltip_text()
                .as_deref(),
            Some("Metalcore · 55 % · 6 h 58 · top: Lorna Shore")
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_19_legend_names_every_segment_and_defers_the_rest_to_the_tooltip() {
        gtk4::init().unwrap();
        let card = card();
        card.set_data(&fixture());

        // One entry per segment, including the aggregate the old tile grid
        // filtered out — the strip is the whole reading now.
        assert_eq!(card.legend.observe_children().n_items(), 2);
        let labels = descendant_labels(card.legend.upcast_ref());
        assert!(labels.iter().any(|copy| copy == "Metalcore 70 %"));
        assert!(labels.iter().any(|copy| copy == "Other 30 %"));
        // Duration and leading artist cost no height until hovered.
        assert!(labels.iter().all(|copy| !copy.contains("top:")));
        assert_eq!(
            segment_tooltip(&fixture().segments[0]),
            "Metalcore · 70 % · 6 h 50 · top: Lorna Shore"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_19_genre_segments_fill_the_14px_bar() {
        gtk4::init().unwrap();
        crate::ui::style::install_css_string_for_test(&crate::ui::stats::stats_css::css());
        let card = card();
        card.set_data(&fixture());
        let window = gtk4::Window::builder()
            .default_width(640)
            .child(card.widget())
            .build();
        window.present();
        run_main_loop_for_layout();

        let eyebrow = card.widget().first_child().unwrap();
        let segment_grid = eyebrow.next_sibling().unwrap();
        let segments = card.segment_reveals();
        let occupied_width = segments
            .iter()
            .map(gtk4::prelude::WidgetExt::width)
            .sum::<i32>()
            + (segments.len().saturating_sub(1) as i32 * 2);

        assert_eq!(segment_grid.height(), 14);
        assert!(segments.iter().all(|segment| segment.height() == 14));
        assert!(
            segment_grid.width() >= 500,
            "the stacked bar collapsed to {} px",
            segment_grid.width()
        );
        assert_eq!(
            occupied_width,
            segment_grid.width(),
            "segments and their 2 px gap must occupy the complete card width"
        );
        assert!(
            (segments[0].width() as f64 / segment_grid.width() as f64 - 0.70).abs() < 0.02,
            "the 70 percent segment received {} of {} px",
            segments[0].width(),
            segment_grid.width()
        );
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn stats_19_segment_and_legend_request_the_genre_scope() {
        gtk4::init().unwrap();
        let card = card();
        let opened = Rc::new(RefCell::new(Vec::new()));
        card.set_on_open_genre({
            let opened = opened.clone();
            move |genre| opened.borrow_mut().push(genre)
        });
        card.set_data(&fixture());

        card.genre_buttons.borrow()[0].emit_clicked();
        card.genre_buttons.borrow()[1].emit_clicked();

        assert_eq!(
            *opened.borrow(),
            vec!["Metalcore".to_string(), "Metalcore".to_string()]
        );
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

    fn run_main_loop_for_layout() {
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
            quit.quit();
        });
        main_loop.run();
    }
}
