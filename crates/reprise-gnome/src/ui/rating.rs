//! `RatingWidget`: a width-responsive interactive rating cell. Its compact
//! state shows one `★ N` menu button whose popover contains the five rating
//! choices; when the column is widened it promotes to five inline flat
//! buttons. Both states use text star glyphs (filled `★` U+2605 vs outline
//! `☆` U+2606), so they stay readable across icon themes.
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
use libadwaita as adw;
use libadwaita::prelude::*;

use crate::ui::strings;

const STAR_COUNT: i32 = 5;
const RATING_MIN: i32 = 0;
const RATING_MAX: i32 = STAR_COUNT;
pub(super) const COMPACT_RATING_COLUMN_WIDTH: i32 = 88;
const WIDE_RATING_MIN_WIDTH: i32 = 132;
const COMPACT_CONTROL_MIN_WIDTH: i32 = 44;
const RESPONSIVE_MIN_HEIGHT: i32 = 1;
const COMPACT_STACK_CHILD: &str = "compact";
const WIDE_STACK_CHILD: &str = "wide";
const INLINE_STAR_CSS_CLASS: &str = "reprise-rating-inline-star";
const COMPACT_BUTTON_CSS_CLASS: &str = "reprise-rating-compact-button";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RatingPresentation {
    Compact,
    Wide,
}

impl RatingPresentation {
    fn stack_child(self) -> &'static str {
        match self {
            Self::Compact => COMPACT_STACK_CHILD,
            Self::Wide => WIDE_STACK_CHILD,
        }
    }
}

fn rating_presentation(width: i32) -> RatingPresentation {
    if width >= WIDE_RATING_MIN_WIDTH {
        RatingPresentation::Wide
    } else {
        RatingPresentation::Compact
    }
}

fn compact_rating_text(rating: i32) -> String {
    let rating = rating.clamp(RATING_MIN, RATING_MAX);
    if rating == RATING_MIN {
        format!("{STAR_OUTLINE_GLYPH} —")
    } else {
        format!("{STAR_FILLED_GLYPH} {rating}")
    }
}

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

thread_local! {
    static RATING_STYLE_INSTALLED: Cell<bool> = const { Cell::new(false) };
}

