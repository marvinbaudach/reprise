//! The now-playing bloom: an out-of-focus, enlarged copy of the current cover
//! lying behind the cover, breathing with the bass.
//!
//! Honest by construction — the colour comes from the artwork itself, not from
//! a hue derived from it. The blur is bought once per track: the texture is
//! rasterized down to a 32 px square and painted back up across the panel, so
//! Cairo's own interpolation does the blurring. There is no blur node in the
//! snapshot path and nothing per frame but an alpha and a scale.
//!
//! While playing, the spectrum events are the frame source (they arrive every
//! 11.6 ms) — no tick callback runs. The only tick here drives the slow breath
//! while playback is paused, and it stops the moment it is not needed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::cairo;
use gtk4::prelude::*;
use reprise_core::playback::PlaybackState;

use crate::ui::motion;

/// Edge length the cover is rasterized down to. The upscale back across the
/// panel is the blur; 32 px over ~372 px is a factor of ~11.6.
const BLOOM_EDGE: i32 = 32;
/// Height of the bloom band, from the top of the head overlay: enough for the
/// cover and the title block, stopping short of the tabs.
const BLOOM_HEIGHT: f64 = 330.0;
/// Width as a share of the panel width. The overflow is clipped by the panel.
const BLOOM_WIDTH_FACTOR: f64 = 1.24;

const REST_OPACITY: f64 = 0.10;
const OPACITY_PER_IMPACT: f64 = 0.16;
const REST_SCALE: f64 = 1.0;
const SCALE_PER_IMPACT: f64 = 0.02;

/// The paused breath. Deliberately dimmer than the playing rest value: pause
/// should look calmer, not merely slower.
const PAUSE_BASE_OPACITY: f64 = 0.06;
const PAUSE_SWING: f64 = 0.04;
const PAUSE_PERIOD_S: f64 = 6.0;

/// A reading that moves the alpha by less than this cannot be seen and does not
/// earn a redraw.
const IMPACT_EPSILON: f64 = 0.01;

pub(super) fn bloom_opacity(impact: f64) -> f64 {
    REST_OPACITY + OPACITY_PER_IMPACT * impact.clamp(0.0, 1.0)
}

pub(super) fn bloom_scale(impact: f64) -> f64 {
    REST_SCALE + SCALE_PER_IMPACT * impact.clamp(0.0, 1.0)
}

pub(super) fn pause_opacity(elapsed_s: f64) -> f64 {
    let phase = std::f64::consts::TAU * elapsed_s / PAUSE_PERIOD_S;
    PAUSE_BASE_OPACITY + PAUSE_SWING * (0.5 + 0.5 * phase.sin())
}

/// The blurred surface is cached against the panel's cover generation, which is
/// bumped once per rendered track.
pub(super) fn needs_rebuild(cached: Option<u64>, incoming: u64) -> bool {
    cached != Some(incoming)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Reacting to the live reading.
    Live,
    /// Slow breath while a track is loaded but not playing.
    Breathing,
    /// Held at the rest value: Visual tab open, plugin off, or motion off.
    Pinned,
}

struct Inner {
    surface: RefCell<Option<cairo::ImageSurface>>,
    generation: Cell<Option<u64>>,
    impact: Cell<f64>,
    mode: Cell<Mode>,
    breath_start_us: Cell<i64>,
    tick_id: RefCell<Option<gtk4::TickCallbackId>>,
}

#[derive(Clone)]
pub(super) struct CoverBloom {
    area: gtk4::DrawingArea,
    inner: Rc<Inner>,
}

impl CoverBloom {
    pub(super) fn new() -> Self {
        let area = gtk4::DrawingArea::new();
        area.add_css_class("reprise-now-playing-bloom");
        // Decoration only: it must never take a click meant for the cover.
        area.set_can_target(false);
        area.set_can_focus(false);
        let inner = Rc::new(Inner {
            surface: RefCell::new(None),
            generation: Cell::new(None),
            impact: Cell::new(0.0),
            mode: Cell::new(Mode::Pinned),
            breath_start_us: Cell::new(0),
            tick_id: RefCell::new(None),
        });
        area.set_draw_func({
            let inner = inner.clone();
            move |_, cr, width, height| draw(cr, width, height, &inner)
        });
        Self { area, inner }
    }

