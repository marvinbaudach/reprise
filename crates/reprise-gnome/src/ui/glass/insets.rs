//! Exact scroll-end padding derived from allocated glass-zone heights.

use std::cell::{Cell, RefCell};

use gtk4::glib;
use gtk4::prelude::*;

use super::scroll_inset::ScrollInset;

const TOP_INSET_ANCHOR_CLASS: &str = "reprise-glass-top-inset-anchor";

pub(crate) fn mark_top_inset_anchor(widget: &impl IsA<gtk4::Widget>) {
    widget.add_css_class(TOP_INSET_ANCHOR_CLASS);
}

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

enum InsetTarget {
    Margin {
        widget: glib::WeakRef<gtk4::Widget>,
        base_top: i32,
        base_bottom: i32,
        apply_top: bool,
        apply_bottom: bool,
    },
    Scrolled {
        scrolled: glib::WeakRef<gtk4::ScrolledWindow>,
        content: RefCell<glib::WeakRef<gtk4::Widget>>,
        base: Cell<(i32, i32)>,
        apply_top: bool,
        apply_bottom: bool,
    },
}

pub(crate) struct SafeInsetApplier {
    targets: Vec<InsetTarget>,
    current: Cell<SafeInsets>,
}

impl SafeInsetApplier {
    pub(crate) fn discover(root: &impl IsA<gtk4::Widget>) -> Self {
        let mut targets = Vec::new();
        collect_targets(root.as_ref(), false, &mut targets);
        Self {
            targets,
            current: Cell::new(SafeInsets::default()),
        }
    }

    pub(crate) fn apply(&self, insets: SafeInsets) {
        let unchanged = self.current.replace(insets) == insets;
        for target in &self.targets {
            match target {
                InsetTarget::Margin {
                    widget,
                    base_top,
                    base_bottom,
                    apply_top,
                    apply_bottom,
                } => {
                    if unchanged {
                        continue;
                    }
                    let Some(widget) = widget.upgrade() else {
                        continue;
                    };
                    widget.set_margin_top(inset_value(*base_top, *apply_top, insets.top));
                    widget.set_margin_bottom(inset_value(
                        *base_bottom,
                        *apply_bottom,
                        insets.bottom,
                    ));
                }
                InsetTarget::Scrolled {
                    scrolled,
                    content,
                    base,
                    apply_top,
                    apply_bottom,
                } => {
                    let Some(scrolled) = scrolled.upgrade() else {
                        continue;
                    };
                    let Some(resolved) = resolve_scrolled_content(&scrolled) else {
                        continue;
                    };
                    let swapped = content.borrow().upgrade().as_ref() != Some(&resolved);
                    if swapped {
                        base.set(base_margins(&resolved));
                        content.replace(resolved.downgrade());
                    } else if unchanged {
                        continue;
                    }
                    let (base_top, base_bottom) = base.get();
                    apply_to_scrolled_content(
                        &resolved,
                        inset_value(base_top, *apply_top, insets.top),
                        inset_value(base_bottom, *apply_bottom, insets.bottom),
                    );
                }
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn target_count(&self) -> usize {
        self.targets.len()
    }
}

fn inset_value(base: i32, applies: bool, inset: i32) -> i32 {
    if applies {
        base.saturating_add(inset)
    } else {
        base
    }
}

fn base_margins(content: &gtk4::Widget) -> (i32, i32) {
    if content.is::<ScrollInset>() {
        (0, 0)
    } else {
        (content.margin_top(), content.margin_bottom())
    }
}

fn apply_to_scrolled_content(content: &gtk4::Widget, top: i32, bottom: i32) {
    if let Some(inset) = content.downcast_ref::<ScrollInset>() {
        inset.set_insets(top, bottom);
    } else {
        content.set_margin_top(top);
        content.set_margin_bottom(bottom);
    }
}

/// Resolves the current scroll content and preserves native list virtualization.
///
/// GTK inserts a `Viewport` around non-scrollable children; those children keep
/// using margins. Native `GridView`, `ListView`, and `ColumnView` children are
/// instead wrapped in `ScrollInset`, which itself implements `GtkScrollable`
/// and extends the adjustment range without padding the virtualized widget.
fn resolve_scrolled_content(scrolled: &gtk4::ScrolledWindow) -> Option<gtk4::Widget> {
    let child = scrolled.child()?;
    if child.is::<ScrollInset>() {
        return Some(child);
    }
    if child.is::<gtk4::Scrollable>() {
        scrolled.set_child(gtk4::Widget::NONE);
        let inset = ScrollInset::new(&child);
        scrolled.set_child(Some(&inset));
        return Some(inset.upcast());
    }
    child
        .downcast_ref::<gtk4::Viewport>()
        .and_then(gtk4::Viewport::child)
}

fn collect_targets(
    widget: &gtk4::Widget,
    ancestor_has_top_anchor: bool,
    targets: &mut Vec<InsetTarget>,
) {
    let has_top_anchor = widget.has_css_class(TOP_INSET_ANCHOR_CLASS);
    if has_top_anchor {
        targets.push(InsetTarget::Margin {
            base_top: widget.margin_top(),
            base_bottom: widget.margin_bottom(),
            widget: widget.downgrade(),
            apply_top: true,
            apply_bottom: false,
        });
    }
    let top_is_anchored = ancestor_has_top_anchor || has_top_anchor;

    if let Some(scrolled) = widget.downcast_ref::<gtk4::ScrolledWindow>() {
        if let Some(content) = resolve_scrolled_content(scrolled) {
            targets.push(InsetTarget::Scrolled {
                base: Cell::new(base_margins(&content)),
                content: RefCell::new(content.downgrade()),
                scrolled: scrolled.downgrade(),
                apply_top: !top_is_anchored,
                apply_bottom: true,
            });
        }
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        collect_targets(&current, top_is_anchored, targets);
        child = current.next_sibling();
    }
}
