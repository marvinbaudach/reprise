//! Widget construction for the full-width Library player bar.

use gtk4::{pango, prelude::*};

use super::cover_loader::CoverLoader;
use super::player_bar::{ICON_NEXT, ICON_PLAY, ICON_PREVIOUS, ICON_REPEAT_ALL, ICON_SHUFFLE};
use super::strings;

const VOLUME_ICONS: [&str; 4] = [
    "audio-volume-muted-symbolic",
    "audio-volume-low-symbolic",
    "audio-volume-medium-symbolic",
    "audio-volume-high-symbolic",
];
pub(super) const VOLUME_MIN: f64 = 0.0;
pub(super) const VOLUME_MAX: f64 = 1.0;
const VOLUME_STEP: f64 = 0.05;
const VOLUME_DEFAULT: f64 = 1.0;
const TRACK_INFO_WIDTH: i32 = 220;
const ZERO_TIME_LABEL: &str = "0:00";
const ZONE_SPACING: i32 = 8;
const COVER_PIXEL_SIZE: i32 = 48;
const COVER_CSS_CLASS: &str = "player-bar-cover";
const PLAY_CSS_CLASS: &str = "player-bar-play";
const SURFACE_CSS_CLASS: &str = "player-bar-surface";

pub(super) struct PlayerBarWidgets {
    pub(super) root: gtk4::ActionBar,
    pub(super) info_box: gtk4::Box,
    #[cfg(test)]
    pub(super) center_zone: gtk4::Box,
    #[cfg(test)]
    pub(super) transport_row: gtk4::Box,
    #[cfg(test)]
    pub(super) seek_row: gtk4::Box,
    #[cfg(test)]
    pub(super) secondary_zone: gtk4::Box,
    pub(super) cover: gtk4::Image,
    pub(super) title_label: gtk4::Label,
    pub(super) artist_label: gtk4::Label,
    pub(super) shuffle_button: gtk4::ToggleButton,
    pub(super) prev_button: gtk4::Button,
    pub(super) play_pause_button: gtk4::Button,
    pub(super) next_button: gtk4::Button,
    pub(super) repeat_button: gtk4::Button,
    pub(super) position_label: gtk4::Label,
    pub(super) duration_label: gtk4::Label,
    pub(super) scale: gtk4::Scale,
    pub(super) volume_button: gtk4::ScaleButton,
}

pub(super) fn build() -> PlayerBarWidgets {
    let cover = gtk4::Image::new();
    cover.set_pixel_size(COVER_PIXEL_SIZE);
    cover.add_css_class(COVER_CSS_CLASS);
    CoverLoader::set_placeholder(&cover);

    let title_label = build_track_label();
    let bold = pango::AttrList::new();
    bold.insert(pango::AttrInt::new_weight(pango::Weight::Bold));
    title_label.set_attributes(Some(&bold));

    let artist_label = build_track_label();
    artist_label.add_css_class("dim-label");

    let track_box = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    track_box.append(&title_label);
    track_box.append(&artist_label);
    track_box.set_valign(gtk4::Align::Center);
    track_box.set_width_request(TRACK_INFO_WIDTH);

    let info_box = gtk4::Box::new(gtk4::Orientation::Horizontal, ZONE_SPACING);
    info_box.append(&cover);
    info_box.append(&track_box);
    info_box.set_valign(gtk4::Align::Center);

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

    // Mock layout: shuffle · prev · play · next · repeat grouped and centered.
    let transport_row = gtk4::Box::new(gtk4::Orientation::Horizontal, ZONE_SPACING);
    transport_row.append(&shuffle_button);
    transport_row.append(&prev_button);
    transport_row.append(&play_pause_button);
    transport_row.append(&next_button);
    transport_row.append(&repeat_button);
    transport_row.set_halign(gtk4::Align::Center);

    let position_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));
    let duration_label = gtk4::Label::new(Some(ZERO_TIME_LABEL));
    let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, None::<&gtk4::Adjustment>);
    scale.set_range(0.0, 1.0);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_valign(gtk4::Align::Center);
    scale.set_tooltip_text(Some(&strings::text(strings::PLAYBACK_POSITION)));

    let seek_row = gtk4::Box::new(gtk4::Orientation::Horizontal, ZONE_SPACING);
    seek_row.append(&position_label);
    seek_row.append(&scale);
    seek_row.append(&duration_label);
    seek_row.set_hexpand(true);

    let center_zone = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    center_zone.append(&transport_row);
    center_zone.append(&seek_row);
    center_zone.set_hexpand(true);
    center_zone.set_valign(gtk4::Align::Center);

    let volume_button = gtk4::ScaleButton::new(VOLUME_MIN, VOLUME_MAX, VOLUME_STEP, &VOLUME_ICONS);
    volume_button.set_value(VOLUME_DEFAULT);
    volume_button.set_tooltip_text(Some(&strings::text(strings::VOLUME)));
    volume_button.set_valign(gtk4::Align::Center);

    let secondary_zone = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
    secondary_zone.append(&volume_button);
    secondary_zone.set_valign(gtk4::Align::Center);

    let root = gtk4::ActionBar::new();
    root.add_css_class(SURFACE_CSS_CLASS);
    root.pack_start(&info_box);
    root.set_center_widget(Some(&center_zone));
    root.pack_end(&secondary_zone);
    root.set_sensitive(false);

    PlayerBarWidgets {
        root,
        info_box,
        #[cfg(test)]
        center_zone,
        #[cfg(test)]
        transport_row,
        #[cfg(test)]
        seek_row,
        #[cfg(test)]
        secondary_zone,
        cover,
        title_label,
        artist_label,
        shuffle_button,
        prev_button,
        play_pause_button,
        next_button,
        repeat_button,
        position_label,
        duration_label,
        scale,
        volume_button,
    }
}

