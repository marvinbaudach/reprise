//! `RatingWidget`: an interactive, hover-reveal rating cell.
//!
//! - **Unrated, at rest:** a single dim dash `—`, nothing else.
//! - **Rated, at rest:** the five stars, filled (accent) up to the rating and
//!   empty (dim) after it — a compact read-only summary, and still clickable
//!   so a rated row stays keyboard-editable.
//! - **Pointer over the cell:** five clickable star buttons; the star under
//!   the pointer and every star before it light up in the accent as a live
//!   preview. Clicking commits that rating (the Rhythmbox re-click-to-clear
//!   rule still applies). Moving the pointer out without clicking reverts to
//!   the at-rest summary.
//!
//! This replaced an earlier column-*width*-responsive design (a compact
//! `★ N` menu button that promoted to five inline stars via `AdwBreakpointBin`).
//! Hover reveal made that machinery redundant: the cell is always the compact
//! width and reveals its stars on demand.
//!
//! ## Why real `gtk::Button`s with text glyphs (unchanged rationale)
//!
//! The first implementation was `gtk::Image`s (`starred-symbolic` /
//! `non-starred-symbolic`) with one `GestureClick` mapping the press
//! x-coordinate to a star. Field testing on a real desktop killed both halves:
//!
//! - **Clicks never arrived.** Inside a `GtkColumnView` cell, the list row's
//!   own click/selection machinery won the event over a plain `GestureClick`
//!   on a non-interactive child. Real `gtk::Button`s don't have this problem —
//!   GTK treats them as genuinely interactive children and delivers their
//!   clicks reliably, plus keyboard activation for free.
//! - **The icons were theme-dependent.** On Papirus-Dark `non-starred-symbolic`
//!   renders nearly identical to `starred-symbolic`. Text glyphs `★`/`☆` come
//!   from the font, not the icon theme, so they read correctly everywhere.
//!
//! ## Why a `gtk::Box` subclass (unchanged)
//!
//! `GtkColumnView`'s `SignalListItemFactory` builds each cell once and rebinds
//! it to a new row many times as the list recycles. `connect_bind` must
//! recover *this exact* instance from `ListItem::child()` — which only returns
//! a `gtk::Widget` — so the widget must itself be the GObject for a safe
//! `downcast::<RatingWidget>()`. Hence `RatingWidget` extends `gtk::Box`.
//!
//! ## No `RefCell` borrow ever spans an external/GTK call (unchanged)
//!
//! `track_list.rs`'s `on_changed` callback runs sqlite → model
//! `items_changed`, which `GtkColumnView` may react to synchronously by
//! rebinding this very row — calling back into `set_on_changed` on this widget
//! while the click handler is still on the stack. `handle_star_activated`
//! therefore clones the `Rc<dyn Fn(i32)>` out of the `RefCell` in its own
//! statement, letting the borrow drop before the callback (and any reentrancy
//! it triggers) runs.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::ui::strings;

const STAR_COUNT: i32 = 5;
const RATING_MIN: i32 = 0;
const RATING_MAX: i32 = STAR_COUNT;

/// Fixed width of the Rating column (`column_layout.rs`). Five ~16 px star
/// buttons fit comfortably; the old compact/wide breakpoint is gone — hover
/// reveal replaces it, so the column is always this width.
pub(super) const COMPACT_RATING_COLUMN_WIDTH: i32 = 88;

/// Filled star glyph (U+2605), shown for star positions `<= threshold`.
const STAR_FILLED_GLYPH: &str = "\u{2605}";
/// Outline star glyph (U+2606), shown for positions `> threshold`.
const STAR_OUTLINE_GLYPH: &str = "\u{2606}";
/// Em dash (U+2014) — the unrated, at-rest summary.
const DASH_GLYPH: &str = "\u{2014}";

const STAR_CSS_CLASS: &str = "reprise-rating-star";
const FILLED_CSS_CLASS: &str = "reprise-rating-filled";
const EMPTY_CSS_CLASS: &str = "reprise-rating-empty";
const DASH_CSS_CLASS: &str = "reprise-rating-dash";

/// Shared alias for the click-reporting callback's storage type.
type OnChangedCallback = Option<Rc<dyn Fn(i32)>>;

