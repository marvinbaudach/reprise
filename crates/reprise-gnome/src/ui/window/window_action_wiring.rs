//! Cross-feature action wiring extracted from the main-window composition root.

use std::cell::RefCell;
use std::path::Path;
use std::rc::{Rc, Weak};

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use reprise_core::library::watcher::WatcherHandle;
use reprise_core::view_source::ViewSource;

use super::album_view::AlbumView;
use super::artist_view::ArtistView;
use super::player_controller::PlayerController;
use super::scan_flow::ScanControls;
use super::sidebar::Sidebar;
use super::track_list::TrackList;
use crate::ui::playback::play_origin;

#[derive(Clone, Copy)]
pub(in crate::ui) struct ActionWiring<'a> {
    pub(in crate::ui) conn: &'a Rc<RefCell<Connection>>,
    pub(in crate::ui) db_path: &'a Path,
    pub(in crate::ui) window: &'a adw::ApplicationWindow,
    pub(in crate::ui) toast_overlay: &'a adw::ToastOverlay,
    pub(in crate::ui) track_list: &'a Rc<TrackList>,
    pub(in crate::ui) sidebar: &'a Rc<Sidebar>,
    pub(in crate::ui) album_view: &'a AlbumView,
    pub(in crate::ui) artist_view: &'a Rc<ArtistView>,
    pub(in crate::ui) player: &'a Option<Rc<PlayerController>>,
    pub(in crate::ui) content_stack: &'a gtk4::Stack,
    pub(in crate::ui) library_stack: &'a gtk4::Stack,
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
        album_view,
        artist_view,
        player,
        content_stack,
        library_stack,
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

    // Stage 3 Task 5: context menu action wiring. `track_list` stays
    // decoupled from `PlayerController`/`Sidebar` themselves (same
    // decoupling-via-closure seam as `on_activate`/`queue_ids_provider`
    // above) — these three closures are the only place that bridges them.
    // `window` already exists (built at the top of this function), so `set_
    // window` could technically be a constructor parameter, but every other
    // post-construction seam on `track_list` is wired here too, so this
    // keeps all of them in one place.
    track_list.set_window(window);
    // Wire player for tag-edit flow to refresh now-playing metadata
    if let Some(player) = &player {
        track_list.set_player(player);
    }
    {
        let player = player.clone();
        let conn_for_play = conn.clone();
        track_list.set_on_play_selected(move |ids, start_index, source| match &player {
            Some(player) => {
                let origin = {
                    let conn = conn_for_play.borrow();
                    play_origin::resolve(&conn, &source)
                };
                player.play_from_view(ids, start_index, origin);
            }
            None => tracing::warn!("player unavailable; ignoring context menu play action"),
        });
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
    // Album view playback wiring.
    {
        let player = player.clone();
        album_view.set_on_play(move |ids, start_index, source| match &player {
            Some(player) => {
                player.play_from_view(ids, start_index, play_origin::from_album_source(source));
            }
            None => tracing::warn!("player unavailable; ignoring album play action"),
        });
    }
    {
        let player = player.clone();
        album_view.set_on_queue(move |ids| match &player {
            Some(player) => player.append_to_queue(&ids),
            None => tracing::warn!("player unavailable; ignoring album queue action"),
        });
    }
    {
        let player = player.clone();
        album_view.set_on_shuffle(move |ids, start_index, source| match &player {
            Some(player) => {
                player.play_from_view(ids, start_index, play_origin::from_album_source(source));
            }
            None => tracing::warn!("player unavailable; ignoring album shuffle action"),
        });
    }
    // Wire the album context menu's toast overlay so "Added N tracks to
    // Playlist" toasts reach the window surface. Same post-construction
    // injection reason as `player.set_toast_overlay` just above: `toast_
    // overlay` is built after `album_view`.
    album_view.set_toast_overlay(toast_overlay);
    // Wire now-playing fan-out to album grid EQ markers.
    if let Some(ref player) = player {
        let album_view_np = album_view.now_playing_callback();
        player.set_on_now_playing_album_changed(move |album| {
            album_view_np(album);
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
        // Stage 3 Task 6: queue drag-reorder — see `ui::track_list_dnd`'s
        // doc comment. Same decoupling-via-closure seam as `on_play_
        // selected`/`on_queue_selected` just above.
        let player = player.clone();
        track_list.set_on_queue_reorder(move |op| match &player {
            Some(player) => player.reorder_queue_rows(op),
            None => {
                tracing::warn!("player unavailable; ignoring queue drag-reorder");
                false
            }
        });
    }
    // Task 9a: Artists detail-pane hero playback actions. Player-dependent, so
    // wired here (where `player` + `conn` + `artist_view` are all in scope)
    // rather than in `wire_artist_view`, which handles only the
    // navigation-only setters. Each closure resolves the artist's ordered track
    // ids via `query_track_ids` (album-ordered — a natural "Play all") and hands
    // them to the player.
    {
        // `player` is captured `Weak`: this closure is stored on `ArtistView`,
        // which the controller retains strongly (see
        // `current_track_selection::wire`'s doc comment), so a strong capture
        // here would close the cycle back to the controller.
        let player = player.as_ref().map(Rc::downgrade);
        let conn = conn.clone();
        artist_view.set_on_play_all(move |artist| {
            let Some(player) = player.as_ref().and_then(Weak::upgrade) else {
                return;
            };
            let origin = play_origin::from_artist(&artist);
            match artist_track_ids(&conn, artist) {
                Ok(ids) if !ids.is_empty() => player.play_from_view(ids, 0, origin),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "artist play-all query failed"),
            }
        });
    }
    {
        // Weak `player` capture — see the `set_on_play_all` comment above.
        let player = player.as_ref().map(Rc::downgrade);
        let conn = conn.clone();
        artist_view.set_on_shuffle(move |artist| {
            let Some(player) = player.as_ref().and_then(Weak::upgrade) else {
                return;
            };
            let origin = play_origin::from_artist(&artist);
            match artist_track_ids(&conn, artist) {
                Ok(ids) if !ids.is_empty() => player.play_from_view(shuffle_ids(ids), 0, origin),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "artist shuffle query failed"),
            }
        });
    }
    {
        // Weak `player` capture — see the `set_on_play_all` comment above.
        let player = player.as_ref().map(Rc::downgrade);
        let conn = conn.clone();
        artist_view.set_on_add_to_queue(move |artist| {
            let Some(player) = player.as_ref().and_then(Weak::upgrade) else {
                return;
            };
            match artist_track_ids(&conn, artist) {
                Ok(ids) if !ids.is_empty() => player.append_to_queue(&ids),
                Ok(_) => {}
                Err(error) => tracing::warn!(%error, "artist add-to-queue query failed"),
            }
        });
    }
    {
        let conn = conn.clone();
        artist_view.set_on_go_to_folder(move |artist| open_artist_folder(&conn, &artist));
    }
    if let Some(player) = &player {
        // Task 9b: clicking the player-bar artist name deep-links to the
        // Artists tab and selects the playing album artist (no history/back
        // stack — out of scope). `player` is captured `Weak`: the closure is
        // stored on the bar, itself owned by the controller, so a strong
        // capture would cycle (same reason as `set_track_list_reload` above).
        // The two stacks are cheap GObject clones; `select_artist` is a
        // self-contained callable that holds no strong controller/view
        // reference (see `ArtistMaster::select_callback`).
        let player_weak = Rc::downgrade(player);
        let content_stack = content_stack.clone();
        let library_stack = library_stack.clone();
        let select_artist = artist_view.select_artist_callback();
        player.connect_artist_clicked(move || {
            let Some(player) = player_weak.upgrade() else {
                return;
            };
            let Some(artist) = player.current_track_album_artist() else {
                return;
            };
            content_stack.set_visible_child_name("library");
            // Switching to the Artists tab synchronously fires the stack's
            // `visible-child-name` notify handler, which reloads the master
            // (see `library_shell::wire_artist_view`), so the target row
            // exists by the time `select_artist` runs on the next line.
            library_stack.set_visible_child_name(super::library_shell::LIBRARY_VIEW_ARTISTS);
            select_artist(&artist);
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
        // table itself, same decoupling-via-closure seam as `on_play_
        // selected`/`on_queue_selected` above.
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

/// Ordered track ids for `artist`, album-ordered — the natural order for the
/// Artists hero's Play all / Shuffle / Add-to-queue actions.
fn artist_track_ids(
    conn: &Rc<RefCell<Connection>>,
    artist: String,
) -> Result<Vec<i64>, rusqlite::Error> {
    let conn = conn.borrow();
    reprise_core::queries::query_track_ids(
        &conn,
        &ViewSource::Artist(artist),
        "album",
        "asc",
        "",
        &[],
    )
}

/// Fisher–Yates shuffle for the Artists hero "Shuffle" action. `reprise-gnome`
/// carries no direct `rand`/`fastrand` dependency (the crate split kept its dep
/// set minimal), so this seeds a tiny xorshift64 from the wall clock rather
/// than pulling in a new crate. A listen-order shuffle is not security
/// sensitive, so a non-cryptographic PRNG is appropriate here.
fn shuffle_ids(mut ids: Vec<i64>) -> Vec<i64> {
    // `| 1` guards against the degenerate all-zero xorshift state.
    let mut state = (glib::real_time() as u64) | 1;
    for i in (1..ids.len()).rev() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let j = (state % (i as u64 + 1)) as usize;
        ids.swap(i, j);
    }
    ids
}

/// Opens the containing folder of the artist's first (album-ordered) track in
/// the desktop file manager, via `gio::AppInfo::launch_default_for_uri` on the
/// parent directory's `file://` URI — the same default-handler path
/// `preference_lastfm.rs` uses for external URLs. Logs and returns on any
/// lookup/launch failure.
fn open_artist_folder(conn: &Rc<RefCell<Connection>>, artist: &str) {
    let path = {
        let conn = conn.borrow();
        let ids = match reprise_core::queries::query_track_ids(
            &conn,
            &ViewSource::Artist(artist.to_string()),
            "album",
            "asc",
            "",
            &[],
        ) {
            Ok(ids) => ids,
            Err(error) => {
                tracing::warn!(%error, "artist go-to-folder query failed");
                return;
            }
        };
        let Some(&first) = ids.first() else {
            return;
        };
        match reprise_core::queries::query_track_summary(&conn, first) {
            Ok(Some(summary)) => summary.path,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(%error, "artist go-to-folder path lookup failed");
                return;
            }
        }
    };

    let Some(dir) = Path::new(&path).parent() else {
        tracing::warn!(path, "artist track has no parent directory");
        return;
    };
    let uri = gtk4::gio::File::for_path(dir).uri();
    if let Err(error) =
        gtk4::gio::AppInfo::launch_default_for_uri(&uri, gtk4::gio::AppLaunchContext::NONE)
    {
        tracing::warn!(%error, %uri, "failed to open artist folder");
    }
}