    pub(super) fn widget(&self) -> &gtk4::DrawingArea {
        &self.area
    }

    /// A resolved cover texture, or `None` for external media, a placeholder,
    /// or no track. Without a texture the bloom stays off — it never falls back
    /// to a substitute colour, because a colour that is not in the artwork is
    /// exactly the dishonesty this effect exists to avoid.
    pub(super) fn set_cover(&self, texture: Option<&gtk4::gdk::Texture>, generation: u64) {
        match texture {
            Some(texture) => {
                if !needs_rebuild(self.inner.generation.get(), generation) {
                    return;
                }
                let built = blurred_surface(texture);
                *self.inner.surface.borrow_mut() = built;
                self.inner.generation.set(Some(generation));
            }
            None => {
                *self.inner.surface.borrow_mut() = None;
                self.inner.generation.set(None);
            }
        }
        self.area.queue_draw();
    }

    pub(super) fn set_impact(&self, impact: f64) {
        if self.inner.mode.get() != Mode::Live {
            return;
        }
        let impact = motion::reactive_amplitude(impact);
        if (self.inner.impact.get() - impact).abs() < IMPACT_EPSILON {
            return;
        }
        self.inner.impact.set(impact);
        self.area.queue_draw();
    }

    pub(super) fn set_playback_state(&self, state: PlaybackState) {
        let mode = if state == PlaybackState::Playing {
            Mode::Live
        } else {
            Mode::Breathing
        };
        self.apply_mode(mode);
    }

    /// Holds the bloom at its rest value while the Visual tab is open (that tab
    /// runs its own light language and two systems pulsing in different colours
    /// against each other is the failure case) or the plugin is off.
    pub(super) fn set_pinned(&self, pinned: bool) {
        if pinned {
            self.apply_mode(Mode::Pinned);
        }
        // Un-pinning restores the mode the next state update brings; the panel
        // re-sends the playback state, so there is nothing to remember here.
    }

    fn apply_mode(&self, mode: Mode) {
        let mode = if motion::animations_enabled() {
            mode
        } else {
            Mode::Pinned
        };
        if self.inner.mode.get() == mode {
            return;
        }
        self.inner.mode.set(mode);
        if mode != Mode::Live {
            self.inner.impact.set(0.0);
        }
        if mode == Mode::Breathing {
            self.inner.breath_start_us.set(0);
            self.start_breath();
        } else {
            self.stop_breath();
        }
        self.area.queue_draw();
    }

    fn start_breath(&self) {
        if self.inner.tick_id.borrow().is_some() {
            return;
        }
        let inner = self.inner.clone();
        let area = self.area.clone();
        let id = self.area.add_tick_callback(move |_, clock| {
            if inner.mode.get() != Mode::Breathing
                || !motion::animations_enabled()
                || !area.is_mapped()
            {
                *inner.tick_id.borrow_mut() = None;
                return gtk4::glib::ControlFlow::Break;
            }
            if inner.breath_start_us.get() == 0 {
                inner.breath_start_us.set(clock.frame_time());
            }
            area.queue_draw();
            gtk4::glib::ControlFlow::Continue
        });
        *self.inner.tick_id.borrow_mut() = Some(id);
    }

    fn stop_breath(&self) {
        if let Some(id) = self.inner.tick_id.borrow_mut().take() {
            id.remove();
        }
    }
}

