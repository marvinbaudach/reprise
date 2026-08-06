//! The surface the bloom is painted on: a texture the snapshot places, rather
//! than a picture Cairo redraws.
//!
//! The breath changes two things per frame — an opacity and a scale — and
//! nothing else. Rasterizing the blurred cover again for each of those was the
//! larger half of the app's idle cost on a GPU (measured: 135 ms/s for the
//! Cairo path against 57 ms/s for a draw function that painted nothing, and
//! 55 ms/s for this). The blur is still bought once per track in
//! `cover_glow::blurred_surface`; only the per-frame rasterizing is gone.
//!
//! The geometry is the Cairo path's, kept deliberately: the band is clipped to
//! the panel, the texture overflows it by `BLOOM_WIDTH_FACTOR` on purpose, and
//! the 11× upscale of a 32 px surface *is* the blur, so the scaling filter is
//! linear by intent and not by omission.

use std::cell::{Cell, RefCell};

use gtk4::gdk;
use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::cover_bloom::{BLOOM_HEIGHT, BLOOM_WIDTH_FACTOR};

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    pub struct BloomArea {
        pub texture: RefCell<Option<gdk::Texture>>,
        pub opacity: Cell<f64>,
        pub scale: Cell<f64>,
    }

    impl Default for BloomArea {
        fn default() -> Self {
            Self {
                texture: RefCell::new(None),
                opacity: Cell::new(0.0),
                scale: Cell::new(1.0),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for BloomArea {
        const NAME: &'static str = "RepriseCoverBloomArea";
        type Type = super::BloomArea;
        type ParentType = gtk4::Widget;

        fn class_init(class: &mut Self::Class) {
            class.set_css_name("reprise-now-playing-bloom");
            class.set_accessible_role(gtk4::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for BloomArea {}

    impl WidgetImpl for BloomArea {
        /// No intrinsic size: the bloom fills whatever the head overlay gives
        /// it and must never push the panel wider.
        fn measure(&self, _orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let texture = self.texture.borrow().clone();
            let Some(texture) = texture else {
                return;
            };
            let widget = self.obj();
            let width = f64::from(widget.width());
            let height = f64::from(widget.height());
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            let opacity = self.opacity.get();
            if opacity <= 0.0 {
                return;
            }

            let band = BLOOM_HEIGHT.min(height);
            let scale = self.scale.get() as f32;
            let rest_width = (width * BLOOM_WIDTH_FACTOR) as f32;
            let rest_height = BLOOM_HEIGHT as f32;
            let rest_x = ((width - width * BLOOM_WIDTH_FACTOR) / 2.0) as f32;

            // Clipped to the panel: the 124 % width overflows both edges on
            // purpose, exactly as the Cairo path clipped it.
            snapshot.push_clip(&gtk4::graphene::Rect::new(
                0.0,
                0.0,
                width as f32,
                band as f32,
            ));
            snapshot.push_opacity(opacity);

            // The breath's scale is a transform around the band's centre, not a
            // different destination rectangle. Same picture either way — but a
            // texture node whose parameters never change can be reused frame to
            // frame, while one that is handed new bounds every frame is new
            // work every frame. Measured: ~96 ms/s for the resized rectangle
            // against ~55 for this.
            let centre_x = (width / 2.0) as f32;
            let centre_y = rest_height / 2.0;
            snapshot.save();
            snapshot.translate(&gtk4::graphene::Point::new(centre_x, centre_y));
            snapshot.scale(scale, scale);
            snapshot.translate(&gtk4::graphene::Point::new(-centre_x, -centre_y));
            snapshot.append_scaled_texture(
                &texture,
                gtk4::gsk::ScalingFilter::Linear,
                &gtk4::graphene::Rect::new(rest_x, 0.0, rest_width, rest_height),
            );
            snapshot.restore();

            snapshot.pop();
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct BloomArea(ObjectSubclass<imp::BloomArea>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Default for BloomArea {
    fn default() -> Self {
        Self::new()
    }
}

impl BloomArea {
    pub(super) fn new() -> Self {
        let area: Self = glib::Object::builder().build();
        // Decoration only: it must never take a click meant for the cover.
        area.set_can_target(false);
        area.set_can_focus(false);
        area
    }

    /// The blurred cover, handed over once per track.
    pub(super) fn set_texture(&self, texture: Option<&gdk::Texture>) {
        *self.imp().texture.borrow_mut() = texture.cloned();
        self.queue_draw();
    }

    #[cfg(test)]
    pub(super) fn has_texture(&self) -> bool {
        self.imp().texture.borrow().is_some()
    }

    /// One frame of the breath. Redraws only when something actually moved —
    /// a frame that would look identical is not worth invalidating for, and
    /// invalidation is what the remaining idle cost is made of.
    pub(super) fn set_light(&self, opacity: f64, scale: f64) {
        let imp = self.imp();
        if (imp.opacity.get() - opacity).abs() < f64::EPSILON
            && (imp.scale.get() - scale).abs() < f64::EPSILON
        {
            return;
        }
        imp.opacity.set(opacity);
        imp.scale.set(scale);
        self.queue_draw();
    }
}
