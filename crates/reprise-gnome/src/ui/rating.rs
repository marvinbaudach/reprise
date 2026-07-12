//! `RatingWidget`: a row of 5 flat `gtk::Button`s showing text star glyphs
//! (filled `★` U+2605 vs outline `☆` U+2606), used as the interactive
//! `Rating` column cell in `track_list.rs`.
//!
//! ## Why buttons with text glyphs, not images with a `GestureClick`
//!
//! The first implementation was five `gtk::Image`s (`starred-symbolic` /
//! `non-starred-symbolic`) with one `GestureClick` on the containing box
//! mapping the press x-coordinate to a star index. Field testing on a real
//! desktop killed both halves of that design at once:
//!
//! - **Clicks never arrived.** Inside a `GtkColumnView` cell, the list
//!   row's own click/selection machinery won the event over a plain
//!   `GestureClick` on a non-interactive `gtk::Box` child — a whole session
//!   of real pointer clicks produced zero rating-click log lines. Real
//!   `gtk::Button`s don't have this problem: GTK treats them as genuinely
//!   interactive children inside list cells and delivers their clicks
//!   reliably, and they add keyboard focus/activation for free.
//! - **The icons were theme-dependent.** On Papirus-Dark,
//!   `non-starred-symbolic` renders nearly identical to `starred-symbolic`,
//!   so an unrated track (rating 0, confirmed in the DB) looked fully
//!   rated. Text glyphs `★`/`☆` come from the font, not the icon theme —
//!   they read correctly on every theme and match the design mockup. The
//!   outline glyph additionally gets the `dim-label` CSS class so the
//!   unfilled state is de-emphasized on top of being a different shape.
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
//! ## Click → rating
//!
//! Button N sets the rating to N — except the Rhythmbox rule: clicking the
//! star that already equals the current rating clears it to 0 (a misclick
//! can be undone with one more click on the same spot, instead of always
//! increasing). The decision is the pure `next_rating` function.
//!
//! ## DB-free by design
//!
//! This widget only tracks and displays an `i32` rating and reports clicks
//! through `set_on_changed`'s callback — it has no knowledge of
//! `library::stats` or any `rusqlite::Connection`. `track_list.rs` is the
//! only place that turns a click into a persisted write.
//!
//! ## No `RefCell` borrow ever spans an external/GTK call
//!
//! `track_list.rs`'s `on_changed` callback runs a chain that is *not*
//! guaranteed to stay inside this module: sqlite write
//! (`stats::set_rating`) → `TrackListModel::invalidate_window_at` →
//! `GListModel::items_changed`. If `GtkColumnView` reacts to that signal
//! synchronously — rebinding the very row being clicked — it calls back
//! into `set_on_changed` on this exact widget while the click handler that
//! triggered it all is still on the stack. `handle_star_activated`
//! therefore never holds the `on_changed` `Ref`/`RefMut` while invoking the
//! callback: it clones the `Rc<dyn Fn(i32)>` out of the `RefCell` in a
//! single expression, letting the borrow drop before the callback (and
//! everything it might reentrantly trigger) runs. The same discipline
//! applies to any future code here that touches GTK or calls out of the
//! widget — no `RefCell` borrow may still be alive at that point.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::glib::subclass::prelude::ObjectSubclassIsExt;
use gtk4::prelude::*;

use crate::ui::strings;

const STAR_COUNT: i32 = 5;
const RATING_MIN: i32 = 0;
const RATING_MAX: i32 = STAR_COUNT;

/// Filled star glyph (U+2605) — shown for star positions `<= rating`.
/// Single-codepoint symbol; stays here rather than strings.rs (see module
/// doc comment in strings.rs).
const STAR_FILLED_GLYPH: &str = "\u{2605}";
/// Outline star glyph (U+2606) — shown for star positions `> rating`, and
/// additionally dimmed via [`STAR_OUTLINE_CSS_CLASS`].
/// Single-codepoint symbol; stays here rather than strings.rs (see module
/// doc comment in strings.rs).
const STAR_OUTLINE_GLYPH: &str = "\u{2606}";
/// De-emphasizes the outline glyph — the same generic Adwaita "dim" class
/// `player_bar.rs` already uses for its inactive repeat state.
const STAR_OUTLINE_CSS_CLASS: &str = "dim-label";

