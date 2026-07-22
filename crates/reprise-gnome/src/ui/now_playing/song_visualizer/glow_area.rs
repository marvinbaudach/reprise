//! `GlowArea`: a `gtk4::Widget` subclass hosting the same `VisualEngine`
//! scene as the Cairo `DrawingArea` canvas (`song_visualizer.rs::drawing_area`),
//! but rendered via GSK's `snapshot()` vfunc instead of a Cairo `draw_func`.
//! That gets us a real GPU-composited Gaussian blur
//! (`gtk4::Snapshot::push_blur`/`pop`) around the `glow > 0` shapes — actual
//! bloom, rather than the wide translucent under-stroke `render::draw_scene`
//! fakes for the Cairo fallback.
//!
//! Selected by `song_visualizer::gpu_visuals_enabled()`; wherever it's built
//! it replaces the `DrawingArea` one-for-one (same `hexpand`/`vexpand`/
//! `height_request` shape, same accessible role/label, same CSS class) so
//! the rest of the module — the tick loop, `queue_registered_areas`, the
//! accent color read — doesn't need to know which canvas kind it's driving.
//! Both canvas kinds upcast to `gtk4::Widget`, which is all the shared
//! plumbing (`SongVisualizer::area`, `areas`) needs.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;
use reprise_core::visuals::VisualEngine;

use crate::ui::strings;

use super::{accent_rgb, render};

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    /// Signature for the vignette-style "paint something behind the scene"
    /// hook the fullscreen canvas installs via `GlowArea::set_pre_paint`;
    /// unused by the inline canvas. Runs on the crisp layer's Cairo context,
    /// before `render::draw_crisp`, in its own unblurred snapshot layer
    /// beneath the (optional) glow blur — see `snapshot()`.
    pub type PrePaint = dyn Fn(&gtk4::cairo::Context, f32, f32);

    #[derive(Default)]
    pub struct GlowArea {
        pub engine: RefCell<Option<Rc<RefCell<VisualEngine>>>>,
        /// Minimum height reported to the layout manager; `<= 0` reports no
        /// minimum (the fullscreen canvas relies on its `Overlay` parent and
        /// `vexpand` to fill the available height instead).
        pub min_height: Cell<i32>,
        pub pre_paint: RefCell<Option<Rc<PrePaint>>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GlowArea {
        const NAME: &'static str = "RepriseGlowArea";
        type Type = super::GlowArea;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for GlowArea {}

    impl WidgetImpl for GlowArea {
        fn measure(&self, orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            let minimum = if orientation == gtk4::Orientation::Vertical {
                self.min_height.get().max(0)
            } else {
                0
            };
            (minimum, minimum, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            let (width, height) = (widget.width() as f32, widget.height() as f32);
            if width < 1.0 || height < 1.0 {
                return;
            }
            let Some(engine) = self.engine.borrow().clone() else {
                return;
            };
            let accent = accent_rgb(&*widget);
            engine.borrow_mut().set_accent(accent);
            let scene = engine.borrow().scene(width, height);

            let bounds = gtk4::graphene::Rect::new(0.0, 0.0, width, height);

            // Background layer (fullscreen vignette only; a no-op for the
            // inline canvas, which never installs `pre_paint`) — its own
            // unblurred snapshot layer, painted before the glow blur so the
            // vignette sits behind everything, exactly like the Cairo
            // fallback's `paint_vignette` call before `draw_scene`.
            if let Some(pre_paint) = self.pre_paint.borrow().as_ref() {
                let cr = snapshot.append_cairo(&bounds);
                pre_paint(&cr, width, height);
            }

            let radius = render::scene_blur_radius(&scene);
            if radius > 0.5 {
                snapshot.push_blur(f64::from(radius));
                let cr = snapshot.append_cairo(&bounds);
                render::draw_glow_layer(&cr, &scene);
                snapshot.pop();
            }
            let cr = snapshot.append_cairo(&bounds);
            render::draw_crisp(&cr, &scene);
        }
    }
}

glib::wrapper! {
    pub struct GlowArea(ObjectSubclass<imp::GlowArea>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl GlowArea {
    /// Builds a `GlowArea` that mirrors `drawing_area`'s construction shape:
    /// same accessible role/label, hexpand always on, `height_request` via
    /// `min_height` (`-1`/`<=0` for "no minimum, rely on `vexpand`"), and
    /// `vexpand`/`css_class` supplied per call site (the inline canvas is
    /// fixed-height and doesn't vexpand; the fullscreen canvas does both).
    pub fn new(
        engine: Rc<RefCell<VisualEngine>>,
        min_height: i32,
        vexpand: bool,
        css_class: &str,
    ) -> Self {
        let obj: Self = glib::Object::new();
        obj.set_hexpand(true);
        obj.set_vexpand(vexpand);
        obj.add_css_class(css_class);
        obj.set_accessible_role(gtk4::AccessibleRole::Img);
        obj.update_property(&[gtk4::accessible::Property::Label(&strings::text(
            strings::SONG_VISUALS_ACCESSIBLE,
        ))]);

        let imp = obj.imp();
        *imp.engine.borrow_mut() = Some(engine);
        imp.min_height.set(min_height);
        obj
    }

    /// Installs a "paint something behind the scene" hook — the fullscreen
    /// canvas uses this for its dark vignette (`fullscreen::paint_vignette`);
    /// the inline canvas never calls this, so `snapshot()` simply skips the
    /// background layer for it. See `imp::PrePaint`.
    pub fn set_pre_paint(&self, pre_paint: impl Fn(&gtk4::cairo::Context, f32, f32) + 'static) {
        *self.imp().pre_paint.borrow_mut() = Some(Rc::new(pre_paint));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A custom `Widget` subclass with a wrong `measure()` collapses to 0×0
    /// and renders nothing — this pins down that `GlowArea`'s `measure()`
    /// actually reports its `min_height` once realized, both for the
    /// inline-canvas shape (`min_height > 0`) and the fullscreen shape
    /// (`min_height <= 0`, relying on `vexpand` + the parent to size it).
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn glow_area_allocates_a_non_zero_size_once_presented() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();

        // Inline-canvas shape: fixed min height, no vexpand.
        let inline_engine = Rc::new(RefCell::new(VisualEngine::new()));
        let inline_area = GlowArea::new(inline_engine, 220, false, "reprise-song-visual-canvas");
        let inline_window = gtk4::Window::new();
        inline_window.set_default_size(400, 300);
        inline_window.set_child(Some(&inline_area));
        inline_window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(
            inline_area.width() > 0,
            "inline GlowArea width was 0 after present"
        );
        assert!(
            inline_area.height() > 0,
            "inline GlowArea height was 0 after present"
        );
        inline_window.close();

        // Fullscreen-canvas shape: no minimum, expands to fill its parent.
        let fullscreen_engine = Rc::new(RefCell::new(VisualEngine::new()));
        let fullscreen_area = GlowArea::new(
            fullscreen_engine,
            -1,
            true,
            "reprise-song-visual-fullscreen-canvas",
        );
        let fullscreen_window = gtk4::Window::new();
        fullscreen_window.set_default_size(400, 300);
        fullscreen_window.set_child(Some(&fullscreen_area));
        fullscreen_window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}
        assert!(
            fullscreen_area.width() > 0,
            "fullscreen GlowArea width was 0 after present"
        );
        assert!(
            fullscreen_area.height() > 0,
            "fullscreen GlowArea height was 0 after present"
        );
        fullscreen_window.close();
    }
}
