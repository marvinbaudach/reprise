//! Virtualization-preserving scroll-range insets for native GTK list widgets.

use std::cell::{Cell, RefCell};
use std::sync::OnceLock;

use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

const EDGE_EPSILON: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
struct ScrollProjection {
    inner_lower: f64,
    inner_max: f64,
    page_size: f64,
    top: f64,
    bottom: f64,
}

impl ScrollProjection {
    fn new(inner_lower: f64, inner_upper: f64, page_size: f64, top: f64, bottom: f64) -> Self {
        Self {
            inner_lower,
            inner_max: (inner_upper - page_size).max(inner_lower),
            page_size,
            top: top.max(0.0),
            bottom: bottom.max(0.0),
        }
    }

    fn outer_lower(self) -> f64 {
        self.inner_lower - self.top
    }

    fn outer_max(self) -> f64 {
        self.inner_max + self.bottom
    }

    fn outer_upper(self) -> f64 {
        self.outer_max() + self.page_size
    }

    fn project(self, outer_value: f64) -> (f64, f64) {
        if outer_value < self.inner_lower {
            return (self.inner_lower, self.inner_lower - outer_value);
        }
        if outer_value > self.inner_max {
            return (self.inner_max, self.inner_max - outer_value);
        }
        (outer_value, 0.0)
    }
}

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    pub struct ScrollInset {
        pub child: glib::WeakRef<gtk4::Widget>,
        pub inner_vertical: gtk4::Adjustment,
        pub horizontal: RefCell<Option<gtk4::Adjustment>>,
        pub vertical: RefCell<Option<gtk4::Adjustment>>,
        pub horizontal_policy: Cell<gtk4::ScrollablePolicy>,
        pub vertical_policy: Cell<gtk4::ScrollablePolicy>,
        pub top: Cell<i32>,
        pub bottom: Cell<i32>,
        pub offset: Cell<f64>,
        pub syncing: Cell<bool>,
    }

    impl Default for ScrollInset {
        fn default() -> Self {
            Self {
                child: glib::WeakRef::new(),
                inner_vertical: gtk4::Adjustment::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
                horizontal: RefCell::new(None),
                vertical: RefCell::new(None),
                horizontal_policy: Cell::new(gtk4::ScrollablePolicy::Minimum),
                vertical_policy: Cell::new(gtk4::ScrollablePolicy::Minimum),
                top: Cell::new(0),
                bottom: Cell::new(0),
                offset: Cell::new(0.0),
                syncing: Cell::new(false),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ScrollInset {
        const NAME: &'static str = "RepriseScrollInset";
        type Type = super::ScrollInset;
        type ParentType = gtk4::Widget;
        type Interfaces = (gtk4::Scrollable,);

        fn class_init(class: &mut Self::Class) {
            class.set_css_name("reprise-scroll-inset");
        }
    }

    impl ObjectImpl for ScrollInset {
        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
            PROPERTIES.get_or_init(|| {
                [
                    "hadjustment",
                    "vadjustment",
                    "hscroll-policy",
                    "vscroll-policy",
                ]
                .into_iter()
                .map(glib::ParamSpecOverride::for_interface::<gtk4::Scrollable>)
                .collect()
            })
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "hadjustment" => {
                    let adjustment = value
                        .get::<Option<gtk4::Adjustment>>()
                        .expect("GtkScrollable validates horizontal adjustments");
                    self.set_horizontal(adjustment);
                }
                "vadjustment" => {
                    let adjustment = value
                        .get::<Option<gtk4::Adjustment>>()
                        .expect("GtkScrollable validates vertical adjustments");
                    self.set_vertical(adjustment);
                }
                "hscroll-policy" => {
                    let policy = value
                        .get::<gtk4::ScrollablePolicy>()
                        .expect("GtkScrollable validates horizontal policy");
                    self.horizontal_policy.set(policy);
                }
                "vscroll-policy" => {
                    let policy = value
                        .get::<gtk4::ScrollablePolicy>()
                        .expect("GtkScrollable validates vertical policy");
                    self.vertical_policy.set(policy);
                }
                name => unreachable!("unknown GtkScrollable property {name}"),
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "hadjustment" => self.horizontal.borrow().to_value(),
                "vadjustment" => self.vertical.borrow().to_value(),
                "hscroll-policy" => self.horizontal_policy.get().to_value(),
                "vscroll-policy" => self.vertical_policy.get().to_value(),
                name => unreachable!("unknown GtkScrollable property {name}"),
            }
        }

        fn dispose(&self) {
            if let Some(child) = self.child.upgrade() {
                self.child.set(None);
                child.unparent();
            }
        }
    }

    impl WidgetImpl for ScrollInset {
        fn compute_expand(&self, hexpand: &mut bool, vexpand: &mut bool) {
            self.parent_compute_expand(hexpand, vexpand);
            if let Some(child) = self.child.upgrade() {
                *hexpand |= child.compute_expand(gtk4::Orientation::Horizontal);
                *vexpand |= child.compute_expand(gtk4::Orientation::Vertical);
            }
        }

        fn request_mode(&self) -> gtk4::SizeRequestMode {
            self.child
                .upgrade()
                .map_or_else(|| self.parent_request_mode(), |child| child.request_mode())
        }

        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            self.child
                .upgrade()
                .map_or((0, 0, -1, -1), |child| child.measure(orientation, for_size))
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            let Some(child) = self.child.upgrade() else {
                return;
            };
            let transform = gtk4::gsk::Transform::new()
                .translate(&gtk4::graphene::Point::new(0.0, self.offset.get() as f32));
            child.allocate(width, height, baseline, Some(transform));
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            if let Some(child) = self.child.upgrade() {
                self.obj().snapshot_child(&child, snapshot);
            }
        }
    }

    impl ScrollableImpl for ScrollInset {}

    impl ScrollInset {
        fn set_horizontal(&self, adjustment: Option<gtk4::Adjustment>) {
            let adjustment =
                adjustment.unwrap_or_else(|| gtk4::Adjustment::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
            self.horizontal.replace(Some(adjustment.clone()));
            if let Some(child) = self
                .child
                .upgrade()
                .and_then(|child| child.dynamic_cast::<gtk4::Scrollable>().ok())
            {
                child.set_hadjustment(Some(&adjustment));
            }
        }

        fn set_vertical(&self, adjustment: Option<gtk4::Adjustment>) {
            let adjustment =
                adjustment.unwrap_or_else(|| gtk4::Adjustment::new(0.0, 0.0, 0.0, 0.0, 0.0, 0.0));
            self.vertical.replace(Some(adjustment.clone()));
            let weak = self.obj().downgrade();
            adjustment.connect_value_changed(move |_| {
                if let Some(widget) = weak.upgrade() {
                    widget.sync_from_outer();
                }
            });
            self.obj().refresh_range();
        }
    }
}

