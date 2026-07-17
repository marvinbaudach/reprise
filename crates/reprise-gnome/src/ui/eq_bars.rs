//! A small CSS-drawn three-bar equaliser motif, shared by two surfaces:
//!
//! - **Animated** — shown before the now-playing track's title in the track
//!   table (`track_list_columns.rs`). This is the MOT-5 named exception for
//!   EQ indicators, so the loop runs only during active playback. Its bars
//!   pulse in the player accent (`@reprise_player_accent`) and freeze when
//!   playback is paused, driven
//!   entirely by CSS `@keyframes` plus an ancestor `.playback-paused` class
//!   toggled on the `ColumnView` (see `TrackList::set_playback_paused`).
//! - **Static** — the "My Stats" sidebar icon (`sidebar_presentation.rs`),
//!   three fixed ascending bars in `currentColor` so it reads like the other
//!   symbolic nav icons while being visibly distinct from the "Top rated"
//!   star.
//!
//! ## Why a CSS widget, not an icon
//!
//! The app ships no symbolic icon resources (no GResource/`build.rs`) and
//! relies on the system icon theme, which has no dependable "bar chart" name
//! — and a theme symbolic could never animate anyway. Drawing three
//! `gtk::Box` bars styled by app-owned CSS renders identically on every icon
//! theme (the same reasoning the rating widget uses text glyphs over theme
//! symbolics — see `ui::rating`'s module doc), animates for free, and serves
//! both surfaces from one widget.
//!
//! The bars animate `min-height` (not `transform: scaleY`): GTK4 CSS honours
//! keyframed `min-height` reliably, and with each bar bottom-aligned
//! (`valign = End`) inside the fixed-height row it grows upward from a common
//! baseline — the equaliser look — without depending on `transform-origin`
//! support.

use gtk4::prelude::*;

/// Root class carried by every instance; the animated colour + keyframes and
/// the static heights are scoped under the two modifier classes below.
pub(in crate::ui) const EQ_BARS_CLASS: &str = "reprise-eq-bars";
/// Modifier for the animated, accent-coloured now-playing variant.
const EQ_ANIMATED_CLASS: &str = "reprise-eq-animated";
/// Modifier for the static, `currentColor` sidebar variant.
const EQ_STATIC_CLASS: &str = "reprise-eq-static";
/// Per-bar class (`reprise-eq-bar`) plus a 1-based positional class
/// (`reprise-eq-bar-1`…) so each bar can carry its own animation delay and
/// static height without relying on `:nth-child` support.
const EQ_BAR_CLASS: &str = "reprise-eq-bar";

const BAR_COUNT: usize = 3;
/// Inter-bar gap and the widget's aligned footprint (matches the 16 px
/// symbolic nav icons so the static variant lines up with its siblings).
const BAR_SPACING: i32 = 2;
const WIDGET_SIZE: i32 = 16;

/// Which of the two presentations to build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::ui) enum EqVariant {
    /// Pulsing accent bars for the now-playing row.
    Animated,
    /// Fixed ascending `currentColor` bars for the My Stats sidebar icon.
    Static,
}

/// Builds a three-bar equaliser box for `variant`. The caller owns
/// visibility: the animated variant is created hidden in every title cell and
/// only shown on the now-playing row (`track_list_columns.rs`).
pub(in crate::ui) fn build(variant: EqVariant) -> gtk4::Box {
    let container = gtk4::Box::new(gtk4::Orientation::Horizontal, BAR_SPACING);
    container.add_css_class(EQ_BARS_CLASS);
    container.add_css_class(match variant {
        EqVariant::Animated => EQ_ANIMATED_CLASS,
        EqVariant::Static => EQ_STATIC_CLASS,
    });
    container.set_valign(gtk4::Align::Center);
    container.set_halign(gtk4::Align::Center);
    container.set_width_request(WIDGET_SIZE);
    container.set_height_request(WIDGET_SIZE);

    for index in 1..=BAR_COUNT {
        let bar = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        bar.add_css_class(EQ_BAR_CLASS);
        bar.add_css_class(&format!("{EQ_BAR_CLASS}-{index}"));
        // Bottom-align so animated `min-height` growth rises from a shared
        // baseline instead of centring.
        bar.set_valign(gtk4::Align::End);
        container.append(&bar);
    }
    container
}

