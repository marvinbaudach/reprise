//! Backdrop renderer for one clipped chrome zone.

#[cfg(test)]
use std::cell::Cell;
use std::cell::RefCell;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use super::material::{current_theme, GlassEnvironment, GlassMode};

type AllocateCallback = Rc<dyn Fn(i32, i32)>;

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct GlassBackdrop {
        pub paintable: RefCell<Option<gtk4::WidgetPaintable>>,
        pub source: glib::WeakRef<gtk4::Widget>,
        pub on_allocate: RefCell<Option<AllocateCallback>>,
        #[cfg(test)]
        pub snapshot_count: Cell<u32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for GlassBackdrop {
        const NAME: &'static str = "RepriseGlassBackdrop";
        type Type = super::GlassBackdrop;
        type ParentType = gtk4::Widget;

        fn class_init(class: &mut Self::Class) {
            class.set_css_name("reprise-glass-backdrop");
        }
    }

    impl ObjectImpl for GlassBackdrop {}

    impl WidgetImpl for GlassBackdrop {
        fn measure(&self, _orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            #[cfg(test)]
            self.snapshot_count.set(self.snapshot_count.get() + 1);
            let widget = self.obj();
            let width = widget.width();
            let height = widget.height();
            if width <= 0 || height <= 0 {
                return;
            }

            let material = GlassEnvironment::for_widget(widget.as_ref()).material(current_theme());
            if material.mode == GlassMode::BackdropBlur
                && !crate::ui::glass::performance::suppress_backdrop_for_baseline()
            {
                self.snapshot_source(snapshot, width, height, material.blur_radius);
            }
            snapshot.append_color(
                &gdk::RGBA::new(
                    material.tint.red,
                    material.tint.green,
                    material.tint.blue,
                    material.tint.alpha,
                ),
                &gtk4::graphene::Rect::new(0.0, 0.0, width as f32, height as f32),
            );
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            let callback = self.on_allocate.borrow().clone();
            if let Some(callback) = callback {
                callback(width, height);
            }
        }
    }

    impl GlassBackdrop {
        fn snapshot_source(
            &self,
            snapshot: &gtk4::Snapshot,
            width: i32,
            height: i32,
            blur_radius: f32,
        ) {
            let (Some(source), Some(paintable)) =
                (self.source.upgrade(), self.paintable.borrow().clone())
            else {
                return;
            };
            let widget = self.obj();
            let Some(origin) =
                source.compute_point(widget.as_ref(), &gtk4::graphene::Point::new(0.0, 0.0))
            else {
                return;
            };

            snapshot.push_clip(&gtk4::graphene::Rect::new(
                0.0,
                0.0,
                width as f32,
                height as f32,
            ));
            snapshot.push_blur(f64::from(blur_radius));
            snapshot.save();
            snapshot.translate(&origin);
            paintable.snapshot(
                snapshot,
                f64::from(source.width()),
                f64::from(source.height()),
            );
            snapshot.restore();
            snapshot.pop();
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct GlassBackdrop(ObjectSubclass<imp::GlassBackdrop>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl GlassBackdrop {
    pub(crate) fn new(source: &impl IsA<gtk4::Widget>) -> Self {
        let backdrop: Self = glib::Object::new();
        let source = source.clone().upcast::<gtk4::Widget>();
        let paintable = gtk4::WidgetPaintable::new(Some(&source));
        let backdrop_weak = backdrop.downgrade();
        paintable.connect_invalidate_contents(move |_| {
            if let Some(backdrop) = backdrop_weak.upgrade() {
                backdrop.queue_draw();
            }
        });
        let backdrop_weak = backdrop.downgrade();
        paintable.connect_invalidate_size(move |_| {
            if let Some(backdrop) = backdrop_weak.upgrade() {
                backdrop.queue_draw();
            }
        });
        backdrop.imp().source.set(Some(&source));
        *backdrop.imp().paintable.borrow_mut() = Some(paintable);
        backdrop.set_can_target(false);
        backdrop.set_hexpand(true);
        backdrop.set_vexpand(true);
        backdrop
    }

    pub(crate) fn set_on_allocate(&self, callback: Rc<dyn Fn(i32, i32)>) {
        *self.imp().on_allocate.borrow_mut() = Some(callback);
    }

    #[cfg(test)]
    pub(crate) fn snapshot_count(&self) -> u32 {
        self.imp().snapshot_count.get()
    }
}
