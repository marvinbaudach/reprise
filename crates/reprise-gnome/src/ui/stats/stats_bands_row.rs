//! The most-played-bands row: the leader's hero card plus four runner-up
//! tiles, in a 2 : 1 : 1 : 1 : 1 split across the full page width.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::library::stats_snapshot::SpotlightSection;

#[cfg(test)]
use super::stats_artwork::StatsArtworkSource;
use super::stats_band_card::StatsBandCard;
use super::stats_band_tile::StatsBandTile;
use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;

type StringCallback = Rc<RefCell<Option<Rc<dyn Fn(String)>>>>;

pub(super) const RUNNER_UP_COUNT: usize = 4;
/// The grid is homogeneous and the leader spans two of its columns, which is
/// how 2 : 1 : 1 : 1 : 1 is expressed without fractional widths.
const LEADER_SPAN: i32 = 2;

#[derive(Clone)]
pub(in crate::ui) struct StatsBandsRow {
    root: gtk4::Grid,
    leader: StatsBandCard,
    tiles: Vec<StatsBandTile>,
    on_open_artist: StringCallback,
    on_unify: StringCallback,
}

impl StatsBandsRow {
    pub(in crate::ui) fn new() -> Self {
        let root = gtk4::Grid::new();
        root.add_css_class("stats-bands-row");
        root.set_column_spacing(12);
        root.set_column_homogeneous(true);
        root.set_hexpand(true);

        let on_open_artist: StringCallback = Rc::new(RefCell::new(None));
        let on_unify: StringCallback = Rc::new(RefCell::new(None));

        let leader = StatsBandCard::new();
        leader.forward_callbacks(&on_open_artist, &on_unify);
        root.attach(leader.widget(), 0, 0, LEADER_SPAN, 1);

        let tiles = (0..RUNNER_UP_COUNT)
            .map(|index| {
                let tile = StatsBandTile::new(&on_open_artist, &on_unify);
                root.attach(
                    tile.widget(),
                    LEADER_SPAN + i32::try_from(index).unwrap_or(0),
                    0,
                    1,
                    1,
                );
                tile
            })
            .collect();

        Self {
            root,
            leader,
            tiles,
            on_open_artist,
            on_unify,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Grid {
        &self.root
    }

    pub(in crate::ui) fn set_cover_loader(&self, loader: &Rc<CoverLoader>) {
        self.leader.set_cover_loader(loader.clone());
        for tile in &self.tiles {
            tile.set_cover_loader(loader.clone());
        }
    }

    pub(in crate::ui) fn set_artist_portrait_runtime(&self, runtime: &Rc<ArtistPortraitRuntime>) {
        self.leader.set_artist_portrait_runtime(runtime.clone());
        for tile in &self.tiles {
            tile.set_artist_portrait_runtime(runtime.clone());
        }
    }

    pub(in crate::ui) fn set_on_open_artist(&self, callback: impl Fn(String) + 'static) {
        *self.on_open_artist.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_on_unify(&self, callback: impl Fn(String) + 'static) {
        *self.on_unify.borrow_mut() = Some(Rc::new(callback));
    }

    pub(in crate::ui) fn set_data(&self, section: &SpotlightSection) {
        self.leader.set_data(section);
        let leader_ms = section.artist.group.ms.max(0);
        for (index, tile) in self.tiles.iter().enumerate() {
            match section.also.get(index) {
                Some(ranked) => tile.set_data(index + 2, ranked, leader_ms),
                None => tile.clear_data(),
            }
        }
    }

    pub(in crate::ui) fn clear_data(&self) {
        self.leader.clear_data();
        for tile in &self.tiles {
            tile.clear_data();
        }
    }

    /// The runner-up bars, in rank order — the entrance choreography grows
    /// them from zero (STATS-19).
    pub(super) fn bars(&self) -> Vec<gtk4::LevelBar> {
        self.tiles.iter().map(StatsBandTile::bar).collect()
    }

    #[cfg(test)]
    pub(super) fn leader(&self) -> &StatsBandCard {
        &self.leader
    }

    #[cfg(test)]
    pub(super) fn tiles(&self) -> &[StatsBandTile] {
        &self.tiles
    }
}

#[cfg(test)]
#[path = "stats_bands_row_tests.rs"]
mod tests;
