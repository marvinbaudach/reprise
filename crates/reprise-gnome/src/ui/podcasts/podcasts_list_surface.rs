//! FIL-3a list-page overlay wiring shared by Podcasts and YouTube.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::podcasts::PodcastKind;

use super::podcasts_filter_bar::PodcastsFilterBar;
use super::podcasts_presentation::PodcastFilter;
use super::podcasts_scroller::build_episode_scroller;
use crate::ui::end_of_results::{EndOfResults, EndOfResultsInput, ResultsUnit};

pub(super) fn build(
    kind: PodcastKind,
    filter_bar: &Rc<PodcastsFilterBar>,
) -> (
    gtk4::Box,
    gtk4::ScrolledWindow,
    gtk4::Overlay,
    Rc<EndOfResults>,
) {
    let group_container = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    group_container.set_margin_top(8);
    group_container.set_margin_bottom(8);
    group_container.set_margin_start(12);
    group_container.set_margin_end(12);
    group_container.set_hexpand(true);
    let scroller = build_episode_scroller(group_container.upcast_ref());
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&scroller));
    let unit = match kind {
        PodcastKind::Rss => ResultsUnit::Episodes,
        PodcastKind::Youtube => ResultsUnit::Videos,
    };
    let end_of_results = EndOfResults::install(&overlay, &scroller, &group_container, unit);
    {
        let filter_bar = filter_bar.clone();
        end_of_results.connect_recover(move || filter_bar.clear_all());
    }
    (group_container, scroller, overlay, end_of_results)
}

pub(super) fn update(
    end_of_results: &Rc<EndOfResults>,
    filter: &PodcastFilter,
    shown: usize,
    total: usize,
) {
    end_of_results.update(EndOfResultsInput {
        shown,
        total,
        query: filter.query.clone(),
        facets_restrict: filter.unplayed_only || filter.source.is_some() || filter.downloaded_only,
    });
}
