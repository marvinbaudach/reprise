//! Audio-reactive song visuals for the Now Playing Audio Character page.
//!
//! The reactivity signals (`level`, `bass`, `beat`, `dynamics`) are derived in
//! `reprise-core`'s `SpectrumAnalyzer`; this module only renders them. Motion is
//! deliberately punchy and playful: bands ease up fast and fall back slowly so
//! hits land, and discrete events (beats, drops) spawn transient ornaments via
//! the [`impact`] overlay so you can *see* what the music is doing.

mod impact;

use std::cell::{Cell, RefCell};
use std::f64::consts::TAU;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::playback::{PlaybackState, SpectrumFrame, SPECTRUM_BAND_COUNT};

use crate::ui::{motion, strings};

use self::impact::ImpactState;

const DRAW_HEIGHT: i32 = 220;
const EDGE: f64 = 12.0;
const NEUTRAL_PROFILE: [f32; SPECTRUM_BAND_COUNT] = [0.12; SPECTRUM_BAND_COUNT];

/// Bands rise fast (attack) and fall slowly (release): the asymmetry is what
/// makes transients punch instead of averaging away.
const BAND_ATTACK: f32 = 0.55;
const BAND_RELEASE: f32 = 0.14;
const SCALAR_ATTACK: f32 = 0.6;
const SCALAR_RELEASE: f32 = 0.16;
/// Peak-hold markers (Bars mode) fall slowly so the frequency picture is legible.
const PEAK_DECAY: f32 = 0.018;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ui) enum VisualPreset {
    #[default]
    Rings,
    Flow,
    Pulse,
    Bars,
}

impl VisualPreset {
    pub(in crate::ui) const ALL: [Self; 4] = [Self::Rings, Self::Flow, Self::Pulse, Self::Bars];

    fn id(self) -> &'static str {
        match self {
            Self::Rings => "rings",
            Self::Flow => "flow",
            Self::Pulse => "pulse",
            Self::Bars => "bars",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Rings => strings::SONG_VISUALS_RINGS,
            Self::Flow => strings::SONG_VISUALS_FLOW,
            Self::Pulse => strings::SONG_VISUALS_PULSE,
            Self::Bars => strings::SONG_VISUALS_BARS,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Clone, Copy, Debug)]
struct Circle {
    center: Point,
    radius: f64,
    width: f64,
    alpha: f64,
}

#[derive(Clone, Copy, Debug)]
struct Bar {
    center: Point,
    length: f64,
    width: f64,
    alpha: f64,
}

#[derive(Clone, Debug)]
struct Stroke {
    points: Vec<Point>,
    width: f64,
    alpha: f64,
}

#[derive(Clone, Debug, Default)]
struct Scene {
    circles: Vec<Circle>,
    bars: Vec<Bar>,
    strokes: Vec<Stroke>,
}

impl Scene {
    #[cfg(test)]
    fn is_finite_and_bounded(&self, width: f64, height: f64) -> bool {
        let point_ok = |point: Point| {
            point.x.is_finite()
                && point.y.is_finite()
                && (0.0..=width).contains(&point.x)
                && (0.0..=height).contains(&point.y)
        };
        self.circles.iter().all(|circle| {
            point_ok(circle.center) && circle.radius.is_finite() && circle.radius >= 0.0
        }) && self.bars.iter().all(|bar| {
            point_ok(bar.center)
                && bar.length.is_finite()
                && bar.length >= 0.0
                && bar.center.y - bar.length / 2.0 >= 0.0
                && bar.center.y + bar.length / 2.0 <= height
        }) && self
            .strokes
            .iter()
            .flat_map(|stroke| stroke.points.iter().copied())
            .all(point_ok)
    }
}

/// The per-frame render inputs a scene builder needs: smoothed bands plus the
/// derived envelopes and the ambient-drift phase.
struct SceneInput<'a> {
    bands: &'a [f32; SPECTRUM_BAND_COUNT],
    peaks: &'a [f32; SPECTRUM_BAND_COUNT],
    level: f32,
    bass: f32,
    phase: f64,
}

