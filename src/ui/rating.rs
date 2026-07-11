//! `RatingWidget`: a row of 5 clickable stars (`gtk::Image`s toggled between
//! `starred-symbolic`/`non-starred-symbolic`) used as the interactive
//! `Rating` column cell in `track_list.rs`.
//!
//! ## Why a `gtk::Box` subclass, not a plain Rust struct
//!
//! `GtkColumnView`'s `SignalListItemFactory` builds each cell widget once
//! (`connect_setup`) and rebinds it to a new row many times as the list
//! recycles/scrolls (`connect_bind`). Rebinding needs to call back into
//! *this exact* widget instance — set its displayed rating, replace its
//! click callback with one that closes over the newly-bound row — which
//! means `connect_bind` must be able to recover it from
//! `ListItem::child()`. That method only returns a `gtk::Widget`; getting a
//! usable Rust type back out of it requires either an unsafe `set_data`/
//! `data` pair on the GObject, or making the widget itself the GObject, so
//! a plain safe `downcast::<RatingWidget>()` works — exactly the tradeoff
//! `TrackListModel` (`track_list_model.rs`) already made for the same
//! reason (a `GListModel` subclass instead of a bespoke Rust struct). This
//! module follows that precedent: `RatingWidget` extends `gtk::Box`.
//!
//! ## Click → star index
//!
//! One `GestureClick` on the whole box (not one gesture per star) maps the
//! press x-coordinate to a 1-based star index by integer division against
//! the fixed per-star pixel width (`STAR_SIZE`). Rhythmbox behavior:
//! clicking the star that already equals the current rating clears it to 0
//! (a misclick can be undone with one more click on the same spot, instead
//! of always increasing).
//!
//! ## DB-free by design
//!
//! This widget only tracks and displays an `i32` rating and reports clicks
//! through `set_on_changed`'s callback — it has no knowledge of
//! `library::stats` or any `rusqlite::Connection`. `track_list.rs` is the
//! only place that turns a click into a persisted write.

use std::cell::{Cell, RefCell};

use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::ui::strings;

const STAR_COUNT: i32 = 5;
const RATING_MIN: i32 = 0;
const RATING_MAX: i32 = STAR_COUNT;

const ICON_STARRED: &str = "starred-symbolic";
const ICON_NON_STARRED: &str = "non-starred-symbolic";

