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
#[cfg(test)]
mod accessibility_semantics;
mod artist_news;
mod browse;
mod compact;
mod cover;
pub mod delete_tracks;
mod device_sync;
mod device_view;
pub mod dialogs;
pub(crate) mod ellipsis_tooltip;
pub(crate) mod eq_bars;
pub(crate) mod file_open;
pub mod first_run;
pub(crate) mod glass;
mod help;
pub mod import_errors_view;
pub(crate) mod info_panel;
mod issues;
mod library_doctor;
mod library_views;
pub(crate) mod link_activation;
mod lyrics;
pub(crate) mod motion;
mod mounts;
pub mod mpris_mirror;
pub(crate) mod nav_history;
mod new_releases;
mod notifications;
pub(crate) mod now_playing;
mod one_shot_task;
mod playback;
pub(crate) mod player_bar;
pub(crate) mod playing_marker;
mod playlists;
mod popover_lifecycle;
pub(crate) mod preferences;
pub mod primary_menu;
mod runtime_performance;
mod scan;
mod scrobbling;
mod scroll_center;
pub mod session_restore;
pub mod shortcuts;
pub(in crate::ui) mod show_in_files;
pub(crate) mod sidebar;
mod stats;
pub mod status_bar;
pub mod strings;
mod style;
mod tag_edit;
mod tag_write_gate;
#[cfg(test)]
pub(crate) mod test_main_context;
pub mod toasts;
#[cfg(test)]
pub(crate) mod tooltip_discipline;
pub(crate) mod track_list;
mod transient_focus;
pub mod view_session;
pub(crate) mod window;
mod window_audio_analysis;

// Compatibility surface for the existing frontend. The ownership of every
// implementation module now lives with its feature directory; these explicit
// imports keep call sites stable while preventing ui/mod.rs from becoming a
// second, flattened module tree again.
#[allow(unused_imports)]
use artist_news::artist_news_worker;
#[allow(unused_imports)]
pub(crate) use browse::browse_bar;
#[allow(unused_imports)]
use browse::{browse_filter_count, browse_filter_strings};
#[allow(unused_imports)]
use compact::{
    compact_mode_controls, compact_player, compact_player_layouts, compact_player_menu,
    compact_player_scroll, minimal_view,
};
#[allow(unused_imports)]
use cover::{cover_download_batch, main_cover_download_progress};
#[allow(unused_imports)]
pub(crate) use cover::{cover_download_worker, cover_loader};
#[allow(unused_imports)]
use device_sync::{
    device_sync_actions, device_sync_backend, device_sync_feedback, device_sync_runtime,
    device_sync_smoke, device_sync_strings,
};
#[allow(unused_imports)]
use library_views::{
    album_card, album_card_actions, album_card_css, album_card_state, album_context_menu,
    album_glow, album_header, album_view, album_view_actions, album_view_memory, album_view_state,
    artist_avatar, artist_detail_hero, artist_detail_pane, artist_detail_row, artist_master,
    artist_master_row, artist_view, artist_view_css, discovery_hint, library_view_css,
};
#[allow(unused_imports)]
use lyrics::{
    lyrics_smoke, lyrics_state, lyrics_strings, lyrics_view, lyrics_worker, player_lyrics,
};
#[allow(unused_imports)]
use now_playing::{artist_portrait_worker, now_playing_column};
#[allow(unused_imports)]
use playback::{audio_effects, play_tracking, player_event_handling, up_next_transport};
#[allow(unused_imports)]
pub(crate) use playback::{
    now_playing_wiring, playback_faults, player_controller, player_controller_wiring,
    queue_transport, session_player,
};
#[allow(unused_imports)]
use player_bar::{
    library_player_bar, player_bar_layout, player_bar_seek, player_bar_state, waveform_seek,
};
#[allow(unused_imports)]
pub(crate) use playlists::playlist_io;
#[allow(unused_imports)]
use playlists::{playlist_import_navigation, playlist_io_names};
#[allow(unused_imports)]
use preferences::{
    preference_appearance, preference_audio_analysis, preference_choice_cards,
    preference_dependencies, preference_effects, preference_lastfm, preference_layout,
    preference_library, preference_listenbrainz, preference_playback, preference_plugins,
    preference_rhythmbox, preference_sync, preference_visual_strings,
    preference_window_decorations, preferences_window,
};
#[allow(unused_imports)]
pub(crate) use scan::{scan_card_css, scan_flow};
#[allow(unused_imports)]
use scan::{scan_controls, scan_progress, scan_watcher, scan_worker};
#[allow(unused_imports)]
use scrobbling::{lastfm_secret, listenbrainz_secret, scrobble_runtime, scrobble_session};
#[allow(unused_imports)]
use sidebar::{
    sidebar_device_card, sidebar_issue_cleanup, sidebar_issue_strings, sidebar_playlist_creation,
    sidebar_presentation, sidebar_rebuild,
};
#[allow(unused_imports)]
pub(crate) use sidebar::{sidebar_dnd, sidebar_export, sidebar_session};
#[allow(unused_imports)]
use stats::{hourly_chart, hourly_chart_math};
#[allow(unused_imports)]
pub(crate) use stats::{stats_css, stats_view};
#[allow(unused_imports)]
use tag_edit::{
    autocomplete_entry, tag_editor_dirty, tag_editor_failures, tag_editor_form, tag_editor_save,
    tag_editor_state, tag_editor_style, tag_editor_widgets,
};
#[allow(unused_imports)]
pub(crate) use tag_edit::{tag_edit_flow, tag_editor};
#[allow(unused_imports)]
use track_list::{
    column_header_dnd, column_layout_editor, column_widths, current_track_selection, list_density,
    track_content, track_cover, track_list_builder, track_list_context_keys,
    track_list_header_style, track_list_layout, track_list_queue_menu, track_list_reload,
    track_list_rescan, track_list_row_interaction,
};
#[allow(unused_imports)]
pub(crate) use track_list::{
    column_layout, rating, track_actions, track_list_activation, track_list_columns,
    track_list_context_menu, track_list_dnd, track_list_dnd_smoke, track_list_model,
    track_list_smoke, track_list_sort,
};
#[allow(unused_imports)]
use window::{
    library_chrome, library_shell, library_view_memory_wiring, navigation_context,
    window_action_wiring, window_decoration_strings, window_decorations, window_navigation,
    window_runtime_wiring, window_smoke,
};
