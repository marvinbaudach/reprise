//! Widget construction for the single Mini-Player layout.

use gtk4::pango;
use gtk4::prelude::*;

use super::cover_loader::CoverLoader;
use super::strings;
use super::style::buttons;
use super::style::tokens::TRANSITION;
use super::waveform_seek::WaveformSeek;

pub(in crate::ui) const MINI_WIDTH: i32 = 430;
pub(in crate::ui) const MINI_HEIGHT: i32 = 76;

const COVER_SIZE: i32 = 52;
const PLAY_SIZE: i32 = 38;
const CARD_RADIUS: i32 = 16;
const COVER_RADIUS: i32 = 10;
const PADDING: i32 = 12;
const INNER_SPACING: i32 = 13;

const CSS_CARD: &str = "mini-player-card";
const CSS_COVER: &str = "mini-player-cover";
const CSS_PLAY: &str = "mini-player-play";
const CSS_TITLE: &str = "mini-player-title";
const CSS_ARTIST: &str = "mini-player-artist";
const CSS_ICON_BTN: &str = "mini-player-icon-btn";
const CSS_VOL_BAR: &str = "mini-player-vol-bar";

const ICON_PLAY: &str = "media-playback-start-symbolic";

pub(in crate::ui) struct MiniWidgets {
    pub(in crate::ui) root: gtk4::WindowHandle,
    pub(in crate::ui) card: gtk4::Box,
    pub(in crate::ui) cover: gtk4::Image,
    pub(in crate::ui) title_label: gtk4::Label,
    pub(in crate::ui) artist_label: gtk4::Label,
    pub(in crate::ui) waveform: WaveformSeek,
    pub(in crate::ui) play_pause_button: gtk4::Button,
    pub(in crate::ui) hover_revealer: gtk4::Revealer,
    pub(in crate::ui) restore_button: gtk4::Button,
    pub(in crate::ui) close_button: gtk4::Button,
    pub(in crate::ui) volume_bar: gtk4::DrawingArea,
}

pub(in crate::ui) fn build_mini() -> MiniWidgets {
    // — Cover —
    let cover = gtk4::Image::new();
    cover.set_pixel_size(COVER_SIZE);
    cover.add_css_class(CSS_COVER);
    cover.set_valign(gtk4::Align::Center);
    CoverLoader::set_placeholder(&cover);

    // — Title label —
    let title_label = gtk4::Label::new(None);
    title_label.set_halign(gtk4::Align::Start);
    title_label.set_ellipsize(pango::EllipsizeMode::End);
    title_label.set_xalign(0.0);
    title_label.set_hexpand(true);
    title_label.add_css_class(CSS_TITLE);

    // — Artist label —
    let artist_label = gtk4::Label::new(None);
    artist_label.set_halign(gtk4::Align::Start);
    artist_label.set_ellipsize(pango::EllipsizeMode::End);
    artist_label.set_xalign(0.0);
    // Title owns the surplus width; the artist keeps its natural size and
    // ellipsizes first, so the pair reads as a single title-priority baseline.
    artist_label.set_hexpand(false);
    artist_label.add_css_class(CSS_ARTIST);
    crate::ui::ellipsis_tooltip::arm(&title_label);
    crate::ui::ellipsis_tooltip::arm(&artist_label);

    // — Meta row (title · artist) —
    let meta_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    meta_row.set_hexpand(true);
    meta_row.append(&title_label);
    meta_row.append(&artist_label);

    // — Waveform (mini height) —
    let waveform = WaveformSeek::new_mini();
    waveform.widget().set_hexpand(true);

    // — Text column (meta row top + waveform bottom) —
    let text_col = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    text_col.set_hexpand(true);
    text_col.set_valign(gtk4::Align::Center);
    text_col.append(&meta_row);
    text_col.append(waveform.widget());

    // — Play/pause button —
    let play_pause_button = gtk4::Button::from_icon_name(ICON_PLAY);
    play_pause_button.set_tooltip_text(Some(&strings::text(strings::TOOLTIP_PLAY)));
    play_pause_button.set_valign(gtk4::Align::Center);
    play_pause_button.add_css_class("circular");
    play_pause_button.add_css_class(CSS_PLAY);
    // Same primary tier as the full player bar's play button, from the same
    // central set — the two players must not drift apart (BTN-4).
    buttons::arm(&play_pause_button, buttons::PRIMARY_CLASS);

    // — Card (cover | text | play) —
    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, INNER_SPACING);
    card.set_margin_start(PADDING);
    card.set_margin_end(PADDING);
    card.set_margin_top(PADDING);
    card.set_margin_bottom(PADDING);
    card.append(&cover);
    card.append(&text_col);
    card.append(&play_pause_button);
    card.add_css_class(CSS_CARD);
    card.set_size_request(MINI_WIDTH, MINI_HEIGHT);

    // — Volume feedback bar (3 px, top edge, initially hidden) —
    let volume_bar = gtk4::DrawingArea::new();
    volume_bar.set_height_request(3);
    volume_bar.set_hexpand(true);
    volume_bar.set_valign(gtk4::Align::Start);
    volume_bar.set_halign(gtk4::Align::Fill);
    volume_bar.add_css_class(CSS_VOL_BAR);
    volume_bar.set_opacity(0.0);

    // — Hover overlay buttons —
    let restore_button = gtk4::Button::from_icon_name("window-restore-symbolic");
    restore_button.add_css_class("circular");
    restore_button.add_css_class(CSS_ICON_BTN);
    buttons::arm(&restore_button, buttons::ICON_CLASS);
    restore_button.set_tooltip_text(Some(&strings::text(strings::TOOLTIP_RESTORE_FULL_WINDOW)));
    restore_button.set_width_request(26);
    restore_button.set_height_request(26);

    let close_button = gtk4::Button::from_icon_name("window-close-symbolic");
    close_button.add_css_class("circular");
    close_button.add_css_class(CSS_ICON_BTN);
    buttons::arm(&close_button, buttons::ICON_CLASS);
    // MINI-2: the close button quits the app (standard window-close
    // semantics); the restore button / Ctrl+M is the keep-the-big-window path.
    close_button.set_tooltip_text(Some(&strings::shortcut_tooltip(
        strings::QUIT_REPRISE,
        strings::SHORTCUT_QUIT,
    )));
    close_button.set_width_request(26);
    close_button.set_height_request(26);

    let hover_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    hover_row.append(&restore_button);
    hover_row.append(&close_button);
    hover_row.set_halign(gtk4::Align::End);
    hover_row.set_valign(gtk4::Align::Start);
    hover_row.set_margin_top(6);
    hover_row.set_margin_end(6);

    let hover_revealer = gtk4::Revealer::new();
    hover_revealer.set_transition_type(gtk4::RevealerTransitionType::Crossfade);
    hover_revealer.set_transition_duration(super::motion::MICRO_MS);
    hover_revealer.set_child(Some(&hover_row));
    hover_revealer.set_reveal_child(false);
    hover_revealer.set_can_target(false);
    hover_revealer.set_halign(gtk4::Align::Fill);
    hover_revealer.set_valign(gtk4::Align::Fill);

    // — Overlay: card + volume bar + hover buttons —
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&card));
    overlay.add_overlay(&volume_bar);
    overlay.add_overlay(&hover_revealer);

    // — WindowHandle enables drag-to-move —
    let root = gtk4::WindowHandle::new();
    root.set_child(Some(&overlay));

    MiniWidgets {
        root,
        card,
        cover,
        title_label,
        artist_label,
        waveform,
        play_pause_button,
        hover_revealer,
        restore_button,
        close_button,
        volume_bar,
    }
}