glib::wrapper! {
    pub struct ScrollInset(ObjectSubclass<imp::ScrollInset>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget,
                    gtk4::Scrollable;
}

impl ScrollInset {
    pub(crate) fn new(child: &gtk4::Widget) -> Self {
        let widget: Self = glib::Object::new();
        let child_widget = child.clone();
        let child = child_widget
            .clone()
            .dynamic_cast::<gtk4::Scrollable>()
            .expect("ScrollInset children must implement GtkScrollable");
        widget.imp().child.set(Some(&child_widget));
        child_widget.set_parent(&widget);
        child.set_vadjustment(Some(&widget.imp().inner_vertical));
        if let Some(horizontal) = widget.imp().horizontal.borrow().clone() {
            child.set_hadjustment(Some(&horizontal));
        }
        widget.set_overflow(gtk4::Overflow::Hidden);

        let weak = widget.downgrade();
        widget.imp().inner_vertical.connect_changed(move |_| {
            if let Some(widget) = weak.upgrade() {
                widget.refresh_range();
            }
        });
        let weak = widget.downgrade();
        widget.imp().inner_vertical.connect_value_changed(move |_| {
            if let Some(widget) = weak.upgrade() {
                widget.sync_from_inner();
            }
        });
        widget
    }

    pub(crate) fn set_insets(&self, top: i32, bottom: i32) {
        let top = top.max(0);
        let bottom = bottom.max(0);
        let old_top = self.imp().top.replace(top);
        let old_bottom = self.imp().bottom.replace(bottom);
        if old_top == top && old_bottom == bottom {
            return;
        }
        self.refresh_range();
    }

    #[cfg(test)]
    pub(crate) fn child(&self) -> Option<gtk4::Widget> {
        self.imp().child.upgrade()
    }

    fn projection(&self) -> ScrollProjection {
        let inner = &self.imp().inner_vertical;
        ScrollProjection::new(
            inner.lower(),
            inner.upper(),
            inner.page_size(),
            f64::from(self.imp().top.get()),
            f64::from(self.imp().bottom.get()),
        )
    }

    fn refresh_range(&self) {
        let Some(outer) = self.imp().vertical.borrow().clone() else {
            return;
        };
        if self.imp().syncing.replace(true) {
            return;
        }
        let old_at_start = (outer.value() - outer.lower()).abs() <= EDGE_EPSILON;
        let old_at_end =
            (outer.value() - (outer.upper() - outer.page_size())).abs() <= EDGE_EPSILON;
        let projection = self.projection();
        let value = if old_at_start {
            projection.outer_lower()
        } else if old_at_end {
            projection.outer_max()
        } else {
            self.imp().inner_vertical.value()
        };
        outer.configure(
            value,
            projection.outer_lower(),
            projection.outer_upper(),
            self.imp().inner_vertical.step_increment(),
            self.imp().inner_vertical.page_increment(),
            projection.page_size,
        );
        self.imp().syncing.set(false);
        self.sync_from_outer();
    }

    fn sync_from_outer(&self) {
        if self.imp().syncing.replace(true) {
            return;
        }
        let Some(outer) = self.imp().vertical.borrow().clone() else {
            self.imp().syncing.set(false);
            return;
        };
        let (inner_value, offset) = self.projection().project(outer.value());
        self.imp().inner_vertical.set_value(inner_value);
        let changed = (self.imp().offset.replace(offset) - offset).abs() > f64::EPSILON;
        self.imp().syncing.set(false);
        if changed {
            self.queue_allocate();
        }
    }

    fn sync_from_inner(&self) {
        if self.imp().syncing.replace(true) {
            return;
        }
        let Some(outer) = self.imp().vertical.borrow().clone() else {
            self.imp().syncing.set(false);
            return;
        };
        let projection = self.projection();
        let inner_value = self.imp().inner_vertical.value();
        let outer_value = if (inner_value - projection.inner_lower).abs() <= EDGE_EPSILON {
            projection.outer_lower()
        } else if (inner_value - projection.inner_max).abs() <= EDGE_EPSILON {
            projection.outer_max()
        } else {
            inner_value
        };
        outer.set_value(outer_value);
        self.imp().offset.set(projection.project(outer_value).1);
        self.imp().syncing.set(false);
        self.queue_allocate();
    }
}

#[cfg(test)]
mod tests {
    use super::ScrollProjection;

    #[test]
    fn edge_scroll_range_moves_real_content_through_both_glass_zones() {
        let projection = ScrollProjection::new(0.0, 1_000.0, 600.0, 90.0, 96.0);

        assert_eq!(projection.outer_lower(), -90.0);
        assert_eq!(projection.outer_max(), 496.0);
        assert_eq!(projection.project(-90.0), (0.0, 90.0));
        assert_eq!(projection.project(0.0), (0.0, 0.0));
        assert_eq!(projection.project(400.0), (400.0, 0.0));
        assert_eq!(projection.project(496.0), (400.0, -96.0));
    }
}
