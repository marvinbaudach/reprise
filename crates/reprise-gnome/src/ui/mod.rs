//! # GTK/libadwaita containment policy (refactor stage, 2026-07)
//!
//! - `reprise-core` never sees gtk4/libadwaita — and never sees gstreamer
//!   or zbus either: those live in `reprise-platform-linux` behind core's
//!   `playback`/`media_integration` contracts. Both boundaries are
//!   enforced by the crate graph, not by convention
//!   (`cargo tree -p reprise-core` is the proof).
//! - Inside this frontend, adw *structural* widgets (NavigationSplitView,
//!   ToolbarView, HeaderBar, StatusPage, WindowTitle) are used directly and
//!   deliberately unwrapped: an adw major-version port rewrites layout code
//!   wholesale, and each of these types lives in at most two files — a
//!   mirror-wrapper would only add a second rewrite site.
//! - Repeated adw *patterns* are funneled: plain toasts via `toasts::show`,
//!   name-prompt dialogs via `dialogs::prompt_name`. Add the next funnel
//!   only when a third call site repeats a shape.
//! - adw/gtk types must not appear in function signatures of modules whose
//!   job is not widgetry (e.g. `track_actions` takes ids and callbacks, not
//!   widgets) — keeps porting cost proportional to the widget layer only.
//! - Platform concretes (`reprise_platform_linux::…`) are named at the
//!   composition root only (controller construction); everything else goes
//!   through `reprise_core::playback` / `reprise_core::media_integration`.

mod about;
mod artist_news_worker;
mod audio_effects;
pub mod browse_bar;
mod browse_filter_count;
mod browse_filter_strings;
mod column_header_menu;
pub mod column_layout;
mod column_layout_editor;
mod compact_mode_controls;
mod compact_player;
mod compact_player_layouts;
mod compact_player_menu;
mod compact_player_scroll;
mod compact_player_state;
mod cover_download_batch;
pub mod cover_download_worker;
pub mod cover_loader;
mod current_track_selection;
pub mod delete_tracks;
mod device_sync_backend;
mod device_sync_runtime;
#[cfg(test)]
mod device_sync_runtime_tests;
mod device_sync_smoke;
mod device_sync_strings;
pub mod dialogs;
pub(crate) mod file_open;
pub mod first_run;
mod help;
pub mod import_errors_view;
mod info_panel;
mod info_panel_empty_state;
mod info_panel_feedback;
mod info_panel_state;
mod lastfm_secret;
mod library_chrome;
mod library_player_bar;
mod library_shell;
mod list_density;
mod listenbrainz_secret;
mod lyrics_smoke;
mod lyrics_state;
mod lyrics_strings;
mod lyrics_view;
mod lyrics_worker;
mod main_cover_download_progress;
mod minimal_view;
pub mod mpris_mirror;
mod notifications;
pub mod now_playing;
pub mod now_playing_wiring;
mod play_tracking;
pub mod playback_faults;
pub mod player_bar;
mod player_bar_layout;
mod player_bar_seek;
mod player_bar_state;
pub mod player_controller;
pub mod player_controller_wiring;
mod player_lyrics;
mod playlist_import_navigation;
pub mod playlist_io;
mod playlist_io_names;
mod popover_lifecycle;
mod preference_appearance;
mod preference_choice_cards;
mod preference_dependencies;
mod preference_effects;
mod preference_lastfm;
mod preference_layout;
mod preference_library;
mod preference_listenbrainz;
mod preference_playback;
mod preference_plugins;
mod preference_sync;
mod preference_visual_strings;
mod preference_window_decorations;
mod preferences;
mod preferences_window;
pub mod primary_menu;
pub mod queue_transport;
pub mod rating;
pub mod scan_flow;
mod scan_progress;
mod scrobble_runtime;
mod scrobble_session;
pub mod session_player;
pub mod session_restore;
pub mod shortcuts;
pub mod sidebar;
pub mod sidebar_dnd;
pub mod sidebar_export;
mod sidebar_issue_cleanup;
mod sidebar_issue_strings;
mod sidebar_playlist_creation;
mod sidebar_presentation;
pub mod sidebar_session;
pub mod status_bar;
pub mod strings;
pub mod tag_edit_flow;
pub mod tag_editor;
pub mod toasts;
pub mod track_actions;
mod track_content;
mod track_cover;
pub mod track_list;
pub mod track_list_activation;
pub mod track_list_columns;
mod track_list_context_keys;
pub mod track_list_context_menu;
pub mod track_list_dnd;
pub mod track_list_dnd_smoke;
mod track_list_layout;
pub mod track_list_model;
mod track_list_queue_menu;
mod track_list_rescan;
mod track_list_row_interaction;
mod track_list_selection;
pub mod track_list_smoke;
pub mod track_list_sort;
mod up_next_transport;
pub mod view_session;
pub mod window;
mod window_decoration_strings;
mod window_decorations;
mod window_navigation;
mod window_smoke;

#[cfg(test)]
mod lyrics_view_tests;

#[cfg(test)]
mod player_lyrics_tests;
