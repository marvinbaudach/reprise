//! Cross-feature action wiring extracted from the main-window composition root.

use std::cell::RefCell;
use std::path::Path;
use std::rc::{Rc, Weak};

use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use reprise_core::library::watcher::WatcherHandle;
use reprise_core::library::{group_key::GroupKind, playlists, stats_screen::group_track_ids};
use reprise_core::view_source::ViewSource;

use super::player_controller::PlayerController;
use super::scan_flow::ScanControls;
use super::sidebar::Sidebar;
use super::stats_view::StatsView;
use super::track_list::TrackList;
use crate::ui::nav_history::{NavHistory, NavPlace};
use crate::ui::playback::play_origin;
use crate::ui::stats::stats_highlights::TopGenre;

#[derive(Clone, Copy)]
pub(in crate::ui) struct ActionWiring<'a> {
    pub(in crate::ui) conn: &'a Rc<RefCell<Connection>>,
    pub(in crate::ui) db_path: &'a Path,
    pub(in crate::ui) window: &'a adw::ApplicationWindow,
    pub(in crate::ui) toast_overlay: &'a adw::ToastOverlay,
    pub(in crate::ui) track_list: &'a Rc<TrackList>,
    pub(in crate::ui) sidebar: &'a Rc<Sidebar>,
    pub(in crate::ui) player: &'a Option<Rc<PlayerController>>,
    pub(in crate::ui) stats_view: &'a StatsView,
    pub(in crate::ui) nav_history: &'a Rc<NavHistory>,
    pub(in crate::ui) content_stack: &'a gtk4::Stack,
    pub(in crate::ui) scan_controls: &'a ScanControls,
    pub(in crate::ui) watcher_state: &'a Rc<RefCell<Option<WatcherHandle>>>,
}

