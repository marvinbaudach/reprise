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
/// Margin between the card and the borderless window edge. Kept at 0 so the
/// card fills the transparent window edge-to-edge: any positive margin exposes
/// the window-sized container surface (AdwToastOverlay) as a second ring behind
/// the card, and the compositor's own subtle window shadow already provides the
/// floating look (MINI-1).
pub(in crate::ui) const CARD_MARGIN: i32 = 0;
const INNER_SPACING: i32 = 13;

/// Applied to the toplevel while compact so the borderless card floats on a
/// transparent window instead of a second opaque, rounded adwaita surface.
pub(in crate::ui) const CSS_WINDOW_CLASS: &str = "reprise-mini-window";
/// Applied at runtime to every container between the window and the card so
/// none of them paints an opaque edge behind the card's rounded corners.
pub(in crate::ui) const CSS_PASSTHROUGH: &str = "reprise-mini-passthrough";

const CSS_CARD: &str = "mini-player-card";
const CSS_COVER: &str = "mini-player-cover";
const CSS_PLAY: &str = "mini-player-play";
const CSS_TITLE: &str = "mini-player-title";
const CSS_ARTIST: &str = "mini-player-artist";
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
    pub(in crate::ui) volume_bar: gtk4::DrawingArea,
}

pub(in crate::ui) fn build_mini() -> MiniWidgets {
    // — Cover —
    let cover = gtk4::Image::new();
    cover.set_pixel_size(COVER_SIZE);
    cover.add_css_class(CSS_COVER);
    cover.set_valign(gtk4::Align::Center);
    // Clip the cover texture to the rounded corners so the art itself reads as
    // rounded (frame 1e) — not square pixels under an overlaid rounded border.
    cover.set_overflow(gtk4::Overflow::Hidden);
    CoverLoader::set_placeholder(&cover);

    // — Title label —
    let title_label = gtk4::Label::new(None);
    title_label.set_halign(gtk4::Align::Start);
    title_label.set_valign(gtk4::Align::Baseline);
    title_label.set_ellipsize(pango::EllipsizeMode::End);
    title_label.set_xalign(0.0);
    // Natural width, left-packed (a trailing spacer eats the surplus), so the
    // artist sits directly behind the title instead of being shoved to the
    // right edge; the title ellipsizes first when the row runs out of room (1e).
    title_label.set_hexpand(false);
    title_label.add_css_class(CSS_TITLE);

    // — Artist label —
    let artist_label = gtk4::Label::new(None);
    artist_label.set_halign(gtk4::Align::Start);
    artist_label.set_valign(gtk4::Align::Baseline);
    artist_label.set_ellipsize(pango::EllipsizeMode::End);
    artist_label.set_xalign(0.0);
    // Directly behind the title on the same baseline (frame 1e); keeps its
    // natural size so a long title ellipsizes before ever reaching it.
    artist_label.set_hexpand(false);
    artist_label.add_css_class(CSS_ARTIST);
    crate::ui::ellipsis_tooltip::arm(&title_label);
    crate::ui::ellipsis_tooltip::arm(&artist_label);

    // — Meta row (title · artist on one baseline, left-packed) —
    let meta_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    meta_row.set_hexpand(true);
    meta_row.append(&title_label);
    meta_row.append(&artist_label);
    // Surplus sink: collapses to zero first so title + artist stay adjacent and
    // left-packed; only once it is gone does the title ellipsize (frame 1e).
    let meta_spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    meta_spacer.set_hexpand(true);
    meta_row.append(&meta_spacer);

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

    // — Card (cover | text | play). Internal breathing room is CSS padding
    // (frame 1e: 10/14/10/10), so the cover and the play button both sit fully
    // inside the rounded card — nothing bleeds to the window edge (MINI-1). —
    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, INNER_SPACING);
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

    // — Overlay: card + volume feedback bar. No hover chrome: Restore/Quit live
    // in the right-click menu (MINI-3), keyboard (Ctrl+M / Ctrl+Q, MINI-4) and
    // a double-click on the cover/title. —
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&card));
    overlay.add_overlay(&volume_bar);

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
        volume_bar,
    }
}