pub(in crate::ui) fn mini_css() -> String {
    format!(
        ".{CSS_CARD} {{ \
           background-color: rgba(34, 34, 34, 0.92); \
           border: 1px solid alpha(white, 0.09); \
           border-radius: {CARD_RADIUS}px; \
           box-shadow: 0 20px 50px rgba(0, 0, 0, 0.55); }}\n\
         .{CSS_COVER} {{ \
           border-radius: {COVER_RADIUS}px; \
           box-shadow: inset 0 0 0 1px alpha(white, 0.08); }}\n\
         .{CSS_PLAY} {{ \
           min-width: {PLAY_SIZE}px; min-height: {PLAY_SIZE}px; \
           background-color: @reprise_player_accent; \
           color: #ffffff; \
           box-shadow: 0 0 12px alpha(@reprise_player_accent, 0.40); \
           transition: box-shadow {TRANSITION}, background-color {TRANSITION}, \
                       transform {TRANSITION}; }}\n\
         .{CSS_PLAY}:hover {{ box-shadow: 0 0 18px alpha(@reprise_player_accent, 0.60); }}\n\
         /* BTN-3: the press sink comes from `style::buttons`; the mini card \
            only adds the accent ring its main action is allowed. */\n\
         .{CSS_PLAY}:active {{ \
           box-shadow: 0 0 0 3px alpha(@reprise_player_accent, 0.45), \
                       0 0 18px alpha(@reprise_player_accent, 0.70); }}\n\
         .{CSS_TITLE} {{ font-weight: bold; font-size: 13px; }}\n\
         .{CSS_ARTIST} {{ color: alpha(@window_fg_color, 0.55); font-size: 11.5px; }}\n\
         .{CSS_ICON_BTN} {{ min-width: 26px; min-height: 26px; padding: 3px; \
           background-color: alpha(@window_bg_color, 0.80); \
           transition: background-color {TRANSITION}; }}\n\
         .{CSS_ICON_BTN}:hover {{ background-color: alpha(@window_bg_color, 0.95); }}\n\
         .{CSS_VOL_BAR} {{ background-color: @reprise_player_accent; \
           border-radius: 0 {CARD_RADIUS}px 0 {CARD_RADIUS}px; }}\n\
         .waveform-seek {{ color: @reprise_player_accent; }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mini_1_card_matches_frame_geometry() {
        assert_eq!(MINI_WIDTH, 430);
        assert_eq!(MINI_HEIGHT, 76);
    }

    #[test]
    fn mini_1_card_css_matches_frame() {
        let css = mini_css();
        assert!(css.contains("mini-player-card"));
        assert!(css.contains("@reprise_player_accent"));
        assert!(css.contains("rgba(34, 34, 34, 0.92)"));
        assert!(css.contains("border-radius: 16px"));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn tip_1d_mini_player_buttons_follow_tooltip_discipline() {
        if gtk4::init().is_err() {
            return;
        }
        let layout = build_mini();
        let violations =
            crate::ui::tooltip_discipline::tooltip_violations(layout.root.upcast_ref());
        assert!(violations.is_empty(), "{violations:?}");
    }
}
