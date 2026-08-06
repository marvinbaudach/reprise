//! Shared persistent playing marker used across Reprise surfaces.

use gtk4::prelude::*;

use crate::ui::eq_bars::{self, EqVariant};

pub(in crate::ui) const PLAYING_MARKER_CLASS: &str = "reprise-playing-marker";
pub(in crate::ui) const PLAYING_TITLE_CLASS: &str = "now-playing-title";

pub(in crate::ui) fn build() -> gtk4::Box {
    let marker = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    marker.add_css_class(PLAYING_MARKER_CLASS);
    marker.set_valign(gtk4::Align::Center);
    marker.set_halign(gtk4::Align::Center);
    marker.append(&eq_bars::build(EqVariant::Animated));
    marker
}

pub(in crate::ui) fn set_playing(marker: &gtk4::Box, playing: bool) {
    if playing {
        marker.remove_css_class("playback-paused");
    } else {
        marker.add_css_class("playback-paused");
    }
}

pub(in crate::ui) fn css() -> String {
    format!(".{PLAYING_MARKER_CLASS} {{ color: @reprise_player_accent; }}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_uses_the_playback_accent_role() {
        assert!(css().contains("@reprise_player_accent"));
    }

    #[test]
    fn nav_10b_every_list_surface_places_the_marker_the_same_way() {
        for source in [
            include_str!("track_list/track_list_columns.rs"),
            include_str!("podcasts/podcasts_groups.rs"),
            include_str!("podcasts/youtube_channel_detail.rs"),
            include_str!("radio/radio_columns.rs"),
        ] {
            assert!(source.contains("playing_marker::build"));
            assert!(!source.contains("eq_bars::build"));
            assert!(!source.contains("set_state_overlay"));
        }
        // The player bar shows the running state through the play/pause button.
        // A second copy of the list marker there animates the same three bars
        // twice on every view where the running track is visible in the list.
        let player_bar = include_str!("player_bar/player_bar_layout.rs");
        assert!(!player_bar.contains("playing_marker::build"));
    }
}
