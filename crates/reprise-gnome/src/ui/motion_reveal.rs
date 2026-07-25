//! Layout-neutral left-to-right reveal for proportional horizontal content.
//!
//! `HorizontalReveal` delegates measurement and allocation to `GtkBinLayout`
//! and clips only the child's snapshot. Its reserved width therefore stays
//! stable while `reveal-fraction` grows from zero to one.

use std::cell::{Cell, RefCell};
use std::sync::LazyLock;

use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    pub struct HorizontalReveal {
        pub child: RefCell<Option<gtk4::Widget>>,
        pub reveal_fraction: Cell<f32>,
    }

    impl Default for HorizontalReveal {
        fn default() -> Self {
            Self {
                child: RefCell::new(None),
                reveal_fraction: Cell::new(1.0),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for HorizontalReveal {
        const NAME: &'static str = "RepriseHorizontalReveal";
        type Type = super::HorizontalReveal;
        type ParentType = gtk4::Widget;

        fn class_init(class: &mut Self::Class) {
            class.set_layout_manager_type::<gtk4::BinLayout>();
            class.set_css_name("reprise-horizontal-reveal");
            class.set_accessible_role(gtk4::AccessibleRole::Presentation);
        }
    }

    impl ObjectImpl for HorizontalReveal {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: LazyLock<Vec<glib::ParamSpec>> = LazyLock::new(|| {
                vec![glib::ParamSpecFloat::builder("reveal-fraction")
                    .minimum(0.0)
                    .maximum(1.0)
                    .default_value(1.0)
                    .readwrite()
                    .build()]
            });
            PROPERTIES.as_ref()
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "reveal-fraction" => {
                    self.reveal_fraction
                        .set(value.get().expect("reveal-fraction must be an f32"));
                    self.obj().queue_draw();
                }
                name => unreachable!("unknown HorizontalReveal property {name}"),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "reveal-fraction" => self.reveal_fraction.get().to_value(),
                name => unreachable!("unknown HorizontalReveal property {name}"),
            }
        }

        fn dispose(&self) {
            if let Some(child) = self.child.borrow_mut().take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for HorizontalReveal {
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
            snapshot.push_clip(&gtk4::graphene::Rect::new(
                0.0,
                0.0,
                reveal_width,
                widget.height() as f32,
            ));
            widget.snapshot_child(&child, snapshot);
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    pub struct HorizontalReveal(ObjectSubclass<imp::HorizontalReveal>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl HorizontalReveal {
    pub(crate) fn new(child: &impl IsA<gtk4::Widget>) -> Self {
        let reveal: Self = glib::Object::new();
        reveal.set_child(Some(child));
        reveal
    }

    fn set_child(&self, child: Option<&impl IsA<gtk4::Widget>>) {
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

    use super::HorizontalReveal;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn horizontal_reveal_clips_without_changing_the_width_requirement() {
        gtk4::init().unwrap();
        let child = gtk4::Label::new(Some("Reserved width"));
        child.set_size_request(180, 48);
        let reveal = HorizontalReveal::new(&child);
        let window = gtk4::Window::builder()
            .default_width(320)
            .default_height(160)
            .child(&reveal)
            .build();
        window.present();
        run_main_loop_for_layout();

        let initial_measure = reveal.measure(gtk4::Orientation::Horizontal, -1);
        let initial_allocation = (reveal.width(), reveal.height());
        reveal.set_reveal_fraction(0.25);
        run_main_loop_for_layout();

        assert_eq!(reveal.reveal_fraction(), 0.25);
        assert_eq!(
            reveal.measure(gtk4::Orientation::Horizontal, -1),
            initial_measure
        );
        assert_eq!((reveal.width(), reveal.height()), initial_allocation);
        window.close();
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn horizontal_reveal_preserves_child_expansion_in_its_parent_layout() {
        gtk4::init().unwrap();
        let child = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        child.set_hexpand(true);
        child.set_vexpand(true);
        child.set_halign(gtk4::Align::Start);
        child.set_valign(gtk4::Align::End);
        child.set_size_request(140, 24);
        let reveal = HorizontalReveal::new(&child);

        assert!(reveal.compute_expand(gtk4::Orientation::Horizontal));
        assert!(reveal.compute_expand(gtk4::Orientation::Vertical));
        assert_eq!(reveal.halign(), gtk4::Align::Start);
        assert_eq!(reveal.valign(), gtk4::Align::End);
        assert_eq!(reveal.width_request(), 140);
        assert_eq!(reveal.height_request(), 24);
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
