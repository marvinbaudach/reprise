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

mod audio_effects;
pub mod browse_bar;
pub mod column_layout;
mod column_layout_editor;
mod compact_player;
mod compact_player_state;
mod cover_download_batch;
pub mod cover_download_worker;
pub mod cover_loader;
mod current_track_selection;
pub mod delete_tracks;
pub mod dialogs;
pub mod first_run;
pub mod import_errors_view;
mod listenbrainz_runtime;
mod main_cover_download_progress;
mod minimal_view;
pub mod mpris_mirror;
mod notifications;
pub mod now_playing;
pub mod now_playing_wiring;
mod play_tracking;
pub mod playback_faults;
pub mod player_bar;
mod player_bar_seek;
mod player_bar_state;
pub mod player_controller;
pub mod player_controller_wiring;
mod playlist_import_navigation;
pub mod playlist_io;
mod playlist_io_names;
mod popover_lifecycle;
mod preference_cover_download;
mod preference_effects;
mod preference_library;
mod preferences;
pub mod primary_menu;
pub mod queue_transport;
pub mod rating;
pub mod scan_flow;
mod scan_progress;
mod scrobble_session;
pub mod session_player;
pub mod session_restore;
pub mod shortcuts;
pub mod sidebar;
pub mod sidebar_dnd;
pub mod sidebar_export;
mod sidebar_playlist_creation;
pub mod sidebar_session;
pub mod status_bar;
pub mod strings;
pub mod tag_edit_flow;
pub mod tag_editor;
pub mod toasts;
pub mod track_actions;
pub mod track_list;
pub mod track_list_activation;
pub mod track_list_columns;
mod track_list_context_keys;
pub mod track_list_context_menu;
pub mod track_list_dnd;
pub mod track_list_dnd_smoke;
pub mod track_list_model;
mod track_list_rescan;
mod track_list_row_interaction;
pub mod track_list_smoke;
pub mod track_list_sort;
pub mod view_session;
pub mod window;
mod window_navigation;
mod window_smoke;