/// Shared alias for the click-reporting callback's storage type — see the
/// `on_changed` field doc comment for why it's `Rc`-wrapped and `Option`al.
type OnChangedCallback = Option<Rc<dyn Fn(i32)>>;

mod imp {
    use super::*;
    use gtk4::subclass::prelude::*;

    #[derive(Default)]
    pub struct RatingWidget {
        /// One `(button, its label child)` pair per star, in order. The
        /// label is kept alongside the button so display updates don't
        /// have to re-downcast `button.child()` on every `set_rating`.
        pub stars: RefCell<Vec<(gtk4::Button, gtk4::Label)>>,
        pub rating: Cell<i32>,
        /// Replaced wholesale by `set_on_changed` on every list-item
        /// rebind; `None` before the first `set_on_changed` call, so a
        /// stray click that arrives before then is simply a no-op instead
        /// of needing a placeholder closure.
        ///
        /// `Rc`, not `Box`: `handle_star_activated` needs to clone the
        /// callback out of the `RefCell` and drop the borrow before
        /// invoking it (see the module doc comment), which requires a
        /// cheaply-cloneable handle rather than owned-in-place storage.
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

    /// Builds the five star buttons and wires their click handlers. Called
    /// once from `constructed()`, i.e. exactly once per widget instance —
    /// the same instance is then reused across every list-item rebind.
    fn build_ui(&self) {
        self.set_orientation(gtk4::Orientation::Horizontal);
        // Tight spacing: the flat buttons already carry their own padding;
        // any extra box spacing would spread the five stars into reading
        // as separate controls instead of one rating row.
        self.set_spacing(0);
        self.set_tooltip_text(Some(&strings::text(strings::RATING)));

        let stars: Vec<(gtk4::Button, gtk4::Label)> = (1..=STAR_COUNT)
            .map(|star| {
                let label = gtk4::Label::new(Some(STAR_OUTLINE_GLYPH));
                label.add_css_class(STAR_OUTLINE_CSS_CLASS);

                let button = gtk4::Button::new();
                button.set_child(Some(&label));
                // Flat/frameless so five adjacent buttons read as one star
                // row, not a toolbar (`set_has_frame(false)` applies GTK's
                // "flat" style class).
                button.set_has_frame(false);
                button.set_valign(gtk4::Align::Center);
                // Accessible tooltip for screen readers.
                let action_name = strings::rate_n_stars(star);
                button.set_tooltip_text(Some(&action_name));

                let widget = self.downgrade();
                button.connect_clicked(move |_| {
                    let Some(widget) = widget.upgrade() else {
                        return;
                    };
                    widget.handle_star_activated(star);
                });

                self.append(&button);
                (button, label)
            })
            .collect();
        self.imp().stars.replace(stars);
    }

    /// Applies the Rhythmbox clear-on-reclick rule for a click on star
    /// `star` (1-based, from `connect_clicked` wiring), updates the
    /// display, and reports the new value through the current `on_changed`
    /// callback.
    fn handle_star_activated(&self, star: i32) {
        let new_rating = next_rating(star, self.imp().rating.get());
        self.set_rating(new_rating);
        // Clone the callback out of the `RefCell` first — this borrow ends
        // when the `let` statement completes — so no borrow is held while
        // the callback (and whatever it reentrantly triggers, up to and
        // including a synchronous `set_on_changed` on this same widget) is
        // running. See the module doc comment.
        let callback = self.imp().on_changed.borrow().clone();
        if let Some(callback) = callback {
            callback(new_rating);
        }
    }