fn draw(cr: &cairo::Context, width: i32, height: i32, inner: &Inner) {
    if width <= 0 || height <= 0 {
        return;
    }
    let surface = inner.surface.borrow();
    let Some(surface) = surface.as_ref() else {
        return;
    };
    let (opacity, scale) = match inner.mode.get() {
        Mode::Live => (
            bloom_opacity(inner.impact.get()),
            bloom_scale(inner.impact.get()),
        ),
        Mode::Breathing => {
            let start = inner.breath_start_us.get();
            let elapsed = if start == 0 {
                0.0
            } else {
                (gtk4::glib::monotonic_time() - start) as f64 / 1_000_000.0
            };
            (pause_opacity(elapsed), REST_SCALE)
        }
        Mode::Pinned => (bloom_opacity(0.0), bloom_scale(0.0)),
    };

    let w = f64::from(width);
    let band = BLOOM_HEIGHT.min(f64::from(height));
    let target_w = w * BLOOM_WIDTH_FACTOR * scale;
    let target_h = BLOOM_HEIGHT * scale;

    cr.save().ok();
    // Clipped to the panel: the 124 % width overflows both edges on purpose.
    cr.rectangle(0.0, 0.0, w, band);
    cr.clip();
    cr.translate((w - target_w) / 2.0, (BLOOM_HEIGHT - target_h) / 2.0);
    cr.scale(
        target_w / f64::from(BLOOM_EDGE),
        target_h / f64::from(BLOOM_EDGE),
    );
    if cr.set_source_surface(surface, 0.0, 0.0).is_ok() {
        // Bilinear over an 11x upscale is the blur; Pad keeps the edges from
        // bleeding to transparent inside the clip.
        cr.source().set_filter(cairo::Filter::Bilinear);
        cr.source().set_extend(cairo::Extend::Pad);
        cr.paint_with_alpha(opacity).ok();
    }
    cr.restore().ok();
}

/// Rasterizes `texture` down to a [`BLOOM_EDGE`] square. Same technique as
/// `song_visualizer_util::downscale_cover_rgba`, but the surface itself is
/// kept — it is what gets painted back up.
fn blurred_surface(texture: &gtk4::gdk::Texture) -> Option<cairo::ImageSurface> {
    let snapshot = gtk4::Snapshot::new();
    let bounds = gtk4::graphene::Rect::new(0.0, 0.0, BLOOM_EDGE as f32, BLOOM_EDGE as f32);
    snapshot.append_texture(texture, &bounds);
    let node = snapshot.to_node()?;
    let surface =
        cairo::ImageSurface::create(cairo::Format::ARgb32, BLOOM_EDGE, BLOOM_EDGE).ok()?;
    {
        let cr = cairo::Context::new(&surface).ok()?;
        node.draw(&cr);
    }
    surface.flush();
    Some(surface)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_24_bloom_curve_hits_the_three_agreed_points() {
        // Rest, the value a loud track sits at under load, and the peak.
        assert!((bloom_opacity(0.0) - 0.10).abs() < 1e-9);
        assert!((bloom_opacity(0.35) - 0.156).abs() < 1e-9);
        assert!((bloom_opacity(1.0) - 0.26).abs() < 1e-9);

        assert!((bloom_scale(0.0) - 1.00).abs() < 1e-9);
        assert!((bloom_scale(0.35) - 1.007).abs() < 1e-9);
        assert!((bloom_scale(1.0) - 1.02).abs() < 1e-9);
    }

    #[test]
    fn ac_24_bloom_curve_clamps_instead_of_extrapolating() {
        assert!((bloom_opacity(-1.0) - 0.10).abs() < 1e-9);
        assert!((bloom_opacity(4.0) - 0.26).abs() < 1e-9);
        assert!((bloom_scale(4.0) - 1.02).abs() < 1e-9);
    }

    #[test]
    fn ac_24_pause_breath_stays_below_the_playing_rest() {
        // Six-second period: trough at 4.5 s, crest at 1.5 s.
        assert!((pause_opacity(0.0) - 0.08).abs() < 1e-9);
        assert!((pause_opacity(1.5) - 0.10).abs() < 1e-9);
        assert!((pause_opacity(4.5) - 0.06).abs() < 1e-9);
        // Never brighter than the playing rest value, and darker on average:
        // pause must look calmer, not merely slower.
        for step in 0..=60 {
            let t = f64::from(step) / 10.0;
            assert!(pause_opacity(t) <= bloom_opacity(0.0) + 1e-9);
        }
        // A full period returns to where it started.
        assert!((pause_opacity(0.0) - pause_opacity(6.0)).abs() < 1e-9);
    }

    #[test]
    fn ac_24_blurred_cover_is_rebuilt_once_per_track_change() {
        // The cache is keyed on the panel's cover generation, which the panel
        // bumps exactly once per rendered track. Same generation must never
        // pay for a second rasterization.
        assert!(needs_rebuild(None, 7));
        assert!(!needs_rebuild(Some(7), 7));
        assert!(needs_rebuild(Some(7), 8));
        // Generations wrap; a wrapped value is still a change.
        assert!(needs_rebuild(Some(u64::MAX), 0));
    }
}
