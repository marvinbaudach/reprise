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
#[path = "info_panel/artist_news_worker.rs"]
mod artist_news_worker;
#[path = "playback/audio_effects.rs"]
mod audio_effects;
#[path = "browse/browse_bar.rs"]
pub mod browse_bar;
#[path = "browse/browse_filter_count.rs"]
mod browse_filter_count;
#[path = "browse/browse_filter_strings.rs"]
mod browse_filter_strings;
#[path = "track_list/column_header_menu.rs"]
mod column_header_menu;
#[path = "track_list/column_layout.rs"]
pub mod column_layout;
#[path = "track_list/column_layout_editor.rs"]
mod column_layout_editor;
#[path = "compact/compact_mode_controls.rs"]
mod compact_mode_controls;
#[path = "compact/compact_player.rs"]
mod compact_player;
#[path = "compact/compact_player_layouts.rs"]
mod compact_player_layouts;
#[path = "compact/compact_player_menu.rs"]
mod compact_player_menu;
#[path = "compact/compact_player_scroll.rs"]
mod compact_player_scroll;
#[path = "compact/compact_player_state.rs"]
mod compact_player_state;
#[path = "cover/cover_download_batch.rs"]
mod cover_download_batch;
#[path = "cover/cover_download_worker.rs"]
pub mod cover_download_worker;
#[path = "cover/cover_loader.rs"]
pub mod cover_loader;
#[path = "track_list/current_track_selection.rs"]
mod current_track_selection;
pub mod delete_tracks;
#[path = "device_sync/device_sync_backend.rs"]
mod device_sync_backend;
#[path = "device_sync/device_sync_runtime.rs"]
mod device_sync_runtime;
#[cfg(test)]
#[path = "device_sync/device_sync_runtime_tests.rs"]
mod device_sync_runtime_tests;
#[path = "device_sync/device_sync_smoke.rs"]
mod device_sync_smoke;
#[path = "device_sync/device_sync_strings.rs"]
mod device_sync_strings;
pub mod dialogs;
pub(crate) mod file_open;
pub mod first_run;
mod help;
pub mod import_errors_view;
#[path = "info_panel/info_panel.rs"]
mod info_panel;
#[path = "info_panel/info_panel_empty_state.rs"]
mod info_panel_empty_state;
#[path = "info_panel/info_panel_feedback.rs"]
mod info_panel_feedback;
#[path = "info_panel/info_panel_state.rs"]
mod info_panel_state;
#[path = "info_panel/information_column.rs"]
mod information_column;
#[path = "scrobbling/lastfm_secret.rs"]
mod lastfm_secret;
#[path = "window/library_chrome.rs"]
mod library_chrome;
#[path = "player_bar/library_player_bar.rs"]
mod library_player_bar;
#[path = "window/library_shell.rs"]
mod library_shell;
#[path = "track_list/list_density.rs"]
mod list_density;
#[path = "scrobbling/listenbrainz_secret.rs"]
mod listenbrainz_secret;
#[path = "lyrics/lyrics_smoke.rs"]
mod lyrics_smoke;
#[path = "lyrics/lyrics_state.rs"]
mod lyrics_state;
#[path = "lyrics/lyrics_strings.rs"]
mod lyrics_strings;
#[path = "lyrics/lyrics_view.rs"]
mod lyrics_view;
#[path = "lyrics/lyrics_worker.rs"]
mod lyrics_worker;
#[path = "cover/main_cover_download_progress.rs"]
mod main_cover_download_progress;
#[path = "compact/minimal_view.rs"]
mod minimal_view;
pub mod mpris_mirror;
mod notifications;
#[path = "playback/now_playing.rs"]
pub mod now_playing;
#[path = "playback/now_playing_wiring.rs"]
pub mod now_playing_wiring;
#[path = "playback/play_tracking.rs"]
mod play_tracking;
#[path = "playback/playback_faults.rs"]
pub mod playback_faults;
#[path = "player_bar/player_bar.rs"]
pub mod player_bar;
#[path = "player_bar/player_bar_layout.rs"]
mod player_bar_layout;
#[path = "player_bar/player_bar_seek.rs"]
mod player_bar_seek;
#[path = "player_bar/player_bar_state.rs"]
mod player_bar_state;
#[path = "playback/player_controller.rs"]
pub mod player_controller;
#[path = "playback/player_controller_wiring.rs"]
pub mod player_controller_wiring;
#[path = "lyrics/player_lyrics.rs"]
mod player_lyrics;
#[path = "playlists/playlist_import_navigation.rs"]
mod playlist_import_navigation;
#[path = "playlists/playlist_io.rs"]
pub mod playlist_io;
#[path = "playlists/playlist_io_names.rs"]
mod playlist_io_names;
mod popover_lifecycle;
#[path = "preferences/preference_appearance.rs"]
mod preference_appearance;
#[path = "preferences/preference_choice_cards.rs"]
mod preference_choice_cards;
#[path = "preferences/preference_dependencies.rs"]
mod preference_dependencies;
#[path = "preferences/preference_effects.rs"]
mod preference_effects;
#[path = "preferences/preference_lastfm.rs"]
mod preference_lastfm;
#[path = "preferences/preference_layout.rs"]
mod preference_layout;
#[path = "preferences/preference_library.rs"]
mod preference_library;
#[path = "preferences/preference_listenbrainz.rs"]
mod preference_listenbrainz;
#[path = "preferences/preference_playback.rs"]
mod preference_playback;
#[path = "preferences/preference_plugins.rs"]
mod preference_plugins;
#[path = "preferences/preference_rhythmbox.rs"]
mod preference_rhythmbox;
#[path = "preferences/preference_sync.rs"]
mod preference_sync;
#[path = "preferences/preference_visual_strings.rs"]
mod preference_visual_strings;
#[path = "preferences/preference_window_decorations.rs"]
mod preference_window_decorations;
#[path = "preferences/preferences.rs"]
mod preferences;
#[path = "preferences/preferences_window.rs"]
mod preferences_window;
pub mod primary_menu;
#[path = "playback/queue_transport.rs"]
pub mod queue_transport;
#[path = "track_list/rating.rs"]
pub mod rating;
#[path = "scan/scan_flow.rs"]
pub mod scan_flow;
#[path = "scan/scan_progress.rs"]
mod scan_progress;
#[path = "scrobbling/scrobble_runtime.rs"]
mod scrobble_runtime;
#[path = "scrobbling/scrobble_session.rs"]
mod scrobble_session;
#[path = "playback/session_player.rs"]
pub mod session_player;
pub mod session_restore;
pub mod shortcuts;
#[path = "sidebar/sidebar.rs"]
pub mod sidebar;
#[path = "sidebar/sidebar_dnd.rs"]
pub mod sidebar_dnd;
#[path = "sidebar/sidebar_export.rs"]
pub mod sidebar_export;
#[path = "sidebar/sidebar_issue_cleanup.rs"]
mod sidebar_issue_cleanup;
#[path = "sidebar/sidebar_issue_strings.rs"]
mod sidebar_issue_strings;
#[path = "sidebar/sidebar_playlist_creation.rs"]
mod sidebar_playlist_creation;
#[path = "sidebar/sidebar_presentation.rs"]
mod sidebar_presentation;
#[path = "sidebar/sidebar_session.rs"]
pub mod sidebar_session;
pub mod status_bar;
pub mod strings;
mod style;
#[path = "tag_edit/tag_edit_flow.rs"]
pub mod tag_edit_flow;
#[path = "tag_edit/tag_editor.rs"]
pub mod tag_editor;
pub mod toasts;
#[path = "track_list/track_actions.rs"]
pub mod track_actions;
#[path = "track_list/track_content.rs"]
mod track_content;
#[path = "track_list/track_cover.rs"]
mod track_cover;
#[path = "track_list/track_list.rs"]
pub mod track_list;
#[path = "track_list/track_list_activation.rs"]
pub mod track_list_activation;
#[path = "track_list/track_list_columns.rs"]
pub mod track_list_columns;
#[path = "track_list/track_list_context_keys.rs"]
mod track_list_context_keys;
#[path = "track_list/track_list_context_menu.rs"]
pub mod track_list_context_menu;
#[path = "track_list/track_list_dnd.rs"]
pub mod track_list_dnd;
#[path = "track_list/track_list_dnd_smoke.rs"]
pub mod track_list_dnd_smoke;
#[path = "track_list/track_list_header_style.rs"]
mod track_list_header_style;
#[path = "track_list/track_list_layout.rs"]
mod track_list_layout;
#[path = "track_list/track_list_model.rs"]
pub mod track_list_model;
#[path = "track_list/track_list_queue_menu.rs"]
mod track_list_queue_menu;
#[path = "track_list/track_list_rescan.rs"]
mod track_list_rescan;
#[path = "track_list/track_list_row_interaction.rs"]
mod track_list_row_interaction;
#[path = "track_list/track_list_selection.rs"]
mod track_list_selection;
#[path = "track_list/track_list_smoke.rs"]
pub mod track_list_smoke;
#[path = "track_list/track_list_sort.rs"]
pub mod track_list_sort;
#[path = "playback/up_next_transport.rs"]
mod up_next_transport;
pub mod view_session;
#[allow(dead_code)] // Datenschicht ohne UI-Konsumenten bisher
#[path = "playback/waveform_peaks.rs"]
mod waveform_peaks;
#[path = "window/window.rs"]
pub mod window;
#[path = "window/window_decoration_strings.rs"]
mod window_decoration_strings;
#[path = "window/window_decorations.rs"]
mod window_decorations;
#[path = "window/window_navigation.rs"]
mod window_navigation;
#[path = "window/window_smoke.rs"]
mod window_smoke;

#[cfg(test)]
#[path = "lyrics/lyrics_view_tests.rs"]
mod lyrics_view_tests;

#[cfg(test)]
#[path = "lyrics/player_lyrics_tests.rs"]
mod player_lyrics_tests;