pub(in crate::ui) fn mini_css() -> String {
    format!(
        ".{CSS_CARD} {{ \
           padding: 10px 14px 10px 10px; \
           background-color: rgba(34, 34, 34, 0.92); \
           border: 1px solid alpha(white, 0.09); \
           border-radius: {CARD_RADIUS}px; \
           box-shadow: none; }}\n\
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
         /* Artist on the tint: raised from 0.5 to keep CONTRAST-Glas ≥ 4.5:1 \
            against the card's near-black; still clearly secondary to the title. */\n\
         .{CSS_ARTIST} {{ color: alpha(@window_fg_color, 0.6); font-size: 11.5px; }}\n\
         .{CSS_VOL_BAR} {{ background-color: @reprise_player_accent; \
           border-radius: 0 {CARD_RADIUS}px 0 {CARD_RADIUS}px; }}\n\
         .waveform-seek {{ color: @reprise_player_accent; }}\n\
         /* Replace GTK's default dotted keyboard-focus outline with the accent \
            ring (BTN-1): at rest the card shows only its hairline; on \
            :focus-visible a focusable child gets a solid accent outline. \
            Buttons keep their own `style::buttons` focus treatment. */\n\
         window.{CSS_WINDOW_CLASS} *:focus-visible:not(button) {{ \
           outline-color: @reprise_player_accent; outline-style: solid; \
           outline-width: 2px; outline-offset: 1px; }}\n\
         /* Every container between the transparent window and the card carries \
            this class at runtime (minimal_view::set_container_passthrough), so \
            no opaque container edge shows past the card's rounded corners. \
            Class-based — never depends on libadwaita's internal CSS node names \
            (a node-name miss was what still leaked a background edge). (MINI-1) */\n\
         .{CSS_PASSTHROUGH} {{ \
           background: none; background-color: transparent; \
           box-shadow: none; border: none; border-radius: 0; }}\n\
         /* Kill GTK's client-side window-decoration shadow, border and corner \
            radius on the card-sized toplevel — otherwise Adwaita's `window.csd` \
            renders an oversized halo/edge behind the card. !important beats the \
            themed rule's specificity (MINI-1/MINI-2). */\n\
         window.{CSS_WINDOW_CLASS} {{ \
           box-shadow: none !important; border: none !important; border-radius: 0 !important; \
           background: none !important; background-color: transparent !important; }}"
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
        // The window floats the card on a transparent toplevel (MINI-1).
        assert!(css.contains(CSS_WINDOW_CLASS));
        assert!(css.contains("background-color: transparent"));
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

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mini_2_card_has_no_hover_chrome_buttons() {
        if gtk4::init().is_err() {
            return;
        }
        let w = build_mini();
        // Only the play/pause button exists on the card; Restore/Quit are
        // reachable via the context menu, keyboard and double-click (MINI-2).
        let mut buttons = 0;
        let mut stack = vec![w.root.clone().upcast::<gtk4::Widget>()];
        while let Some(widget) = stack.pop() {
            if widget.is::<gtk4::Button>() {
                buttons += 1;
            }
            let mut child = widget.first_child();
            while let Some(c) = child {
                child = c.next_sibling();
                stack.push(c);
            }
        }
        assert_eq!(buttons, 1, "compact card must carry only the play button");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mini_play_button_inside_card_bounds() {
        if gtk4::init().is_err() {
            return;
        }
        // build_mini() does not install the stylesheet (CompactPlayer::new
        // does, via style::install); load it so the card padding — the whole
        // point of this test — actually applies.
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(&mini_css());
        gtk4::style_context_add_provider_for_display(
            &gtk4::gdk::Display::default().expect("display"),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let w = build_mini();
        let win = gtk4::Window::new();
        win.set_child(Some(&w.root));
        win.set_default_size(MINI_WIDTH, MINI_HEIGHT);
        win.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        let card = w.card.compute_bounds(&w.root).expect("card bounds");
        let play = w
            .play_pause_button
            .compute_bounds(&w.root)
            .expect("play bounds");
        // Fully inside the card on every edge …
        assert!(play.x() >= card.x() - 0.5, "play bleeds past the left edge");
        assert!(play.y() >= card.y() - 0.5, "play bleeds past the top edge");
        assert!(
            play.y() + play.height() <= card.y() + card.height() + 0.5,
            "play bleeds past the bottom edge"
        );
        // … and inset from the right edge, not flush against the rounded corner
        // (frame 1e ≈ 14px). A no-padding card would leave it flush (inset ~0).
        let right_inset = (card.x() + card.width()) - (play.x() + play.width());
        assert!(
            right_inset >= 8.0,
            "play button right inset {right_inset} too small — it reaches the card edge"
        );
        win.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn mini_title_artist_left_packed_not_right_aligned() {
        if gtk4::init().is_err() {
            return;
        }
        let w = build_mini();
        // Neither label expands — the old title `hexpand(true)` shoved the
        // artist to the card's right edge (frame 1e wants them adjacent).
        assert!(!w.title_label.hexpands(), "title must not expand");
        assert!(!w.artist_label.hexpands(), "artist must not expand");
        // meta_row = [title][artist][spacer]: a trailing expanding, non-label
        // spacer eats the surplus and keeps the pair left-packed.
        let meta_row = w.title_label.parent().expect("meta row");
        let spacer = meta_row.last_child().expect("trailing spacer");
        assert!(spacer.hexpands(), "meta row needs a trailing expanding spacer");
        assert!(
            !spacer.is::<gtk4::Label>(),
            "the trailing child is the spacer, not the artist label"
        );
    }

    #[test]
    fn mini_artist_contrast_on_tint() {
        // Artist = white at the alpha in `.mini-player-artist`, composited over
        // the card tint rgba(34,34,34,0.92) on a dark desktop ≈ #222. The pair
        // must clear WCAG AA body text, ≥ 4.5:1 (CONTRAST-Glas).
        const ARTIST_ALPHA: f64 = 0.6;
        let bg = 34.0 / 255.0;
        let fg = ARTIST_ALPHA + bg * (1.0 - ARTIST_ALPHA); // white over the tint
        let luminance = |c: f64| {
            if c <= 0.03928 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let (l_fg, l_bg) = (luminance(fg), luminance(bg));
        let ratio = (l_fg.max(l_bg) + 0.05) / (l_fg.min(l_bg) + 0.05);
        assert!(ratio >= 4.5, "artist contrast {ratio:.2} < 4.5:1");
        // … and the stylesheet actually uses that alpha.
        assert!(mini_css().contains("alpha(@window_fg_color, 0.6)"));
    }
}
