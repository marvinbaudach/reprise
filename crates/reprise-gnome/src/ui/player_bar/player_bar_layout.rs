//! Widget construction for the full-width Library player bar.

use gtk4::{pango, prelude::*};

use super::cover_loader::CoverLoader;
use super::player_bar::{ICON_NEXT, ICON_PLAY, ICON_PREVIOUS, ICON_REPEAT_ALL, ICON_SHUFFLE};
use super::strings;

pub(super) const VOLUME_MIN: f64 = 0.0;
pub(super) const VOLUME_MAX: f64 = 1.0;
const VOLUME_STEP: f64 = 0.05;
const VOLUME_DEFAULT: f64 = 1.0;

const COVER_PIXEL_SIZE: i32 = 56;
const BAR_HEIGHT: i32 = 86;
const START_ZONE_WIDTH: i32 = 300;
const END_ZONE_WIDTH: i32 = 250;
const CENTER_ZONE_MAX_WIDTH: i32 = 620;
const VOLUME_SLIDER_WIDTH: i32 = 80;
const PLAY_BUTTON_SIZE: i32 = 44;

const ZERO_TIME_LABEL: &str = "0:00";
const ZONE_SPACING: i32 = 8;

const COVER_CSS_CLASS: &str = "player-bar-cover";
const PLAY_CSS_CLASS: &str = "player-bar-play";
const SURFACE_CSS_CLASS: &str = "player-bar-surface";

const ICON_VOLUME_HIGH: &str = "audio-volume-high-symbolic";
const ICON_QUEUE: &str = "view-list-symbolic";

/// CSS for the three-bar animated mini-EQ indicator (playing state).
const MINI_EQ_CSS: &str = "\
.mini-eq { \
  display: flex; \
  align-items: flex-end; \
  gap: 2px; \
}\
.mini-eq > box { \
  background-color: @reprise_player_accent; \
  border-radius: 1px; \
  min-width: 3px; \
  min-height: 3px; \
  animation-duration: 600ms; \
  animation-timing-function: ease-in-out; \
  animation-iteration-count: infinite; \
  animation-direction: alternate; \
}\
.mini-eq.playing > box:nth-child(1) { animation-name: mini-eq-bar; animation-delay: 0ms; }\
.mini-eq.playing > box:nth-child(2) { animation-name: mini-eq-bar; animation-delay: 200ms; }\
.mini-eq.playing > box:nth-child(3) { animation-name: mini-eq-bar; animation-delay: 400ms; }\
@keyframes mini-eq-bar { \
  from { min-height: 4px; } \
  to   { min-height: 14px; } \
}";

pub(super) struct PlayerBarWidgets {
    pub(super) root: gtk4::Box,
    pub(super) center_box: gtk4::CenterBox,
    pub(super) info_box: gtk4::Box,
    pub(super) cover: gtk4::Image,
    pub(super) title_label: gtk4::Label,
    pub(super) artist_label: gtk4::Label,
    pub(super) mini_eq: gtk4::Box,
    pub(super) shuffle_button: gtk4::ToggleButton,
    pub(super) prev_button: gtk4::Button,
    pub(super) play_pause_button: gtk4::Button,
    pub(super) next_button: gtk4::Button,
    pub(super) repeat_button: gtk4::Button,
    pub(super) position_label: gtk4::Label,
    pub(super) duration_label: gtk4::Label,
    pub(super) waveform: super::waveform_seek::WaveformSeek,
    pub(super) volume_icon: gtk4::Button,
    pub(super) volume_scale: gtk4::Scale,
    pub(super) queue_button: gtk4::Button,
}

