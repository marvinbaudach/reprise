//! Podcasts source surface.
#![allow(dead_code)]

mod add_dialog;
mod add_dialog_chips;
mod add_dialog_followers;
mod add_dialog_input;
mod add_dialog_results;
mod add_dialog_rows;
mod add_dialog_subscription;
mod css;
mod podcasts_batch_actions;
mod podcasts_callbacks;
mod podcasts_context_menu;
mod podcasts_context_surface;
mod podcasts_deferred_actions;
mod podcasts_dnd;
mod podcasts_download_presentation;
mod podcasts_empty_state;
mod podcasts_episode_files;
mod podcasts_episode_window;
mod podcasts_filter_bar;
mod podcasts_footer;
mod podcasts_groups;
mod podcasts_list_surface;
mod podcasts_model;
mod podcasts_playback;
mod podcasts_presentation;
mod podcasts_refresh_decision;
mod podcasts_removal;
mod podcasts_rendered_order;
mod podcasts_reveal;
mod podcasts_row_interaction;
mod podcasts_row_state;
mod podcasts_scroller;
mod podcasts_selection;
mod podcasts_sync_row;
#[cfg(test)]
#[path = "podcasts_sync_row_display_tests.rs"]
mod podcasts_sync_row_display_tests;
mod podcasts_sync_state;
mod podcasts_title;
mod podcasts_view;
mod podcasts_view_data;
mod podcasts_worker;
pub(crate) mod source_image;
mod youtube_channel_detail;

pub(in crate::ui) use add_dialog_chips::dialog_country;
pub(in crate::ui) use podcasts_callbacks::PodcastsCallbacks;
pub(in crate::ui) use podcasts_playback::{episode_mark_from_snapshot, EpisodeMark};
pub(in crate::ui) use podcasts_refresh_decision::{scope_status, RefreshWindow, ScopeStatus};
pub(in crate::ui) use podcasts_view::PodcastsView;
pub(in crate::ui) use podcasts_worker::PodcastsRuntime;

pub(in crate::ui) fn install(
    conn: std::rc::Rc<reprise_core::db::Db>,
    runtime: std::rc::Rc<PodcastsRuntime>,
    callbacks: PodcastsCallbacks,
    kind: reprise_core::podcasts::PodcastKind,
) -> std::rc::Rc<PodcastsView> {
    PodcastsView::install(conn, runtime, callbacks, kind)
}

pub(in crate::ui) fn css() -> String {
    css::css()
}

fn metadata_ytdlp(
    setting_path: Option<&str>,
    browser: Option<reprise_core::podcasts::config::YoutubeBrowser>,
) -> reprise_core::podcasts::ytdlp::YtDlp {
    reprise_core::podcasts::ytdlp::YtDlp::discover_with_browser(setting_path, browser)
        .with_metadata_language(crate::i18n::active_gui_language())
}