/// Pure decision of what the cell shows, given the stored `rating`, whether
/// the pointer is over the cell (`hovered`), and which star the pointer is
/// over (`preview`, 1-based; `0` = none, i.e. just entered). `threshold` is
/// how many stars are filled. Side-effect free so it is unit-testable without
/// a running GTK application.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RatingDisplay {
    show_stars: bool,
    threshold: i32,
}

fn rating_display(rating: i32, hovered: bool, preview: i32) -> RatingDisplay {
    if hovered {
        // Reveal the interactive stars; the preview drives the fill (0 = all
        // empty until the pointer reaches a star, per the spec baseline).
        RatingDisplay {
            show_stars: true,
            threshold: preview,
        }
    } else if rating > 0 {
        RatingDisplay {
            show_stars: true,
            threshold: rating,
        }
    } else {
        RatingDisplay {
            show_stars: false,
            threshold: 0,
        }
    }
}

/// Which star (1-based) a pointer at `x` in a `width`-wide row of `STAR_COUNT`
/// equal-width stars is over. Clamped to `1..=STAR_COUNT`; `0` only when
/// `width <= 0` (unallocated).
fn star_at_x(x: f64, width: f64) -> i32 {
    if width <= 0.0 {
        return 0;
    }
    let slot = width / STAR_COUNT as f64;
    (((x / slot).floor() as i32) + 1).clamp(1, STAR_COUNT)
}