fn average(bands: &[f32; SPECTRUM_BAND_COUNT], range: std::ops::Range<usize>) -> f64 {
    let count = range.len().max(1) as f64;
    range.map(|index| f64::from(bands[index])).sum::<f64>() / count
}

fn scene(preset: VisualPreset, input: &SceneInput, width: f64, height: f64) -> Scene {
    let width = width.max(1.0);
    let height = height.max(1.0);
    match preset {
        VisualPreset::Rings => rings_scene(input, width, height),
        VisualPreset::Flow => flow_scene(input, width, height),
        VisualPreset::Pulse => pulse_scene(input, width, height),
        VisualPreset::Bars => bars_scene(input, width, height),
    }
}

fn rings_scene(input: &SceneInput, width: f64, height: f64) -> Scene {
    let center = Point {
        x: width / 2.0,
        y: height / 2.0,
    };
    let min = width.min(height);
    let low = average(input.bands, 0..8);
    let mid = average(input.bands, 8..20);
    let high = average(input.bands, 20..SPECTRUM_BAND_COUNT);
    // The whole ring stack breathes with the kick.
    let base = min * (0.11 + f64::from(input.bass) * 0.12);
    let energies = [low, mid, high];
    let circles = energies
        .into_iter()
        .enumerate()
        .map(|(index, energy)| Circle {
            center,
            radius: base + index as f64 * min * 0.09 + energy * min * 0.10,
            width: if index == 0 { 3.0 } else { 1.6 },
            alpha: 0.30 + energy * 0.6,
        })
        .collect();
    // Bands as radial spokes, slowly rotating. Capped to the canvas half so the
    // base composition stays inside the frame.
    let inner = base + 4.0;
    let max_reach = min * 0.5 - 3.0;
    let strokes = input
        .bands
        .iter()
        .enumerate()
        .map(|(index, band)| {
            let energy = f64::from(*band);
            let angle = index as f64 / SPECTRUM_BAND_COUNT as f64 * TAU + input.phase * 0.15;
            let outer = (inner + 8.0 + energy * min * 0.34).min(max_reach);
            Stroke {
                points: vec![
                    Point {
                        x: center.x + angle.cos() * inner,
                        y: center.y + angle.sin() * inner,
                    },
                    Point {
                        x: center.x + angle.cos() * outer,
                        y: center.y + angle.sin() * outer,
                    },
                ],
                width: 2.0 + energy * 3.0,
                alpha: 0.4 + energy * 0.6,
            }
        })
        .collect();
    Scene {
        circles,
        strokes,
        ..Scene::default()
    }
}

fn flow_scene(input: &SceneInput, width: f64, height: f64) -> Scene {
    let usable = width - EDGE * 2.0;
    let steps = SPECTRUM_BAND_COUNT;
    let level = f64::from(input.level);
    let strokes = (0..3)
        .map(|trail| {
            let points = (0..=steps)
                .map(|index| {
                    let band = f64::from(input.bands[index.min(steps - 1)]);
                    let x = EDGE + usable * index as f64 / steps as f64;
                    let wave = (index as f64 * 0.55 + input.phase * TAU + trail as f64 * 0.9).sin();
                    // Amplitude tracks overall level; per-band energy sharpens
                    // the crests so onsets tear through the trail.
                    let amplitude = 8.0
                        + level * height * (0.16 - trail as f64 * 0.03)
                        + band * height * (0.22 - trail as f64 * 0.04);
                    Point {
                        x,
                        y: (height / 2.0 + wave * amplitude).clamp(EDGE, height - EDGE),
                    }
                })
                .collect();
            Stroke {
                points,
                width: 3.4 - trail as f64 * 0.7,
                alpha: 0.82 - trail as f64 * 0.2,
            }
        })
        .collect();
    Scene {
        strokes,
        ..Scene::default()
    }
}

