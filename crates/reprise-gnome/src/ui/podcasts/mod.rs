//! Podcasts source surface.
#![allow(dead_code)]

mod add_dialog;
mod css;
mod podcasts_columns;
mod podcasts_context_menu;
mod podcasts_download_presentation;
mod podcasts_empty_state;
mod podcasts_filter_bar;
mod podcasts_groups;
mod podcasts_model;
mod podcasts_presentation;
mod podcasts_removal;
mod podcasts_scroller;
mod podcasts_view;
mod podcasts_view_data;
mod podcasts_worker;
mod search_results;
pub(crate) mod source_image;

pub(in crate::ui) use podcasts_view::{PodcastsCallbacks, PodcastsView};
pub(in crate::ui) use podcasts_worker::PodcastsRuntime;

pub(in crate::ui) fn install(
    conn: std::rc::Rc<std::cell::RefCell<rusqlite::Connection>>,
    runtime: std::rc::Rc<PodcastsRuntime>,
    callbacks: PodcastsCallbacks,
    kind: reprise_core::podcasts::PodcastKind,
) -> std::rc::Rc<PodcastsView> {
    PodcastsView::install(conn, runtime, callbacks, kind)
}

pub(in crate::ui) fn css() -> String {
    css::css()
}
