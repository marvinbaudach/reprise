//! The now-playing bloom: an out-of-focus, enlarged copy of the current cover
//! lying behind the cover, breathing with the bass.
//!
//! Honest by construction — the colour comes from the artwork itself, not from
//! a hue derived from it. The blur is bought once per track: the cover is
//! rasterized down to a 32 px square and handed to the renderer as a texture,
//! and painting it back up across the panel is what blurs it. There is no blur
//! node in the snapshot path and nothing per frame but an alpha and a scale.
//!
//! Those two are all a frame ever changes, so a frame costs a snapshot and not
//! a rasterization: `cover_bloom_area` places the texture, and the widget it
//! replaced — a `GtkDrawingArea` re-painting the blurred cover through Cairo
//! every frame — was the larger half of the app's whole idle cost on a GPU.
//!
//! While playing, the spectrum events are the frame source (they arrive every
//! 11.6 ms) — no tick callback runs. The only tick here drives the slow breath
//! while playback is paused, and it stops the moment it is not needed.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::cairo;
use gtk4::prelude::*;
use reprise_core::playback::PlaybackState;

use super::cover_bloom_area::BloomArea;
use crate::ui::cover_glow;

type OnFrame = Rc<dyn Fn(i64)>;
/// Height of the bloom band, from the top of the head overlay: enough for the
/// cover and the title block, stopping short of the tabs.
pub(super) const BLOOM_HEIGHT: f64 = 330.0;
/// Width as a share of the panel width. The overflow is clipped by the panel.
pub(super) const BLOOM_WIDTH_FACTOR: f64 = 1.24;

const REST_OPACITY: f64 = 0.06;
const OPACITY_PER_PRESSURE: f64 = 0.15;
const OPACITY_PER_SWELL: f64 = 0.16;
const REST_SCALE: f64 = 1.0;
const SCALE_PER_SWELL: f64 = 0.025;

/// A reading that moves the alpha by less than this cannot be seen and does not
/// earn a redraw.
const LIGHT_EPSILON: f64 = 0.01;
/// Redraw interval (µs) of the paused breath. A six-second sine does not need
/// sixty frames a second; the slow envelope only needs this tick as a clock.
const BREATH_FRAME_INTERVAL_US: i64 = 33_000;

pub(super) fn bloom_opacity(pressure: f64, swell: f64) -> f64 {
    REST_OPACITY
        + OPACITY_PER_PRESSURE * pressure.clamp(0.0, 1.0)
        + OPACITY_PER_SWELL * swell.clamp(0.0, 1.0)
}

pub(super) fn bloom_scale(swell: f64) -> f64 {
    REST_SCALE + SCALE_PER_SWELL * swell.clamp(0.0, 1.0)
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
    /// Held at the rest value: Visual tab open, plugin off, or panel hidden.
    Pinned,
}

struct Inner {
    generation: Cell<Option<u64>>,
    swell: Cell<f64>,
    pressure: Cell<f64>,
    mode: Cell<Mode>,
    last_breath_frame_us: Cell<i64>,
    tick_id: RefCell<Option<gtk4::TickCallbackId>>,
    on_frame: RefCell<Option<OnFrame>>,
}

#[derive(Clone)]
pub(super) struct CoverBloom {
    area: BloomArea,
    inner: Rc<Inner>,
}

/// The blurred 32 px surface as something the renderer can keep.
///
/// Built once per track. The data guard locks the surface for as long as it
/// lives, so it is dropped before the texture leaves this function.
fn texture_from_surface(surface: &mut cairo::ImageSurface) -> Option<gtk4::gdk::MemoryTexture> {
    let width = surface.width();
    let height = surface.height();
    let stride = surface.stride() as usize;
    match surface.data() {
        Ok(data) => {
            let bytes = gtk4::glib::Bytes::from(&*data);
            Some(gtk4::gdk::MemoryTexture::new(
                width,
                height,
                // Cairo's ARgb32 is premultiplied BGRA on little-endian.
                gtk4::gdk::MemoryFormat::B8g8r8a8Premultiplied,
                &bytes,
                stride,
            ))
        }
        Err(error) => {
            tracing::warn!(%error, "bloom: blurred cover surface was not readable");
            None
        }
    }
}

impl CoverBloom {
    pub(super) fn new() -> Self {
        let area = BloomArea::new();
        let inner = Rc::new(Inner {
            generation: Cell::new(None),
            swell: Cell::new(0.0),
            pressure: Cell::new(0.0),
            mode: Cell::new(Mode::Pinned),
            last_breath_frame_us: Cell::new(0),
            tick_id: RefCell::new(None),
            on_frame: RefCell::new(None),
        });
        let bloom = Self { area, inner };
        bloom.apply_light();
        bloom
    }