fn pulse_scene(input: &SceneInput, width: f64, height: f64) -> Scene {
    let center = Point {
        x: width / 2.0,
        y: height / 2.0,
    };
    let min = width.min(height);
    // Core punches on the kick.
    let base = min * (0.11 + f64::from(input.bass) * 0.16);
    let circles = vec![
        Circle {
            center,
            radius: base,
            width: 3.0,
            alpha: 0.9,
        },
        Circle {
            center,
            radius: base + 18.0 + f64::from(input.level) * 22.0,
            width: 1.4,
            alpha: 0.32,
        },
    ];
    let inner = base + 6.0;
    let max_reach = min * 0.5 - 3.0;
    let strokes = input
        .bands
        .iter()
        .enumerate()
        .map(|(index, band)| {
            let energy = f64::from(*band);
            let angle = index as f64 / SPECTRUM_BAND_COUNT as f64 * TAU + input.phase * 0.3;
            let outer = (inner + 8.0 + energy * min * 0.34).min(max_reach);
            Stroke {
                points: vec![
                    Point {
                        x: center.x + angle.cos() * inner,
                        y: center.y + angle.sin() * inner,
                    },
                    Point {
                        x: center.x + angle.cos() * outer,
                        y: center.y + angle.sin() * outer,
                    },
                ],
                width: 2.0 + energy * 3.5,
                alpha: 0.42 + energy * 0.58,
            }
        })
        .collect();
    Scene {
        circles,
        strokes,
        ..Scene::default()
    }
}

fn bars_scene(input: &SceneInput, width: f64, height: f64) -> Scene {
    let usable_width = width - EDGE * 2.0;
    let usable_height = height - EDGE * 2.0;
    let step = usable_width / SPECTRUM_BAND_COUNT as f64;
    let bottom = height - EDGE;
    let bar_width = (step * 0.62).clamp(2.0, 10.0);
    let mut bars = Vec::with_capacity(SPECTRUM_BAND_COUNT * 2);
    for index in 0..SPECTRUM_BAND_COUNT {
        let x = EDGE + step * (index as f64 + 0.5);
        let value = f64::from(input.bands[index]).clamp(0.0, 1.0);
        let length = (value * usable_height * 0.92).max(2.0);
        bars.push(Bar {
            center: Point {
                x,
                y: bottom - length / 2.0,
            },
            length,
            width: bar_width,
            alpha: 0.45 + value * 0.55,
        });
        // Peak-hold tick.
        let peak = f64::from(input.peaks[index]).clamp(0.0, 1.0);
        let peak_y = (bottom - peak * usable_height * 0.92).clamp(EDGE, bottom);
        bars.push(Bar {
            center: Point { x, y: peak_y },
            length: 3.0,
            width: bar_width,
            alpha: 0.85,
        });
    }
    Scene {
        bars,
        ..Scene::default()
    }
}

struct RenderState {
    current: [f32; SPECTRUM_BAND_COUNT],
    target: [f32; SPECTRUM_BAND_COUNT],
    peaks: [f32; SPECTRUM_BAND_COUNT],
    static_profile: [f32; SPECTRUM_BAND_COUNT],
    level: f32,
    bass: f32,
    level_target: f32,
    bass_target: f32,
    phase: f64,
    preset: VisualPreset,
    playback: PlaybackState,
    impact: ImpactState,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            current: [0.0; SPECTRUM_BAND_COUNT],
            target: [0.0; SPECTRUM_BAND_COUNT],
            peaks: [0.0; SPECTRUM_BAND_COUNT],
            static_profile: NEUTRAL_PROFILE,
            level: 0.0,
            bass: 0.0,
            level_target: 0.0,
            bass_target: 0.0,
            phase: 0.0,
            preset: VisualPreset::Rings,
            playback: PlaybackState::Stopped,
            impact: ImpactState::new(),
        }
    }
}

impl RenderState {
    fn scene_input(&self) -> SceneInput<'_> {
        SceneInput {
            bands: &self.current,
            peaks: &self.peaks,
            level: self.level,
            bass: self.bass,
            phase: self.phase,
        }
    }
}

fn drawing_area(
    state: &Rc<RefCell<RenderState>>,
    height_request: i32,
    css_class: &str,
) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::builder()
        .height_request(height_request)
        .hexpand(true)
        .accessible_role(gtk4::AccessibleRole::Img)
        .build();
    area.add_css_class(css_class);
    area.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::SONG_VISUALS_ACCESSIBLE,
    ))]);
    let state = state.clone();
    area.set_draw_func(move |area, cr, width, height| {
        let state = state.borrow();
        let scene = scene(
            state.preset,
            &state.scene_input(),
            f64::from(width),
            f64::from(height),
        );
        draw_scene(
            cr,
            &scene,
            &state.impact,
            f64::from(width),
            f64::from(height),
            f64::from(state.level),
            accent_rgb(area),
        );
    });
    area
}

