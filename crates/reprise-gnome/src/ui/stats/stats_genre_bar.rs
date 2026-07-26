//! Proportional, non-reflowing layout for the interactive genre segments.

use std::cell::RefCell;

use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::ui::motion_reveal::HorizontalReveal;

const SEGMENT_GAP: i32 = 2;
const BAR_HEIGHT: i32 = 22;

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct StatsGenreBar {
        pub children: RefCell<Vec<HorizontalReveal>>,
        pub shares: RefCell<Vec<f64>>,
        pub target_shares: RefCell<Vec<f64>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StatsGenreBar {
        const NAME: &'static str = "RepriseStatsGenreBar";
        type Type = super::StatsGenreBar;
        type ParentType = gtk4::Widget;

        fn class_init(class: &mut Self::Class) {
            class.set_css_name("reprise-stats-genre-bar");
            class.set_accessible_role(gtk4::AccessibleRole::Group);
        }
    }

    impl ObjectImpl for StatsGenreBar {
        fn dispose(&self) {
            for child in self.children.borrow_mut().drain(..) {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for StatsGenreBar {
        fn measure(&self, orientation: gtk4::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk4::Orientation::Horizontal {
                return (0, 0, -1, -1);
            }
            let natural = self
                .children
                .borrow()
                .iter()
                .map(|child| child.measure(orientation, for_size).1)
                .max()
                .unwrap_or(BAR_HEIGHT)
                .max(BAR_HEIGHT);
            (BAR_HEIGHT.min(natural), natural, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            let children = self.children.borrow().clone();
            let shares = self.shares.borrow().clone();
            let widths = segment_widths(width, &shares, children.len());
            let mut x = 0;
            for (index, (child, child_width)) in children.iter().zip(widths).enumerate() {
                let transform = gtk4::gsk::Transform::new()
                    .translate(&gtk4::graphene::Point::new(x as f32, 0.0));
                child.allocate(child_width, height, baseline, Some(transform));
                x += child_width
                    + if index + 1 < children.len() {
                        SEGMENT_GAP
                    } else {
                        0
                    };
            }
        }

        fn snapshot(&self, snapshot: &gtk4::Snapshot) {
            let widget = self.obj();
            for child in self.children.borrow().iter() {
                widget.snapshot_child(child, snapshot);
            }
        }
    }
}

glib::wrapper! {
    pub struct StatsGenreBar(ObjectSubclass<imp::StatsGenreBar>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl StatsGenreBar {
    pub(super) fn new() -> Self {
        let bar: Self = glib::Object::new();
        bar.add_css_class("stats-genre-bar");
        bar.set_height_request(BAR_HEIGHT);
        bar.set_hexpand(true);
        bar.set_overflow(gtk4::Overflow::Hidden);
        bar
    }

    pub(super) fn set_segments(&self, children: &[HorizontalReveal], shares: &[f64]) {
        for child in self.imp().children.borrow_mut().drain(..) {
            child.unparent();
        }
        for child in children {
            child.set_parent(self);
        }
        self.imp().children.replace(children.to_vec());
        self.imp().shares.replace(shares.to_vec());
        self.imp().target_shares.replace(shares.to_vec());
        self.queue_resize();
    }

    pub(super) fn target_shares(&self) -> Vec<f64> {
        self.imp().target_shares.borrow().clone()
    }

    pub(super) fn set_shares(&self, shares: &[f64]) {
        let targets = self.target_shares();
        self.imp().shares.replace(
            targets
                .iter()
                .enumerate()
                .map(|(index, target)| shares.get(index).copied().unwrap_or(*target).max(0.0))
                .collect(),
        );
        self.queue_resize();
    }
}

fn segment_widths(total_width: i32, shares: &[f64], count: usize) -> Vec<i32> {
    if count == 0 {
        return Vec::new();
    }
    let gap_total = SEGMENT_GAP * count.saturating_sub(1) as i32;
    let usable_width = total_width.saturating_sub(gap_total);
    let total = shares.iter().take(count).sum::<f64>();
    if usable_width <= 0 || total <= 0.0 {
        return vec![0; count];
    }
    let mut used = 0;
    (0..count)
        .map(|index| {
            let width = if index + 1 == count {
                usable_width.saturating_sub(used)
            } else {
                ((f64::from(usable_width) * shares.get(index).copied().unwrap_or(0.0) / total)
                    .round() as i32)
                    .clamp(0, usable_width.saturating_sub(used))
            };
            used += width;
            width
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::segment_widths;

    #[test]
    fn proportional_widths_fill_the_track_with_fixed_gaps() {
        let widths = segment_widths(500, &[70.0, 30.0], 2);

        assert_eq!(widths.iter().sum::<i32>() + 2, 500);
        assert!((widths[0] as f64 / 498.0 - 0.70).abs() < 0.01);
    }
}
