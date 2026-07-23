//! Queue and playback wiring for the Now Playing panel.

use std::rc::Rc;

use crate::ui::now_playing::NowPlayingPanel;
use crate::ui::player_controller::PlayerController;

use super::window_queue_model::SharedQueueModel;

pub(in crate::ui) fn install(
    player: &Rc<PlayerController>,
    panel: &Rc<NowPlayingPanel>,
    queue_model: &SharedQueueModel,
) {
    let panel_weak = Rc::downgrade(panel);
    player.set_on_now_playing_panel_track_changed(move |track| {
        if let Some(panel) = panel_weak.upgrade() {
            panel.set_loaded_track(track);
        }
    });
    let panel_weak = Rc::downgrade(panel);
    player.set_on_now_playing_panel_state_changed(move |state| {
        if let Some(panel) = panel_weak.upgrade() {
            panel.set_playback_state(state);
        }
    });
    let panel_weak = Rc::downgrade(panel);
    player.set_on_song_visual_spectrum_changed(move |frame| {
        if let Some(panel) = panel_weak.upgrade() {
            panel.set_spectrum(frame);
        }
    });

    // Fullscreen visualizer transport, seek, and volume all drive the same
    // player actions as the player bar.
    let hook = |player: &Rc<PlayerController>, action: fn(&PlayerController)| -> Rc<dyn Fn()> {
        let weak = Rc::downgrade(player);
        Rc::new(move || {
            if let Some(player) = weak.upgrade() {
                action(&player);
            }
        })
    };
    let seek_weak = Rc::downgrade(player);
    let volume_weak = Rc::downgrade(player);
    panel.set_visual_player_hooks(crate::ui::now_playing::PlayerHooks {
        previous: hook(player, PlayerController::previous),
        play_pause: hook(player, PlayerController::toggle_pause),
        stop: hook(player, PlayerController::reset_to_stopped),
        next: hook(player, PlayerController::next),
        seek_to_ms: Rc::new(move |ms| {
            if let Some(player) = seek_weak.upgrade() {
                player.seek(ms);
            }
        }),
        set_volume: Rc::new(move |volume| {
            if let Some(player) = volume_weak.upgrade() {
                player.player.set_volume(volume);
                player.sync_volume_indicator(volume);
                player.volume.set(volume);
                player.update_mpris_volume(volume);
            }
        }),
        initial_volume: player.volume.get(),
    });
    let panel_weak = Rc::downgrade(panel);
    player.set_on_now_playing_panel_position_changed(move |position_ms, duration_ms| {
        if let Some(panel) = panel_weak.upgrade() {
            panel.set_position(position_ms, duration_ms);
        }
    });

    let refresh = {
        let panel = Rc::downgrade(panel);
        let queue_model = Rc::downgrade(queue_model);
        Rc::new(move || {
            let (Some(panel), Some(queue_model)) = (panel.upgrade(), queue_model.upgrade()) else {
                return;
            };
            if !panel.is_up_next_visible() {
                return;
            }
            let snapshot = queue_model.borrow().clone();
            panel.set_up_next_model(&snapshot);
        })
    };
    let refresh_on_queue_change = refresh.clone();
    player.add_on_queue_changed(move || refresh_on_queue_change());
    panel.set_on_up_next_refresh(move || refresh());

    let player_for_jump = Rc::downgrade(player);
    panel.set_on_up_next_jump(move |row| {
        if let Some(player) = player_for_jump.upgrade() {
            player.jump_to_queue_row(row);
        }
    });
    let player_for_remove = Rc::downgrade(player);
    panel.set_on_up_next_remove(move |row| {
        if let Some(player) = player_for_remove.upgrade() {
            player.remove_queue_rows(&[row]);
        }
    });
    let player_for_reorder = Rc::downgrade(player);
    panel.set_on_up_next_reorder(move |from, to| {
        let Some(player) = player_for_reorder.upgrade() else {
            return;
        };
        let Some(op) = crate::ui::track_list::queue_row_mapping::reorder_rows(from, to) else {
            return;
        };
        player.reorder_queue_rows(op);
    });
}
