//! Queue and playback wiring for the Now Playing panel.

use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::now_playing::NowPlayingPanel;
use crate::ui::player_controller::PlayerController;

use super::window_queue_model::SharedQueueModel;

pub(in crate::ui) fn install(
    player: &Rc<PlayerController>,
    panel: &Rc<NowPlayingPanel>,
    queue_model: &SharedQueueModel,
    window: &adw::ApplicationWindow,
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

    let panel_weak = Rc::downgrade(panel);
    let window_weak = window.downgrade();
    player.connect_analysis_clicked(move || {
        let (Some(panel), Some(window)) = (panel_weak.upgrade(), window_weak.upgrade()) else {
            return;
        };
        panel.present_audio_character(&window);
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
