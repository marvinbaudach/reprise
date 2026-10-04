//! Volume-control methods extracted to keep the player-bar owner below the
//! project file-size limit.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::player_bar_layout::{VOLUME_MAX, VOLUME_MIN};
use super::PlayerBar;

const ICON_VOLUME_MUTED: &str = "audio-volume-muted-symbolic";
const ICON_VOLUME_LOW: &str = "audio-volume-low-symbolic";
const ICON_VOLUME_MEDIUM: &str = "audio-volume-medium-symbolic";
const ICON_VOLUME_HIGH: &str = "audio-volume-high-symbolic";

impl PlayerBar {
    pub fn connect_volume_changed<F: Fn(f64) + 'static>(&self, f: F) {
        let updating_volume = self.updating_volume.clone();
        self.volume_scale.connect_value_changed(move |scale| {
            if !updating_volume.get() {
                f(scale.value());
            }
        });
    }

    pub fn set_volume_indicator(&self, volume: f64) {
        self.updating_volume.set(true);
        let clamped = volume.clamp(VOLUME_MIN, VOLUME_MAX);
        self.volume_scale.set_value(clamped);
        self.update_volume_icon(clamped);
        self.updating_volume.set(false);
    }

    pub fn connect_mute_toggled<F: Fn(f64) + 'static>(&self, f: F) {
        let volume_scale = self.volume_scale.clone();
        let muted = Rc::new(Cell::new(false));
        let pre_mute_volume = Rc::new(Cell::new(1.0f64));
        let updating_volume = self.updating_volume.clone();
        let volume_icon = self.volume_icon.clone();
        self.volume_icon.connect_clicked(move |_| {
            let is_muted = muted.get();
            let result_volume = if is_muted {
                let restore = pre_mute_volume.get();
                updating_volume.set(true);
                volume_scale.set_value(restore);
                updating_volume.set(false);
                Self::set_volume_icon_static(&volume_icon, restore);
                muted.set(false);
                restore
            } else {
                let current = volume_scale.value();
                pre_mute_volume.set(current);
                updating_volume.set(true);
                volume_scale.set_value(0.0);
                updating_volume.set(false);
                Self::set_volume_icon_static(&volume_icon, 0.0);
                muted.set(true);
                0.0
            };
            f(result_volume);
        });
    }

    fn update_volume_icon(&self, volume: f64) {
        Self::set_volume_icon_static(&self.volume_icon, volume);
    }

    fn set_volume_icon_static(button: &gtk4::Button, volume: f64) {
        let icon = if volume <= 0.0 {
            ICON_VOLUME_MUTED
        } else if volume < 0.33 {
            ICON_VOLUME_LOW
        } else if volume < 0.66 {
            ICON_VOLUME_MEDIUM
        } else {
            ICON_VOLUME_HIGH
        };
        button.set_icon_name(icon);
    }
}