fn install_rating_style(widget: &impl IsA<gtk4::Widget>) {
    RATING_STYLE_INSTALLED.with(|installed| {
        if installed.replace(true) {
            return;
        }
        let provider = gtk4::CssProvider::new();
        provider.load_from_string(&format!(
            ".{INLINE_STAR_CSS_CLASS} {{ min-width: 20px; min-height: 26px; padding: 1px; }}\n\
             .{COMPACT_BUTTON_CSS_CLASS} {{ min-width: {COMPACT_CONTROL_MIN_WIDTH}px; \
             padding: 2px 6px; }}"
        ));
        gtk4::style_context_add_provider_for_display(
            &widget.display(),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
}

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
        pub chooser_stars: RefCell<Vec<(gtk4::Button, gtk4::Label)>>,
        pub compact_label: RefCell<Option<gtk4::Label>>,
        pub compact_button: RefCell<Option<gtk4::MenuButton>>,
        pub presentation_stack: RefCell<Option<gtk4::Stack>>,
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

    /// Builds compact and wide controls once. The `BreakpointBin` switches
    /// between them from the cell's own allocation, so a narrow window or a
    /// user-narrowed Rating column does not reserve five inline buttons.
    fn build_ui(&self) {
        install_rating_style(self);
        self.set_orientation(gtk4::Orientation::Horizontal);
        self.set_spacing(0);
        self.set_tooltip_text(Some(&strings::text(strings::RATING)));

        let wide = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        let stars: Vec<(gtk4::Button, gtk4::Label)> = (1..=STAR_COUNT)
            .map(|star| {
                let (button, label) = self.build_star_control(star, true, None);
                wide.append(&button);
                (button, label)
            })
            .collect();
        self.imp().stars.replace(stars);

        let chooser = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        chooser.set_margin_top(6);
        chooser.set_margin_bottom(6);
        chooser.set_margin_start(6);
        chooser.set_margin_end(6);
        let popover = gtk4::Popover::new();
        let chooser_stars: Vec<(gtk4::Button, gtk4::Label)> = (1..=STAR_COUNT)
            .map(|star| {
                let (button, label) = self.build_star_control(star, false, Some(&popover));
                chooser.append(&button);
                (button, label)
            })
            .collect();
        self.imp().chooser_stars.replace(chooser_stars);
        popover.set_child(Some(&chooser));

        let compact_label = gtk4::Label::new(Some(&compact_rating_text(RATING_MIN)));
        let compact_button = gtk4::MenuButton::new();
        compact_button.set_child(Some(&compact_label));
        compact_button.set_popover(Some(&popover));
        compact_button.set_always_show_arrow(false);
        compact_button.set_has_frame(false);
        compact_button.set_tooltip_text(Some(&strings::text(strings::RATING)));
        compact_button.add_css_class(COMPACT_BUTTON_CSS_CLASS);
        compact_button.add_css_class("reprise-rating-star");
        self.imp().compact_label.replace(Some(compact_label));
        self.imp()
            .compact_button
            .replace(Some(compact_button.clone()));

        let stack = gtk4::Stack::new();
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        stack.add_named(&compact_button, Some(COMPACT_STACK_CHILD));
        stack.add_named(&wide, Some(WIDE_STACK_CHILD));
        stack
            .set_visible_child_name(rating_presentation(COMPACT_RATING_COLUMN_WIDTH).stack_child());
        self.imp().presentation_stack.replace(Some(stack.clone()));

        let responsive = adw::BreakpointBin::new();
        responsive.set_hexpand(true);
        responsive.set_width_request(COMPACT_CONTROL_MIN_WIDTH);
        responsive.set_height_request(RESPONSIVE_MIN_HEIGHT);
        responsive.set_child(Some(&stack));
        let condition = adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MinWidth,
            f64::from(WIDE_RATING_MIN_WIDTH),
            adw::LengthUnit::Px,
        );
        let breakpoint = adw::Breakpoint::new(condition);
        breakpoint.add_setter(
            &stack,
            "visible-child-name",
            Some(&WIDE_STACK_CHILD.to_value()),
        );
        responsive.add_breakpoint(breakpoint);
        self.append(&responsive);
    }

    fn build_star_control(
        &self,
        star: i32,
        inline: bool,
        popover: Option<&gtk4::Popover>,
    ) -> (gtk4::Button, gtk4::Label) {
        let label = gtk4::Label::new(Some(STAR_OUTLINE_GLYPH));
        label.add_css_class(STAR_OUTLINE_CSS_CLASS);

        let button = gtk4::Button::new();
        button.add_css_class("reprise-rating-star");
        button.set_child(Some(&label));
        button.set_has_frame(false);
        button.set_valign(gtk4::Align::Center);
        button.set_tooltip_text(Some(&strings::rate_n_stars(star)));
        if inline {
            button.add_css_class(INLINE_STAR_CSS_CLASS);
        }

        let widget = self.downgrade();
        let popover = popover.map(gtk4::glib::object::ObjectExt::downgrade);
        button.connect_clicked(move |_| {
            let Some(widget) = widget.upgrade() else {
                return;
            };
            widget.handle_star_activated(star);
            if let Some(popover) = popover.as_ref().and_then(glib::WeakRef::upgrade) {
                popover.popdown();
            }
        });
        (button, label)
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
        let labels = stars
            .iter()
            .enumerate()
            .map(|(index, (_, label))| (index as i32 + 1, label.clone()))
            .collect::<Vec<_>>();
        drop(stars);
        let Ok(chooser_stars) = self.imp().chooser_stars.try_borrow() else {
            tracing::warn!("rating widget: chooser stars borrow unavailable; skipping redraw");
            return;
        };
        let chooser_labels = chooser_stars
            .iter()
            .enumerate()
            .map(|(index, (_, label))| (index as i32 + 1, label.clone()))
            .collect::<Vec<_>>();
        drop(chooser_stars);
        for (star, label) in labels.into_iter().chain(chooser_labels) {
            let filled = star <= clamped;
            if filled {
                label.set_text(STAR_FILLED_GLYPH);
                label.remove_css_class(STAR_OUTLINE_CSS_CLASS);
            } else {
                label.set_text(STAR_OUTLINE_GLYPH);
                label.add_css_class(STAR_OUTLINE_CSS_CLASS);
            }
        }
        let compact_label = self.imp().compact_label.borrow().clone();
        if let Some(compact_label) = compact_label {
            compact_label.set_text(&compact_rating_text(clamped));
        }
        let compact_button = self.imp().compact_button.borrow().clone();
        if let Some(compact_button) = compact_button {
            let tooltip = if clamped == RATING_MIN {
                strings::text(strings::RATING)
            } else {
                strings::rate_n_stars(clamped)
            };
            compact_button.set_tooltip_text(Some(&tooltip));
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

    #[cfg(test)]
    pub fn click_compact_choice_for_test(&self, index: i32) {
        let button = self
            .imp()
            .chooser_stars
            .borrow()
            .get(usize::try_from(index - 1).expect("star index must be >= 1"))
            .map(|(button, _)| button.clone());
        match button {
            Some(button) => button.emit_clicked(),
            None => panic!("no compact rating choice at index {index}"),
        }
    }

    #[cfg(test)]
    pub fn compact_text_for_test(&self) -> String {
        self.imp()
            .compact_label
            .borrow()
            .as_ref()
            .map(|label| label.text().to_string())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub fn presentation_for_test(&self) -> String {
        self.imp()
            .presentation_stack
            .borrow()
            .as_ref()
            .and_then(gtk4::Stack::visible_child_name)
            .map(|name| name.to_string())
            .unwrap_or_default()
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
    fn rating_presentation_adapts_at_the_compact_width_boundary() {
        assert_eq!(rating_presentation(88), RatingPresentation::Compact);
        assert_eq!(
            rating_presentation(WIDE_RATING_MIN_WIDTH - 1),
            RatingPresentation::Compact
        );
        assert_eq!(
            rating_presentation(WIDE_RATING_MIN_WIDTH),
            RatingPresentation::Wide
        );
    }

    #[test]
    fn compact_rating_text_keeps_zero_and_values_distinct() {
        assert_eq!(compact_rating_text(0), "☆ —");
        assert_eq!(compact_rating_text(1), "★ 1");
        assert_eq!(compact_rating_text(5), "★ 5");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn compact_chooser_updates_the_value_and_reports_the_change() {
        gtk4::init().unwrap();
        let widget = RatingWidget::new();
        widget.set_rating(2);
        assert_eq!(widget.compact_text_for_test(), "★ 2");

        let reported = Rc::new(Cell::new(-1));
        let reported_for_callback = reported.clone();
        widget.set_on_changed(move |rating| reported_for_callback.set(rating));
        widget.click_compact_choice_for_test(4);

        assert_eq!(reported.get(), 4);
        assert_eq!(widget.compact_text_for_test(), "★ 4");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn rating_widget_promotes_to_inline_stars_when_given_space() {
        gtk4::init().unwrap();
        let widget = RatingWidget::new();
        let window = gtk4::Window::builder()
            .default_width(COMPACT_RATING_COLUMN_WIDTH)
            .default_height(48)
            .child(&widget)
            .build();
        window.present();
        drain_main_context();
        assert_eq!(widget.presentation_for_test(), COMPACT_STACK_CHILD);

        window.set_size_request(WIDE_RATING_MIN_WIDTH + 24, 48);
        drain_main_context();
        assert_eq!(widget.presentation_for_test(), WIDE_STACK_CHILD);
        window.close();
    }

    fn drain_main_context() {
        let context = glib::MainContext::default();
        for _ in 0..10 {
            while context.pending() {
                context.iteration(false);
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
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