fn register_area(
    areas: &Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::DrawingArea>>>>,
    area: &gtk4::DrawingArea,
) {
    let weak = gtk4::glib::WeakRef::new();
    weak.set(Some(area));
    areas.borrow_mut().push(weak);
}

fn queue_registered_areas(areas: &Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::DrawingArea>>>>) {
    areas.borrow_mut().retain(|weak| {
        let Some(area) = weak.upgrade() else {
            return false;
        };
        area.queue_draw();
        true
    });
}

fn preset_controls(
    state: &Rc<RefCell<RenderState>>,
    areas: &Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::DrawingArea>>>>,
) -> gtk4::Box {
    let modes = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    modes.set_halign(gtk4::Align::Center);
    modes.add_css_class("reprise-song-visual-presets");
    let selected = state.borrow().preset;
    let mut previous: Option<gtk4::ToggleButton> = None;
    for preset in VisualPreset::ALL {
        let button = gtk4::ToggleButton::builder()
            .label(strings::text(preset.label()))
            .css_classes(["reprise-btn-toggle", "reprise-song-visual-preset"])
            .active(preset == selected)
            .build();
        button.set_widget_name(preset.id());
        if let Some(previous) = &previous {
            button.set_group(Some(previous));
        }
        let state = state.clone();
        let areas = areas.clone();
        button.connect_toggled(move |button| {
            if !button.is_active() {
                return;
            }
            state.borrow_mut().preset = preset;
            queue_registered_areas(&areas);
        });
        modes.append(&button);
        previous = Some(button);
    }
    modes
}

fn ease(current: f32, target: f32, attack: f32, release: f32) -> f32 {
    let delta = target - current;
    let coeff = if delta > 0.0 { attack } else { release };
    current + delta * coeff
}

fn advance_state(state: &mut RenderState) -> bool {
    let mut settled = true;
    for index in 0..SPECTRUM_BAND_COUNT {
        let next = ease(
            state.current[index],
            state.target[index],
            BAND_ATTACK,
            BAND_RELEASE,
        );
        settled &= (state.target[index] - next).abs() < 0.002;
        state.current[index] = next;
        // Peak-hold: instant rise, slow fall.
        state.peaks[index] = state.peaks[index].max(next);
        state.peaks[index] = (state.peaks[index] - PEAK_DECAY).max(next);
    }
    state.level = ease(state.level, state.level_target, SCALAR_ATTACK, SCALAR_RELEASE);
    state.bass = ease(state.bass, state.bass_target, SCALAR_ATTACK, SCALAR_RELEASE);
    state.impact.advance();
    if state.playback == PlaybackState::Playing {
        state.phase = (state.phase + 0.0018 + f64::from(state.level) * 0.02) % 1.0;
        settled = false;
    }
    settled && state.impact.is_idle()
}

fn clear_static_profile(state: &mut RenderState, animations_enabled: bool) {
    state.static_profile = NEUTRAL_PROFILE;
    state.target = NEUTRAL_PROFILE;
    if !animations_enabled {
        state.current = NEUTRAL_PROFILE;
    }
}

#[derive(Clone)]
pub(in crate::ui) struct SongVisualizer {
    root: gtk4::Box,
    area: gtk4::DrawingArea,
    areas: Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::DrawingArea>>>>,
    state: Rc<RefCell<RenderState>>,
    panel_active: Rc<Cell<bool>>,
    fullscreen_active: Rc<Cell<bool>>,
    fullscreen_window: Rc<RefCell<Option<gtk4::glib::WeakRef<gtk4::Window>>>>,
    tick_id: Rc<RefCell<Option<gtk4::TickCallbackId>>>,
}