    /// Sets the displayed rating without invoking the `on_changed`
    /// callback — used by `track_list.rs` to show a freshly-bound row's
    /// stored rating, a programmatic update rather than a user click.
    /// Clamps out-of-range input (e.g. stale/corrupt DB data) rather than
    /// trusting it.
    pub fn set_rating(&self, rating: i32) {
        let clamped = rating.clamp(RATING_MIN, RATING_MAX);
        self.imp().rating.set(clamped);
        let Ok(stars) = self.imp().stars.try_borrow() else {
            tracing::warn!("rating widget: stars borrow unavailable; skipping redraw");
            return;
        };
        for (i, (_, label)) in stars.iter().enumerate() {
            let filled = (i as i32) < clamped;
            if filled {
                label.set_text(STAR_FILLED_GLYPH);
                label.remove_css_class(STAR_OUTLINE_CSS_CLASS);
            } else {
                label.set_text(STAR_OUTLINE_GLYPH);
                label.add_css_class(STAR_OUTLINE_CSS_CLASS);
            }
        }
    }

    /// Replaces the click callback. `track_list.rs` calls this on every
    /// list-item bind so the callback closes over whichever row is
    /// currently shown — the widget instance itself is recycled across
    /// many rows as the list scrolls (see the module doc comment).
    pub fn set_on_changed(&self, f: impl Fn(i32) + 'static) {
        *self.imp().on_changed.borrow_mut() = Some(Rc::new(f));
    }

    /// Test-only seam for driving a star click without a real pointer:
    /// presses button `index` (1-based) via `emit_clicked`, so the call
    /// goes through the exact same `connect_clicked` →
    /// `handle_star_activated` path a real click or keyboard activation
    /// would. The `stars` borrow is hoisted into its own statement and
    /// dropped before the click (which runs arbitrary callback code) fires.
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
}

impl Default for RatingWidget {
    fn default() -> Self {
        Self::new()
    }
}

/// The rating a click on star `clicked_star` (1-based) should produce given
/// the `current` rating: normally the star's own value, but re-clicking the
/// star that already equals the current rating clears to 0 (the Rhythmbox
/// rule — see the module doc comment). Pure so the rule is unit-testable
/// without any GTK widgets.
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
        // Star buttons are 1-based, so a click on an unrated (0) row always
        // rates — it can never accidentally match-and-clear.
        for star in 1..=STAR_COUNT {
            assert_ne!(next_rating(star, 0), RATING_MIN);
        }
    }

    /// Regression test for the `BorrowMutError` described in the module doc
    /// comment: a click callback that reentrantly calls `set_on_changed` on
    /// the same widget (simulating GTK synchronously rebinding the just-
    /// clicked row) must not panic. Needs a real GTK/GDK display, so it's
    /// `#[ignore]`d by default — run with `xvfb-run -a cargo test --
    /// --ignored reentrant`.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn reentrant_set_on_changed_does_not_panic() {
        if gtk4::init().is_err() {
            eprintln!(
                "skipping reentrant_set_on_changed_does_not_panic: gtk4::init() failed \
                 (no display available)"
            );
            return;
        }

        let widget = RatingWidget::new();
        let widget_weak = widget.downgrade();
        widget.set_on_changed(move |_| {
            let Some(widget) = widget_weak.upgrade() else {
                return;
            };
            // Simulates `connect_bind` reacting synchronously to this same
            // callback's side effects (e.g. a DB write triggering a model
            // `items_changed`) and reinstalling a fresh callback — while
            // `handle_star_activated` is still on the stack below us.
            widget.set_on_changed(|_| {});
        });

        // Pre-fix, the click handler held a `Ref` on `on_changed` for the
        // whole statement that invokes the callback above, so the
        // reentrant `set_on_changed` call's `borrow_mut()` panicked with
        // `BorrowMutError`. Post-fix, the borrow is dropped before the
        // callback runs, so this completes cleanly. Driving it through
        // button 3's `emit_clicked` exercises the real `connect_clicked` →
        // `handle_star_activated` path.
        widget.click_star_for_test(3);
    }
}
