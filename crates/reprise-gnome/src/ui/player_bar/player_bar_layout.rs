//! Widget construction for the full-width Library player bar.

use gtk4::{pango, prelude::*};
use libadwaita as adw;
use libadwaita::prelude::*;

use super::cover_loader::CoverLoader;
use super::strings;
use super::{ICON_NEXT, ICON_PLAY, ICON_PREVIOUS, ICON_REPEAT_ALL, ICON_SHUFFLE};
use crate::ui::playing_marker;
use crate::ui::style::buttons;

pub(in crate::ui) const VOLUME_MIN: f64 = 0.0;
pub(in crate::ui) const VOLUME_MAX: f64 = 1.0;
const VOLUME_STEP: f64 = 0.05;
const VOLUME_DEFAULT: f64 = 1.0;

const COVER_PIXEL_SIZE: i32 = 56;
const BAR_HEIGHT: i32 = 86;
// The info zone also owns a 12 px leading margin. Keep its requested content
// width below 300 so the complete 1,170 px three-zone layout fits inside a
// 1,200 px decorated window without a transient two-pixel overflow.
const START_ZONE_WIDTH: i32 = 288;
const END_ZONE_WIDTH: i32 = 250;
const CENTER_ZONE_MAX_WIDTH: i32 = 620;
const NARROW_BREAKPOINT_WIDTH: i32 = 900;
const NARROW_START_ZONE_WIDTH: i32 = 160;
const NARROW_END_ZONE_WIDTH: i32 = 172;
const TRACK_INFO_MAX_WIDTH: i32 = 220;
const NARROW_TRACK_INFO_MAX_WIDTH: i32 = 84;
const VOLUME_SLIDER_WIDTH: i32 = 80;
const PLAY_BUTTON_SIZE: i32 = 44;
const TRACK_LABEL_MAX_CHARS: i32 = 22;

const ZERO_TIME_LABEL: &str = "0:00";
const ZONE_SPACING: i32 = 8;

const COVER_CSS_CLASS: &str = "player-bar-cover";
const PLAY_CSS_CLASS: &str = "player-bar-play";
const SURFACE_CSS_CLASS: &str = "player-bar-surface";
/// CSS class on the transport button row, targeted by the hover-highlight rules.
const TRANSPORT_ROW_CSS_CLASS: &str = "player-bar-transport";
/// CSS class on the volume scale, toggled on hover to reveal the knob.
const VOLUME_SCALE_CSS_CLASS: &str = "player-bar-volume";
/// CSS class added/removed by the hover controller to show the volume knob.
pub(in crate::ui) const KNOB_VISIBLE_CSS_CLASS: &str = "knob-visible";

const ICON_VOLUME_HIGH: &str = "audio-volume-high-symbolic";

pub(in crate::ui) struct PlayerBarWidgets {
    pub(in crate::ui) root: gtk4::Box,
    #[allow(dead_code)]
    pub(in crate::ui) center_box: gtk4::CenterBox,
    #[allow(dead_code)]
    pub(in crate::ui) info_box: gtk4::Box,
    pub(in crate::ui) cover: gtk4::Image,
    pub(in crate::ui) cover_button: gtk4::Button,
    pub(in crate::ui) title_label: gtk4::Label,
    pub(in crate::ui) title_button: gtk4::Button,
    pub(in crate::ui) artist_label: gtk4::Label,
    pub(in crate::ui) artist_button: gtk4::Button,
    pub(in crate::ui) mini_eq: gtk4::Box,
    pub(in crate::ui) shuffle_button: gtk4::ToggleButton,
    pub(in crate::ui) prev_button: gtk4::Button,
    pub(in crate::ui) play_pause_button: gtk4::Button,
    pub(in crate::ui) next_button: gtk4::Button,
    pub(in crate::ui) repeat_button: gtk4::ToggleButton,
    pub(in crate::ui) play_next_episode_button: gtk4::Button,
    pub(in crate::ui) retry_external_button: gtk4::Button,
    pub(in crate::ui) position_label: gtk4::Label,
    pub(in crate::ui) duration_label: gtk4::Label,
    pub(in crate::ui) waveform: super::waveform_seek::WaveformSeek,
    pub(in crate::ui) volume_icon: gtk4::Button,
    pub(in crate::ui) volume_scale: gtk4::Scale,
}