impl SongVisualizer {
    pub(in crate::ui) fn new() -> Self {
        let state = Rc::new(RefCell::new(RenderState::default()));
        let areas = Rc::new(RefCell::new(Vec::new()));
        let area = drawing_area(&state, DRAW_HEIGHT, "reprise-song-visual-canvas");
        register_area(&areas, &area);
        let modes = preset_controls(&state, &areas);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        root.add_css_class("reprise-song-visuals");
        root.append(&area);
        root.append(&modes);
        Self {
            root,
            area,
            areas,
            state,
            panel_active: Rc::new(Cell::new(false)),
            fullscreen_active: Rc::new(Cell::new(false)),
            fullscreen_window: Rc::new(RefCell::new(None)),
            tick_id: Rc::new(RefCell::new(None)),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_profile(&self, dimensions: &[u8; 4]) {
        let profile = std::array::from_fn(|index| {
            let dimension = dimensions[index / 8] as f32 / 100.0;
            (0.08 + dimension * 0.34).clamp(0.0, 1.0)
        });
        let mut state = self.state.borrow_mut();
        state.static_profile = profile;
        if state.playback != PlaybackState::Playing || !motion::animations_enabled() {
            state.current = profile;
            state.target = profile;
        }
        drop(state);
        queue_registered_areas(&self.areas);
    }

    pub(in crate::ui) fn clear_profile(&self) {
        let animations_enabled = motion::animations_enabled();
        let mut state = self.state.borrow_mut();
        clear_static_profile(&mut state, animations_enabled);
        drop(state);
        if animations_enabled {
            self.ensure_tick();
        } else {
            queue_registered_areas(&self.areas);
        }
    }

    pub(in crate::ui) fn set_spectrum(&self, frame: SpectrumFrame) {
        if !motion::animations_enabled() {
            return;
        }
        let mut state = self.state.borrow_mut();
        if state.playback != PlaybackState::Playing {
            return;
        }
        state.target = *frame.bands();
        state.level_target = frame.level();
        state.bass_target = frame.bass();
        let beat = frame.beat();
        if beat.fired {
            state.impact.spawn_beat(beat.strength);
        }
        state.impact.spawn_drop(frame.dynamics());
        drop(state);
        self.ensure_tick();
    }

    pub(in crate::ui) fn set_playback_state(&self, playback: PlaybackState) {
        let mut state = self.state.borrow_mut();
        state.playback = playback;
        if playback != PlaybackState::Playing || !motion::animations_enabled() {
            state.target = state.static_profile;
            state.level_target = 0.0;
            state.bass_target = 0.0;
            if !motion::animations_enabled() {
                state.current = state.static_profile;
            }
        }
        drop(state);
        if playback == PlaybackState::Playing || motion::animations_enabled() {
            self.ensure_tick();
        } else {
            self.stop_tick();
            queue_registered_areas(&self.areas);
        }
    }

    pub(in crate::ui) fn set_active(&self, active: bool) {
        self.panel_active.set(active);
        if self.is_active() {
            self.ensure_tick();
        } else {
            self.stop_tick();
        }
    }

    pub(in crate::ui) fn close_fullscreen(&self) {
        let window = self
            .fullscreen_window
            .borrow_mut()
            .take()
            .and_then(|window| window.upgrade());
        if let Some(window) = window {
            window.close();
        }
    }

    pub(in crate::ui) fn toggle_fullscreen(&self, parent: &adw::ApplicationWindow) {
        let existing = self
            .fullscreen_window
            .borrow_mut()
            .take()
            .and_then(|window| window.upgrade());
        if let Some(window) = existing {
            window.close();
            return;
        }

        let area = drawing_area(&self.state, -1, "reprise-song-visual-fullscreen-canvas");
        area.set_vexpand(true);
        register_area(&self.areas, &area);
        let controls = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
        controls.set_halign(gtk4::Align::Center);
        controls.set_valign(gtk4::Align::End);
        controls.set_margin_bottom(28);
        controls.append(&preset_controls(&self.state, &self.areas));
        let hint = gtk4::Label::new(Some(&strings::text(strings::SONG_VISUALS_FULLSCREEN_HINT)));
        hint.add_css_class("dim-label");
        controls.append(&hint);

        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&area));
        overlay.add_overlay(&controls);
        let window = gtk4::Window::builder()
            .title(strings::text(strings::SONG_VISUALS))
            .transient_for(parent)
            .decorated(false)
            .child(&overlay)
            .build();
        window.add_css_class("reprise-song-visual-fullscreen");

        let key = gtk4::EventControllerKey::new();
        let window_weak = window.downgrade();
        key.connect_key_pressed(move |_, key, _, _| {
            if matches!(key, gtk4::gdk::Key::Escape | gtk4::gdk::Key::F11) {
                if let Some(window) = window_weak.upgrade() {
                    window.close();
                }
                return gtk4::glib::Propagation::Stop;
            }
            gtk4::glib::Propagation::Proceed
        });
        window.add_controller(key);

        let fullscreen_active = self.fullscreen_active.clone();
        let panel_active = self.panel_active.clone();
        let tick_id = self.tick_id.clone();
        let slot = self.fullscreen_window.clone();
        window.connect_destroy(move |_| {
            fullscreen_active.set(false);
            slot.borrow_mut().take();
            if !panel_active.get() {
                if let Some(id) = tick_id.borrow_mut().take() {
                    id.remove();
                }
            }
        });
        self.fullscreen_active.set(true);
        *self.fullscreen_window.borrow_mut() = Some(window.downgrade());
        self.ensure_tick();
        window.fullscreen();
        window.present();
    }