pub(super) fn build() -> PlayerBarWidgets {
    // — Cover —
    let cover = gtk4::Image::new();
    cover.set_pixel_size(COVER_PIXEL_SIZE);
    cover.add_css_class(COVER_CSS_CLASS);
    CoverLoader::set_placeholder(&cover);

    // — Track labels —
    let title_label = build_track_label();
    title_label.add_css_class("player-bar-title");

    let artist_label = build_track_label();
    artist_label.add_css_class("player-bar-artist");

    // — Mini-EQ (3 animated bars, toggled via "playing" CSS class) —
    let mini_eq = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    mini_eq.add_css_class("mini-eq");
    mini_eq.set_valign(gtk4::Align::Center);
    for _ in 0..3 {
        let bar = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        mini_eq.append(&bar);
    }

    let track_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    track_box.append(&title_label);
    track_box.append(&artist_label);
    track_box.set_valign(gtk4::Align::Center);

    // — Start zone (cover + mini-eq + track info) —
    let info_box = gtk4::Box::new(gtk4::Orientation::Horizontal, ZONE_SPACING);
    info_box.append(&cover);
    info_box.append(&mini_eq);
    info_box.append(&track_box);
    info_box.set_valign(gtk4::Align::Center);
    info_box.set_width_request(START_ZONE_WIDTH);

    // — Transport controls —
    let shuffle_button = gtk4::ToggleButton::builder()
        .icon_name(ICON_SHUFFLE)
        .tooltip_text(strings::text(strings::SHUFFLE))
        .valign(gtk4::Align::Center)
        .build();
    let prev_button = transport_button(ICON_PREVIOUS, strings::PREVIOUS);
    prev_button.set_sensitive(false);
    let play_pause_button = transport_button(ICON_PLAY, strings::PLAY);
    // The play/pause control is the accent-glow focal point of the bar.
    play_pause_button.add_css_class("circular");
    play_pause_button.add_css_class(PLAY_CSS_CLASS);
    let next_button = transport_button(ICON_NEXT, strings::NEXT);
    next_button.set_sensitive(false);
    let repeat_button = transport_button(ICON_REPEAT_ALL, strings::REPEAT);

    let transport_row = gtk4::Box::new(gtk4::Orientation::Horizontal, ZONE_SPACING);
    transport_row.append(&shuffle_button);
    transport_row.append(&prev_button);
    transport_row.append(&play_pause_button);
    transport_row.append(&next_button);
    transport_row.append(&repeat_button);
    transport_row.set_halign(gtk4::Align::Center);

    // — Seek row —
    let position_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));
    position_label.add_css_class("player-bar-time");
    let duration_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));
    duration_label.add_css_class("player-bar-time");

    let waveform = super::waveform_seek::WaveformSeek::new();
    waveform
        .widget()
        .set_tooltip_text(Some(&strings::text(strings::PLAYBACK_POSITION)));

    let seek_row = gtk4::Box::new(gtk4::Orientation::Horizontal, ZONE_SPACING);
    seek_row.append(&position_label);
    seek_row.append(waveform.widget());
    seek_row.append(&duration_label);
    seek_row.set_hexpand(true);

    // — Center zone (transport + seek) —
    let center_zone = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    center_zone.append(&transport_row);
    center_zone.append(&seek_row);
    center_zone.set_hexpand(true);
    center_zone.set_size_request(CENTER_ZONE_MAX_WIDTH, -1);
    center_zone.set_valign(gtk4::Align::Center);

    // — End zone (volume icon + slider + queue button) —
    let volume_scale =
        gtk4::Scale::with_range(gtk4::Orientation::Horizontal, VOLUME_MIN, VOLUME_MAX, VOLUME_STEP);
    volume_scale.set_value(VOLUME_DEFAULT);
    volume_scale.set_draw_value(false);
    volume_scale.set_width_request(VOLUME_SLIDER_WIDTH);
    volume_scale.set_tooltip_text(Some(&strings::text(strings::VOLUME)));
    volume_scale.set_valign(gtk4::Align::Center);

    let volume_icon = gtk4::Button::from_icon_name(ICON_VOLUME_HIGH);
    volume_icon.set_tooltip_text(Some(&strings::text(strings::VOLUME)));
    volume_icon.set_valign(gtk4::Align::Center);
    volume_icon.add_css_class("flat");

    let queue_button = gtk4::Button::from_icon_name(ICON_QUEUE);
    queue_button.set_tooltip_text(Some(&strings::text(strings::QUEUE)));
    queue_button.set_valign(gtk4::Align::Center);
    queue_button.add_css_class("flat");

    let end_zone = gtk4::Box::new(gtk4::Orientation::Horizontal, ZONE_SPACING);
    end_zone.append(&volume_icon);
    end_zone.append(&volume_scale);
    end_zone.append(&queue_button);
    end_zone.set_valign(gtk4::Align::Center);
    end_zone.set_halign(gtk4::Align::End);
    end_zone.set_width_request(END_ZONE_WIDTH);

    // — CenterBox assembles the three zones —
    let center_box = gtk4::CenterBox::new();
    center_box.set_start_widget(Some(&info_box));
    center_box.set_center_widget(Some(&center_zone));
    center_box.set_end_widget(Some(&end_zone));
    center_box.set_hexpand(true);

    // — Root wrapper gives us height-request without fighting CenterBox —
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.add_css_class(SURFACE_CSS_CLASS);
    root.append(&center_box);
    root.set_height_request(BAR_HEIGHT);
    root.set_sensitive(false);

    PlayerBarWidgets {
        root,
        center_box,
        info_box,
        cover,
        title_label,
        artist_label,
        mini_eq,
        shuffle_button,
        prev_button,
        play_pause_button,
        next_button,
        repeat_button,
        position_label,
        duration_label,
        waveform,
        volume_icon,
        volume_scale,
        queue_button,
    }
}

