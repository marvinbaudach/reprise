//! Audio-reactive song visuals for the Now Playing Audio Character page.
//!
//! All reactive state (eased spectrum bands, envelopes, water, dust, impact
//! overlay, accent palette) and the per-mode geometry live in
//! `reprise_core::visuals::VisualEngine` — a portable core the GUI never has
//! to reimplement. This module only owns the widget shell (inline canvas,
//! fullscreen overlay, transport chrome, auto-hide, mode picker) and turns
//! the engine's [`reprise_core::visuals::Scene`] into pixels via [`render`].

mod render;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::playback::{PlaybackState, SpectrumFrame};
use reprise_core::visuals::{VisualEngine, VisualMode};

use crate::ui::style::buttons;
use crate::ui::{motion, strings};

const DRAW_HEIGHT: i32 = 220;

/// Transport actions the fullscreen overlay can trigger. Wired from the player
/// controller; each is optional so the visualizer works standalone (tests).
#[derive(Default, Clone)]
struct Transport {
    previous: Option<Rc<dyn Fn()>>,
    play_pause: Option<Rc<dyn Fn()>>,
    stop: Option<Rc<dyn Fn()>>,
    next: Option<Rc<dyn Fn()>>,
}

/// Accessor for one [`Transport`] slot, used to wire a fullscreen button to
/// its callback without repeating the field type at every call site.
type TransportSlot = fn(&Transport) -> &Option<Rc<dyn Fn()>>;

#[derive(Clone)]
pub(in crate::ui) struct SongVisualizer {
    root: gtk4::Box,
    area: gtk4::DrawingArea,
    areas: Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::DrawingArea>>>>,
    engine: Rc<RefCell<VisualEngine>>,
    /// Mirrored outside the engine (which has no getter) so `set_spectrum`
    /// can gate on "are we actually playing" without borrowing it.
    playback: Rc<Cell<PlaybackState>>,
    panel_active: Rc<Cell<bool>>,
    fullscreen_active: Rc<Cell<bool>>,
    fullscreen_window: Rc<RefCell<Option<gtk4::glib::WeakRef<gtk4::Window>>>>,
    tick_id: Rc<RefCell<Option<gtk4::TickCallbackId>>>,
    /// Current track title + subtitle, mirrored into the fullscreen header.
    meta: Rc<RefCell<(String, String)>>,
    transport: Rc<RefCell<Transport>>,
    /// Live fullscreen header labels + play/pause button, so metadata and
    /// playback-state changes reflect while the overlay is open.
    fullscreen_meta: Rc<RefCell<Option<(gtk4::Label, gtk4::Label)>>>,
    fullscreen_play_pause: Rc<RefCell<Option<gtk4::Button>>>,
}

