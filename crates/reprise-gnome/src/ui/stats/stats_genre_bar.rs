//! Display-only genre spectrum and legend.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::library::stats_snapshot::GenreSection;

use crate::ui::strings;

type UnifyCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;

#[derive(Clone)]
pub(in crate::ui) struct StatsGenreBar {
    root: gtk4::Box,
    segments: gtk4::Grid,
    legend: gtk4::Box,
    on_unify: UnifyCallback,
}

impl StatsGenreBar {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        root.add_css_class("stats-genre-spectrum");
        let segments = gtk4::Grid::builder()
            .column_spacing(2)
            .column_homogeneous(true)
            .build();
        segments.add_css_class("stats-genre-bar");
        segments.set_height_request(20);
        root.append(&segments);
        let legend = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        legend.add_css_class("stats-genre-legend");
        root.append(&legend);
        Self {
            root,
            segments,
            legend,
            on_unify: Rc::new(RefCell::new(None)),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_data(&self, section: &GenreSection) {
        clear_grid(&self.segments);
        clear(&self.legend);
        let mut column = 0;
        for (index, segment) in section.segments.iter().enumerate() {
            let bar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            bar.add_css_class("stats-genre-segment");
            bar.add_css_class(&format!("stats-genre-segment-{}", index.min(5)));
            bar.set_hexpand(true);
            bar.set_tooltip_text(Some(&format!(
                "{}: {}%",
                segment.label, segment.share_percent
            )));
            let width = segment.share_percent.max(1) as i32;
            self.segments.attach(&bar, column, 0, width, 1);
            column += width;

            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
            let dot = gtk4::Label::new(Some("\u{2022}"));
            dot.add_css_class(&format!("stats-genre-dot-{}", index.min(5)));
            row.append(&dot);
            let label = gtk4::Label::new(Some(&format!(
                "{} \u{00b7} {}%",
                segment.label, segment.share_percent
            )));
            label.set_xalign(0.0);
            label.set_hexpand(true);
            row.append(&label);
            if segment.variant_count >= 2 {
                let hint = gtk4::Button::with_label("Tag spellings");
                hint.add_css_class("flat");
                hint.add_css_class("stats-unify-hint");
                hint.set_tooltip_text(Some(&strings::spellings_merged_hint(segment.variant_count)));
                hint.connect_clicked({
                    let key = segment.key.clone();
                    let on_unify = self.on_unify.clone();
                    move |_| {
                        if let Some(callback) = on_unify.borrow().clone() {
                            callback(key.clone());
                        }
                    }
                });
                row.append(&hint);
            }
            self.legend.append(&row);
        }
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

fn clear_grid(container: &gtk4::Grid) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;
    use reprise_core::library::stats_snapshot::{GenreSection, GenreSegment};

    use super::*;

    fn fixture() -> GenreSection {
        GenreSection {
            segments: vec![
                GenreSegment {
                    label: "Metal".to_string(),
                    key: "metal".to_string(),
                    plays: 7,
                    total_ms: 700,
                    share_percent: 70,
                    variant_count: 1,
                    top_artist: Some("Artist".to_string()),
                    representative_track_path: "/music/track.flac".to_string(),
                },
                GenreSegment {
                    label: "Other".to_string(),
                    key: "other".to_string(),
                    plays: 3,
                    total_ms: 300,
                    share_percent: 30,
                    variant_count: 1,
                    top_artist: None,
                    representative_track_path: String::new(),
                },
            ],
            denominator_ms: 1_000,
        }
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn genre_bar_renders_one_segment_per_share_plus_legend() {
        gtk4::init().unwrap();
        let bar = StatsGenreBar::new();
        bar.set_data(&fixture());

        assert_eq!(bar.segments.observe_children().n_items(), 2);
        assert_eq!(bar.legend.observe_children().n_items(), 2);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn genre_bar_has_no_click_controller() {
        gtk4::init().unwrap();
        let bar = StatsGenreBar::new();
        bar.set_data(&fixture());
        let segment = bar.segments.first_child().unwrap();
        let controllers = segment.observe_controllers();

        assert!((0..controllers.n_items()).all(|index| {
            !controllers
                .item(index)
                .is_some_and(|item| item.is::<gtk4::GestureClick>())
        }));
    }
}
