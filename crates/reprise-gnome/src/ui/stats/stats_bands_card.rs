//! One expandable card for the complete My Stats artist ranking (STATS-23).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::stats_screen::RankedGroup;
use reprise_core::library::stats_snapshot::{SortBy, StatsSnapshot};

use super::stats_artist_image::StatsArtistImage;
use super::stats_bands_more::{self, ContinuationCallbacks, ContinuationRow};
use super::stats_bands_row::{StatsBandsRow, RUNNER_UP_COUNT};
use super::stats_view_widgets::clear;
use crate::ui::strings;

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
    on_unify: StringCallback,
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
        let leader_metric = artists.first().map_or(0, |artist| {
            super::stats_bands_row::artist_metric(artist, sort_by)
        });
        let continuation = artists
            .iter()
            .skip(RUNNER_UP_COUNT + 1)
            .take(ARTIST_ROW_EXTRA)
            .collect::<Vec<_>>();
        let first_column_rows = continuation.len().div_ceil(2);
        let mut rendered_rows = Vec::with_capacity(continuation.len());
        for (offset, artist) in continuation.into_iter().enumerate() {
            let open_callback = self.on_open_artist.clone();
            let unify_callback = self.on_unify.clone();
            let row = stats_bands_more::build_row(
                offset + first_continuation_rank(),
                artist,
                leader_metric,
                sort_by,
                &self.artist_image,
                &generation,
                ContinuationCallbacks {
                    open_artist: Rc::new(move |artist| invoke(&open_callback, artist)),
                    unify: Rc::new(move |key| invoke(&unify_callback, key)),
                },
            );
            let column = usize::from(offset >= first_column_rows);
            self.columns[column].append(&row.root);
            rendered_rows.push(row);
        }
        *self.rows.borrow_mut() = rendered_rows;
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
            .label(strings::stats_sort_by_plays())
            .build();
        let time_sort = adw::Toggle::builder()
            .name("time")
            .label(strings::stats_sort_by_time())
            .build();
        let sort_toggle = adw::ToggleGroup::new();
        sort_toggle.add(plays_sort);
        sort_toggle.add(time_sort);
        sort_toggle.set_active_name(Some("time"));
        sort_toggle.set_halign(gtk4::Align::End);
        sort_toggle.update_property(&[gtk4::accessible::Property::Label(
            &strings::stats_sort_top_artists(),
        )]);

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);
        header.append(&sort_toggle);
        root.append(&header);

        let bands_row = StatsBandsRow::new();
        bands_row.set_artist_image(&artist_image);
        root.append(bands_row.widget());

        let reveal_button = gtk4::Button::with_label(&strings::stats_show_more_top_artists());
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
        reveal_button.update_relation(&[gtk4::accessible::Relation::Controls(&[
            revealer.upcast_ref()
        ])]);
        reveal_button.update_state(&[gtk4::accessible::State::Expanded(Some(false))]);

        let state = RankingState {
            bands_row,
            columns,
            rows: Rc::new(RefCell::new(Vec::new())),
            artist_image,
            generation: Rc::new(Cell::new(0)),
            snapshot: Rc::new(RefCell::new(None)),
            sort_by: Rc::new(Cell::new(SortBy::Time)),
            on_open_artist: Rc::new(RefCell::new(None)),
            on_unify: Rc::new(RefCell::new(None)),
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
                update_reveal_button(button, reveal);
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
                    update_reveal_button(&reveal_button, false);
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
            update_reveal_button(&self.reveal_button, false);
        }
    }

    #[cfg(test)]
    pub(super) fn artwork_generations_for_test(&self) -> Vec<u64> {
        self.state.bands_row.artwork_generations_for_test()
    }

    pub(in crate::ui) fn clear_data(&self) {
        *self.state.snapshot.borrow_mut() = None;
        self.state.render(false);
        self.revealer.set_reveal_child(false);
        update_reveal_button(&self.reveal_button, false);
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
        *self.state.on_unify.borrow_mut() = Some(Rc::new(callback));
        self.state.bands_row.set_on_unify({
            let callback = self.state.on_unify.clone();
            move |key| invoke(&callback, key)
        });
    }

    pub(super) fn bars(&self) -> Vec<gtk4::LevelBar> {
        self.state.bands_row.bars()
    }

    #[cfg(test)]
    pub(super) fn leader_label(&self) -> String {
        self.state.bands_row.leader_label()
    }

    #[cfg(test)]
    pub(super) fn leader_summary(&self) -> String {
        self.state.bands_row.leader_summary()
    }

    #[cfg(test)]
    pub(super) fn runner_up_labels(&self) -> Vec<String> {
        self.state.bands_row.runner_up_labels()
    }

    #[cfg(test)]
    pub(super) fn continuation_labels(&self) -> Vec<String> {
        self.state
            .rows
            .borrow()
            .iter()
            .map(ContinuationRow::artist_label)
            .collect()
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

fn sort_for_toggle_name(name: Option<&str>) -> SortBy {
    if name == Some("plays") {
        SortBy::Plays
    } else {
        SortBy::Time
    }
}

fn update_reveal_button(button: &gtk4::Button, expanded: bool) {
    button.set_label(&if expanded {
        strings::stats_hide_more_top_artists()
    } else {
        strings::stats_show_more_top_artists()
    });
    button.update_state(&[gtk4::accessible::State::Expanded(Some(expanded))]);
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