/// Player-bar chrome CSS: the accent-glow circular play button and a hairline
/// top border on the bar surface. Installed app-wide by [`super::style`]; the
/// glow reads `@reprise_player_accent`, so it recolors with the active theme.
pub(super) fn css() -> String {
    use super::style::tokens::TRANSITION;
    format!(
        ".{PLAY_CSS_CLASS} {{ \
           min-width: 40px; min-height: 40px; \
           background-color: @reprise_player_accent; color: #ffffff; \
           box-shadow: 0 0 14px alpha(@reprise_player_accent, 0.45); \
           transition: box-shadow {TRANSITION}, background-color {TRANSITION}; }}\n\
         .{PLAY_CSS_CLASS}:hover {{ \
           box-shadow: 0 0 18px alpha(@reprise_player_accent, 0.6); }}\n\
         .{SURFACE_CSS_CLASS} {{ \
           border-top: 1px solid alpha(@window_fg_color, 0.06); }}"
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
    fn library_bar_has_distinct_info_transport_seek_and_secondary_zones() {
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
        assert!(layout.info_box.is_ancestor(&layout.root));
        assert!(layout.cover.is_ancestor(&layout.info_box));
        assert!(layout.title_label.is_ancestor(&layout.info_box));
        assert!(layout.artist_label.is_ancestor(&layout.info_box));
        assert_eq!(
            layout.center_zone.first_child(),
            Some(layout.transport_row.clone().upcast())
        );
        assert_eq!(
            layout.center_zone.last_child(),
            Some(layout.seek_row.clone().upcast())
        );
        assert_eq!(
            layout.transport_row.first_child(),
            Some(layout.shuffle_button.clone().upcast())
        );
        assert_eq!(
            layout.transport_row.last_child(),
            Some(layout.repeat_button.clone().upcast())
        );
        assert!(layout.play_pause_button.is_ancestor(&layout.transport_row));
        assert!(layout.shuffle_button.is_ancestor(&layout.transport_row));
        assert!(layout.repeat_button.is_ancestor(&layout.transport_row));
        assert!(!layout.volume_button.is_ancestor(&layout.center_zone));
        assert_eq!(
            layout.seek_row.first_child(),
            Some(layout.position_label.clone().upcast())
        );
        assert_eq!(
            layout.seek_row.last_child(),
            Some(layout.duration_label.clone().upcast())
        );
        assert!(layout.scale.is_ancestor(&layout.seek_row));
        assert!(layout.scale.width() > 0);
        assert!(!layout.shuffle_button.is_ancestor(&layout.secondary_zone));
        assert!(!layout.repeat_button.is_ancestor(&layout.secondary_zone));
        assert!(layout.volume_button.is_ancestor(&layout.secondary_zone));
        window.close();
    }

    #[test]
    fn css_styles_the_glow_play_button_and_surface() {
        let css = super::css();
        assert!(css.contains(".player-bar-play"));
        assert!(css.contains("@reprise_player_accent"));
        assert!(css.contains(".player-bar-surface"));
    }
}
