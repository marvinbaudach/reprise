//! External-media projection for the full player bar.

use std::time::Instant;

use gtk4::prelude::*;

use super::player_bar_state::external_bar_display;
use super::surface::PlayerBar;
use crate::ui::playback::external_media::ExternalPlaybackSnapshot;

impl PlayerBar {
    pub(in crate::ui) fn set_external_snapshot(&self, snapshot: Option<&ExternalPlaybackSnapshot>) {
        let Some(snapshot) = snapshot else {
            self.seek_enabled.set(true);
            self.external_podcast.set(false);
            *self.live_started_at.borrow_mut() = None;
            self.waveform.widget().set_opacity(1.0);
            self.waveform.widget().set_sensitive(true);
            self.title_label.remove_css_class("dim-label");
            self.title_label.set_tooltip_text(None);
            self.retry_external_button.set_visible(false);
            self.shuffle_button.set_sensitive(true);
            self.repeat_button.set_sensitive(true);
            self.prev_button.set_sensitive(self.queue_has_tracks.get());
            self.next_button.set_sensitive(self.queue_has_tracks.get());
            self.refresh_sensitivity();
            return;
        };
        let display = external_bar_display(snapshot);
        self.set_track(&display.title, &display.subtitle);
        self.set_state(display.playback);
        self.external_podcast.set(!display.live);
        self.waveform.set_peaks(Vec::new());
        self.seek_enabled.set(!display.live);
        self.waveform.widget().set_sensitive(!display.live);
        self.waveform
            .widget()
            .set_opacity(if display.live { 0.0 } else { 1.0 });
        self.shuffle_button.set_sensitive(false);
        self.repeat_button.set_sensitive(false);
        self.prev_button.set_sensitive(false);
        self.next_button.set_sensitive(false);
        let reconnecting = snapshot.radio.as_ref().is_some_and(|radio| {
            radio.phase() == crate::ui::playback::external_media::RadioPhase::Reconnecting
        });
        if display.live
            && display.playback == reprise_core::playback::PlaybackState::Playing
            && (reconnecting || self.live_started_at.borrow().is_none())
        {
            *self.live_started_at.borrow_mut() = Some(Instant::now());
            self.position_label.set_text("0:00");
            self.duration_label.set_text("");
        } else if !display.live {
            *self.live_started_at.borrow_mut() = None;
        }
        if display.title_dimmed {
            self.title_label.add_css_class("dim-label");
        } else {
            self.title_label.remove_css_class("dim-label");
        }
        self.title_label
            .set_tooltip_text(display.inline_error.as_deref());
        self.retry_external_button
            .set_visible(display.inline_error.is_some());
    }

    pub(in crate::ui) fn show_play_next_episode(&self, visible: bool) {
        self.play_next_available.set(visible);
        self.play_next_episode_button.set_visible(visible);
        self.refresh_sensitivity();
    }

    pub(in crate::ui) fn connect_play_next_episode(&self, callback: impl Fn() + 'static) {
        self.play_next_episode_button
            .connect_clicked(move |_| callback());
    }

    pub(in crate::ui) fn connect_retry_external(&self, callback: impl Fn() + 'static) {
        self.retry_external_button
            .connect_clicked(move |_| callback());
    }
}
