//! Viewport-stable reactive wash for realised now-playing title cells.

use std::rc::Rc;

use gtk4::prelude::*;

use super::{Shared, TrackList};

const ROW_WASH_REST: f64 = 0.08;
const ROW_WASH_PER_PRESSURE: f64 = 0.08;
const ROW_WASH_PER_KICK: f64 = 0.16;
const READING_EPSILON: f64 = 0.01;
const ROW_WASH_CLASS: &str = "now-playing-wash";

pub(in crate::ui) fn row_wash_alpha(kick: f64, pressure: f64) -> f64 {
    ROW_WASH_REST
        + ROW_WASH_PER_PRESSURE * pressure.clamp(0.0, 1.0)
        + ROW_WASH_PER_KICK * kick.clamp(0.0, 1.0)
}

pub(in crate::ui) fn css() -> String {
    format!(
        ".{ROW_WASH_CLASS} {{ background-image: linear-gradient(90deg, \
         @accent_color 0%, alpha(@accent_color, 0) 58%); }}"
    )
}

pub(in crate::ui) fn build() -> gtk4::Box {
    let wash = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    wash.add_css_class(ROW_WASH_CLASS);
    wash.set_can_target(false);
    wash.set_can_focus(false);
    wash.set_opacity(ROW_WASH_REST);
    wash.set_visible(false);
    wash
}

pub(in crate::ui) fn apply(
    wash: &impl IsA<gtk4::Widget>,
    playing: bool,
    selected: bool,
    shared: &Shared,
) {
    wash.set_visible(playing);
    if !playing {
        return;
    }
    let alpha = if selected {
        ROW_WASH_REST
    } else {
        row_wash_alpha(shared.row_wash_kick.get(), shared.row_wash_pressure.get())
    };
    wash.set_opacity(alpha);
}

pub(in crate::ui) struct RowWashMarker {
    item: gtk4::glib::WeakRef<gtk4::ListItem>,
    apply: Rc<dyn Fn()>,
}

pub(in crate::ui) fn register_cell(
    shared: &Rc<Shared>,
    item: &gtk4::ListItem,
    apply: impl Fn(&Shared) + 'static,
) {
    let weak = Rc::downgrade(shared);
    shared.register_row_wash(
        item,
        Rc::new(move || {
            if let Some(shared) = weak.upgrade() {
                apply(&shared);
            }
        }),
    );
}

pub(in crate::ui) fn unregister_cell(shared: &Shared, item: &gtk4::ListItem) {
    shared.unregister_row_wash(item);
}

impl Shared {
    fn register_row_wash(&self, item: &gtk4::ListItem, apply: Rc<dyn Fn()>) {
        let target = item.as_ptr();
        let mut markers = self.row_washes.borrow_mut();
        markers.retain(|marker| {
            marker
                .item
                .upgrade()
                .is_some_and(|live| live.as_ptr() != target)
        });
        let weak = gtk4::glib::WeakRef::new();
        weak.set(Some(item));
        markers.push(RowWashMarker { item: weak, apply });
    }

    fn unregister_row_wash(&self, item: &gtk4::ListItem) {
        let target = item.as_ptr();
        self.row_washes.borrow_mut().retain(|marker| {
            marker
                .item
                .upgrade()
                .is_some_and(|live| live.as_ptr() != target)
        });
    }

    pub(in crate::ui) fn reapply_row_washes(&self) {
        let appliers: Vec<Rc<dyn Fn()>> = {
            let mut markers = self.row_washes.borrow_mut();
            markers.retain(|marker| marker.item.upgrade().is_some());
            markers.iter().map(|marker| marker.apply.clone()).collect()
        };
        for apply in appliers {
            apply();
        }
    }
}

impl TrackList {
    pub(in crate::ui) fn set_bass(&self, kick: f64, pressure: f64) {
        let kick = crate::ui::motion::reactive_amplitude(kick);
        let pressure = crate::ui::motion::reactive_amplitude(pressure);
        if (self.shared.row_wash_kick.get() - kick).abs() < READING_EPSILON
            && (self.shared.row_wash_pressure.get() - pressure).abs() < READING_EPSILON
        {
            return;
        }
        self.shared.row_wash_kick.set(kick);
        self.shared.row_wash_pressure.set(pressure);
        self.shared.reapply_row_washes();
    }
}
