use std::path::{Path, PathBuf};

use super::*;

pub(super) fn wire_close(w: &RuntimeWiring<'_>) {
    let RuntimeWiring {
        scan_controls,
        window,
        toast_overlay,
        db_path,
        track_list,
        sidebar,
        watcher_state,
        conn,
        session_state,
        device_sync,
        player,
        library_player_bar,
        info_panel,
        ..
    } = *w;
    super::scan_flow::wire_scan_button(
        scan_controls,
        window,
        toast_overlay,
        db_path.to_path_buf(),
        track_list.clone(),
        sidebar.clone(),
        watcher_state.clone(),
    );
    super::scan_flow::arm_smoke_rescan(
        scan_controls,
        toast_overlay,
        db_path.to_path_buf(),
        track_list.clone(),
        sidebar.clone(),
        watcher_state.clone(),
    );
    start_persisted_watcher(
        conn,
        db_path,
        session_state,
        scan_controls,
        track_list,
        sidebar,
        watcher_state,
    );
    super::startup_report::mark("start_persisted_watcher");
    external_changes_wiring::start_external_changes_refresh(
        db_path,
        track_list,
        sidebar,
        device_sync,
    );
    super::startup_report::mark("start_external_changes_refresh");
    wire_queue_episode_marker(track_list, player.as_ref());
    super::mounts::install(&super::mounts::MountWiring {
        conn,
        db_path,
        controls: scan_controls,
        toast_overlay,
        track_list,
        sidebar,
        watcher_state,
    });
    super::startup_report::mark("mounts::install");

    super::playlist_io::wire_import_action(window, toast_overlay, conn.clone(), sidebar);
    super::startup_report::mark("playlist_io::wire_import_action");
    super::playlist_io::arm_smoke_m3u(conn.clone(), toast_overlay, sidebar.clone());
    super::window_smoke::arm_bar_position(conn, library_player_bar);
    super::lyrics_smoke::arm(player.as_ref(), info_panel, conn);
}

fn start_persisted_watcher(
    conn: &Rc<Db>,
    db_path: &Path,
    previous_session: &SessionState,
    scan_controls: &ScanControls,
    track_list: &Rc<TrackList>,
    sidebar: &Rc<Sidebar>,
    watcher_state: &Rc<RefCell<Option<WatcherHandle>>>,
) {
    let root = {
        let conn = &conn;
        reprise_core::library::settings::get_library_root(conn)
    };
    match root {
        Ok(Some(root)) => {
            let start = if reprise_core::library::startup_tasks::should_run_time_window(
                reprise_core::library::startup_tasks::TimeWindowTask::LibraryScan,
                previous_session,
                &root,
            ) {
                super::scan_flow::start_or_restart_watcher
            } else {
                super::scan_flow::start_or_restart_live_watcher
            };
            start(
                watcher_state,
                &PathBuf::from(root),
                db_path.to_path_buf(),
                scan_controls.clone(),
                Rc::downgrade(track_list),
                Rc::downgrade(sidebar),
            );
        }
        Ok(None) => tracing::debug!("no persisted library root; watcher not started at startup"),
        Err(error) => tracing::error!(%error, "failed to read persisted library root at startup"),
    }
}

/// Keeps the Queue surfaces' now-playing marker in step with a queued episode.
///
/// The track-side marker is driven by `playing_track_id`, written when a track
/// starts. An episode never goes through that path — it plays through the
/// external-media controller — so without this the app can be playing a queued
/// episode while every queue surface shows nothing as playing. The Podcasts and
/// YouTube views already subscribe to the same signal for their own marker;
/// this adds the queue's.
fn wire_queue_episode_marker(track_list: &Rc<TrackList>, player: Option<&Rc<PlayerController>>) {
    let Some(player) = player else {
        return;
    };
    let track_list = Rc::downgrade(track_list);
    player.add_on_external_changed(move |snapshot| {
        let Some(track_list) = track_list.upgrade() else {
            return;
        };
        let episode_mark = crate::ui::podcasts::episode_mark_from_snapshot(snapshot.as_ref());
        track_list.set_playing_episode(episode_mark);
    });
}