    pub(super) fn widget(&self) -> &BloomArea {
        &self.area
    }

    pub(super) fn set_on_frame(&self, callback: impl Fn(i64) + 'static) {
        *self.inner.on_frame.borrow_mut() = Some(Rc::new(callback));
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
                let blurred = cover_glow::blurred_surface(texture)
                    .as_mut()
                    .and_then(texture_from_surface);
                self.area
                    .set_texture(blurred.as_ref().map(gtk4::prelude::Cast::upcast_ref));
                self.inner.generation.set(Some(generation));
            }
            None => {
                self.area.set_texture(None);
                self.inner.generation.set(None);
            }
        }
        self.apply_light();
    }

    #[cfg(test)]
    pub(super) fn has_cover_for_test(&self) -> bool {
        self.area.has_texture()
    }

    pub(super) fn set_light(&self, pressure: f64, swell: f64) {
        if self.inner.mode.get() == Mode::Pinned {
            return;
        }
        let pressure = pressure.clamp(0.0, 1.0);
        let swell = swell.clamp(0.0, 1.0);
        if (self.inner.swell.get() - swell).abs() < LIGHT_EPSILON
            && (self.inner.pressure.get() - pressure).abs() < LIGHT_EPSILON
        {
            return;
        }
        self.inner.swell.set(swell);
        self.inner.pressure.set(pressure);
        self.apply_light();
    }

    /// Hands the current reading to the surface. The whole per-frame cost.
    fn apply_light(&self) {
        let (opacity, scale) = match self.inner.mode.get() {
            Mode::Live | Mode::Breathing => (
                bloom_opacity(self.inner.pressure.get(), self.inner.swell.get()),
                bloom_scale(self.inner.swell.get()),
            ),
            Mode::Pinned => (bloom_opacity(0.0, 0.0), bloom_scale(0.0)),
        };
        self.area.set_light(opacity, scale);
    }

    fn notify_frame(&self, frame_time_us: i64) {
        let callback = self.inner.on_frame.borrow().clone();
        if let Some(callback) = callback {
            callback(frame_time_us);
        }
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
        if mode == Mode::Pinned {
            self.notify_frame(0);
        }
        if self.inner.mode.get() == mode {
            return;
        }
        self.inner.mode.set(mode);
        if mode == Mode::Pinned {
            self.inner.swell.set(0.0);
            self.inner.pressure.set(0.0);
        }
        if mode == Mode::Breathing {
            self.inner.last_breath_frame_us.set(0);
            self.start_breath();
        } else {
            self.stop_breath();
        }
        self.apply_light();
    }

    fn start_breath(&self) {
        if self.inner.tick_id.borrow().is_some() {
            return;
        }
        let inner = self.inner.clone();
        let bloom = self.clone();
        let id = self.area.add_tick_callback(move |_, clock| {
            if inner.mode.get() != Mode::Breathing {
                *inner.tick_id.borrow_mut() = None;
                return gtk4::glib::ControlFlow::Break;
            }
            let now = clock.frame_time();
            let last = inner.last_breath_frame_us.get();
            if last != 0 && now - last < BREATH_FRAME_INTERVAL_US {
                return gtk4::glib::ControlFlow::Continue;
            }
            inner.last_breath_frame_us.set(now);
            let callback = inner.on_frame.borrow().clone();
            if let Some(callback) = callback {
                callback(now);
            }
            bloom.apply_light();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ac_24_bloom_adds_a_swell_on_top_of_a_pressure_bed() {
        // Silence: the rest value, and nothing else.
        assert!((bloom_opacity(0.0, 0.0) - 0.06).abs() < 1e-9);
        // A held breakdown: no attack left, but the light stays up.
        assert!((bloom_opacity(0.9, 0.0) - 0.195).abs() < 1e-9);
        // A broad swell on a lit bed.
        assert!((bloom_opacity(0.85, 0.8) - 0.3155).abs() < 1e-9);
        // Both at full: the ceiling.
        assert!((bloom_opacity(1.0, 1.0) - 0.37).abs() < 1e-9);
        // The bed alone must never out-shine bed plus hit.
        assert!(bloom_opacity(1.0, 0.0) < bloom_opacity(1.0, 1.0));

        assert!((bloom_scale(0.0) - 1.0).abs() < 1e-9);
        assert!((bloom_scale(1.0) - 1.025).abs() < 1e-9);

        // Out-of-range readings clamp, never extrapolate.
        assert!((bloom_opacity(4.0, 4.0) - 0.37).abs() < 1e-9);
        assert!((bloom_opacity(-1.0, -1.0) - 0.06).abs() < 1e-9);
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
