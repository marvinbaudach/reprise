//! Headless verification hooks for external (podcast / YouTube) playback.
//!
//! Same shape as the other `REPRISE_SMOKE_*` hooks (`window_smoke.rs`,
//! `track_list_smoke.rs`): permanent, env-gated, inert unless the variable is
//! set. POD-21's neighbour transport had no headless route at all — none of the
//! existing hooks can start an episode or press ⏮/⏭ — so the path could only be
//! covered by unit tests, never by a run of the real application.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use super::player_controller::PlayerController;

const EPISODE_PLAY_ENV_VAR: &str = "REPRISE_SMOKE_EPISODE_PLAY";
const TRANSPORT_ENV_VAR: &str = "REPRISE_SMOKE_TRANSPORT";
const TRANSPORT_DELAY_ENV_VAR: &str = "REPRISE_SMOKE_TRANSPORT_DELAY_SECS";
const TRANSPORT_DELAY_DEFAULT: u32 = 6;

/// Arms `REPRISE_SMOKE_EPISODE_PLAY=<episode id>`: once the main loop is up,
/// activates the view's own `podcasts.play` action for that id. Going through
/// the action rather than calling the player directly is the point — it is the
/// exact path a context-menu click takes, so the run exercises the real
/// `neighbour_ids_for_episode` projection instead of a list built for the test.
pub(in crate::ui) fn arm_episode_play(view: &Rc<crate::ui::podcasts::PodcastsView>) {
    let Ok(value) = std::env::var(EPISODE_PLAY_ENV_VAR) else {
        return;
    };
    let Ok(episode_id) = value.trim().parse::<i64>() else {
        tracing::warn!(
            %value,
            "{EPISODE_PLAY_ENV_VAR} set to a non-numeric episode id; ignoring"
        );
        return;
    };
    let view = Rc::downgrade(view);
    glib::idle_add_local_once(move || {
        let Some(view) = view.upgrade() else {
            return;
        };
        // Both source views arm this hook. Only the one that actually renders
        // the episode may start it — the other would start it neighbourless
        // (it is absent from that view's rendered order) and clobber the
        // correct session.
        if !view.renders_episode(episode_id) {
            return;
        }
        tracing::info!(episode_id, "{EPISODE_PLAY_ENV_VAR} set: activating episode");
        if let Err(error) = view
            .root()
            .activate_action("podcasts.play", Some(&episode_id.to_variant()))
        {
            tracing::error!(%error, episode_id, "smoke episode activation failed");
        }
    });
}

/// Arms `REPRISE_SMOKE_TRANSPORT=next|previous` (comma-separated for several
/// presses): fires the transport after `REPRISE_SMOKE_TRANSPORT_DELAY_SECS`,
/// then once more per further entry at the same interval. Calls
/// `transport_next`/`transport_previous` — the same methods the ⏭/⏮ buttons and
/// the MPRIS commands are wired to — so a run proves the neighbour path end to
/// end, resolution included.
pub(in crate::ui) fn arm_transport(player: &Rc<PlayerController>) {
    let Ok(value) = std::env::var(TRANSPORT_ENV_VAR) else {
        return;
    };
    let steps: Vec<String> = value
        .split(',')
        .map(|step| step.trim().to_ascii_lowercase())
        .filter(|step| !step.is_empty())
        .collect();
    if steps
        .iter()
        .any(|step| step != "next" && step != "previous")
    {
        tracing::warn!(
            %value,
            "{TRANSPORT_ENV_VAR} accepts only 'next' and 'previous'; ignoring"
        );
        return;
    }
    let delay_secs = std::env::var(TRANSPORT_DELAY_ENV_VAR)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(TRANSPORT_DELAY_DEFAULT)
        .max(1);
    for (index, step) in steps.into_iter().enumerate() {
        let player = Rc::downgrade(player);
        // Space the presses out so each one acts on a settled session rather
        // than on one still resolving — an immediate second press would only
        // prove that the generation guard swallows it.
        let after = delay_secs.saturating_mul(index as u32 + 1);
        glib::timeout_add_seconds_local_once(after, move || {
            let Some(player) = player.upgrade() else {
                return;
            };
            tracing::info!(
                step = %step,
                after_secs = after,
                "{TRANSPORT_ENV_VAR} firing transport step"
            );
            if step == "next" {
                player.transport_next();
            } else {
                player.transport_previous();
            }
        });
    }
}
