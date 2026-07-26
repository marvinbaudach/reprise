//! Podcasts source surface.
#![allow(dead_code)]

mod add_dialog;
mod css;
mod podcasts_columns;
mod podcasts_context_menu;
mod podcasts_empty_state;
mod podcasts_filter_bar;
mod podcasts_model;
mod podcasts_presentation;
mod podcasts_view;
mod podcasts_worker;

pub(in crate::ui) use podcasts_view::{PodcastsCallbacks, PodcastsView};
pub(in crate::ui) use podcasts_worker::PodcastsRuntime;

pub(in crate::ui) fn install(
    conn: std::rc::Rc<std::cell::RefCell<rusqlite::Connection>>,
    runtime: std::rc::Rc<PodcastsRuntime>,
    callbacks: PodcastsCallbacks,
) -> std::rc::Rc<PodcastsView> {
    PodcastsView::install(conn, runtime, callbacks)
}

pub(in crate::ui) fn css() -> String {
    css::css()
}