pub(in crate::ui) fn build() -> PlayerBarWidgets {
    // — Cover —
    let cover = gtk4::Image::new();
    cover.set_pixel_size(COVER_PIXEL_SIZE);
    cover.add_css_class(COVER_CSS_CLASS);
    CoverLoader::set_placeholder(&cover);
    let cover_button = gtk4::Button::builder()
        .child(&cover)
        .has_frame(false)
        .tooltip_text(strings::text(strings::REVEAL_PLAYING_ALBUM))
        .build();
    cover_button.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::REVEAL_PLAYING_ALBUM,
    ))]);

    // — Track labels —
    let title_label = build_track_label();
    title_label.add_css_class("player-bar-title");

    let artist_label = build_track_label();
    artist_label.add_css_class("player-bar-artist");

    // — Shared persistent playing marker —
    let mini_eq = playing_marker::build();
    mini_eq.add_css_class("mini-eq");

    // Title row: title label + mini-EQ side by side (spec 1.5: "neben dem Titel").
    let title_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    title_row.append(&title_label);
    title_row.append(&mini_eq);
    title_row.set_valign(gtk4::Align::Center);
    let title_button = gtk4::Button::builder()
        .child(&title_row)
        .has_frame(false)
        .tooltip_text(strings::text(strings::JUMP_TO_NOW_PLAYING))
        .build();
    title_button.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::JUMP_TO_NOW_PLAYING,
    ))]);
    let artist_button = gtk4::Button::builder()
        .child(&artist_label)
        .has_frame(false)
        .tooltip_text(strings::text(strings::GO_TO_PLAYING_ARTIST))
        .build();

    let track_box = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    track_box.append(&title_button);
    track_box.append(&artist_button);
    track_box.set_valign(gtk4::Align::Center);
    let track_info_clamp = adw::Clamp::builder()
        .orientation(gtk4::Orientation::Horizontal)
        .maximum_size(TRACK_INFO_MAX_WIDTH)
        .tightening_threshold(TRACK_INFO_MAX_WIDTH)
        .child(&track_box)
        .build();
    track_info_clamp.set_hexpand(true);

    // — Start zone (cover + track info) —
    let info_box = gtk4::Box::new(gtk4::Orientation::Horizontal, ZONE_SPACING);
    info_box.set_margin_start(12);
    info_box.append(&cover_button);
    info_box.append(&track_info_clamp);
    info_box.set_valign(gtk4::Align::Center);
    info_box.set_width_request(START_ZONE_WIDTH);

    // — Transport controls —
    // Shuffle and Repeat are both toggles, so both speak the one `:checked`
    // state language from `style::buttons` (BTN-2) — Repeat's three modes ride
    // on top of it via the icon swap in `set_repeat_indicator`.
    let shuffle_button = transport_toggle(ICON_SHUFFLE, strings::SHUFFLE);
    let prev_button = transport_button(ICON_PREVIOUS, strings::TOOLTIP_PREVIOUS);
    prev_button.set_sensitive(false);
    // The play/pause control is the accent-glow focal point of the bar and the
    // primary tier of BTN-3: it may react more visibly than its neighbours.
    let play_pause_button = gtk4::Button::from_icon_name(ICON_PLAY);
    play_pause_button.set_tooltip_text(Some(&strings::text(strings::TOOLTIP_PLAY)));
    play_pause_button.set_valign(gtk4::Align::Center);
    play_pause_button.add_css_class("circular");
    play_pause_button.add_css_class(PLAY_CSS_CLASS);
    buttons::arm(&play_pause_button, buttons::PRIMARY_CLASS);
    let next_button = transport_button(ICON_NEXT, strings::TOOLTIP_NEXT);
    next_button.set_sensitive(false);
    let repeat_button = transport_toggle(ICON_REPEAT_ALL, strings::REPEAT);
    let play_next_episode_button =
        gtk4::Button::with_label(&strings::text(strings::PODCAST_PLAY_NEXT_EPISODE));
    play_next_episode_button.set_visible(false);
    buttons::arm(&play_next_episode_button, buttons::ADD_ACTION_CLASS);
    let retry_external_button = gtk4::Button::with_label(&strings::text(strings::RADIO_RETRY));
    retry_external_button.set_visible(false);
    buttons::arm(&retry_external_button, buttons::ADD_ACTION_CLASS);

    let transport_row = gtk4::Box::new(gtk4::Orientation::Horizontal, ZONE_SPACING);
    transport_row.append(&shuffle_button);
    transport_row.append(&prev_button);
    transport_row.append(&play_pause_button);
    transport_row.append(&next_button);
    transport_row.append(&repeat_button);
    transport_row.append(&play_next_episode_button);
    transport_row.append(&retry_external_button);
    transport_row.set_halign(gtk4::Align::Center);
    // CSS class lets transport hover rules target only these buttons (spec 1.5).
    transport_row.add_css_class(TRANSPORT_ROW_CSS_CLASS);

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

    // — End zone (volume icon + slider) —
    let volume_scale = gtk4::Scale::with_range(
        gtk4::Orientation::Horizontal,
        VOLUME_MIN,
        VOLUME_MAX,
        VOLUME_STEP,
    );
    volume_scale.set_value(VOLUME_DEFAULT);
    volume_scale.set_draw_value(false);
    volume_scale.set_width_request(VOLUME_SLIDER_WIDTH);
    volume_scale.set_tooltip_text(Some(&strings::text(strings::VOLUME)));
    volume_scale.set_valign(gtk4::Align::Center);
    // CSS class so knob-hiding rules target this scale specifically.
    volume_scale.add_css_class(VOLUME_SCALE_CSS_CLASS);

    // Scroll ±5 % per tick on the volume slider (spec 1.5).
    // input-parity: ACC-8 keyboard=native-range
    let volume_scroll =
        gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    volume_scroll.connect_scroll({
        let volume_scale = volume_scale.clone();
        move |_, _dx, dy| {
            let new_val = (volume_scale.value() - dy * VOLUME_STEP).clamp(VOLUME_MIN, VOLUME_MAX);
            volume_scale.set_value(new_val);
            gtk4::glib::Propagation::Stop
        }
    });
    volume_scale.add_controller(volume_scroll);

    // Hover controller reveals the knob (spec 1.5: "Knob nur bei Hover sichtbar").
    let knob_motion = gtk4::EventControllerMotion::new();
    knob_motion.connect_enter({
        let volume_scale = volume_scale.clone();
        move |_, _, _| {
            volume_scale.add_css_class(KNOB_VISIBLE_CSS_CLASS);
        }
    });
    knob_motion.connect_leave({
        let volume_scale = volume_scale.clone();
        move |_| {
            volume_scale.remove_css_class(KNOB_VISIBLE_CSS_CLASS);
        }
    });
    volume_scale.add_controller(knob_motion);

    let volume_icon = gtk4::Button::from_icon_name(ICON_VOLUME_HIGH);
    volume_icon.set_tooltip_text(Some(&strings::text(strings::VOLUME)));
    volume_icon.set_valign(gtk4::Align::Center);
    volume_icon.add_css_class("flat");

    let end_zone = gtk4::Box::new(gtk4::Orientation::Horizontal, ZONE_SPACING);
    end_zone.append(&volume_icon);
    end_zone.append(&volume_scale);
    end_zone.set_valign(gtk4::Align::Center);
    end_zone.set_halign(gtk4::Align::End);
    end_zone.set_width_request(END_ZONE_WIDTH);

    // — CenterBox assembles the three zones —
    let center_box = gtk4::CenterBox::new();
    center_box.set_start_widget(Some(&info_box));
    center_box.set_center_widget(Some(&center_zone));
    center_box.set_end_widget(Some(&end_zone));
    center_box.set_hexpand(true);

    // Preserve the spacious three-zone proportions at normal widths while
    // letting every zone surrender its decorative width reservation before it
    // can force the containing window wider.
    let responsive = adw::BreakpointBin::new();
    responsive.set_size_request(1, BAR_HEIGHT);
    responsive.set_child(Some(&center_box));
    let narrow = adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        f64::from(NARROW_BREAKPOINT_WIDTH),
        adw::LengthUnit::Px,
    );
    let breakpoint = adw::Breakpoint::new(narrow);
    breakpoint.add_setter(
        &info_box,
        "width-request",
        Some(&NARROW_START_ZONE_WIDTH.to_value()),
    );
    breakpoint.add_setter(
        &track_info_clamp,
        "maximum-size",
        Some(&NARROW_TRACK_INFO_MAX_WIDTH.to_value()),
    );
    breakpoint.add_setter(
        &track_info_clamp,
        "tightening-threshold",
        Some(&NARROW_TRACK_INFO_MAX_WIDTH.to_value()),
    );
    breakpoint.add_setter(&center_zone, "width-request", Some(&(-1_i32).to_value()));
    breakpoint.add_setter(
        &end_zone,
        "width-request",
        Some(&NARROW_END_ZONE_WIDTH.to_value()),
    );
    responsive.add_breakpoint(breakpoint);

    // — Root wrapper gives us height-request without fighting CenterBox —
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.add_css_class(SURFACE_CSS_CLASS);
    root.append(&responsive);
    root.set_height_request(BAR_HEIGHT);
    root.set_sensitive(false);

    PlayerBarWidgets {
        root,
        center_box,
        info_box,
        cover,
        cover_button,
        title_label,
        title_button,
        artist_label,
        artist_button,
        mini_eq,
        shuffle_button,
        prev_button,
        play_pause_button,
        next_button,
        repeat_button,
        play_next_episode_button,
        retry_external_button,
        position_label,
        duration_label,
        waveform,
        volume_icon,
        volume_scale,
    }
}