    fn is_active(&self) -> bool {
        self.panel_active.get() || self.fullscreen_active.get()
    }

    fn ensure_tick(&self) {
        if !self.is_active() || !motion::animations_enabled() || self.tick_id.borrow().is_some() {
            return;
        }
        let state = self.state.clone();
        let areas = self.areas.clone();
        let panel_active = self.panel_active.clone();
        let fullscreen_active = self.fullscreen_active.clone();
        let slot = self.tick_id.clone();
        let id = self.area.add_tick_callback(move |_, _| {
            if (!panel_active.get() && !fullscreen_active.get()) || !motion::animations_enabled() {
                *slot.borrow_mut() = None;
                return gtk4::glib::ControlFlow::Break;
            }
            let mut state = state.borrow_mut();
            let settled = advance_state(&mut state);
            drop(state);
            queue_registered_areas(&areas);
            if settled {
                *slot.borrow_mut() = None;
                gtk4::glib::ControlFlow::Break
            } else {
                gtk4::glib::ControlFlow::Continue
            }
        });
        *self.tick_id.borrow_mut() = Some(id);
    }

    fn stop_tick(&self) {
        if let Some(id) = self.tick_id.borrow_mut().take() {
            id.remove();
        }
    }
}

fn accent_rgb(area: &gtk4::DrawingArea) -> (f64, f64, f64) {
    let color = area.color();
    (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    )
}

/// Brightens the accent toward white by `boost` (beats lift the color, not just
/// the size), leaving the accent identity intact at rest.
fn lift(rgb: (f64, f64, f64), boost: f64) -> (f64, f64, f64) {
    let boost = boost.clamp(0.0, 1.0) * 0.5;
    (
        rgb.0 + (1.0 - rgb.0) * boost,
        rgb.1 + (1.0 - rgb.1) * boost,
        rgb.2 + (1.0 - rgb.2) * boost,
    )
}

fn draw_scene(
    cr: &gtk4::cairo::Context,
    scene: &Scene,
    impact: &ImpactState,
    width: f64,
    height: f64,
    level: f64,
    rgb: (f64, f64, f64),
) {
    let boosted = lift(rgb, impact.accent_boost());
    // Louder passages read brighter; quiet ones contract.
    let alpha_mult = 1.0 + level * 0.4;
    let center = Point {
        x: width / 2.0,
        y: height / 2.0,
    };
    let min = width.min(height);

    draw_flash(cr, rgb, impact.flash(), &center, min);

    cr.set_line_cap(gtk4::cairo::LineCap::Round);
    cr.set_line_join(gtk4::cairo::LineJoin::Round);
    let paint = |alpha: f64| (alpha * alpha_mult).clamp(0.0, 1.0);
    for circle in &scene.circles {
        cr.set_source_rgba(boosted.0, boosted.1, boosted.2, paint(circle.alpha));
        cr.set_line_width(circle.width);
        cr.arc(circle.center.x, circle.center.y, circle.radius, 0.0, TAU);
        let _ = cr.stroke();
    }
    for bar in &scene.bars {
        cr.set_source_rgba(boosted.0, boosted.1, boosted.2, paint(bar.alpha));
        cr.set_line_width(bar.width);
        cr.move_to(bar.center.x, bar.center.y - bar.length / 2.0);
        cr.line_to(bar.center.x, bar.center.y + bar.length / 2.0);
        let _ = cr.stroke();
    }
    for stroke in &scene.strokes {
        let Some(first) = stroke.points.first() else {
            continue;
        };
        cr.set_source_rgba(boosted.0, boosted.1, boosted.2, paint(stroke.alpha));
        cr.set_line_width(stroke.width);
        cr.move_to(first.x, first.y);
        for point in &stroke.points[1..] {
            cr.line_to(point.x, point.y);
        }
        let _ = cr.stroke();
    }

    draw_impacts(cr, boosted, impact, &center, min);
}

