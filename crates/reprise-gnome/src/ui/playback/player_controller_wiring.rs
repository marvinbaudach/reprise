//! Bar-signal wiring and dev/verification hooks for `PlayerController`,
//! split out of `player_controller.rs` (Stage 3 Task 10, purely to keep
//! that file under the project's file-size limit — no behavioral seam is
//! implied by the split, same rationale as `mpris.rs`'s `state` submodule).
//!
//! - `wire_bar_controls`: connects every `PlayerBar` user-input signal
//!   (`connect_play_pause`/`connect_seek`/`connect_volume_changed`/
//!   `connect_previous`/`connect_next`/`connect_shuffle_toggled`/
//!   `connect_repeat_clicked`) to the matching `PlayerController` call —
//!   `PlayerController::new`'s only caller, right after construction.
//! - `cycle_repeat`: the pure Off→All→One→Off cycling `wire_bar_controls`'s
//!   repeat-button closure uses.
//! - `arm_smoke_repeat`: the `REPRISE_SMOKE_REPEAT=all` dev/verification
//!   hook — `PlayerController::new`'s other post-construction call.
//!
//! Both public entry points take `&Rc<PlayerController>` (not `&self` — they
//! aren't `impl PlayerController` methods) since they're only ever called
//! once, from `PlayerController::new`, before that `Rc` has any other
//! owner — free functions in a sibling module rather than inherent methods
//! for the same reason `mpris_mirror.rs`'s `mpris_status_from_playback_state`
//! is a free function: no `&self` is actually needed. `pub(super)` (visible
//! throughout `ui` and its descendants) so `player_controller.rs` can call
//! them — same seam idiom as `mpris_mirror.rs`/`playback_faults.rs` use for
//! the reverse direction (see `player_controller.rs`'s `## Queue borrow
//! discipline` doc section).

use std::rc::Rc;

use crate::ui::player_controller::PlayerController;
use reprise_core::queue::Repeat;

/// Dev/verification hook (permanent, like `REPRISE_SCAN_DIR`/`REPRISE_
/// SMOKE_QUIT`/`REPRISE_SMOKE_ACTIVATE`): when set to `"all"`, forces
/// `Repeat::All` right after the controller (and its queue) are built, so a
/// headless E2E can observe auto-advance wrapping from the last track back
/// to the first without a human toggling the repeat button.
///
/// Usage: `REPRISE_SMOKE_REPEAT=all REPRISE_SCAN_DIR=… REPRISE_SMOKE_ACTIVATE=1
///  REPRISE_AUDIO_SINK=fakesink REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.
const SMOKE_REPEAT_ENV_VAR: &str = "REPRISE_SMOKE_REPEAT";
const SMOKE_REPEAT_ALL_VALUE: &str = "all";