/// Star and dash hit-area + colour rules; installed app-wide by
/// [`super::style`].
pub(super) fn css() -> String {
    format!(
        ".{STAR_CSS_CLASS} {{ min-width: 16px; min-height: 24px; padding: 0; }}\n\
         .{FILLED_CSS_CLASS} {{ color: @accent_color; }}\n\
         .{EMPTY_CSS_CLASS} {{ color: alpha(@window_fg_color, 0.25); }}\n\
         .{DASH_CSS_CLASS} {{ color: alpha(@window_fg_color, 0.30); }}"
    )
}

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct RatingWidget {
        /// One `(button, its label child)` pair per star, in order.
        pub stars: RefCell<Vec<(gtk4::Button, gtk4::Label)>>,
        pub stars_box: RefCell<Option<gtk4::Box>>,
        pub dash: RefCell<Option<gtk4::Label>>,
        pub rating: Cell<i32>,
        /// Whether the pointer is currently over the cell (drives reveal).
        pub hovered: Cell<bool>,
        /// Star (1-based) the pointer is over during hover, `0` = none.
        pub preview: Cell<i32>,
        /// Replaced wholesale by `set_on_changed` on every rebind; `Rc` so
        /// `handle_star_activated` can clone it out and drop the borrow before
        /// invoking (see the module doc comment).
        pub on_changed: RefCell<OnChangedCallback>,
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

    fn build_ui(&self) {
        self.set_orientation(gtk4::Orientation::Horizontal);
        self.set_tooltip_text(Some(&strings::text(strings::RATING)));

        // Dash summary for the unrated, at-rest state.
        let dash = gtk4::Label::new(Some(DASH_GLYPH));
        dash.add_css_class(DASH_CSS_CLASS);
        dash.set_halign(gtk4::Align::Center);
        dash.set_hexpand(true);
        self.append(&dash);
        self.imp().dash.replace(Some(dash));

        // Five star buttons for the rated / hover-reveal state.
        let stars_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        stars_box.set_homogeneous(true);
        stars_box.set_hexpand(true);
        let stars: Vec<(gtk4::Button, gtk4::Label)> = (1..=STAR_COUNT)
            .map(|star| {
                let (button, label) = self.build_star(star);
                stars_box.append(&button);
                (button, label)
            })
            .collect();
        self.imp().stars.replace(stars);
        stars_box.set_visible(false);
        self.append(&stars_box);
        self.imp().stars_box.replace(Some(stars_box));

        // Pointer hover drives reveal + preview. The controller lives on the
        // widget itself (always present), not on the stars box (hidden while
        // unrated), so hovering an unrated cell can still reveal the stars.
        let motion = gtk4::EventControllerMotion::new();
        let widget = self.downgrade();
        motion.connect_enter({
            let widget = widget.clone();
            move |_, _, _| {
                if let Some(widget) = widget.upgrade() {
                    widget.imp().hovered.set(true);
                    widget.imp().preview.set(0);
                    widget.refresh();
                }
            }
        });
        motion.connect_motion({
            let widget = widget.clone();
            move |_, x, _| {
                if let Some(widget) = widget.upgrade() {
                    let preview = star_at_x(x, f64::from(widget.width()));
                    if widget.imp().preview.get() != preview {
                        widget.imp().preview.set(preview);
                        widget.refresh();
                    }
                }
            }
        });
        motion.connect_leave(move |_| {
            if let Some(widget) = widget.upgrade() {
                widget.imp().hovered.set(false);
                widget.imp().preview.set(0);
                widget.refresh();
            }
        });
        self.add_controller(motion);

        self.refresh();
    }

    fn build_star(&self, star: i32) -> (gtk4::Button, gtk4::Label) {
        let label = gtk4::Label::new(Some(STAR_OUTLINE_GLYPH));
        label.add_css_class(EMPTY_CSS_CLASS);

        let button = gtk4::Button::new();
        button.add_css_class(STAR_CSS_CLASS);
        button.set_child(Some(&label));
        button.set_has_frame(false);
        button.set_valign(gtk4::Align::Center);
        button.set_tooltip_text(Some(&strings::rate_n_stars(star)));

        let widget = self.downgrade();
        button.connect_clicked(move |_| {
            if let Some(widget) = widget.upgrade() {
                widget.handle_star_activated(star);
            }
        });
        (button, label)
    }

    /// Applies the Rhythmbox clear-on-reclick rule for a click on `star`,
    /// updates the display, and reports the new value through `on_changed`.
    fn handle_star_activated(&self, star: i32) {
        let new_rating = next_rating(star, self.imp().rating.get());
        self.imp().rating.set(new_rating);
        self.refresh();
        // Clone the callback out first — this borrow ends with the `let`, so
        // no borrow is held while the callback (and whatever reentrancy it
        // triggers, up to a synchronous `set_on_changed` on this widget) runs.
        let callback = self.imp().on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(new_rating);
        }
    }

    /// Sets the displayed rating without invoking `on_changed` — used by
    /// `track_list.rs` to show a freshly-bound row's stored rating. Clamps
    /// out-of-range input rather than trusting stale/corrupt DB data.
    pub fn set_rating(&self, rating: i32) {
        self.imp().rating.set(rating.clamp(RATING_MIN, RATING_MAX));
        self.refresh();
    }

    /// Replaces the click callback. `track_list.rs` calls this on every rebind
    /// so it closes over whichever row is currently shown.
    pub fn set_on_changed(&self, f: impl Fn(i32) + 'static) {
        *self.imp().on_changed.borrow_mut() = Some(Rc::new(f));
    }

    /// Recomputes visibility and every star's glyph + colour from the current
    /// `(rating, hovered, preview)` state. Cheap; called on each state change.
    fn refresh(&self) {
        let display = rating_display(
            self.imp().rating.get(),
            self.imp().hovered.get(),
            self.imp().preview.get(),
        );
        if let Some(dash) = self.imp().dash.borrow().as_ref() {
            dash.set_visible(!display.show_stars);
        }
        let Some(stars_box) = self.imp().stars_box.borrow().clone() else {
            return;
        };
        stars_box.set_visible(display.show_stars);
        if !display.show_stars {
            return;
        }
        let Ok(stars) = self.imp().stars.try_borrow() else {
            tracing::warn!("rating widget: stars borrow unavailable; skipping redraw");
            return;
        };
        for (index, (_, label)) in stars.iter().enumerate() {
            let filled = (index as i32 + 1) <= display.threshold;
            if filled {
                label.set_text(STAR_FILLED_GLYPH);
                label.add_css_class(FILLED_CSS_CLASS);
                label.remove_css_class(EMPTY_CSS_CLASS);
            } else {
                label.set_text(STAR_OUTLINE_GLYPH);
                label.add_css_class(EMPTY_CSS_CLASS);
                label.remove_css_class(FILLED_CSS_CLASS);
            }
        }
    }

    /// Test-only seam: presses star `index` (1-based) via `emit_clicked`, so
    /// the call goes through the exact `connect_clicked` → `handle_star_
    /// activated` path a real click would. The `stars` borrow is hoisted into
    /// its own statement and dropped before the click fires (which runs
    /// arbitrary callback code).
    #[cfg(test)]
    pub fn click_star_for_test(&self, index: i32) {
        let button = self
            .imp()
            .stars
            .borrow()
            .get(usize::try_from(index - 1).expect("star index must be >= 1"))
            .map(|(button, _)| button.clone());
        match button {
            Some(button) => button.emit_clicked(),
            None => panic!("no star button at index {index}"),
        }
    }

    #[cfg(test)]
    pub fn rating_for_test(&self) -> i32 {
        self.imp().rating.get()
    }
}