fn draw_flash(
    cr: &gtk4::cairo::Context,
    rgb: (f64, f64, f64),
    flash: f64,
    center: &Point,
    min: f64,
) {
    if flash <= 0.0 {
        return;
    }
    let radius = min * 1.2;
    let gradient =
        gtk4::cairo::RadialGradient::new(center.x, center.y, 0.0, center.x, center.y, radius);
    gradient.add_color_stop_rgba(0.0, rgb.0, rgb.1, rgb.2, (flash * 0.15).clamp(0.0, 0.2));
    gradient.add_color_stop_rgba(1.0, rgb.0, rgb.1, rgb.2, 0.0);
    if cr.set_source(&gradient).is_ok() {
        let _ = cr.paint();
    }
}

fn draw_impacts(
    cr: &gtk4::cairo::Context,
    rgb: (f64, f64, f64),
    impact: &ImpactState,
    center: &Point,
    min: f64,
) {
    let base_r = min * 0.1;
    let reach = min * 0.55;
    for wave in impact.shockwaves() {
        let radius = base_r + wave.progress * reach;
        let alpha = ((1.0 - wave.progress) * wave.strength * 0.55).clamp(0.0, 1.0);
        cr.set_source_rgba(rgb.0, rgb.1, rgb.2, alpha);
        cr.set_line_width(1.0 + wave.strength * 2.5);
        cr.arc(center.x, center.y, radius, 0.0, TAU);
        let _ = cr.stroke();
    }
    for spark in impact.particles() {
        let x = center.x + spark.angle.cos() * spark.dist;
        let y = center.y + spark.angle.sin() * spark.dist;
        let radius = 1.4 + spark.life_frac * 2.6;
        cr.set_source_rgba(rgb.0, rgb.1, rgb.2, spark.life_frac.clamp(0.0, 1.0));
        cr.arc(x, y, radius, 0.0, TAU);
        let _ = cr.fill();
    }
}

pub(in crate::ui) fn css() -> String {
    ".reprise-song-visuals { margin: 0 18px 12px; }\n\
     .reprise-song-visual-canvas {\
       color: @reprise_player_accent;\
       background-color: alpha(#ffffff, 0.025);\
       border: 1px solid alpha(@reprise_player_accent, 0.14);\
       border-radius: 24px;\
     }\n\
     .reprise-song-visual-preset {\
       min-height: 0; min-width: 64px; padding: 5px 14px;\
       border-radius: 999px;\
       color: alpha(#ffffff, 0.68);\
       background-color: alpha(#ffffff, 0.06);\
       border: 1px solid alpha(#ffffff, 0.12);\
     }\n\
     .reprise-song-visual-preset:checked {\
       color: #ffffff;\
       background-color: alpha(@reprise_player_accent, 0.18);\
       border-color: alpha(@reprise_player_accent, 0.75);\
     }\n\
     window.reprise-song-visual-fullscreen { background: #090b0c; }\n\
     .reprise-song-visual-fullscreen-canvas {\
       color: @reprise_player_accent;\
       background-image: radial-gradient(ellipse at center,\
         alpha(@reprise_player_accent, 0.12) 0%,\
         alpha(#090b0c, 0) 72%);\
     }"
    .to_owned()
}

#[cfg(test)]
#[path = "song_visualizer_tests.rs"]
mod tests;
