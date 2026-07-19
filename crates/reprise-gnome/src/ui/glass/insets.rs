//! Exact scroll-end padding derived from allocated glass-zone heights.

use std::cell::Cell;

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
    widget: glib::WeakRef<gtk4::Widget>,
    base_top: i32,
    base_bottom: i32,
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
        if self.current.get() == insets {
            return;
        }
        self.current.set(insets);
        for target in &self.targets {
            let Some(widget) = target.widget.upgrade() else {
                continue;
            };
            widget.set_margin_top(target.base_top.saturating_add(insets.top));
            widget.set_margin_bottom(target.base_bottom.saturating_add(insets.bottom));
        }
    }

    #[cfg(test)]
    pub(crate) fn target_count(&self) -> usize {
        self.targets.len()
    }
}

fn collect_scrolled_children(widget: &gtk4::Widget, targets: &mut Vec<InsetTarget>) {
    if let Some(scrolled) = widget.downcast_ref::<gtk4::ScrolledWindow>() {
        let target = scrolled.child();
        let target = match target.and_downcast_ref::<gtk4::Viewport>() {
            Some(viewport) => viewport.child(),
            None => target,
        };
        if let Some(child) = target {
            targets.push(InsetTarget {
                base_top: child.margin_top(),
                base_bottom: child.margin_bottom(),
                widget: child.downgrade(),
            });
        }
    }

    let mut child = widget.first_child();
    while let Some(current) = child {
        collect_scrolled_children(&current, targets);
        child = current.next_sibling();
    }
}