/// The equaliser CSS section; installed app-wide by [`super::style`] (see its
/// `app_css` list). Keyframed `min-height` per bar, staggered so the three
/// bars pulse out of phase; `.playback-paused` (on the `ColumnView`) freezes
/// them; the static variant overrides to fixed ascending heights.
pub(in crate::ui) fn css() -> String {
    format!(
        ".{EQ_BARS_CLASS} {{ min-height: {WIDGET_SIZE}px; }}\n\
         .{EQ_BARS_CLASS} .{EQ_BAR_CLASS} {{ \
           min-width: 3px; min-height: 3px; border-radius: 1px; \
           background-color: currentColor; }}\n\
         .{EQ_ANIMATED_CLASS} .{EQ_BAR_CLASS} {{ \
           background-color: @reprise_player_accent; \
           animation: reprise-eq 1100ms ease-in-out infinite; }}\n\
         .{EQ_BAR_CLASS}-1 {{ animation-delay: 0ms; }}\n\
         .{EQ_BAR_CLASS}-2 {{ animation-delay: -450ms; }}\n\
         .{EQ_BAR_CLASS}-3 {{ animation-delay: -800ms; }}\n\
         .playback-paused .{EQ_ANIMATED_CLASS} .{EQ_BAR_CLASS} {{ \
           animation-play-state: paused; }}\n\
         .{EQ_STATIC_CLASS} .{EQ_BAR_CLASS} {{ animation: none; }}\n\
         .{EQ_STATIC_CLASS} .{EQ_BAR_CLASS}-1 {{ min-height: 6px; }}\n\
         .{EQ_STATIC_CLASS} .{EQ_BAR_CLASS}-2 {{ min-height: 10px; }}\n\
         .{EQ_STATIC_CLASS} .{EQ_BAR_CLASS}-3 {{ min-height: 14px; }}\n\
         @keyframes reprise-eq {{ \
           0% {{ min-height: 4px; }} \
           25% {{ min-height: 14px; }} \
           50% {{ min-height: 6px; }} \
           75% {{ min-height: 12px; }} \
           100% {{ min-height: 4px; }} }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn build_creates_three_marked_bars() {
        if gtk4::init().is_err() {
            return;
        }
        let bars = build(EqVariant::Animated);
        assert!(bars.has_css_class(EQ_BARS_CLASS));
        assert!(bars.has_css_class(EQ_ANIMATED_CLASS));

        let mut count = 0;
        let mut child = bars.first_child();
        while let Some(bar) = child {
            assert!(bar.has_css_class(EQ_BAR_CLASS));
            count += 1;
            child = bar.next_sibling();
        }
        assert_eq!(count, BAR_COUNT);
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn static_variant_is_marked_static_not_animated() {
        if gtk4::init().is_err() {
            return;
        }
        let bars = build(EqVariant::Static);
        assert!(bars.has_css_class(EQ_STATIC_CLASS));
        assert!(!bars.has_css_class(EQ_ANIMATED_CLASS));
    }

    #[test]
    fn css_defines_animation_pause_and_static_overrides() {
        let css = css();
        assert!(css.contains("@keyframes reprise-eq"));
        assert!(css.contains("animation-play-state: paused"));
        assert!(css.contains("@reprise_player_accent"));
        assert!(css.contains(".reprise-eq-static .reprise-eq-bar { animation: none; }"));
        // Pause must be scoped to the animated variant under the ColumnView's
        // `.playback-paused`, never the static sidebar icon.
        assert!(css.contains(".playback-paused .reprise-eq-animated .reprise-eq-bar"));
    }
}
