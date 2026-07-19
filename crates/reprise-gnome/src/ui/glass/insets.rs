//! Exact scroll-end padding derived from allocated glass-zone heights.

use std::cell::{Cell, RefCell};

use gtk4::glib;
use gtk4::prelude::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SafeInsets {
    pub(crate) top: i32,
    pub(crate) bottom: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
pub(crate) enum PlayerBarEdge {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct InsetMeasurements {
    pub(crate) header: i32,
    pub(crate) search: i32,
    pub(crate) player: i32,
    pub(crate) player_edge: PlayerBarEdge,
}

#[cfg(test)]
impl InsetMeasurements {
    pub(crate) fn safe_insets(self) -> SafeInsets {
        let header = self.header.max(0);
        let search = self.search.max(0);
        let player = self.player.max(0);
        match self.player_edge {
            PlayerBarEdge::Top => SafeInsets {
                top: header.saturating_add(search).saturating_add(player),
                bottom: 0,
            },
            PlayerBarEdge::Bottom => SafeInsets {
                top: header.saturating_add(search),
                bottom: player,
            },
        }
    }
}

struct InsetTarget {
    /// The scroller, which outlives its content. Holding the *content* here
    /// would lose the target for good on `set_child()`: GTK keeps the
    /// auto-inserted `Viewport` across a swap but replaces what sits inside it,
    /// so a stored content reference would go stale while the scroller stayed
    /// perfectly alive.
    scrolled: glib::WeakRef<gtk4::ScrolledWindow>,
    /// Content the margins in `base` were snapshotted from, re-resolved on
    /// every apply so a swapped-in child is padded from its own margins rather
    /// than inheriting its predecessor's.
    content: RefCell<glib::WeakRef<gtk4::Widget>>,
    base: Cell<(i32, i32)>,
}

pub(crate) struct SafeInsetApplier {
    targets: Vec<InsetTarget>,
    current: Cell<SafeInsets>,
}

impl SafeInsetApplier {
    pub(crate) fn discover(root: &impl IsA<gtk4::Widget>) -> Self {
        let mut targets = Vec::new();
        collect_scrolled_children(root.as_ref(), &mut targets);
        Self {
            targets,
            current: Cell::new(SafeInsets::default()),
        }
    }

    pub(crate) fn apply(&self, insets: SafeInsets) {
        let unchanged = self.current.get() == insets;
        self.current.set(insets);
        for target in &self.targets {
            let Some(scrolled) = target.scrolled.upgrade() else {
                continue;
            };
            let Some(content) = scrolled_content(&scrolled) else {
                continue;
            };

            // Re-resolving the content each time is what makes a `set_child()`
            // swap survivable. A swapped-in child brings its own margins, so
            // the base must be re-snapshotted before the insets are added —
            // otherwise it would inherit the previous child's base, or (worse)
            // its already-inset margins would be treated as base and compound.
            let swapped = target.content.borrow().upgrade().as_ref() != Some(&content);
            if swapped {
                target
                    .base
                    .set((content.margin_top(), content.margin_bottom()));
                target.content.replace(content.downgrade());
            } else if unchanged {
                // Same content, same insets: the margins already hold. This is
                // the per-allocation hot path, so skip the setters entirely.
                continue;
            }

            let (base_top, base_bottom) = target.base.get();
            content.set_margin_top(base_top.saturating_add(insets.top));
            content.set_margin_bottom(base_bottom.saturating_add(insets.bottom));
        }
    }

    #[cfg(test)]
    pub(crate) fn target_count(&self) -> usize {
        self.targets.len()
    }
}

/// The widget that should carry the insets for `scrolled`.
///
/// `ScrolledWindow::child()` does not return what was handed to `set_child()`:
/// GTK wraps anything that is not `GtkScrollable` in an internal `GtkViewport`.
/// Padding that viewport would shrink the scroll aperture instead of the
/// content — the inverse of the intent — so exactly one level is unwrapped.
/// A deliberate, app-authored viewport is treated the same way, since its
/// child is still the thing to pad.
fn scrolled_content(scrolled: &gtk4::ScrolledWindow) -> Option<gtk4::Widget> {
    let child = scrolled.child();
    match child.and_downcast_ref::<gtk4::Viewport>() {
        Some(viewport) => viewport.child(),
        None => child,
    }
}

/// Walks `widget` once and records every scroller found.
///
/// This is a snapshot: a `ScrolledWindow` added to the tree afterwards (a lazily
/// built stack page, say) is never discovered and never padded. Every scroller
/// in the shell is built up front today, so nothing hits this — a caller that
/// starts adding scrollers late needs to re-run discovery rather than rely on
/// `apply()` finding them.
fn collect_scrolled_children(widget: &gtk4::Widget, targets: &mut Vec<InsetTarget>) {
    if let Some(scrolled) = widget.downcast_ref::<gtk4::ScrolledWindow>() {
        if let Some(child) = scrolled_content(scrolled) {
            targets.push(InsetTarget {
                base: Cell::new((child.margin_top(), child.margin_bottom())),
                content: RefCell::new(child.downgrade()),
                scrolled: scrolled.downgrade(),
            });
        }
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        collect_scrolled_children(&current, targets);
        child = current.next_sibling();
    }
}
