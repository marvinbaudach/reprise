//! External-media projection for the full player bar.

use std::time::Instant;

use gtk4::prelude::*;

use super::player_bar_state::{external_bar_display, BarProgressMode};
use super::surface::PlayerBar;
use crate::ui::playback::external_media::ExternalPlaybackSnapshot;
use crate::ui::playing_links::{self, LinkAvailability};

impl PlayerBar {
    pub(in crate::ui) fn set_external_snapshot(&self, snapshot: Option<&ExternalPlaybackSnapshot>) {
        let Some(snapshot) = snapshot else {
            self.seek_enabled.set(true);
            self.progress_mode.set(BarProgressMode::Local);
            self.set_buffering(0, None);
            self.progress_stack.set_visible_child_name("progress");
            self.live_station_label.set_text("");
            self.refresh_live_badge_pulse();
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
        self.progress_mode.set(display.progress_mode);
        let links = playing_links::player_bar_labels(
            playing_links::external_mode(&snapshot.media),
            LinkAvailability {
                artist: true,
                album: true,
            },
        );
        self.set_track(&display.title, &display.subtitle, links);
        self.set_state(display.playback);
        self.set_buffering(0, None);
        self.waveform.set_peaks(Vec::new());
        let live = display.progress_mode == BarProgressMode::Live;
        self.seek_enabled.set(!live);
        self.waveform.widget().set_sensitive(!live);
        self.progress_stack
            .set_visible_child_name(if live { "live" } else { "progress" });
        let station_name = match &snapshot.media {
            crate::ui::playback::external_media::ExternalMedia::Radio { name, .. } => name,
            crate::ui::playback::external_media::ExternalMedia::Podcast { .. } => "",
        };
        self.live_station_label.set_text(station_name);
        self.refresh_live_badge_pulse();
        if live {
            self.duration_label.set_text("");
        }
        self.shuffle_button.set_sensitive(false);
        self.repeat_button.set_sensitive(false);
        self.prev_button.set_sensitive(snapshot.can_go_previous);
        self.next_button.set_sensitive(snapshot.can_go_next);
        let reconnecting = snapshot.radio.as_ref().is_some_and(|radio| {
            radio.phase() == crate::ui::playback::external_media::RadioPhase::Reconnecting
        });
        if live
            && display.playback == reprise_core::playback::PlaybackState::Playing
            && (reconnecting || self.live_started_at.borrow().is_none())
        {
            *self.live_started_at.borrow_mut() = Some(Instant::now());
            self.position_label.set_text("0:00");
            self.duration_label.set_text("");
        } else if !live {
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

    pub(in crate::ui) fn set_buffering(&self, percent: u8, buffered_ms: Option<i64>) {
        self.buffering_percent.set(percent.min(100));
        self.buffered_ms.set(buffered_ms.map(|value| value.max(0)));
        self.refresh_buffering_presentation();
    }

    pub(super) fn refresh_buffering_presentation(&self) {
        let event = reprise_core::playback::PlayerEvent::Buffering {
            percent: self.buffering_percent.get(),
            buffered_ms: self.buffered_ms.get(),
        };
        let presentation = super::player_bar_state::buffering_presentation(
            &event,
            self.progress_mode.get(),
            self.duration_ms.get(),
        );
        self.waveform
            .set_buffered_fraction(presentation.map(|presentation| presentation.buffered_fraction));
        if let Some(presentation) = presentation.filter(|presentation| presentation.show_status) {
            self.streaming_status_label
                .set_text(&crate::ui::strings::podcast_streaming_loaded(
                    presentation.loaded_percent,
                ));
            self.streaming_status_label.set_visible(true);
        } else {
            self.streaming_status_label.set_visible(false);
            self.streaming_status_label.set_text("");
        }
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