/// Player-bar chrome CSS: accent-glow play button, hairline top border, cover
/// border-radius, title/artist/time label styling, and mini-EQ keyframes.
pub(super) fn css() -> String {
    use super::style::tokens::TRANSITION;
    format!(
        ".{SURFACE_CSS_CLASS} {{ \
           background-color: rgba(26, 26, 26, 0.92); \
           border-top: 1px solid alpha(@window_fg_color, 0.07); }}\n\
         .{PLAY_CSS_CLASS} {{ \
           min-width: {PLAY_BUTTON_SIZE}px; min-height: {PLAY_BUTTON_SIZE}px; \
           background-color: @reprise_player_accent; color: #ffffff; \
           box-shadow: 0 0 16px alpha(@reprise_player_accent, 0.40); \
           transition: box-shadow {TRANSITION}, background-color {TRANSITION}, \
                       transform 120ms ease-out; }}\n\
         .{PLAY_CSS_CLASS}:hover {{ \
           box-shadow: 0 0 20px alpha(@reprise_player_accent, 0.55); }}\n\
         .{PLAY_CSS_CLASS}:active {{ transform: scale(0.94); }}\n\
         .{COVER_CSS_CLASS} {{ \
           border-radius: 8px; \
           box-shadow: inset 0 0 0 1px alpha(white, 0.08); }}\n\
         .player-bar-title {{ font-weight: bold; font-size: 13.5px; }}\n\
         .player-bar-artist {{ color: alpha(@window_fg_color, 0.50); font-size: 12px; }}\n\
         .player-bar-time {{ font-feature-settings: \"tnum\"; }}\n\
         .waveform-seek {{ color: @reprise_player_accent; }}\n\
         {MINI_EQ_CSS}"
    )
}

fn transport_button(icon: &str, tooltip: &str) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(&strings::text(tooltip)));
    button.set_valign(gtk4::Align::Center);
    button
}

fn build_track_label() -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.set_halign(gtk4::Align::Start);
    label.set_ellipsize(pango::EllipsizeMode::End);
    label.set_xalign(0.0);
    label
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gtk4::prelude::*;

    use super::build;

    fn wait_for_layout() {
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        gtk4::glib::timeout_add_local_once(Duration::from_millis(50), move || quit.quit());
        main_loop.run();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn library_bar_has_three_zones_via_centerbox() {
        if gtk4::init().is_err() {
            return;
        }
        let layout = build();
        let window = gtk4::Window::builder()
            .default_width(1_200)
            .child(&layout.root)
            .build();
        window.set_size_request(1_200, -1);
        window.present();
        wait_for_layout();

        assert_eq!(layout.root.width(), 1_200);
        // Start zone: info_box is the start widget of center_box.
        assert_eq!(
            layout.center_box.start_widget(),
            Some(layout.info_box.clone().upcast())
        );
        assert!(layout.cover.is_ancestor(&layout.info_box));
        assert!(layout.title_label.is_ancestor(&layout.info_box));
        assert!(layout.artist_label.is_ancestor(&layout.info_box));
        assert!(layout.mini_eq.is_ancestor(&layout.info_box));
        // Transport controls are within the center zone.
        assert!(layout.shuffle_button.is_ancestor(&layout.root));
        assert!(layout.play_pause_button.is_ancestor(&layout.root));
        assert!(layout.prev_button.is_ancestor(&layout.root));
        assert!(layout.next_button.is_ancestor(&layout.root));
        assert!(layout.repeat_button.is_ancestor(&layout.root));
        // Seek row widgets are present.
        assert!(layout.position_label.is_ancestor(&layout.root));
        assert!(layout.duration_label.is_ancestor(&layout.root));
        assert!(layout.waveform.widget().is_ancestor(&layout.root));
        // End zone has volume and queue controls.
        assert!(layout.volume_scale.is_ancestor(&layout.root));
        assert!(layout.volume_icon.is_ancestor(&layout.root));
        assert!(layout.queue_button.is_ancestor(&layout.root));
        window.close();
    }

    #[test]
    fn css_styles_the_glow_play_button_and_surface() {
        let css = super::css();
        assert!(css.contains(".player-bar-play"));
        assert!(css.contains("@reprise_player_accent"));
        assert!(css.contains(".player-bar-surface"));
    }

    #[test]
    fn css_includes_new_cover_and_label_classes() {
        let css = super::css();
        assert!(css.contains(".player-bar-cover"));
        assert!(css.contains(".player-bar-title"));
        assert!(css.contains(".player-bar-artist"));
        assert!(css.contains("border-radius: 8px"));
        assert!(css.contains("scale(0.94)"));
    }

    #[test]
    fn css_includes_mini_eq_keyframes() {
        let css = super::css();
        assert!(css.contains("mini-eq"));
        assert!(css.contains("mini-eq-bar"));
    }
}
