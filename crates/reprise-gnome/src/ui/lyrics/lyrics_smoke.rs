//! Fully synthetic application smoke for played-track lyrics synchronization.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gtk4::glib;
use rusqlite::Connection;

use super::info_panel::InfoPanel;
use super::player_controller::PlayerController;

const SMOKE_ENV: &str = "REPRISE_SMOKE_LYRICS";

pub(super) fn arm(
    player: Option<&Rc<PlayerController>>,
    panel: &Rc<InfoPanel>,
    conn: &Rc<RefCell<Connection>>,
) {
    if std::env::var(SMOKE_ENV).as_deref() != Ok("1") {
        return;
    }
    let Some(player) = player.cloned() else {
        tracing::error!("lyrics smoke failed: playback is unavailable");
        return;
    };
    let ids = match smoke_track_ids(&conn.borrow()) {
        Ok(ids) => ids,
        Err(error) => {
            tracing::error!(%error, "lyrics smoke failed: could not resolve synthetic tracks");
            return;
        }
    };
    let Some(first) = ids.get("SmokeFirst").copied() else {
        tracing::error!("lyrics smoke failed: first synthetic track is absent");
        return;
    };
    let Some(slow) = ids.get("SmokeSlow").copied() else {
        tracing::error!("lyrics smoke failed: slow synthetic track is absent");
        return;
    };
    let Some(fast) = ids.get("SmokeFast").copied() else {
        tracing::error!("lyrics smoke failed: fast synthetic track is absent");
        return;
    };

    tracing::info!("{SMOKE_ENV} set: arming synchronized lyrics exercise");
    panel.show_lyrics();

    let first_player = player.clone();
    glib::timeout_add_local_once(Duration::from_millis(250), move || {
        first_player.play_track_id(first);
    });
    let first_pause = player.clone();
    glib::timeout_add_local_once(Duration::from_millis(350), move || {
        first_pause.toggle_pause();
    });

    let first_position = player.clone();
    let first_view = panel.lyrics_view();
    glib::timeout_add_local_once(Duration::from_millis(700), move || {
        first_position.seek(100);
        first_position.sync_lyrics_position(100);
        log_snapshot("first-line", &first_view, "First current", "Slow stale");
    });

    let second_position = player.clone();
    let second_view = panel.lyrics_view();
    glib::timeout_add_local_once(Duration::from_millis(950), move || {
        second_position.seek(700);
        second_position.sync_lyrics_position(700);
        log_snapshot("second-line", &second_view, "First later", "Slow stale");
    });

    let slow_player = player.clone();
    glib::timeout_add_local_once(Duration::from_millis(1_200), move || {
        slow_player.play_track_id(slow);
    });
    let fast_player = player.clone();
    glib::timeout_add_local_once(Duration::from_millis(1_250), move || {
        fast_player.play_track_id(fast);
    });
    let fast_pause = player;
    glib::timeout_add_local_once(Duration::from_millis(1_350), move || {
        fast_pause.toggle_pause();
    });

    let final_view = panel.lyrics_view();
    glib::timeout_add_local_once(Duration::from_millis(2_500), move || {
        log_snapshot("latest-track", &final_view, "Fast current", "Slow stale");
    });
}

fn smoke_track_ids(conn: &Connection) -> rusqlite::Result<HashMap<String, i64>> {
    let mut statement = conn.prepare(
        "SELECT title, id FROM tracks WHERE title IN ('SmokeFirst', 'SmokeSlow', 'SmokeFast')",
    )?;
    let ids = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect();
    ids
}

fn log_snapshot(
    phase: &str,
    view: &super::lyrics_view::LyricsView,
    expected: &str,
    rejected: &str,
) {
    let (line_count, active_line, latest) = view.smoke_snapshot(expected, rejected);
    if !latest {
        tracing::error!(
            phase,
            line_count,
            ?active_line,
            "lyrics smoke state is stale or missing"
        );
        return;
    }
    tracing::info!(
        phase,
        line_count,
        ?active_line,
        latest,
        "lyrics smoke state"
    );
}
