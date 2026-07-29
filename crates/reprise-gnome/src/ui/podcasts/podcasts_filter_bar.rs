use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::podcasts::{self, PodcastKind};
use rusqlite::Connection;

// `active` lives in `podcasts_presentation` (also read by the empty-state
// classification), not duplicated here.
use super::podcasts_presentation::{active, LibrarySummary, PodcastFilter};
use crate::ui::browse::browse_bar::CHIP_CSS_CLASS;
use crate::ui::strings;
use crate::ui::style::buttons;

const FILTER_BAR_MIN_HEIGHT: i32 = 34;
type OnChanged = Rc<dyn Fn(PodcastFilter)>;

pub(super) struct PodcastsFilterBar {
    root: gtk4::Box,
    conn: Rc<RefCell<Connection>>,
    filter: RefCell<PodcastFilter>,
    chips: gtk4::Box,
    popover_box: gtk4::Box,
    popover: gtk4::Popover,
    result: gtk4::Label,
    shows: RefCell<Vec<String>>,
    on_changed: RefCell<Option<OnChanged>>,
}

impl PodcastsFilterBar {
    pub(super) fn new(conn: Rc<RefCell<Connection>>, kind: PodcastKind) -> Rc<Self> {
        let stored = podcasts::config::load_filter(&conn.borrow()).unwrap_or_default();
        let filter = PodcastFilter {
            unplayed_only: stored.unplayed_only,
            show: stored.show,
            source: None,
            downloaded_only: stored.downloaded_only,
        };
        let root = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        root.set_margin_top(6);
        root.set_margin_bottom(6);
        root.set_margin_start(12);
        root.set_margin_end(12);
        root.set_size_request(-1, FILTER_BAR_MIN_HEIGHT);
        root.add_css_class("toolbar");

        let add = gtk4::Button::builder()
            .icon_name("list-add-symbolic")
            .label(strings::text(match kind {
                PodcastKind::Rss => strings::PODCAST_ADD,
                PodcastKind::Youtube => strings::YOUTUBE_ADD,
            }))
            .build();
        buttons::arm(&add, buttons::ADD_ACTION_CLASS);
        add.set_action_name(Some("podcasts.open-add"));
        root.append(&add);

        let chips = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        chips.set_hexpand(true);
        root.append(&chips);
        let popover_box = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        popover_box.set_margin_top(8);
        popover_box.set_margin_bottom(8);
        popover_box.set_margin_start(8);
        popover_box.set_margin_end(8);
        let popover = gtk4::Popover::new();
        popover.set_child(Some(&popover_box));
        let add_filter = gtk4::MenuButton::builder()
            .label(format!("+ {}", strings::text(strings::PODCAST_ADD_FILTER)))
            .popover(&popover)
            .build();
        add_filter.add_css_class("pill");
        chips.append(&add_filter);
        let result = gtk4::Label::new(None);
        result.add_css_class("dim-label");
        result.add_css_class("caption");
        root.append(&result);

        let bar = Rc::new(Self {
            root,
            conn,
            filter: RefCell::new(filter),
            chips,
            popover_box,
            popover,
            result,
            shows: RefCell::new(Vec::new()),
            on_changed: RefCell::new(None),
        });
        bar.rebuild();
        bar
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn filter(&self) -> PodcastFilter {
        self.filter.borrow().clone()
    }

    pub(super) fn set_on_changed(&self, callback: impl Fn(PodcastFilter) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    pub(super) fn set_context(
        self: &Rc<Self>,
        shows: Vec<String>,
        shown: usize,
        summary: LibrarySummary,
    ) {
        if self
            .filter
            .borrow()
            .show
            .as_ref()
            .is_some_and(|selected| !shows.contains(selected))
        {
            self.filter.borrow_mut().show = None;
        }
        self.shows.replace(shows);
        self.result.set_text(&if active(&self.filter()) {
            strings::podcast_filtered_count(shown, summary.episodes)
        } else {
            // `G2` (design 6a): "4 shows · 41 episodes · 7 new" replaces the
            // bare episode count once nothing is filtered. The filtered
            // branch above is unchanged — "shown of total" is what matters
            // once a filter has narrowed the view.
            strings::podcast_library_summary(summary.shows, summary.episodes, summary.new)
        });
        self.rebuild();
    }

    pub(super) fn clear_all(self: &Rc<Self>) {
        self.apply(PodcastFilter::default());
    }

    fn apply(self: &Rc<Self>, filter: PodcastFilter) {
        if let Err(error) = podcasts::config::save_filter(&self.conn.borrow(), &filter) {
            tracing::warn!(%error, "could not persist podcast filters");
            return;
        }
        self.filter.replace(filter.clone());
        self.popover.popdown();
        self.rebuild();
        let callback = self.on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(filter);
        }
    }

    fn rebuild(self: &Rc<Self>) {
        while let Some(child) = self.chips.first_child() {
            if child.is::<gtk4::MenuButton>() {
                break;
            }
            self.chips.remove(&child);
        }
        let filter = self.filter();
        if filter.unplayed_only {
            self.prepend_chip(&strings::text(strings::PODCAST_FILTER_UNPLAYED), |filter| {
                PodcastFilter {
                    unplayed_only: false,
                    ..filter
                }
            });
        }
        if let Some(show) = filter.show.clone() {
            self.prepend_chip(&show, |filter| PodcastFilter {
                show: None,
                ..filter
            });
        }
        if filter.downloaded_only {
            self.prepend_chip(
                &strings::text(strings::PODCAST_FILTER_DOWNLOADED),
                |filter| PodcastFilter {
                    downloaded_only: false,
                    ..filter
                },
            );
        }
        if active(&filter) {
            let clear = gtk4::Button::with_label(&format!(
                "{}  ×",
                strings::text(strings::PODCAST_CLEAR_ALL)
            ));
            clear.add_css_class(CHIP_CSS_CLASS);
            clear.add_css_class("flat");
            let weak = Rc::downgrade(self);
            clear.connect_clicked(move |_| {
                if let Some(bar) = weak.upgrade() {
                    bar.clear_all();
                }
            });
            self.chips.prepend(&clear);
        }
        self.rebuild_popover();
    }

    fn prepend_chip(
        self: &Rc<Self>,
        label: &str,
        remove: impl Fn(PodcastFilter) -> PodcastFilter + 'static,
    ) {
        let button = gtk4::Button::with_label(&format!("{label}  ×"));
        button.add_css_class(CHIP_CSS_CLASS);
        button.add_css_class("flat");
        button.set_size_request(-1, 20);
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(bar) = weak.upgrade() {
                bar.apply(remove(bar.filter()));
            }
        });
        self.chips.prepend(&button);
    }

    fn rebuild_popover(self: &Rc<Self>) {
        while let Some(child) = self.popover_box.first_child() {
            self.popover_box.remove(&child);
        }
        self.add_value_button(&strings::text(strings::PODCAST_FILTER_UNPLAYED), |filter| {
            PodcastFilter {
                unplayed_only: true,
                ..filter
            }
        });
        self.add_value_button(
            &strings::text(strings::PODCAST_FILTER_DOWNLOADED),
            |filter| PodcastFilter {
                downloaded_only: true,
                ..filter
            },
        );
        self.add_heading(strings::PODCAST_FILTER_SHOW);
        for show in self.shows.borrow().clone() {
            let value = show.clone();
            self.add_value_button(&show, move |filter| PodcastFilter {
                show: Some(value.clone()),
                ..filter
            });
        }
    }

    fn add_heading(&self, label: &str) {
        let heading = gtk4::Label::new(Some(&strings::text(label)));
        heading.add_css_class("caption");
        heading.add_css_class("dim-label");
        heading.set_xalign(0.0);
        self.popover_box.append(&heading);
    }

    fn add_value_button(
        self: &Rc<Self>,
        label: &str,
        apply: impl Fn(PodcastFilter) -> PodcastFilter + 'static,
    ) {
        let button = gtk4::Button::with_label(label);
        button.add_css_class("flat");
        button.set_halign(gtk4::Align::Fill);
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(bar) = weak.upgrade() {
                bar.apply(apply(bar.filter()));
            }
        });
        self.popover_box.append(&button);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_2_add_action_is_tinted_button_not_chip() {
        assert_eq!(buttons::ADD_ACTION_CLASS, "reprise-btn-add");
        assert_ne!(buttons::ADD_ACTION_CLASS, CHIP_CSS_CLASS);
        assert!(!buttons::ADD_ACTION_CLASS.contains("chip"));
    }

    #[test]
    fn active_filter_detection_is_composable() {
        assert!(!active(&PodcastFilter::default()));
        assert!(active(&PodcastFilter {
            unplayed_only: true,
            ..PodcastFilter::default()
        }));
    }
}