pub(in crate::ui) fn wire(context: ActionWiring<'_>) {
    let ActionWiring {
        conn,
        db_path,
        window,
        toast_overlay,
        track_list,
        sidebar,
        player,
        stats_view,
        nav_history,
        content_stack,
        scan_controls,
        watcher_state,
    } = context;

    // Stage 2 Task 5 fault-tolerance seam: the toast overlay and the track
    // list are both built after the controller (see `PlayerController::
    // new`'s call above and the module doc comment on `set_toast_overlay`/
    // `set_track_list_reload`), so they're injected here instead of being
    // constructor parameters. The reload closure captures `Weak<TrackList>`/
    // `Weak<Sidebar>` — never strong `Rc`s — so the controller can't form an
    // `Rc` cycle with `track_list`'s own strong `Rc<PlayerController>` (held
    // by its `on_activate` closure). This is also sidebar-refresh trigger #3
    // from `Sidebar::refresh`'s doc comment (Stage 3 Task 4 review finding
    // #2c): `PlayerController::reload_track_list` is called from exactly one
    // place — `playback_faults.rs`'s `handle_unplayable_track`, after a
    // successful `mark_track_missing` — so refreshing the sidebar here,
    // alongside the track-list reload, is the specific "Missing badge can
    // have changed" hook rather than a blanket one.
    if let Some(player) = &player {
        player.set_toast_overlay(toast_overlay);
        let track_list_weak = Rc::downgrade(track_list);
        let sidebar_weak = Rc::downgrade(sidebar);
        player.set_track_list_reload(move || {
            match track_list_weak.upgrade() {
                Some(track_list) => track_list.reload(),
                None => tracing::warn!("track list reload skipped: track list is gone"),
            }
            match sidebar_weak.upgrade() {
                Some(sidebar) => sidebar.refresh("track marked missing"),
                None => tracing::warn!("sidebar refresh skipped: sidebar is gone"),
            }
        });
        let stats_view = stats_view.clone();
        let stats_conn = conn.clone();
        let content_stack = content_stack.downgrade();
        player.set_on_listen_event_recorded(move || {
            let Some(content_stack) = content_stack.upgrade() else {
                return;
            };
            if content_stack.visible_child_name().as_deref() == Some("stats") {
                stats_view.refresh(&stats_conn);
            }
        });
    }
    // Stage 3 Task 1 backlog item (a): same post-construction injection
    // reason as the player's toast overlay above — `track_list` is built
    // before `toast_overlay` exists.
    track_list.set_toast_overlay(toast_overlay);
    // Embed a lightweight scan-progress indicator in the empty-library status
    // page so the user sees scanning feedback during a first scan (before any
    // tracks are in the list). Created here — after both `track_list` and
    // `scan_controls` exist — and wired in both directions.
    {
        let empty_indicator = super::scan_progress::EmptyScanIndicator::new();
        track_list.set_empty_scan_widget(empty_indicator.widget());
        scan_controls.set_empty_indicator(&empty_indicator);
    }
    // Same reason again: the sidebar is built before `toast_overlay` exists.
    sidebar.set_toast_overlay(toast_overlay);
    {
        let track_list = Rc::downgrade(track_list);
        sidebar.set_on_remove_missing(move |ids| match track_list.upgrade() {
            Some(track_list) => track_list.remove_missing_with_undo(ids),
            None => tracing::warn!("track list is gone; skipping Missing-files bulk removal"),
        });
    }
    {
        // Dropping tracks onto the sidebar's Queue row appends them, exactly
        // like the context menu's "Add to queue" action wired below — same
        // decoupling-via-closure seam, same degraded-no-op convention (no
        // player at all reports `false` rather than a false "appended").
        let player = player.clone();
        sidebar.set_on_queue_drop(move |ids| match &player {
            Some(player) => {
                player.append_to_queue(ids);
                true
            }
            None => {
                tracing::warn!("player unavailable; ignoring queue drop");
                false
            }
        });
    }
    super::tag_edit_flow::wire_refresh(track_list, sidebar, player);

    {
        let player = player.as_ref().map(Rc::downgrade);
        let conn = conn.clone();
        stats_view.set_on_spotlight_play(move |artist, key| {
            let Some(player) = player.as_ref().and_then(Weak::upgrade) else {
                return;
            };
            match stats_spotlight_track_ids(&conn, &key) {
                Ok(ids) if !ids.is_empty() => {
                    player.play_from_view(ids, 0, play_origin::from_artist(&artist));
                }
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "stats spotlight track query failed"),
            }
        });
    }
    {
        let track_list = Rc::downgrade(track_list);
        let content_stack = content_stack.clone();
        let nav_history = nav_history.clone();
        stats_view.set_on_go_to_artist(move |artist| {
            let Some(track_list) = track_list.upgrade() else {
                return;
            };
            let source = ViewSource::Artist(artist);
            nav_history.record_route_from(
                &NavPlace::source(source.clone()),
                track_list.browser_place(),
            );
            track_list.set_source(source);
            content_stack.set_visible_child_name("library");
        });
    }
    {
        let conn = conn.clone();
        let sidebar = Rc::downgrade(sidebar);
        stats_view.set_on_create_smart_mix(move |genre| {
            let created = {
                let mut conn = conn.borrow_mut();
                create_stats_smart_mix(&mut conn, &genre)
            };
            match created {
                Ok(Some(source)) => {
                    if let Some(sidebar) = sidebar.upgrade() {
                        sidebar.refresh_and_select(source, "stats smart mix created");
                    }
                }
                Ok(None) => {}
                Err(error) => tracing::warn!(%error, "stats smart mix creation failed"),
            }
        });
    }
    // Stage 3 Task 5: context menu action wiring. `track_list` stays
    // decoupled from `PlayerController`/`Sidebar` themselves (same
    // decoupling-via-closure seam as `on_activate`/`queue_ids_provider`
    // above) — these closures are the only place that bridges them.
    // `window` already exists (built at the top of this function), so `set_
    // window` could technically be a constructor parameter, but every other
    // post-construction seam on `track_list` is wired here too, so this
    // keeps all of them in one place.
    track_list.set_window(window);
    track_list.set_missing_relink_db_path(db_path.to_path_buf());
    {
        let sidebar = Rc::downgrade(sidebar);
        track_list.set_on_missing_relink_progress_activate(move |target| {
            if let Some(sidebar) = sidebar.upgrade() {
                sidebar.refresh_and_select(target, "relink progress card");
            }
        });
    }
    // Wire player for tag-edit flow to refresh now-playing metadata
    if let Some(player) = &player {
        track_list.set_player(player);
    }
    {
        let player = player.clone();
        track_list.set_on_play_next_selected(move |ids| match &player {
            Some(player) => player.play_next(&ids),
            None => {
                tracing::warn!("player unavailable; ignoring play-next action");
            }
        });
    }
    {
        let player = player.clone();
        track_list.set_on_queue_selected(move |ids| match &player {
            Some(player) => player.append_to_queue(&ids),
            None => {
                tracing::warn!("player unavailable; ignoring context menu add-to-queue action");
            }
        });
    }
    {
        let sidebar_weak = Rc::downgrade(sidebar);
        track_list.set_on_show_missing(move |target| match sidebar_weak.upgrade() {
            Some(sidebar) => sidebar.refresh_and_select(target, "missing row details"),
            None => tracing::warn!("sidebar is gone; cannot show Missing files"),
        });
    }
    {
        let player = player.clone();
        track_list.set_on_queue_activate(move |row| {
            if let Some(player) = &player {
                player.jump_to_queue_row(row);
            }
        });
    }
    {
        let player = player.clone();
        track_list.set_on_queue_remove(move |rows| {
            player
                .as_ref()
                .map_or(0, |player| player.remove_queue_rows(rows))
        });
    }
    {
        let player = player.clone();
        track_list.set_on_queue_move_to_top(move |rows| {
            player
                .as_ref()
                .map_or(0, |player| player.move_queue_rows_to_top(rows))
        });
    }
    {
        let track_list_weak = Rc::downgrade(track_list);
        let sidebar_weak = Rc::downgrade(sidebar);
        let content_stack = content_stack.downgrade();
        track_list.set_on_go_to_album(move |album, album_artist| {
            let Some(track_list) = track_list_weak.upgrade() else {
                return;
            };
            let source = ViewSource::Album {
                album,
                album_artist,
            };
            if let Some(sidebar) = sidebar_weak.upgrade() {
                crate::ui::sidebar_session::sync_current_source(&sidebar.shared, &source);
            }
            track_list.set_source(source);
            if let Some(content_stack) = content_stack.upgrade() {
                content_stack.set_visible_child_name("library");
            }
        });
    }
    {
        let track_list_weak = Rc::downgrade(track_list);
        let sidebar_weak = Rc::downgrade(sidebar);
        let content_stack = content_stack.downgrade();
        track_list.set_on_go_to_artist(move |artist| {
            let Some(track_list) = track_list_weak.upgrade() else {
                return;
            };
            let source = ViewSource::Artist(artist);
            if let Some(sidebar) = sidebar_weak.upgrade() {
                crate::ui::sidebar_session::sync_current_source(&sidebar.shared, &source);
            }
            track_list.set_source(source);
            if let Some(content_stack) = content_stack.upgrade() {
                content_stack.set_visible_child_name("library");
            }
        });
    }
    {
        let track_list_weak = Rc::downgrade(track_list);
        let sidebar_weak = Rc::downgrade(sidebar);
        track_list.set_on_show_missing_files(move || {
            let Some(sidebar) = sidebar_weak.upgrade() else {
                return;
            };
            if let Some(track_list) = track_list_weak.upgrade() {
                crate::ui::sidebar_session::sync_current_source(
                    &sidebar.shared,
                    &track_list.current_source(),
                );
            }
            sidebar.refresh_and_select(ViewSource::Missing, "track context menu");
        });
    }
    {
        // Stage 3 Task 6: queue drag-reorder — see `ui::track_list_dnd`'s
        // doc comment. Same decoupling-via-closure seam as `on_queue_
        // selected` just above.
        let player = player.clone();
        track_list.set_on_queue_reorder(move |op| match &player {
            Some(player) => player.reorder_queue_rows(op),
            None => {
                tracing::warn!("player unavailable; ignoring queue drag-reorder");
                false
            }
        });
    }
    {
        // `Weak`, not a strong `Rc`: mirrors the `sidebar_weak`/`track_list_
        // weak` pattern already used for `player.set_track_list_reload`
        // just above — `track_list` must not keep `sidebar` alive past its
        // natural lifetime.
        let sidebar_weak = Rc::downgrade(sidebar);
        track_list.set_on_playlist_mutated(move || match sidebar_weak.upgrade() {
            Some(sidebar) => sidebar.refresh("context menu playlist change"),
            None => tracing::warn!(
                "sidebar is gone; skipping refresh after context menu playlist change"
            ),
        });
    }
    {
        // Missing-view tombstone/Undo sends an empty id slice for the
        // immediate sidebar refresh. Only committed expiry/auto-clean sends
        // hard-purged ids, which are then removed from the playback queue.
        let sidebar_weak = Rc::downgrade(sidebar);
        let player = player.clone();
        let scan_player = player.clone();
        track_list.set_on_scan_queue_purge_ids(move || {
            scan_player
                .as_ref()
                .map_or_else(Vec::new, |player| player.scan_queue_purge_ids())
        });
        track_list.set_on_library_mutated(move |removed_ids| {
            match sidebar_weak.upgrade() {
                Some(sidebar) => sidebar.refresh("track removed from library"),
                None => {
                    tracing::warn!("sidebar is gone; skipping refresh after a library removal");
                }
            }
            if let Some(player) = &player {
                player.purge_queue_ids(removed_ids);
            }
        });
    }
    {
        // Stage 3 Task 8: the ImportErrors source's own Retry/Dismiss
        // actions change the Import-errors badge count — a fifth sidebar-
        // refresh trigger alongside scan completion, playlist CRUD,
        // missing-marking, and context-menu playlist mutation (see `Sidebar
        // ::refresh`'s doc comment).
        let sidebar_weak = Rc::downgrade(sidebar);
        track_list.set_on_import_errors_mutated(move || match sidebar_weak.upgrade() {
            Some(sidebar) => sidebar.refresh("import error mutated"),
            None => {
                tracing::warn!("sidebar is gone; skipping refresh after an import-error mutation");
            }
        });
    }
    {
        // Stage 3 Task 8: "Rescan library" (Missing source context menu)
        // re-runs the persisted library root through the exact same scan
        // flow "Scan folder…" uses — see `trigger_rescan_of_library_root`.
        // `track_list` stays decoupled from the scan machinery/settings
        // table itself, same decoupling-via-closure seam as the queue
        // callbacks above.
        let conn = conn.clone();
        let scan_controls = scan_controls.clone();
        let toast_overlay = toast_overlay.clone();
        let db_path = db_path.to_path_buf();
        let track_list_for_rescan = track_list.clone();
        let sidebar_for_rescan = sidebar.clone();
        let watcher_state = watcher_state.clone();
        track_list.set_on_rescan_library(move || {
            super::scan_flow::trigger_rescan_of_library_root(
                &conn,
                &scan_controls,
                &toast_overlay,
                db_path.clone(),
                track_list_for_rescan.clone(),
                sidebar_for_rescan.clone(),
                &watcher_state,
            );
        });
    }
}

