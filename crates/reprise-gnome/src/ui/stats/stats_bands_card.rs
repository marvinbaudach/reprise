//! One expandable card for the complete My Stats artist ranking (STATS-23).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::stats_screen::RankedGroup;
use reprise_core::library::stats_snapshot::{SortBy, StatsSnapshot};

use super::stats_artist_image::StatsArtistImage;
use super::stats_bands_more::{self, ContinuationRow};
use super::stats_bands_row::{StatsBandsRow, RUNNER_UP_COUNT};
use super::stats_view_widgets::clear;

type StringCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;

/// Ranks the expander adds below the five artist surfaces already on screen.
pub(super) const ARTIST_ROW_EXTRA: usize = 15;

pub(super) fn first_continuation_rank() -> usize {
    RUNNER_UP_COUNT + 2
}

pub(super) fn has_continuation(artists: usize) -> bool {
    artists > RUNNER_UP_COUNT + 1
}

#[derive(Clone)]
struct RankingState {
    bands_row: StatsBandsRow,
    columns: [gtk4::Box; 2],
    rows: Rc<RefCell<Vec<ContinuationRow>>>,
    artist_image: Rc<StatsArtistImage>,
    generation: Rc<Cell<u64>>,
    snapshot: Rc<RefCell<Option<StatsSnapshot>>>,
    sort_by: Rc<Cell<SortBy>>,
    on_open_artist: StringCallback,
}

impl RankingState {
    fn render(&self, expanded: bool) -> bool {
        let Some(snapshot) = self.snapshot.borrow().clone() else {
            self.bands_row.clear_data();
            self.clear_continuation();
            return false;
        };
        let sort_by = self.sort_by.get();
        let artists = snapshot.top_artists_sorted(sort_by);
        let share = artists
            .first()
            .map_or(0, |leader| snapshot.artist_share_percent(leader));
        self.bands_row.set_data(&artists, share, sort_by);
        if expanded {
            self.render_continuation(&artists, sort_by);
        } else {
            self.clear_continuation();
        }
        has_continuation(artists.len())
    }

    fn clear_continuation(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        for column in &self.columns {
            clear(column);
        }
        self.rows.borrow_mut().clear();
    }

    fn render_continuation(&self, artists: &[RankedGroup], sort_by: SortBy) {
        self.clear_continuation();
        let token = self.generation.get();
        let generation = self.generation.clone();
        let leader_metric = artists
            .first()
            .map_or(0, |artist| artist_metric(artist, sort_by));
        let continuation = artists
            .iter()
            .skip(RUNNER_UP_COUNT + 1)
            .take(ARTIST_ROW_EXTRA)
            .collect::<Vec<_>>();
        let first_column_rows = continuation.len().div_ceil(2);
        let mut rows = self.rows.borrow_mut();
        for (offset, artist) in continuation.into_iter().enumerate() {
            let callback = self.on_open_artist.clone();
            let row = stats_bands_more::build_row(
                offset + first_continuation_rank(),
                artist,
                leader_metric,
                sort_by,
                &self.artist_image,
                &generation,
                Rc::new(move |artist| invoke(&callback, artist)),
            );
            let column = usize::from(offset >= first_column_rows);
            self.columns[column].append(&row.root);
            rows.push(row);
        }
        debug_assert_eq!(generation.get(), token);
    }
}

#[derive(Clone)]
pub(in crate::ui) struct StatsBandsCard {
    root: gtk4::Box,
    state: RankingState,
    pub(super) revealer: gtk4::Revealer,
    pub(super) reveal_button: gtk4::Button,
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) sort_toggle: adw::ToggleGroup,
}

impl StatsBandsCard {
    pub(in crate::ui) fn new(artist_image: Rc<StatsArtistImage>) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        root.add_css_class("stats-bands-card");

