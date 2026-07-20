//! Audio-reactive song visuals for the Now Playing Audio Character page.

use std::cell::{Cell, RefCell};
use std::f64::consts::TAU;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::playback::{PlaybackState, SpectrumFrame, SPECTRUM_BAND_COUNT};

use crate::ui::{motion, strings};

const DRAW_HEIGHT: i32 = 220;
const EDGE: f64 = 12.0;
const NEUTRAL_PROFILE: [f32; SPECTRUM_BAND_COUNT] = [0.12; SPECTRUM_BAND_COUNT];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(in crate::ui) enum VisualPreset {
    #[default]
    Rings,
    Flow,
    Pulse,
}

impl VisualPreset {
    pub(in crate::ui) const ALL: [Self; 3] = [Self::Rings, Self::Flow, Self::Pulse];

    fn id(self) -> &'static str {
        match self {
            Self::Rings => "rings",
            Self::Flow => "flow",
            Self::Pulse => "pulse",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Rings => strings::SONG_VISUALS_RINGS,
            Self::Flow => strings::SONG_VISUALS_FLOW,
            Self::Pulse => strings::SONG_VISUALS_PULSE,
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
            point_ok(circle.center)
                && circle.radius.is_finite()
                && circle.radius >= 0.0
                && circle.radius <= width.min(height) / 2.0
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

fn average(bands: &[f32; SPECTRUM_BAND_COUNT], range: std::ops::Range<usize>) -> f64 {
    let count = range.len().max(1) as f64;
    range.map(|index| f64::from(bands[index])).sum::<f64>() / count
}

fn scene(
    preset: VisualPreset,
    bands: &[f32; SPECTRUM_BAND_COUNT],
    width: f64,
    height: f64,
    phase: f64,
) -> Scene {
    let width = width.max(1.0);
    let height = height.max(1.0);
    match preset {
        VisualPreset::Rings => rings_scene(bands, width, height, phase),
        VisualPreset::Flow => flow_scene(bands, width, height, phase),
        VisualPreset::Pulse => pulse_scene(bands, width, height, phase),
    }
}

fn rings_scene(bands: &[f32; SPECTRUM_BAND_COUNT], width: f64, height: f64, phase: f64) -> Scene {
    let center = Point {
        x: width / 2.0,
        y: height / 2.0,
    };
    let low = average(bands, 0..5);
    let mid = average(bands, 5..11);
    let high = average(bands, 11..16);
    let base = width.min(height) * 0.16;
    let energies = [low, mid, high, (low + mid + high) / 3.0];
    let circles = energies
        .into_iter()
        .enumerate()
        .map(|(index, energy)| Circle {
            center,
            radius: base + index as f64 * width.min(height) * 0.075 + energy * 8.0,
            width: if index == 0 { 2.8 } else { 1.3 },
            alpha: 0.32 + energy * 0.5,
        })
        .collect();
    let step = (width - EDGE * 2.0) / SPECTRUM_BAND_COUNT as f64;
    let bars = bands
        .iter()
        .enumerate()
        .map(|(index, band)| {
            let energy = f64::from(*band);
            let shimmer = (phase * TAU + index as f64 * 0.63).sin() * energy * 3.0;
            Bar {
                center: Point {
                    x: EDGE + step * (index as f64 + 0.5),
                    y: center.y,
                },
                length: 18.0 + energy * height * 0.34 + shimmer,
                width: (step * 0.48).clamp(3.0, 8.0),
                alpha: 0.46 + energy * 0.54,
            }
        })
        .collect();
    Scene {
        circles,
        bars,
        strokes: Vec::new(),
    }
}

fn flow_scene(bands: &[f32; SPECTRUM_BAND_COUNT], width: f64, height: f64, phase: f64) -> Scene {
    let usable = width - EDGE * 2.0;
    let strokes = (0..3)
        .map(|trail| {
            let points = (0..=SPECTRUM_BAND_COUNT)
                .map(|index| {
                    let band = f64::from(bands[index.min(SPECTRUM_BAND_COUNT - 1)]);
                    let x = EDGE + usable * index as f64 / SPECTRUM_BAND_COUNT as f64;
                    let wave = (index as f64 * 0.72 + phase * TAU + trail as f64 * 0.9).sin();
                    let amplitude = 12.0 + band * height * (0.23 - trail as f64 * 0.035);
                    Point {
                        x,
                        y: (height / 2.0 + wave * amplitude).clamp(EDGE, height - EDGE),
                    }
                })
                .collect();
            Stroke {
                points,
                width: 3.2 - trail as f64 * 0.7,
                alpha: 0.78 - trail as f64 * 0.2,
            }
        })
        .collect();
    Scene {
        strokes,
        ..Scene::default()
    }
}

fn pulse_scene(bands: &[f32; SPECTRUM_BAND_COUNT], width: f64, height: f64, phase: f64) -> Scene {
    let center = Point {
        x: width / 2.0,
        y: height / 2.0,
    };
    let energy = average(bands, 0..SPECTRUM_BAND_COUNT);
    let base = width.min(height) * (0.16 + energy * 0.035);
    let circles = vec![
        Circle {
            center,
            radius: base,
            width: 2.6,
            alpha: 0.85,
        },
        Circle {
            center,
            radius: base + 20.0 + energy * 10.0,
            width: 1.2,
            alpha: 0.34,
        },
    ];
    let strokes = bands
        .iter()
        .enumerate()
        .map(|(index, band)| {
            let angle = index as f64 / SPECTRUM_BAND_COUNT as f64 * TAU + phase * 0.35;
            let inner = base + 7.0;
            let outer = inner + 10.0 + f64::from(*band) * width.min(height) * 0.2;
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
                width: 2.0 + f64::from(*band) * 3.0,
                alpha: 0.42 + f64::from(*band) * 0.58,
            }
        })
        .collect();
    Scene {
        circles,
        strokes,
        ..Scene::default()
    }
}

struct RenderState {
    current: [f32; SPECTRUM_BAND_COUNT],
    target: [f32; SPECTRUM_BAND_COUNT],
    static_profile: [f32; SPECTRUM_BAND_COUNT],
    phase: f64,
    preset: VisualPreset,
    playback: PlaybackState,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            current: [0.0; SPECTRUM_BAND_COUNT],
            target: [0.0; SPECTRUM_BAND_COUNT],
            static_profile: NEUTRAL_PROFILE,
            phase: 0.0,
            preset: VisualPreset::Rings,
            playback: PlaybackState::Stopped,
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
        draw_scene(
            area,
            cr,
            &scene(
                state.preset,
                &state.current,
                f64::from(width),
                f64::from(height),
                state.phase,
            ),
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

fn advance_state(state: &mut RenderState) -> bool {
    let mut settled = true;
    for index in 0..SPECTRUM_BAND_COUNT {
        let delta = state.target[index] - state.current[index];
        state.current[index] += delta * 0.18;
        settled &= delta.abs() < 0.002;
    }
    if state.playback == PlaybackState::Playing {
        let energy = average(&state.current, 0..SPECTRUM_BAND_COUNT);
        state.phase = (state.phase + 0.004 + energy * 0.014) % 1.0;
        settled = false;
    }
    settled
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
            let dimension = dimensions[index / 4] as f32 / 100.0;
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
        drop(state);
        self.ensure_tick();
    }

    pub(in crate::ui) fn set_playback_state(&self, playback: PlaybackState) {
        let mut state = self.state.borrow_mut();
        state.playback = playback;
        if playback != PlaybackState::Playing || !motion::animations_enabled() {
            state.target = state.static_profile;
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

#[allow(deprecated)] // GTK4 has no non-deprecated API for resolving a named CSS color.
fn draw_scene(area: &gtk4::DrawingArea, cr: &gtk4::cairo::Context, scene: &Scene) {
    let color = area
        .style_context()
        .lookup_color("reprise_player_accent")
        .unwrap_or_else(|| area.color());
    let rgb = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
    );
    cr.set_line_cap(gtk4::cairo::LineCap::Round);
    cr.set_line_join(gtk4::cairo::LineJoin::Round);
    for circle in &scene.circles {
        cr.set_source_rgba(rgb.0, rgb.1, rgb.2, circle.alpha.clamp(0.0, 1.0));
        cr.set_line_width(circle.width);
        cr.arc(circle.center.x, circle.center.y, circle.radius, 0.0, TAU);
        let _ = cr.stroke();
    }
    for bar in &scene.bars {
        cr.set_source_rgba(rgb.0, rgb.1, rgb.2, bar.alpha.clamp(0.0, 1.0));
        cr.set_line_width(bar.width);
        cr.move_to(bar.center.x, bar.center.y - bar.length / 2.0);
        cr.line_to(bar.center.x, bar.center.y + bar.length / 2.0);
        let _ = cr.stroke();
    }
    for stroke in &scene.strokes {
        let Some(first) = stroke.points.first() else {
            continue;
        };
        cr.set_source_rgba(rgb.0, rgb.1, rgb.2, stroke.alpha.clamp(0.0, 1.0));
        cr.set_line_width(stroke.width);
        cr.move_to(first.x, first.y);
        for point in &stroke.points[1..] {
            cr.line_to(point.x, point.y);
        }
        let _ = cr.stroke();
    }
}

pub(in crate::ui) fn css() -> String {
    ".reprise-song-visuals { margin: 0 18px 12px; }\n\
     .reprise-song-visual-canvas {\
       background-color: alpha(#ffffff, 0.025);\
       border: 1px solid alpha(@reprise_player_accent, 0.14);\
       border-radius: 24px;\
     }\n\
     .reprise-song-visual-preset {\
       min-height: 0; min-width: 72px; padding: 5px 14px;\
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
       background-image: radial-gradient(ellipse at center,\
         alpha(@reprise_player_accent, 0.12) 0%,\
         alpha(#090b0c, 0) 72%);\
     }"
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const BANDS: [f32; 16] = [
        0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 0.8, 0.6, 0.4, 0.3, 0.2, 0.1,
    ];

    #[test]
    fn ac_10_rings_flow_and_pulse_have_distinct_bounded_geometry() {
        let rings = scene(VisualPreset::Rings, &BANDS, 240.0, 220.0, 0.25);
        let flow = scene(VisualPreset::Flow, &BANDS, 240.0, 220.0, 0.25);
        let pulse = scene(VisualPreset::Pulse, &BANDS, 240.0, 220.0, 0.25);

        assert_eq!(rings.circles.len(), 4);
        assert_eq!(rings.bars.len(), 16);
        assert_eq!(flow.strokes.len(), 3);
        assert!(flow.circles.is_empty());
        assert_eq!(pulse.circles.len(), 2);
        assert_eq!(pulse.strokes.len(), 16);
        for scene in [&rings, &flow, &pulse] {
            assert!(scene.is_finite_and_bounded(240.0, 220.0));
        }
    }

    #[test]
    fn ac_10_visual_presets_are_stable_keyboard_labels() {
        assert_eq!(
            VisualPreset::ALL.map(VisualPreset::label),
            ["Rings", "Flow", "Pulse"]
        );
    }

    #[test]
    fn ac_10_louder_spectrum_changes_geometry_without_changing_cardinality() {
        let quiet = scene(VisualPreset::Rings, &[0.0; 16], 240.0, 220.0, 0.0);
        let loud = scene(VisualPreset::Rings, &[1.0; 16], 240.0, 220.0, 0.0);

        assert_eq!(quiet.bars.len(), loud.bars.len());
        assert!(
            loud.bars.iter().map(|bar| bar.length).sum::<f64>()
                > quiet.bars.iter().map(|bar| bar.length).sum::<f64>()
        );
    }

    #[test]
    fn ac_10_visual_chrome_uses_the_shared_cover_accent_and_press_vocabulary() {
        let css = css();
        assert!(css.matches("@reprise_player_accent").count() >= 4);
        assert!(css.contains(".reprise-song-visual-preset:checked"));
        assert!(css.contains(".reprise-song-visual-fullscreen-canvas"));

        let buttons = crate::ui::style::buttons::css();
        assert!(buttons.contains(".reprise-btn-toggle:active"));
        assert!(buttons.contains(".reprise-btn-toggle:focus-visible"));
    }

    #[test]
    fn ac_11_playing_moves_while_pause_settles_to_the_static_profile() {
        let mut state = RenderState {
            current: [0.0; SPECTRUM_BAND_COUNT],
            target: [1.0; SPECTRUM_BAND_COUNT],
            static_profile: [0.2; SPECTRUM_BAND_COUNT],
            phase: 0.0,
            preset: VisualPreset::Rings,
            playback: PlaybackState::Playing,
        };

        assert!(!advance_state(&mut state));
        assert!(state.phase > 0.0);
        assert!(state.current.iter().all(|band| *band > 0.0));

        let phase = state.phase;
        state.playback = PlaybackState::Paused;
        state.target = state.static_profile;
        assert!(!advance_state(&mut state));
        assert_eq!(state.phase, phase);
        assert!(state.current.iter().all(|band| *band < 0.2));
    }

    #[test]
    fn ac_11_stop_then_track_clear_settles_instead_of_snapping() {
        let mut state = RenderState {
            current: [0.8; SPECTRUM_BAND_COUNT],
            target: [0.2; SPECTRUM_BAND_COUNT],
            static_profile: [0.2; SPECTRUM_BAND_COUNT],
            phase: 0.5,
            preset: VisualPreset::Rings,
            playback: PlaybackState::Stopped,
        };

        clear_static_profile(&mut state, true);
        assert_eq!(state.current, [0.8; SPECTRUM_BAND_COUNT]);
        assert_eq!(state.target, NEUTRAL_PROFILE);
        assert!(!advance_state(&mut state));
        assert!(state
            .current
            .iter()
            .all(|band| *band < 0.8 && *band > NEUTRAL_PROFILE[0]));
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn ac_10_visual_widget_exposes_a_labeled_canvas_and_three_keyboard_presets() {
        gtk4::init().unwrap();
        let visualizer = SongVisualizer::new();

        assert_eq!(visualizer.area.accessible_role(), gtk4::AccessibleRole::Img);
        assert!(gtk4::test_accessible_has_property(
            &visualizer.area,
            gtk4::AccessibleProperty::Label
        ));
        let presets = visualizer
            .root
            .last_child()
            .expect("preset row")
            .downcast::<gtk4::Box>()
            .unwrap();
        let mut child = presets.first_child();
        let mut labels = Vec::new();
        while let Some(widget) = child {
            let button = widget.clone().downcast::<gtk4::ToggleButton>().unwrap();
            assert!(button.is_focusable());
            labels.push(button.label().unwrap().to_string());
            child = widget.next_sibling();
        }
        assert_eq!(labels, ["Rings", "Flow", "Pulse"]);
    }
}
