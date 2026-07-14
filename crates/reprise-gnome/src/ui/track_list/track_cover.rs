//! Density-aware cover cell for the library `GtkColumnView`.
//!
//! Cached list thumbnails are 48 px, while the table supports multiple row
//! densities. A plain `GtkImage` contributes an intrinsic content size that
//! can override a compact row. This widget deliberately reports no intrinsic
//! content size and scales the paintable into the allocation chosen by the
//! live density styles.

use std::cell::RefCell;

use gtk4::gdk;
use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

const PLACEHOLDER_ICON: &str = "audio-x-generic-symbolic";

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct TrackCover {
        pub paintable: RefCell<Option<gdk::Paintable>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TrackCover {
        const NAME: &'static str = "RepriseTrackCover";
        type Type = super::TrackCover;
        type ParentType = gtk4::Widget;

        fn class_init(class: &mut Self::Class) {
            class.set_css_name("reprise-track-cover");
            class.set_accessible_role(gtk4::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for TrackCover {}

    impl WidgetImpl for TrackCover {
        fn measure(&self, _orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let paintable = self.paintable.borrow().clone();
            let Some(paintable) = paintable else {
                return;
            };
            let widget = self.obj();
            let width = widget.width() as f64;
            let height = widget.height() as f64;
            let side = width.min(height);
            if side <= 0.0 {
                return;
            }

            snapshot.save();
            snapshot.translate(&gtk4::graphene::Point::new(
                ((width - side) / 2.0) as f32,
                ((height - side) / 2.0) as f32,
            ));
            paintable.snapshot(snapshot, side, side);
            snapshot.restore();
        }
    }
}

glib::wrapper! {
    pub struct TrackCover(ObjectSubclass<imp::TrackCover>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl TrackCover {
    pub fn new() -> Self {
        glib::Object::new()
    }

    pub fn set_paintable(&self, paintable: Option<&impl IsA<gdk::Paintable>>) {
        *self.imp().paintable.borrow_mut() = paintable.map(|value| value.clone().upcast());
        self.queue_draw();
    }

    pub fn set_placeholder(&self) {
        let theme = gtk4::IconTheme::for_display(&self.display());
        let icon = theme.lookup_icon(
            PLACEHOLDER_ICON,
            &[],
            24,
            self.scale_factor(),
            self.direction(),
            gtk4::IconLookupFlags::empty(),
        );
        self.set_paintable(Some(&icon));
    }
}

impl Default for TrackCover {
    fn default() -> Self {
        Self::new()
    }
}
