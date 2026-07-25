//! Layout-neutral vertical translation for entrance motion.
//!
//! `SlideBin` delegates measurement and allocation to `GtkBinLayout`, then
//! translates only the child's snapshot. Its reserved space therefore never
//! changes while `offset-y` is animated.

use std::cell::{Cell, RefCell};
use std::sync::LazyLock;

use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    pub struct SlideBin {
        pub child: RefCell<Option<gtk4::Widget>>,
        pub offset_y: Cell<f32>,
        pub reveal_fraction: Cell<f32>,
    }

    impl Default for SlideBin {
        fn default() -> Self {
            Self {
                child: RefCell::new(None),
                offset_y: Cell::new(0.0),
                reveal_fraction: Cell::new(1.0),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for SlideBin {
        const NAME: &'static str = "RepriseSlideBin";
        type Type = super::SlideBin;
        type ParentType = gtk4::Widget;

        fn class_init(class: &mut Self::Class) {
            class.set_layout_manager_type::<gtk4::BinLayout>();
            class.set_css_name("reprise-slide-bin");
            class.set_accessible_role(gtk4::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for SlideBin {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: LazyLock<Vec<glib::ParamSpec>> = LazyLock::new(|| {
                vec![
                    glib::ParamSpecFloat::builder("offset-y")
                        .minimum(f32::MIN)
                        .maximum(f32::MAX)
                        .default_value(0.0)
                        .readwrite()
                        .build(),
                    glib::ParamSpecFloat::builder("reveal-fraction")
                        .minimum(0.0)
                        .maximum(1.0)
                        .default_value(1.0)
                        .readwrite()
                        .build(),
                ]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "offset-y" => {
                    self.offset_y
                        .set(value.get().expect("offset-y must be an f32"));
                    self.obj().queue_draw();
                }
                "reveal-fraction" => {
                    self.reveal_fraction
                        .set(value.get().expect("reveal-fraction must be an f32"));
                    self.obj().queue_draw();
                }
                name => unreachable!("unknown SlideBin property {name}"),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "offset-y" => self.offset_y.get().to_value(),
                "reveal-fraction" => self.reveal_fraction.get().to_value(),
                name => unreachable!("unknown SlideBin property {name}"),
            }
        }

        fn dispose(&self) {
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for SlideBin {
        fn compute_expand(&self, hexpand: &mut bool, vexpand: &mut bool) {
            self.parent_compute_expand(hexpand, vexpand);
            let child = self.child.borrow().clone();
            let Some(child) = child else { return };
            *hexpand |= child.compute_expand(gtk4::Orientation::Horizontal);
            *vexpand |= child.compute_expand(gtk4::Orientation::Vertical);
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let child = self.child.borrow().clone();
            let Some(child) = child else { return };
            let widget = self.obj();
            let reveal_width = widget.width() as f32 * self.reveal_fraction.get();
            snapshot.save();
            snapshot.push_clip(&gtk4::graphene::Rect::new(
                0.0,
                0.0,
                reveal_width,
                widget.height() as f32,
            ));
            snapshot.translate(&gtk4::graphene::Point::new(0.0, self.offset_y.get()));
            widget.snapshot_child(&child, snapshot);
            snapshot.pop();
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    pub struct SlideBin(ObjectSubclass<imp::SlideBin>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl SlideBin {
    pub(crate) fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let slide: Self = glib::Object::new();
        slide.set_child(Some(child));
        slide
    }

    pub(crate) fn set_child(&self, child: Option<&impl IsA<gtk4::Widget>>) {
        let child = child.map(|child| child.clone().upcast::<gtk4::Widget>());
        if self.imp().child.borrow().as_ref() == child.as_ref() {
            return;
        }
        if let Some(previous) = self.imp().child.borrow_mut().take() {
            previous.unparent();
        }
        if let Some(child) = &child {
            self.set_halign(child.halign());
            self.set_valign(child.valign());
            self.set_width_request(child.width_request());
            self.set_height_request(child.height_request());
            child.set_parent(self);
        }
        self.imp().child.replace(child);
    }

    pub(crate) fn offset_y(&self) -> f32 {
        self.property("offset-y")
    }

    pub(crate) fn set_offset_y(&self, offset_y: f32) {
        self.set_property("offset-y", offset_y);
    }

    pub(crate) fn set_reveal_fraction(&self, reveal_fraction: f32) {
        self.set_property("reveal-fraction", reveal_fraction);
    }

    #[cfg(test)]
    pub(crate) fn reveal_fraction(&self) -> f32 {
        self.property("reveal-fraction")
    }
}

#[cfg(test)]
mod tests {
    use gtk4::prelude::*;

    use super::SlideBin;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn slide_bin_translates_paint_without_changing_layout() {
        gtk4::init().unwrap();
        let child = gtk4::Label::new(Some("Reserved space"));
        child.set_size_request(180, 48);
        let slide = SlideBin::new(&child);
        let window = gtk4::Window::builder()
            .default_width(320)
            .default_height(160)
            .child(&slide)
            .build();
        window.present();
        run_main_loop_for_layout();

        let initial_size = (slide.width(), slide.height(), child.width(), child.height());
        slide.set_offset_y(16.0);
        run_main_loop_for_layout();

        assert_eq!(slide.offset_y(), 16.0);
        assert_eq!(
            (slide.width(), slide.height(), child.width(), child.height()),
            initial_size,
            "snapshot translation must not trigger a layout change"
        );
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn slide_bin_preserves_child_expansion_in_its_parent_layout() {
        gtk4::init().unwrap();
        let child = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        child.set_hexpand(true);
        child.set_vexpand(true);
        child.set_halign(gtk4::Align::Start);
        child.set_valign(gtk4::Align::End);
        child.set_size_request(140, 24);
        let slide = SlideBin::new(&child);

        assert!(
            slide.compute_expand(gtk4::Orientation::Horizontal),
            "a hexpanding child must make its transparent wrapper expand"
        );
        assert!(
            slide.compute_expand(gtk4::Orientation::Vertical),
            "a vexpanding child must make its transparent wrapper expand"
        );
        assert_eq!(slide.halign(), gtk4::Align::Start);
        assert_eq!(slide.valign(), gtk4::Align::End);
        assert_eq!(slide.width_request(), 140);
        assert_eq!(slide.height_request(), 24);
    }

    fn run_main_loop_for_layout() {
        let main_loop = gtk4::glib::MainLoop::new(None, false);
        let quit = main_loop.clone();
        gtk4::glib::timeout_add_local_once(std::time::Duration::from_millis(50), move || {
            quit.quit();
        });
        main_loop.run();
    }
}