/// Player-bar chrome CSS: accent-glow play button, hairline top border, cover
/// border-radius, title/artist/time label styling, transport hover, mini-EQ
/// animation, volume-knob visibility, and artist-label hover colour.
pub(in crate::ui) fn css() -> String {
    use super::{motion, style::tokens::TRANSITION};
    let micro_ms = motion::MICRO_MS;
    let micro_easing = motion::MICRO_CSS_EASING;
    format!(
        ".{SURFACE_CSS_CLASS} {{ \
           background-color: @headerbar_bg_color; \
           border-top: 1px solid alpha(@window_fg_color, 0.07); }}\n\
         .{PLAY_CSS_CLASS} {{ \
           min-width: {PLAY_BUTTON_SIZE}px; min-height: {PLAY_BUTTON_SIZE}px; \
           background-color: @reprise_player_accent; color: #ffffff; \
           box-shadow: inset 0 2px 1px alpha(#ffffff, 0.34), \
                       inset 0 -4px 3px alpha(#000000, 0.30), \
                       0 6px 12px alpha(#000000, 0.36), \
                       0 0 12px alpha(@reprise_player_accent, 0.60), \
                       0 0 26px 6px alpha(@reprise_player_accent, 0.35); \
           transition: box-shadow {TRANSITION}, background-color {TRANSITION}, \
                       transform {TRANSITION}; }}\n\
         .{PLAY_CSS_CLASS}:hover {{ \
           box-shadow: inset 0 2px 1px alpha(#ffffff, 0.42), \
                       inset 0 -4px 3px alpha(#000000, 0.26), \
                       0 7px 14px alpha(#000000, 0.34), \
                       0 0 16px alpha(@reprise_player_accent, 0.75), \
                       0 0 34px 8px alpha(@reprise_player_accent, 0.48); }}\n\
         /* BTN-3: the main action may answer a press more loudly than its \
            neighbours — a ring pulse in the playback accent on top of the \
            shared press sink from `style::buttons`. */\n\
         .{PLAY_CSS_CLASS}:active {{ \
           box-shadow: inset 0 4px 6px alpha(#000000, 0.44), \
                       inset 0 -1px 0 alpha(#ffffff, 0.12), \
                       0 1px 2px alpha(#000000, 0.22), \
                       0 0 0 4px alpha(@reprise_player_accent, 0.45), \
                       0 0 18px alpha(@reprise_player_accent, 0.80); }}\n\
         .{PLAY_CSS_CLASS}.pulsing {{ \
           animation: reprise-play-pulse {micro_ms}ms {micro_easing} 1; }}\n\
         @keyframes reprise-play-pulse {{ \
           0%   {{ transform: scale(1.0); }} \
           50%  {{ transform: scale(0.92); }} \
           100% {{ transform: scale(1.0); }} }}\n\
         .{COVER_CSS_CLASS} {{ \
           border-radius: 8px; \
           box-shadow: inset 0 0 0 1px alpha(white, 0.08); \
           opacity: 0.92; transition: opacity {TRANSITION}; }}\n\
         .{COVER_CSS_CLASS}.hovered {{ opacity: 1.0; }}\n\
         .player-bar-title {{ font-weight: bold; font-size: 13.5px; }}\n\
         .player-bar-artist {{ \
           color: alpha(@window_fg_color, 0.82); font-size: 12px; \
           transition: color {TRANSITION}; }}\n\
         .player-bar-artist.artist-hovered {{ color: @window_fg_color; }}\n\
         .player-bar-time {{ font-feature-settings: \"tnum\"; }}\n\
         .waveform-seek {{ color: @reprise_player_accent; }}\n\
         /* Shape only. Hover, press, focus and the checked state all come from \
            the one central set in `style::buttons` (BTN-4) — no local tint. */\n\
         .{TRANSPORT_ROW_CSS_CLASS} button {{ border-radius: 50%; }}\n\
         .{TRANSPORT_ROW_CSS_CLASS} button.reprise-btn-add {{ border-radius: 8px; }}\n\
         .{VOLUME_SCALE_CSS_CLASS} trough > slider {{ \
           opacity: 0; transition: opacity {TRANSITION}; }}\n\
         .{VOLUME_SCALE_CSS_CLASS}.{KNOB_VISIBLE_CSS_CLASS} trough > slider {{ opacity: 1; }}\n\
         .mini-eq {{ margin-left: 4px; }}"
    )
}