        let plays_sort = adw::Toggle::builder()
            .name("plays")
            .label("by plays")
            .build();
        let time_sort = adw::Toggle::builder().name("time").label("by time").build();
        let sort_toggle = adw::ToggleGroup::new();
        sort_toggle.add(plays_sort);
        sort_toggle.add(time_sort);
        sort_toggle.set_active_name(Some("time"));
        sort_toggle.set_halign(gtk4::Align::End);
        sort_toggle.update_property(&[gtk4::accessible::Property::Label("Sort top artists")]);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);
        header.append(&sort_toggle);
        root.append(&header);

        let bands_row = StatsBandsRow::new();
        bands_row.set_artist_image(&artist_image);
        root.append(bands_row.widget());

        let reveal_button = gtk4::Button::with_label("Show more top artists");
        reveal_button.add_css_class("flat");
        reveal_button.add_css_class("stats-songs-reveal");
        reveal_button.set_halign(gtk4::Align::Start);
        root.append(&reveal_button);

        let continuation = gtk4::Box::new(gtk4::Orientation::Horizontal, 24);
        continuation.set_homogeneous(true);
        let columns = [
            gtk4::Box::new(gtk4::Orientation::Vertical, 2),
            gtk4::Box::new(gtk4::Orientation::Vertical, 2),
        ];
        for column in &columns {
            column.set_hexpand(true);
            continuation.append(column);
        }
        let revealer = gtk4::Revealer::new();
        revealer.set_reveal_child(false);
        revealer.set_child(Some(&continuation));
        revealer.set_visible(false);
        revealer.connect_child_revealed_notify(|revealer| {
            if !revealer.is_child_revealed() && !revealer.reveals_child() {
                revealer.set_visible(false);
            }
        });
        root.append(&revealer);

        let state = RankingState {
            bands_row,
            columns,
            rows: Rc::new(RefCell::new(Vec::new())),
            artist_image,
            generation: Rc::new(Cell::new(0)),
            snapshot: Rc::new(RefCell::new(None)),
            sort_by: Rc::new(Cell::new(SortBy::Time)),
            on_open_artist: Rc::new(RefCell::new(None)),
        };

        reveal_button.connect_clicked({
            let state = state.clone();
            let revealer = revealer.clone();
            move |button| {
                let reveal = !revealer.reveals_child();
                state.render(reveal);
                if reveal {
                    revealer.set_visible(true);
                    revealer.set_reveal_child(true);
                } else {
                    revealer.set_reveal_child(false);
                }
                button.set_label(if reveal {
                    "Hide more top artists"
                } else {
                    "Show more top artists"
                });
            }
        });

        sort_toggle.connect_active_name_notify({
            let state = state.clone();
            let reveal_button = reveal_button.clone();
            let revealer = revealer.clone();
            move |toggle| {
                state
                    .sort_by
                    .set(sort_for_toggle_name(toggle.active_name().as_deref()));
                let offer = state.render(revealer.reveals_child());
                reveal_button.set_visible(offer);
                if !offer {
                    revealer.set_reveal_child(false);
                    reveal_button.set_label("Show more top artists");
                }
            }
        });

        Self {
            root,
            state,
            revealer,
            reveal_button,
            sort_toggle,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_data(&self, snapshot: &StatsSnapshot) {
        *self.state.snapshot.borrow_mut() = Some(snapshot.clone());
        let offer = self.state.render(self.revealer.reveals_child());
        self.reveal_button.set_visible(offer);
        if !offer {
            self.revealer.set_reveal_child(false);
            self.reveal_button.set_label("Show more top artists");
        }
    }

    pub(in crate::ui) fn clear_data(&self) {
        *self.state.snapshot.borrow_mut() = None;
        self.state.render(false);
        self.revealer.set_reveal_child(false);
        self.reveal_button.set_label("Show more top artists");
        self.reveal_button.set_visible(false);
    }

    pub(in crate::ui) fn set_on_open_artist(&self, callback: impl Fn(String) + 'static) {
        *self.state.on_open_artist.borrow_mut() = Some(Rc::new(callback));
        self.state.bands_row.set_on_open_artist({
            let callback = self.state.on_open_artist.clone();
            move |artist| invoke(&callback, artist)
        });
    }

    pub(in crate::ui) fn set_on_unify(&self, callback: impl Fn(String) + 'static) {
        self.state.bands_row.set_on_unify(callback);
    }

    pub(super) fn bars(&self) -> Vec<gtk4::LevelBar> {
        self.state.bands_row.bars()
    }

    #[cfg(test)]
    pub(super) fn leader_label(&self) -> String {
        self.state.bands_row.leader_label()
    }

    #[cfg(test)]
    pub(super) fn continuation_rows(&self) -> usize {
        self.state.rows.borrow().len()
    }

    #[cfg(test)]
    pub(super) fn emit_unify(&self, key: &str) {
        self.state.bands_row.leader().emit_unify(key);
    }
}

fn artist_metric(artist: &RankedGroup, sort_by: SortBy) -> i64 {
    match sort_by {
        SortBy::Plays => artist.group.plays,
        SortBy::Time => artist.group.ms,
    }
}

fn sort_for_toggle_name(name: Option<&str>) -> SortBy {
    if name == Some("plays") {
        SortBy::Plays
    } else {
        SortBy::Time
    }
}

fn invoke(callback: &StringCallback, artist: String) {
    let callback = callback.borrow().clone();
    if let Some(callback) = callback {
        callback(artist);
    }
}

#[cfg(test)]
#[path = "stats_bands_card_tests.rs"]
mod tests;