/// Wires the bar's user-input signals to player calls. Each closure holds a
/// `Weak` controller reference: the bar is owned *by* the controller, so a
/// strong reference here would be a leak-guaranteeing Rc cycle.
pub(super) fn wire_bar_controls(controller: &Rc<PlayerController>) {
    let weak = Rc::downgrade(controller);
    controller.bar.connect_play_pause(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.toggle_pause();
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_seek(move |position_ms| {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        // Stage 3 Task 10: routed through the same `seek` every
        // MPRIS-initiated seek uses, so the bar's own seeks also trigger
        // `Seeked` (the spec requires it for "app-internal" seeks too — see
        // `seek`'s doc comment in `mpris_mirror.rs`) rather than calling
        // `Player::seek_to` directly the way this closure used to.
        controller.seek(position_ms);
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_volume_changed(move |volume| {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.player.set_volume(volume);
        controller.sync_volume_indicator(volume);
        // Stage 3 Task 10: keeps the tracked volume + MPRIS mirror current
        // immediately, rather than waiting for an unrelated status/track
        // transition to refresh it via `update_mpris_mirror` — see
        // `volume`'s and `update_mpris_volume`'s doc comments.
        controller.volume.set(volume);
        controller.update_mpris_volume(volume);
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_previous(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.previous();
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_next(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.next();
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_shuffle_toggled(move |active| {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        controller.queue.borrow_mut().set_shuffle(active);
        // Read back the queue's own idea of shuffle state (rather than just
        // logging `active`) so a log line always reflects what `Queue`
        // actually did, not just what the button asked for.
        let is_shuffled = controller.queue.borrow().is_shuffled();
        // Task 8: keeps the Now-Playing page's shuffle toggle in sync with
        // the bar's (the bar's own toggle is already correct — it's the
        // click origin — but `sync_shuffle_indicator`'s `updating_shuffle`
        // guard makes re-setting it here a harmless no-op; see that
        // method's doc comment in `now_playing_wiring.rs`).
        controller.sync_shuffle_indicator(is_shuffled);
        // Stage 3 Task 10: keeps the MPRIS mirror current immediately — see
        // `update_mpris_shuffle`'s doc comment.
        controller.update_mpris_shuffle(is_shuffled);
        tracing::debug!(is_shuffled, "shuffle toggled");
    });

    let weak = Rc::downgrade(controller);
    controller.bar.connect_repeat_clicked(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        // Explicit block (not a single statement): reading the current mode
        // and setting the new one both need the same borrow, so they're
        // scoped together here — still dropped before `set_repeat_
        // indicator` (a GTK call) runs after the block. See `player_
        // controller.rs`'s `## Queue borrow discipline` doc section.
        let next_repeat = {
            let mut queue = controller.queue.borrow_mut();
            let next_repeat = cycle_repeat(queue.repeat());
            queue.set_repeat(next_repeat);
            next_repeat
        };
        // Task 8: syncs the bar AND the Now-Playing page — see
        // `now_playing_wiring.rs`'s `sync_repeat_indicator` doc comment.
        controller.sync_repeat_indicator(next_repeat);
        // Stage 3 Task 10: keeps the MPRIS mirror current immediately — see
        // `update_mpris_repeat`'s doc comment.
        controller.update_mpris_repeat(next_repeat);
    });
}

pub(super) fn wire_compact_controls(controller: &Rc<PlayerController>) {
    let weak = Rc::downgrade(controller);
    controller
        .compact_player
        .set_on_layout(Rc::new(move |layout| {
            if let Some(controller) = weak.upgrade() {
                controller.compact_player.set_layout(layout);
            }
        }));
    controller.compact_player.set_on_restore(Rc::new(|| {
        tracing::debug!("compact restore requested before window mode coordinator is installed");
    }));
    controller.compact_player.set_on_preferences(Rc::new(|| {
        tracing::debug!(
            "compact preferences requested before window mode coordinator is installed"
        );
    }));

    let weak = Rc::downgrade(controller);
    controller.compact_player.connect_play_pause(move || {
        if let Some(controller) = weak.upgrade() {
            controller.toggle_pause();
        }
    });

    let weak = Rc::downgrade(controller);
    controller.compact_player.connect_seek(move |position_ms| {
        if let Some(controller) = weak.upgrade() {
            controller.seek(position_ms);
        }
    });

    let weak = Rc::downgrade(controller);
    controller
        .compact_player
        .connect_volume_changed(move |volume| {
            let Some(controller) = weak.upgrade() else {
                return;
            };
            controller.player.set_volume(volume);
            controller.volume.set(volume);
            controller.sync_volume_indicator(volume);
            controller.update_mpris_volume(volume);
        });

    let weak = Rc::downgrade(controller);
    controller.compact_player.connect_previous(move || {
        if let Some(controller) = weak.upgrade() {
            controller.previous();
        }
    });

    let weak = Rc::downgrade(controller);
    controller.compact_player.connect_next(move || {
        if let Some(controller) = weak.upgrade() {
            controller.next();
        }
    });

    let weak = Rc::downgrade(controller);
    controller
        .compact_player
        .connect_shuffle_toggled(move |active| {
            let Some(controller) = weak.upgrade() else {
                return;
            };
            controller.queue.borrow_mut().set_shuffle(active);
            let shuffled = controller.queue.borrow().is_shuffled();
            controller.sync_shuffle_indicator(shuffled);
            controller.update_mpris_shuffle(shuffled);
        });

    let weak = Rc::downgrade(controller);
    controller.compact_player.connect_repeat_clicked(move || {
        let Some(controller) = weak.upgrade() else {
            return;
        };
        let repeat = {
            let mut queue = controller.queue.borrow_mut();
            let repeat = cycle_repeat(queue.repeat());
            queue.set_repeat(repeat);
            repeat
        };
        controller.sync_repeat_indicator(repeat);
        controller.update_mpris_repeat(repeat);
    });
}

/// Cycles the repeat mode in the mockup's button order: Off -> All -> One ->
/// Off. Pure (no `Queue`/GTK access) so it's unit-testable directly.
/// `pub(super)` (Task 8) so `now_playing_wiring.rs`'s repeat-button handler
/// can share this exact cycling logic (DRY) rather than a second copy.
pub(super) fn cycle_repeat(current: Repeat) -> Repeat {
    match current {
        Repeat::Off => Repeat::All,
        Repeat::All => Repeat::One,
        Repeat::One => Repeat::Off,
    }
}

/// Arms `REPRISE_SMOKE_REPEAT=all` (see the const's doc comment above):
/// forces the queue into `Repeat::All` right after construction and syncs
/// the bar's repeat indicator to match, so a headless E2E run can observe
/// auto-advance wrapping from the last queued track back to the first.
pub(super) fn arm_smoke_repeat(controller: &Rc<PlayerController>) {
    let Ok(value) = std::env::var(SMOKE_REPEAT_ENV_VAR) else {
        return;
    };
    if value != SMOKE_REPEAT_ALL_VALUE {
        tracing::warn!(
            value,
            "{SMOKE_REPEAT_ENV_VAR} set to an unrecognized value; ignoring (expected \"{SMOKE_REPEAT_ALL_VALUE}\")"
        );
        return;
    }
    tracing::info!(
        "{SMOKE_REPEAT_ENV_VAR}={SMOKE_REPEAT_ALL_VALUE} set: forcing Repeat::All for headless wrap-around E2E"
    );
    controller.queue.borrow_mut().set_repeat(Repeat::All);
    controller.sync_repeat_indicator(Repeat::All);
    controller.update_mpris_repeat(Repeat::All);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cycle_repeat_goes_off_all_one_off() {
        assert_eq!(cycle_repeat(Repeat::Off), Repeat::All);
        assert_eq!(cycle_repeat(Repeat::All), Repeat::One);
        assert_eq!(cycle_repeat(Repeat::One), Repeat::Off);
    }
}