impl Default for RatingWidget {
    fn default() -> Self {
        Self::new()
    }
}

/// The rating a click on star `clicked_star` (1-based) should produce given
/// the `current` rating: normally the star's own value, but re-clicking the
/// star that already equals the current rating clears to 0 (the Rhythmbox
/// rule). Pure so the rule is unit-testable without any GTK widgets.
fn next_rating(clicked_star: i32, current: i32) -> i32 {
    if clicked_star == current {
        RATING_MIN
    } else {
        clicked_star
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unrated_at_rest_shows_only_the_dash() {
        let display = rating_display(0, false, 0);
        assert!(!display.show_stars);
    }

    #[test]
    fn rated_at_rest_shows_stars_filled_to_the_rating() {
        let display = rating_display(3, false, 0);
        assert!(display.show_stars);
        assert_eq!(display.threshold, 3);
    }

    #[test]
    fn hover_reveals_stars_and_preview_drives_the_fill() {
        // Just entered (preview 0): all stars empty.
        assert_eq!(
            rating_display(0, true, 0),
            RatingDisplay {
                show_stars: true,
                threshold: 0
            }
        );
        // Pointer over star 4: fill four, regardless of the stored rating.
        assert_eq!(
            rating_display(2, true, 4),
            RatingDisplay {
                show_stars: true,
                threshold: 4
            }
        );
    }

    #[test]
    fn star_at_x_maps_pointer_to_the_right_star() {
        // 100 px wide, 5 stars → 20 px slots.
        assert_eq!(star_at_x(0.0, 100.0), 1);
        assert_eq!(star_at_x(25.0, 100.0), 2);
        assert_eq!(star_at_x(99.0, 100.0), 5);
        // Past the end clamps to the last star; unallocated width is 0.
        assert_eq!(star_at_x(500.0, 100.0), 5);
        assert_eq!(star_at_x(10.0, 0.0), 0);
    }

    #[test]
    fn click_sets_rating_to_star_value() {
        assert_eq!(next_rating(3, 0), 3);
        assert_eq!(next_rating(5, 2), 5);
        assert_eq!(next_rating(1, 5), 1);
    }

    #[test]
    fn click_on_current_rating_clears_to_zero() {
        for star in 1..=STAR_COUNT {
            assert_eq!(next_rating(star, star), RATING_MIN);
        }
    }

    #[test]
    fn unrated_state_never_clears_on_first_click() {
        for star in 1..=STAR_COUNT {
            assert_ne!(next_rating(star, 0), RATING_MIN);
        }
    }

    #[test]
    fn css_defines_star_dash_and_fill_colours() {
        let css = css();
        assert!(css.contains(".reprise-rating-star"));
        assert!(css.contains(".reprise-rating-filled { color: @accent_color; }"));
        assert!(css.contains(".reprise-rating-empty"));
        assert!(css.contains(".reprise-rating-dash"));
    }

    /// Regression test for the `BorrowMutError` in the module doc comment: a
    /// click callback that reentrantly calls `set_on_changed` on the same
    /// widget (simulating GTK synchronously rebinding the just-clicked row)
    /// must not panic. Needs a real GTK/GDK display, so `#[ignore]`d — run
    /// with `xvfb-run -a cargo test -- --ignored reentrant`.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn reentrant_set_on_changed_does_not_panic() {
        if gtk4::init().is_err() {
            eprintln!("skipping: gtk4::init() failed (no display available)");
            return;
        }

        let widget = RatingWidget::new();
        let widget_weak = widget.downgrade();
        widget.set_on_changed(move |_| {
            let Some(widget) = widget_weak.upgrade() else {
                return;
            };
            widget.set_on_changed(|_| {});
        });

        widget.click_star_for_test(3);
        assert_eq!(widget.rating_for_test(), 3);
    }
}