fn stats_spotlight_track_ids(
    conn: &Rc<RefCell<Connection>>,
    key: &str,
) -> Result<Vec<i64>, rusqlite::Error> {
    group_track_ids(&conn.borrow(), GroupKind::Artist, key)
}

/// Builds the mix the My Stats CTA promises, starting from the genre group's
/// **key** — the displayed label is only the group's most common raw spelling
/// (STATS-9), and `genre = '<label>'` misses every other spelling because
/// `tracks.genre` has no `COLLATE NOCASE`.
///
/// The smart-rule engine joins its rules with `AND` and knows neither `OR` nor
/// `IN` (`playlists::smart_rules_to_sql`), so it can only express a genre
/// group that has exactly one spelling. That is the common case and it becomes
/// a real, self-updating smart playlist. When the group folds several
/// spellings, no rule set can express it: the mix is then created as a regular
/// playlist holding exactly the group's tracks, so the playlist and the number
/// on screen agree instead of quietly disagreeing.
fn create_stats_smart_mix(
    conn: &mut Connection,
    genre: &TopGenre,
) -> Result<Option<ViewSource>, rusqlite::Error> {
    if genre.key.trim().is_empty() {
        return Ok(None);
    }
    let ids = group_track_ids(conn, GroupKind::Genre, &genre.key)?;
    if ids.is_empty() {
        tracing::warn!(key = %genre.key, "stats smart mix found no tracks for the genre group");
        return Ok(None);
    }
    let name = format!("My Stats \u{2014} {} Mix", genre.label);
    let spellings = group_genre_spellings(conn, &ids)?;
    let Some(single) = spellings.first().filter(|_| spellings.len() == 1) else {
        return playlists::create_with_tracks(conn, &name, &ids)
            .map(|id| Some(ViewSource::Playlist(id)));
    };
    let rules = serde_json::json!([{
        "field": "genre",
        "op": "=",
        "value": single,
    }])
    .to_string();
    playlists::create_smart(conn, &name, &rules, "play_count", "desc", Some(50))
        .map(|id| Some(ViewSource::Smart(id)))
}

/// The distinct raw `genre` spellings behind a set of tracks — the values a
/// rule would have to match.
fn group_genre_spellings(
    conn: &Connection,
    track_ids: &[i64],
) -> Result<Vec<String>, rusqlite::Error> {
    let placeholders = vec!["?"; track_ids.len()].join(",");
    let mut statement = conn.prepare(&format!(
        "SELECT DISTINCT genre FROM tracks \
         WHERE id IN ({placeholders}) AND TRIM(COALESCE(genre, '')) <> '' \
         ORDER BY genre"
    ))?;
    let spellings = statement
        .query_map(rusqlite::params_from_iter(track_ids), |row| row.get(0))?
        .collect::<Result<Vec<String>, _>>()?;
    Ok(spellings)
}

#[cfg(test)]
#[path = "window_action_wiring_tests.rs"]
mod stats_tests;
