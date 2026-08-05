//! The seek bar's colour-scale legend: a gradient, two words, and a caption,
//! shown under the bar for the first few track changes and then no more.
//!
//! A colour scale nobody explains is a decorative strip. So it gets explained —
//! exactly once, on its own, and afterwards only when asked for from the bar's
//! context menu. There is no close button: it would be larger than the thing it
//! closes.
//!
//! The gradient is drawn by calling [`spectral_colour`] itself rather than by
//! rebuilding the same ramp in CSS, so the legend and the bar cannot drift
//! apart when the axis is retuned.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita::prelude::AnimationExt;

use crate::ui::motion;
use crate::ui::strings;
use reprise_view::spectral_colour::spectral_colour;

/// Gradient swatch geometry, in logical pixels.
const SWATCH_WIDTH: i32 = 150;
const SWATCH_HEIGHT: i32 = 6;
/// How many stops the gradient is built from. The axis is a long curve through
/// OKLCH, not a straight line between two endpoints, so two stops would cut the
/// corner and drop the magentas the bar actually shows.
const GRADIENT_STOPS: usize = 24;
/// How long the legend stays before it leaves on its own.
const DWELL_SECONDS: u32 = 6;
/// The fade rides the Standard token (MOT-1) rather than a duration of its
/// own, and the revealer collapses the row's height over exactly the same span
/// — which is what keeps the seek row from jumping as the legend goes.
const FADE: motion::MotionToken = motion::STANDARD;

const LEGEND_CSS_CLASS: &str = "seek-legend";
const SWATCH_CSS_CLASS: &str = "seek-legend-swatch";

#[derive(Clone)]
pub(in crate::ui) struct SeekLegend {
    revealer: gtk4::Revealer,
    content: gtk4::Box,
    /// The pending dwell timer, so an early dismissal can cancel it instead of
    /// letting it fire into an already-hidden legend.
    dwell: Rc<RefCell<Option<gtk4::glib::SourceId>>>,
    fade: Rc<RefCell<Option<libadwaita::TimedAnimation>>>,
}