/// A standard-tier transport button (BTN-3): flat at rest, carrying the shared
/// hover/press/focus vocabulary from [`buttons`] rather than a local tint.
fn transport_button(icon: &str, tooltip: &str) -> gtk4::Button {
    let button = gtk4::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(&strings::text(tooltip)));
    button.set_valign(gtk4::Align::Center);
    button.add_css_class("flat");
    buttons::arm(&button, buttons::ICON_CLASS);
    button
}

/// The toggle variant of [`transport_button`], adding the persistent
/// `:checked` state display (BTN-2).
fn transport_toggle(icon: &str, tooltip: &str) -> gtk4::ToggleButton {
    let button = gtk4::ToggleButton::builder()
        .icon_name(icon)
        .tooltip_text(strings::text(tooltip))
        .valign(gtk4::Align::Center)
        .build();
    button.add_css_class("flat");
    buttons::arm(&button, buttons::ICON_CLASS);
    buttons::arm(&button, buttons::TOGGLE_CLASS);
    button
}

fn build_track_label() -> gtk4::Label {
    let label = gtk4::Label::new(None);
    label.set_halign(gtk4::Align::Start);
    label.set_ellipsize(pango::EllipsizeMode::End);
    // `ellipsize` lowers the minimum but does not cap the natural width.
    // CenterBox considers natural widths when balancing its side zones, so a
    // very long title could otherwise pull the transport controls off-center.
    label.set_max_width_chars(TRACK_LABEL_MAX_CHARS);
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

    fn descendant_buttons(root: &gtk4::Widget) -> Vec<gtk4::Button> {
        let mut buttons = Vec::new();
        let mut pending = vec![root.clone()];
        while let Some(widget) = pending.pop() {
            if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
                buttons.push(button);
            }
            let mut child = widget.first_child();
            while let Some(current) = child {
                child = current.next_sibling();
                pending.push(current);
            }
        }
        buttons
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn que_1_player_bar_has_no_queue_button() {
        gtk4::init().unwrap();
        let layout = build();
        let queue_buttons = descendant_buttons(layout.root.upcast_ref())
            .into_iter()
            .filter(|button| button.icon_name().as_deref() == Some("view-list-symbolic"))
            .count();

        assert_eq!(
            queue_buttons, 0,
            "the player bar still renders a queue button"
        );
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn library_bar_has_three_zones_via_centerbox() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
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
        // End zone has volume controls.
        assert!(layout.volume_scale.is_ancestor(&layout.root));
        assert!(layout.volume_icon.is_ancestor(&layout.root));
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_5_player_bar_fits_a_narrow_short_window_without_clipping() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let layout = build();
        let content = gtk4::ScrolledWindow::builder()
            .child(&gtk4::Label::new(Some("Scrollable library content")))
            .vexpand(true)
            .build();
        let shell = crate::ui::library_player_bar::LibraryPlayerBarShell::new(
            &content,
            Some(layout.root.upcast_ref()),
            reprise_core::library::settings::PlayerBarPosition::Bottom,
        );
        let window = gtk4::Window::builder()
            .default_width(720)
            .default_height(420)
            .child(shell.widget())
            .build();

        window.present();
        wait_for_layout();

        assert!(
            window.width() <= 720,
            "the full player forced a half-screen window wider: {}",
            window.width()
        );
        assert_eq!(layout.root.width(), shell.widget().width());
        assert!(
            layout.root.height() >= super::BAR_HEIGHT,
            "the player bar lost its full structural height"
        );
        for (name, widget) in [
            ("cover", layout.cover_button.upcast_ref::<gtk4::Widget>()),
            (
                "play/pause",
                layout.play_pause_button.upcast_ref::<gtk4::Widget>(),
            ),
            (
                "waveform",
                layout.waveform.widget().upcast_ref::<gtk4::Widget>(),
            ),
            (
                "position",
                layout.position_label.upcast_ref::<gtk4::Widget>(),
            ),
            (
                "duration",
                layout.duration_label.upcast_ref::<gtk4::Widget>(),
            ),
            ("volume", layout.volume_scale.upcast_ref::<gtk4::Widget>()),
        ] {
            let bounds = widget
                .compute_bounds(&layout.root)
                .unwrap_or_else(|| panic!("{name} has no player-bar bounds"));
            assert!(
                bounds.x() >= 0.0
                    && bounds.y() >= 0.0
                    && bounds.x() + bounds.width() <= layout.root.width() as f32
                    && bounds.y() + bounds.height() <= layout.root.height() as f32,
                "{name} is clipped outside the player bar: {bounds:?}, bar={}x{}",
                layout.root.width(),
                layout.root.height()
            );
        }

        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn style_5_long_title_keeps_transport_controls_centered() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let layout = build();
        layout.title_label.set_text(
            "Siren's Lament — Dark Melodic Metalcore Instrumental Mix | \
             Cinematic Heavy Atmospheric Metal With An Extremely Long Name",
        );
        layout
            .artist_label
            .set_text("Hollow Fallen — Videos With Another Long Channel Name");
        let window = gtk4::Window::builder()
            .default_width(1_200)
            .default_height(180)
            .child(&layout.root)
            .build();

        window.present();
        wait_for_layout();

        let play_bounds = layout
            .play_pause_button
            .compute_bounds(&layout.root)
            .expect("play button has player-bar bounds");
        let play_center = play_bounds.x() + play_bounds.width() / 2.0;
        let bar_center = layout.root.width() as f32 / 2.0;
        assert!(
            (play_center - bar_center).abs() <= 1.0,
            "long metadata shifted transport controls: play={play_center}, bar={bar_center}"
        );

        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn tip_1d_player_bar_buttons_follow_tooltip_discipline() {
        if gtk4::init().is_err() {
            return;
        }
        let layout = build();
        let violations =
            crate::ui::tooltip_discipline::tooltip_violations(layout.root.upcast_ref());
        assert!(violations.is_empty(), "{violations:?}");
    }

    #[test]
    fn css_styles_the_glow_play_button_and_surface() {
        let css = super::css();
        assert!(css.contains(".player-bar-play"));
        assert!(css.contains("@reprise_player_accent"));
        assert!(css.contains(".player-bar-surface"));
        assert!(css.contains("background-color: @headerbar_bg_color"));
        assert!(css.contains("border-top: 1px solid"));
        assert!(css.contains("@keyframes reprise-play-pulse"));
        assert!(css.contains("transform: scale(0.92)"));
        assert!(css.contains("inset 0 2px 1px alpha(#ffffff, 0.34)"));
        assert!(css.contains("inset 0 -4px 3px alpha(#000000, 0.30)"));
        assert!(css.contains("0 6px 12px alpha(#000000, 0.36)"));
        assert!(css.contains("inset 0 4px 6px alpha(#000000, 0.44)"));
        assert!(css.contains("0 1px 2px alpha(#000000, 0.22)"));
        assert!(css.contains(&format!(
            "animation: reprise-play-pulse {}ms {} 1",
            crate::ui::motion::MICRO_MS,
            crate::ui::motion::MICRO_CSS_EASING
        )));
    }

    #[test]
    fn tip_1d_player_bar_artist_names_its_navigation_action() {
        let source = include_str!("player_bar_layout.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        let artist_tooltip = ".tooltip_text(strings::text(strings::GO_TO_PLAYING_ARTIST))";

        assert_eq!(source.matches(&artist_tooltip).count(), 1);
    }

    #[test]
    fn css_includes_new_cover_and_label_classes() {
        let css = super::css();
        assert!(css.contains(".player-bar-cover"));
        assert!(css.contains(".player-bar-title"));
        assert!(css.contains(".player-bar-artist"));
        assert!(css.contains("border-radius: 8px"));
    }

    /// The press sink is no longer the player bar's own business: it comes
    /// from the one central set (BTN-4), and the bar only adds the louder
    /// accent ring the main action is allowed (BTN-3).
    #[test]
    fn btn_3_play_button_has_sculpted_depth_and_a_distinct_pressed_well() {
        use crate::ui::style::buttons;

        let css = super::css();
        assert!(css.contains(&format!(".{}:active", super::PLAY_CSS_CLASS)));
        assert!(css.contains("inset 0 2px 1px alpha(#ffffff, 0.34)"));
        assert!(css.contains("inset 0 -4px 3px alpha(#000000, 0.30)"));
        assert!(css.contains("0 6px 12px alpha(#000000, 0.36)"));
        assert!(css.contains("inset 0 4px 6px alpha(#000000, 0.44)"));
        assert!(css.contains("0 0 0 4px alpha(@reprise_player_accent"));
        // The MOT-5 play/pause pulse keyframes stay; only the *press* scale
        // moved out, so no local rule may restate it.
        let press_scale = format!("scale({})", crate::ui::style::tokens::BTN_PRESS_SCALE);
        assert!(
            !css.contains(&press_scale),
            "the press scale belongs to style::buttons, not to a per-button tint"
        );

        let shared = buttons::css();
        assert!(shared.contains(&format!(".{}:active", buttons::PRIMARY_CLASS)));
        assert!(shared.contains(&press_scale));
    }

    #[test]
    fn css_uses_supported_mini_eq_spacing_property() {
        let css = super::css();
        assert!(css.contains(".mini-eq { margin-left: 4px; }"));
        assert!(!css.contains("margin-start"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn player_bar_css_parses_without_errors() {
        gtk4::init().unwrap();
        let errors = crate::ui::style::css_parse_errors(&super::css());
        assert!(errors.is_empty(), "CSS parse errors: {errors:?}");
    }
}