/// Fixed width (and height) of each star icon, in pixels. Used both to size
/// the `gtk::Image`s and, in `star_index_for_x`, to map a click's
/// x-coordinate back to a star index — the two must stay in sync, which is
/// why both live off this one constant.
const STAR_SIZE: i32 = 16;

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    pub struct RatingWidget {
        pub stars: RefCell<Vec<gtk4::Image>>,
        pub rating: Cell<i32>,
        /// Replaced wholesale by `set_on_changed` on every list-item
        /// rebind; starts as a no-op so a stray click before the first
        /// `set_on_changed` call can never panic on an empty `Option`.
        pub on_changed: RefCell<Box<dyn Fn(i32)>>,
    }

    impl Default for RatingWidget {
        fn default() -> Self {
            Self {
                stars: RefCell::new(Vec::new()),
                rating: Cell::new(0),
                on_changed: RefCell::new(Box::new(|_| {})),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for RatingWidget {
        const NAME: &'static str = "RepriseRatingWidget";
        type Type = super::RatingWidget;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for RatingWidget {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().build_ui();
        }
    }

    impl WidgetImpl for RatingWidget {}
    impl BoxImpl for RatingWidget {}
}

glib::wrapper! {
    pub struct RatingWidget(ObjectSubclass<imp::RatingWidget>)
        @extends gtk4::Box, gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget, gtk4::Orientable;
}

impl RatingWidget {
    pub fn new() -> Self {
        glib::Object::new()
    }

    /// Builds the five star images and wires the click gesture. Called once
    /// from `constructed()`, i.e. exactly once per widget instance — the
    /// same instance is then reused across every list-item rebind.
    fn build_ui(&self) {
        self.set_orientation(gtk4::Orientation::Horizontal);
        self.set_tooltip_text(Some(strings::RATING));

        let stars: Vec<gtk4::Image> = (0..STAR_COUNT)
            .map(|_| {
                let image = gtk4::Image::from_icon_name(ICON_NON_STARRED);
                image.set_pixel_size(STAR_SIZE);
                self.append(&image);
                image
            })
            .collect();
        self.imp().stars.replace(stars);

        let click = gtk4::GestureClick::new();
        let widget = self.downgrade();
        click.connect_pressed(move |_, _, x, _| {
            let Some(widget) = widget.upgrade() else {
                return;
            };
            widget.handle_click(x);
        });
        self.add_controller(click);
    }

    /// Maps a click at `x` to a target star index, applies the Rhythmbox
    /// clear-on-reclick rule, updates the display, and reports the new
    /// value through the current `on_changed` callback.
    fn handle_click(&self, x: f64) {
        let clicked_star = star_index_for_x(x);
        let current = self.imp().rating.get();
        let new_rating = if clicked_star == current {
            RATING_MIN
        } else {
            clicked_star
        };
        self.set_rating(new_rating);
        (self.imp().on_changed.borrow())(new_rating);
    }

    /// Sets the displayed rating without invoking the `on_changed`
    /// callback — used by `track_list.rs` to show a freshly-bound row's
    /// stored rating, a programmatic update rather than a user click.
    /// Clamps out-of-range input (e.g. stale/corrupt DB data) rather than
    /// trusting it.
    pub fn set_rating(&self, rating: i32) {
        let clamped = rating.clamp(RATING_MIN, RATING_MAX);
        self.imp().rating.set(clamped);
        let Some(stars) = self.imp().stars.try_borrow().ok() else {
            tracing::warn!("rating widget: stars borrow unavailable; skipping redraw");
            return;
        };
        for (i, star) in stars.iter().enumerate() {
            let filled = (i as i32) < clamped;
            star.set_icon_name(Some(if filled {
                ICON_STARRED
            } else {
                ICON_NON_STARRED
            }));
        }
    }

    /// Replaces the click callback. `track_list.rs` calls this on every
    /// list-item bind so the callback closes over whichever row is
    /// currently shown — the widget instance itself is recycled across
    /// many rows as the list scrolls (see the module doc comment).
    pub fn set_on_changed(&self, f: impl Fn(i32) + 'static) {
        *self.imp().on_changed.borrow_mut() = Box::new(f);
    }
}

impl Default for RatingWidget {
    fn default() -> Self {
        Self::new()
    }
}

/// Maps a click's x-coordinate (relative to the widget) to a 1-based star
/// index, clamped into `1..=STAR_COUNT`. Pure and panic-free for any finite
/// input, including negative coordinates or ones past the last star (extra
/// padding, a resize mid-click) — both clamp to an end rather than being
/// treated as an error, since a click the box's own gesture received is by
/// definition "on the widget" as far as the user is concerned.
fn star_index_for_x(x: f64) -> i32 {
    let raw = (x / f64::from(STAR_SIZE)).floor() as i32 + 1;
    raw.clamp(RATING_MIN + 1, RATING_MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_index_first_star_at_origin() {
        assert_eq!(star_index_for_x(0.0), 1);
    }

    #[test]
    fn star_index_last_star_near_right_edge() {
        assert_eq!(star_index_for_x(4.5 * f64::from(STAR_SIZE)), 5);
    }

    #[test]
    fn star_index_clamps_negative_x_to_first_star() {
        assert_eq!(star_index_for_x(-10.0), 1);
    }

    #[test]
    fn star_index_clamps_past_last_star() {
        assert_eq!(star_index_for_x(1000.0), 5);
    }
}