impl SeekLegend {
    pub(in crate::ui) fn new() -> Self {
        let swatch = gtk4::DrawingArea::new();
        swatch.set_content_width(SWATCH_WIDTH);
        swatch.set_content_height(SWATCH_HEIGHT);
        swatch.set_valign(gtk4::Align::Center);
        swatch.add_css_class(SWATCH_CSS_CLASS);
        swatch.set_draw_func(|_, cr, width, height| draw_gradient(cr, width, height));

        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        content.add_css_class(LEGEND_CSS_CLASS);
        content.append(&caption_label(strings::SEEK_LEGEND_LOW));
        content.append(&swatch);
        content.append(&caption_label(strings::SEEK_LEGEND_HIGH));
        let explanation = caption_label(strings::SEEK_LEGEND_CAPTION);
        explanation.set_margin_start(8);
        content.append(&explanation);
        content.set_halign(gtk4::Align::Start);

        let revealer = gtk4::Revealer::builder()
            .child(&content)
            // Slide-up collapses the row's height as it goes, which is what
            // keeps the seek row from jumping when the legend leaves.
            .transition_type(gtk4::RevealerTransitionType::SlideUp)
            .transition_duration(motion::STANDARD_MS)
            .reveal_child(false)
            // A collapsed `GtkRevealer` still reports its child's *cross-axis*
            // size, so a slide-up one keeps asking for the legend's full width
            // even while nothing is on screen. That widened the player bar's
            // minimum and clipped the transport controls in a half-screen
            // window. Hidden is the only state that costs nothing, so the
            // legend is hidden whenever it is not on its way in or out.
            .visible(false)
            .build();
        revealer.connect_child_revealed_notify(|revealer| {
            if !revealer.is_child_revealed() && !revealer.reveals_child() {
                revealer.set_visible(false);
            }
        });

        Self {
            revealer,
            content,
            dwell: Rc::new(RefCell::new(None)),
            fade: Rc::new(RefCell::new(None)),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Revealer {
        &self.revealer
    }

    #[cfg(test)]
    pub(in crate::ui) fn is_shown(&self) -> bool {
        self.revealer.reveals_child()
    }

    /// Shows the legend and arms the dwell timer. Showing it again while it is
    /// already up restarts the timer rather than stacking a second one.
    pub(in crate::ui) fn show(&self) {
        self.cancel_dwell();
        self.cancel_fade();
        self.content.set_opacity(1.0);
        // No manual gating on `gtk-enable-animations` here: `GtkRevealer`
        // honours the setting itself, and so does `motion::timed`'s animation.
        self.revealer.set_visible(true);
        self.revealer.set_reveal_child(true);

        let legend = self.clone();
        let id = gtk4::glib::timeout_add_seconds_local_once(DWELL_SECONDS, move || {
            legend.dwell.borrow_mut().take();
            legend.hide();
        });
        *self.dwell.borrow_mut() = Some(id);
    }

    /// Takes the legend away: fade and height collapse together, or at once
    /// when animations are off. Safe to call when it is already hidden.
    pub(in crate::ui) fn hide(&self) {
        self.cancel_dwell();
        if !self.revealer.reveals_child() {
            return;
        }
        self.revealer.set_reveal_child(false);
        if !motion::animations_enabled() {
            self.cancel_fade();
            self.content.set_opacity(1.0);
            return;
        }
        let content = self.content.clone();
        let target = libadwaita::CallbackAnimationTarget::new(move |value| {
            content.set_opacity(value);
        });
        let animation = motion::timed(&self.revealer, self.content.opacity(), 0.0, FADE, target);
        motion::replace_animation(&self.fade, animation.clone());
        animation.play();
    }

    fn cancel_dwell(&self) {
        if let Some(id) = self.dwell.borrow_mut().take() {
            id.remove();
        }
    }

    fn cancel_fade(&self) {
        if let Some(animation) = self.fade.borrow_mut().take() {
            animation.skip();
        }
    }
}

/// Whether a track change should bring the legend up on its own, given how
/// often it already has.
///
/// A count rather than a timestamp: "seen it three times" says more about
/// having understood the scale than "shown two days ago" does.
pub(in crate::ui) fn shows_on_track_change(times_seen: u32) -> bool {
    times_seen < reprise_core::library::settings::SEEK_LEGEND_SHOWS
}

/// Paints the spectral axis across `width`, rounded at both ends.
fn draw_gradient(cr: &gtk4::cairo::Context, width: i32, height: i32) {
    if width <= 0 || height <= 0 {
        return;
    }
    let (w, h) = (f64::from(width), f64::from(height));
    let gradient = gtk4::cairo::LinearGradient::new(0.0, 0.0, w, 0.0);
    for stop in 0..GRADIENT_STOPS {
        let position = stop as f64 / (GRADIENT_STOPS - 1) as f64;
        let (r, g, b) = spectral_colour(position);
        gradient.add_color_stop_rgb(position, r, g, b);
    }
    super::waveform_primitives::rounded_bar(cr, 0.0, 0.0, w, h, h / 2.0);
    if cr.set_source(&gradient).is_ok() {
        let _ = cr.fill();
    }
}

fn caption_label(message: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(&strings::text(message)));
    label.add_css_class(LEGEND_CSS_CLASS);
    label.set_valign(gtk4::Align::Center);
    // The legend is an annotation, so it gives way rather than making the
    // player bar wider. Only the gradient itself keeps a fixed width.
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    label
}

/// Legend chrome: the same muted weight the time labels carry, so the row
/// reads as an annotation rather than as a second control.
pub(in crate::ui) fn css() -> String {
    format!(
        ".{LEGEND_CSS_CLASS} {{ font-size: 11px; color: alpha(@window_fg_color, 0.62); }}\n\
         .{SWATCH_CSS_CLASS} {{ margin: 0; }}"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seek_2_the_legend_appears_three_times_and_then_no_more() {
        assert!(shows_on_track_change(0));
        assert!(shows_on_track_change(1));
        assert!(shows_on_track_change(2));
        assert!(!shows_on_track_change(3));
        assert!(!shows_on_track_change(99));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn seek_2_the_legend_goes_away_when_the_bar_is_pressed() {
        // The dwell timer is six seconds; a press means the user is aiming at
        // the bar rather than reading it, so it must not have to run out.
        gtk4::init().unwrap();
        let legend = SeekLegend::new();
        assert!(!legend.is_shown(), "it starts out of the way");
        // And costs nothing while it is: a collapsed slide-up revealer still
        // reports its child's width, which widened the player bar's minimum
        // and clipped the transport controls in a half-screen window.
        assert!(
            !legend.widget().is_visible(),
            "a legend nobody asked for must not reserve width"
        );
        legend.show();
        assert!(legend.is_shown());
        assert!(legend.widget().is_visible());
        legend.hide();
        assert!(!legend.is_shown());
        // Hiding an already-hidden legend is what a second press does.
        legend.hide();
        assert!(!legend.is_shown());
    }

    #[test]
    fn the_gradient_is_the_bar_axis_rather_than_a_rebuilt_one() {
        // The point of the legend is that it agrees with the bar. Sampling any
        // other ramp here would let the two drift apart the moment the axis is
        // retuned, so this pins the source, not the colours.
        let source = include_str!("seek_legend.rs");
        assert!(source.contains("spectral_colour(position)"));
        // Split, or the assertion's own text would be the match it looks for.
        assert!(
            !source.contains(&["linear-", "gradient"].concat()),
            "the swatch must not rebuild the ramp in CSS"
        );
    }

    #[test]
    fn the_gradient_walks_the_whole_axis_and_ends_on_its_endpoints() {
        // Two stops would cut the corner of a curve that runs the long way
        // round through magenta, and the legend would show a ramp the bar
        // never draws.
        const { assert!(GRADIENT_STOPS >= 8) };
        let first = spectral_colour(0.0);
        let last = spectral_colour(1.0);
        let midpoint = spectral_colour(0.5);
        let straight_line = (
            (first.0 + last.0) / 2.0,
            (first.1 + last.1) / 2.0,
            (first.2 + last.2) / 2.0,
        );
        let distance = (midpoint.0 - straight_line.0).abs()
            + (midpoint.1 - straight_line.1).abs()
            + (midpoint.2 - straight_line.2).abs();
        assert!(
            distance > 0.1,
            "the axis is not a straight line: {distance}"
        );
    }

    #[test]
    fn the_legend_leaves_over_the_same_span_it_collapses_over() {
        // The height animates with the fade so the seek row settles instead of
        // jumping. Both read the Standard token, and this is what keeps them
        // from being retuned apart.
        assert_eq!(FADE, motion::STANDARD);
        assert_eq!(motion::STANDARD_MS, 250);
        assert_eq!(DWELL_SECONDS, 6);
    }
}