impl SongVisualizer {
    pub(in crate::ui) fn new() -> Self {
        let engine = Rc::new(RefCell::new(VisualEngine::new()));
        let areas = Rc::new(RefCell::new(Vec::new()));
        let area = drawing_area(&engine, DRAW_HEIGHT, "reprise-song-visual-canvas");
        register_area(&areas, &area);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 10);
        root.add_css_class("reprise-song-visuals");
        root.append(&area);
        root.append(&mode_controls(&engine, &areas));
        Self {
            root,
            area,
            areas,
            engine,
            playback: Rc::new(Cell::new(PlaybackState::Stopped)),
            panel_active: Rc::new(Cell::new(false)),
            fullscreen_active: Rc::new(Cell::new(false)),
            fullscreen_window: Rc::new(RefCell::new(None)),
            tick_id: Rc::new(RefCell::new(None)),
            meta: Rc::new(RefCell::new((String::new(), String::new()))),
            transport: Rc::new(RefCell::new(Transport::default())),
            fullscreen_meta: Rc::new(RefCell::new(None)),
            fullscreen_play_pause: Rc::new(RefCell::new(None)),
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    /// Mirrors the current track's title and subtitle into the fullscreen
    /// header. No-op on the inline canvas (which sits beside the panel's own
    /// metadata); only the immersive fullscreen view shows it.
    pub(in crate::ui) fn set_track_meta(&self, title: &str, subtitle: &str) {
        *self.meta.borrow_mut() = (title.to_owned(), subtitle.to_owned());
        if let Some((title_label, subtitle_label)) = self.fullscreen_meta.borrow().as_ref() {
            title_label.set_label(title);
            subtitle_label.set_label(subtitle);
            subtitle_label.set_visible(!subtitle.is_empty());
        }
    }

    /// Wires the fullscreen transport buttons to player actions.
    pub(in crate::ui) fn set_transport(
        &self,
        previous: impl Fn() + 'static,
        play_pause: impl Fn() + 'static,
        stop: impl Fn() + 'static,
        next: impl Fn() + 'static,
    ) {
        *self.transport.borrow_mut() = Transport {
            previous: Some(Rc::new(previous)),
            play_pause: Some(Rc::new(play_pause)),
            stop: Some(Rc::new(stop)),
            next: Some(Rc::new(next)),
        };
    }

    pub(in crate::ui) fn set_profile(&self, dimensions: &[u8; 4]) {
        self.engine.borrow_mut().set_static_profile(dimensions);
        self.settle_or_animate();
    }

    pub(in crate::ui) fn clear_profile(&self) {
        self.engine.borrow_mut().clear_static_profile();
        self.settle_or_animate();
    }

    pub(in crate::ui) fn set_spectrum(&self, frame: SpectrumFrame) {
        if !motion::animations_enabled() || self.playback.get() != PlaybackState::Playing {
            return;
        }
        self.engine.borrow_mut().ingest(&frame);
        self.ensure_tick();
    }

    pub(in crate::ui) fn set_playback_state(&self, playback: PlaybackState) {
        if let Some(button) = self.fullscreen_play_pause.borrow().as_ref() {
            button.set_icon_name(if playback == PlaybackState::Playing {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            });
        }
        self.playback.set(playback);
        let animations_enabled = motion::animations_enabled();
        {
            let mut engine = self.engine.borrow_mut();
            engine.set_playing(playback == PlaybackState::Playing);
            if !animations_enabled {
                engine.snap_to_static();
            }
        }
        if playback == PlaybackState::Playing || animations_enabled {
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

        let area = drawing_area(&self.engine, -1, "reprise-song-visual-fullscreen-canvas");
        area.set_vexpand(true);
        register_area(&self.areas, &area);

        let header = self.fullscreen_header();
        header.add_css_class("reprise-song-visual-chrome");
        let controls = gtk4::Box::new(gtk4::Orientation::Vertical, 14);
        controls.set_halign(gtk4::Align::Center);
        controls.set_valign(gtk4::Align::End);
        controls.set_margin_bottom(28);
        controls.add_css_class("reprise-song-visual-chrome");
        controls.append(&self.transport_controls());
        controls.append(&mode_controls(&self.engine, &self.areas));
        let hint = gtk4::Label::new(Some(&strings::text(strings::SONG_VISUALS_FULLSCREEN_HINT)));
        hint.add_css_class("dim-label");
        controls.append(&hint);

        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&area));
        overlay.add_overlay(&header);
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

        install_chrome_autohide(&window, &overlay, &header, &controls);

        let fullscreen_active = self.fullscreen_active.clone();
        let panel_active = self.panel_active.clone();
        let tick_id = self.tick_id.clone();
        let slot = self.fullscreen_window.clone();
        let meta_slot = self.fullscreen_meta.clone();
        let play_pause_slot = self.fullscreen_play_pause.clone();
        window.connect_destroy(move |_| {
            fullscreen_active.set(false);
            slot.borrow_mut().take();
            meta_slot.borrow_mut().take();
            play_pause_slot.borrow_mut().take();
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

    /// Builds the fullscreen header: track title over subtitle, top-centered.
    /// The labels are stashed so `set_track_meta` can update them live.
    fn fullscreen_header(&self) -> gtk4::Box {
        let (title_text, subtitle_text) = self.meta.borrow().clone();
        let header = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        header.set_halign(gtk4::Align::Center);
        header.set_valign(gtk4::Align::Start);
        header.set_margin_top(36);
        header.add_css_class("reprise-song-visual-header");

        let title = gtk4::Label::new(Some(&title_text));
        title.add_css_class("reprise-song-visual-header-title");
        title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let subtitle = gtk4::Label::new(Some(&subtitle_text));
        subtitle.add_css_class("reprise-song-visual-header-subtitle");
        subtitle.add_css_class("dim-label");
        subtitle.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        subtitle.set_visible(!subtitle_text.is_empty());

        header.append(&title);
        header.append(&subtitle);
        *self.fullscreen_meta.borrow_mut() = Some((title, subtitle));
        header
    }

    /// Builds the fullscreen transport row (previous · play/pause · stop · next),
    /// wired to the stored [`Transport`] actions. Buttons read the callbacks at
    /// click time, so they stay valid even if transport is wired after this.
    fn transport_controls(&self) -> gtk4::Box {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 14);
        row.set_halign(gtk4::Align::Center);
        row.add_css_class("reprise-song-visual-transport");

        let button = |icon: &str, label: &str, primary: bool| {
            let button = gtk4::Button::from_icon_name(icon);
            button.add_css_class("circular");
            button.add_css_class("reprise-song-visual-transport-btn");
            if primary {
                button.add_css_class("reprise-song-visual-transport-primary");
            }
            button.update_property(&[gtk4::accessible::Property::Label(&strings::text(label))]);
            button
        };

        let previous = button(
            "media-skip-backward-symbolic",
            strings::SONG_VISUALS_PREVIOUS,
            false,
        );
        let play_pause_icon = if self.playback.get() == PlaybackState::Playing {
            "media-playback-pause-symbolic"
        } else {
            "media-playback-start-symbolic"
        };
        let play_pause = button(play_pause_icon, strings::SONG_VISUALS_PLAY_PAUSE, true);
        let stop = button(
            "media-playback-stop-symbolic",
            strings::SONG_VISUALS_STOP,
            false,
        );
        let next = button(
            "media-skip-forward-symbolic",
            strings::SONG_VISUALS_NEXT,
            false,
        );

        let fire = |slot: TransportSlot, transport: &Rc<RefCell<Transport>>| {
            let transport = transport.clone();
            move || {
                if let Some(callback) = slot(&transport.borrow()) {
                    callback();
                }
            }
        };
        let previous_cb = fire(|t| &t.previous, &self.transport);
        previous.connect_clicked(move |_| previous_cb());
        let play_pause_cb = fire(|t| &t.play_pause, &self.transport);
        play_pause.connect_clicked(move |_| play_pause_cb());
        let stop_cb = fire(|t| &t.stop, &self.transport);
        stop.connect_clicked(move |_| stop_cb());
        let next_cb = fire(|t| &t.next, &self.transport);
        next.connect_clicked(move |_| next_cb());

        row.append(&previous);
        row.append(&play_pause);
        row.append(&stop);
        row.append(&next);
        *self.fullscreen_play_pause.borrow_mut() = Some(play_pause);
        row
    }

    fn is_active(&self) -> bool {
        self.panel_active.get() || self.fullscreen_active.get()
    }

    /// After a static-profile change: ease toward it over the tick loop when
    /// motion is allowed, otherwise snap straight to rest and repaint once.
    fn settle_or_animate(&self) {
        if motion::animations_enabled() {
            self.ensure_tick();
        } else {
            self.engine.borrow_mut().snap_to_static();
            queue_registered_areas(&self.areas);
        }
    }

    fn ensure_tick(&self) {
        if !self.is_active() || !motion::animations_enabled() || self.tick_id.borrow().is_some() {
            return;
        }
        let engine = self.engine.clone();
        let areas = self.areas.clone();
        let panel_active = self.panel_active.clone();
        let fullscreen_active = self.fullscreen_active.clone();
        let slot = self.tick_id.clone();
        let id = self.area.add_tick_callback(move |_, _| {
            if (!panel_active.get() && !fullscreen_active.get()) || !motion::animations_enabled() {
                *slot.borrow_mut() = None;
                return gtk4::glib::ControlFlow::Break;
            }
            let settled = engine.borrow_mut().tick();
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

fn drawing_area(
    engine: &Rc<RefCell<VisualEngine>>,
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
    let engine = engine.clone();
    area.set_draw_func(move |area, cr, width, height| {
        let accent = accent_rgb(area);
        engine.borrow_mut().set_accent(accent);
        let scene = engine.borrow().scene(width as f32, height as f32);
        render::draw_scene(cr, &scene);
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

fn accent_rgb(area: &gtk4::DrawingArea) -> (f32, f32, f32) {
    let color = area.color();
    (color.red(), color.green(), color.blue())
}

/// The picker's user-facing label for one visual mode.
fn mode_label(mode: VisualMode) -> &'static str {
    match mode {
        VisualMode::Grid => strings::SONG_VISUALS_MODE_GRID,
        VisualMode::Bars => strings::SONG_VISUALS_MODE_BARS,
        VisualMode::Rings => strings::SONG_VISUALS_MODE_RINGS,
        VisualMode::Flow => strings::SONG_VISUALS_MODE_FLOW,
        VisualMode::Pulse => strings::SONG_VISUALS_MODE_PULSE,
        VisualMode::Particles => strings::SONG_VISUALS_MODE_PARTICLES,
        VisualMode::Neon => strings::SONG_VISUALS_MODE_NEON,
        VisualMode::Tunnel => strings::SONG_VISUALS_MODE_TUNNEL,
    }
}

/// Builds the grouped mode-toggle row: one [`gtk4::ToggleButton`] per
/// [`VisualMode`], wrapped in a [`gtk4::FlowBox`] so it reflows at narrow
/// widths instead of overflowing. Shared by the inline canvas and the
/// fullscreen overlay — each call builds a fresh, independent row that reads
/// the engine's current mode at construction time.
fn mode_controls(
    engine: &Rc<RefCell<VisualEngine>>,
    areas: &Rc<RefCell<Vec<gtk4::glib::WeakRef<gtk4::DrawingArea>>>>,
) -> gtk4::FlowBox {
    let flow = gtk4::FlowBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .max_children_per_line(4)
        .column_spacing(6)
        .row_spacing(6)
        .halign(gtk4::Align::Center)
        .build();
    flow.add_css_class("reprise-song-visual-modes");

    let current_mode = engine.borrow().mode();
    let mut group_leader: Option<gtk4::ToggleButton> = None;
    for mode in VisualMode::ALL {
        let button = gtk4::ToggleButton::builder()
            .label(strings::text(mode_label(mode)))
            .active(mode == current_mode)
            .build();
        button.set_widget_name(mode.id());
        button.add_css_class("flat");
        buttons::arm(&button, buttons::TOGGLE_CLASS);
        match &group_leader {
            Some(leader) => button.set_group(Some(leader)),
            None => group_leader = Some(button.clone()),
        }

        let engine = engine.clone();
        let areas = areas.clone();
        button.connect_toggled(move |button| {
            if !button.is_active() {
                return;
            }
            engine.borrow_mut().set_mode(mode);
            queue_registered_areas(&areas);
        });

        flow.append(&button);
    }
    flow
}

/// Fades the fullscreen chrome (header + transport) out after the pointer sits
/// idle and brings it — plus the cursor — back on the next mouse move. Immersive
/// by default: the GUI only appears when you reach for it.
fn install_chrome_autohide(
    window: &gtk4::Window,
    overlay: &gtk4::Overlay,
    header: &gtk4::Box,
    controls: &gtk4::Box,
) {
    const IDLE: Duration = Duration::from_millis(2500);
    let timer: Rc<RefCell<Option<gtk4::glib::SourceId>>> = Rc::new(RefCell::new(None));
    let header = header.downgrade();
    let controls = controls.downgrade();
    let window_weak = window.downgrade();

    let reveal = move || {
        if let Some(header) = header.upgrade() {
            header.remove_css_class("reprise-song-visual-chrome-hidden");
        }
        if let Some(controls) = controls.upgrade() {
            controls.remove_css_class("reprise-song-visual-chrome-hidden");
        }
        if let Some(window) = window_weak.upgrade() {
            window.set_cursor(None);
        }
        if let Some(id) = timer.borrow_mut().take() {
            id.remove();
        }
        let header = header.clone();
        let controls = controls.clone();
        let window_weak = window_weak.clone();
        let timer_inner = timer.clone();
        let id = gtk4::glib::timeout_add_local_once(IDLE, move || {
            if let Some(header) = header.upgrade() {
                header.add_css_class("reprise-song-visual-chrome-hidden");
            }
            if let Some(controls) = controls.upgrade() {
                controls.add_css_class("reprise-song-visual-chrome-hidden");
            }
            if let Some(window) = window_weak.upgrade() {
                window.set_cursor_from_name(Some("none"));
            }
            timer_inner.borrow_mut().take();
        });
        *timer.borrow_mut() = Some(id);
    };

    let motion = gtk4::EventControllerMotion::new();
    let reveal = Rc::new(reveal);
    let reveal_motion = reveal.clone();
    motion.connect_motion(move |_, _, _| reveal_motion());
    overlay.add_controller(motion);
    // Show once on open, then arm the idle timer.
    reveal();
}

pub(in crate::ui) fn css() -> String {
    ".reprise-song-visuals { margin: 0 18px 12px; }\n\
     .reprise-song-visual-canvas {\
       color: @reprise_player_accent;\
       background-color: alpha(#ffffff, 0.025);\
       border: 1px solid alpha(@reprise_player_accent, 0.14);\
       border-radius: 24px;\
     }\n\
     .reprise-song-visual-modes { margin-top: 2px; }\n\
     window.reprise-song-visual-fullscreen { background: #090b0c; }\n\
     .reprise-song-visual-chrome {\
       transition: opacity 260ms ease-out;\
       opacity: 1;\
     }\n\
     .reprise-song-visual-chrome-hidden { opacity: 0; }\n\
     .reprise-song-visual-fullscreen-canvas {\
       color: @reprise_player_accent;\
       background-image: radial-gradient(ellipse at center,\
         alpha(@reprise_player_accent, 0.12) 0%,\
         alpha(#090b0c, 0) 72%);\
     }\n\
     .reprise-song-visual-header-title {\
       font-size: 1.6rem; font-weight: 800; color: #ffffff;\
     }\n\
     .reprise-song-visual-header-subtitle { font-size: 1.05rem; }\n\
     .reprise-song-visual-transport-btn {\
       min-width: 46px; min-height: 46px;\
       color: alpha(#ffffff, 0.82);\
       background-color: alpha(#ffffff, 0.08);\
       border: 1px solid alpha(#ffffff, 0.12);\
     }\n\
     .reprise-song-visual-transport-btn:hover {\
       color: #ffffff; background-color: alpha(#ffffff, 0.16);\
     }\n\
     .reprise-song-visual-transport-primary {\
       min-width: 56px; min-height: 56px;\
       color: #ffffff;\
       background-color: alpha(@reprise_player_accent, 0.28);\
       border-color: alpha(@reprise_player_accent, 0.85);\
     }\n\
     .reprise-song-visual-transport-primary:hover {\
       background-color: alpha(@reprise_player_accent, 0.42);\
     }"
    .to_owned()
}

#[cfg(test)]
#[path = "song_visualizer_tests.rs"]
mod tests;
