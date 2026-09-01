//! Demand-driven GTK tooltips for hot widget construction and bind paths.
//!
//! `WidgetExt::set_tooltip_text` immediately asks GDK which surface is under
//! the pointer. On X11 that is a synchronous display roundtrip, so virtualized
//! row factories must only provide tooltip text from `query-tooltip` when GTK
//! is actually about to show one.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::prelude::*;

#[derive(Clone)]
pub(crate) struct LazyTooltip {
    text: Rc<RefCell<Option<String>>>,
}

/// Tooltip state paired with recycled `GtkListItem` cells.
///
/// Factories receive separate setup and bind callbacks, so the state created
/// while constructing a cell has to be retained until that cell is rebound.
#[derive(Clone, Default)]
pub(crate) struct ListItemTooltips {
    states: Rc<RefCell<HashMap<usize, LazyTooltip>>>,
}

impl ListItemTooltips {
    pub(crate) fn install(&self, item: &gtk4::ListItem, widget: &impl IsA<gtk4::Widget>) {
        self.states
            .borrow_mut()
            .insert(item.as_ptr() as usize, LazyTooltip::install(widget));
    }

    pub(crate) fn set_text(
        &self,
        item: &gtk4::ListItem,
        widget: &impl IsA<gtk4::Widget>,
        text: Option<String>,
    ) {
        let tooltip = self.states.borrow().get(&(item.as_ptr() as usize)).cloned();
        if let Some(tooltip) = tooltip {
            tooltip.set_text(widget, text);
        } else {
            tracing::warn!("list item has no lazy tooltip state");
        }
    }
}

impl LazyTooltip {
    pub(crate) fn install(widget: &impl IsA<gtk4::Widget>) -> Self {
        let text = Rc::new(RefCell::new(None::<String>));
        widget.connect_query_tooltip({
            let text = text.clone();
            move |_, _, _, _, tooltip| {
                let text = text.borrow().clone();
                let Some(text) = text else {
                    return false;
                };
                tooltip.set_text(Some(&text));
                true
            }
        });
        Self { text }
    }

    pub(crate) fn set_text(&self, widget: &impl IsA<gtk4::Widget>, text: Option<String>) {
        let enabled = text.is_some();
        self.text.replace(text);
        widget.set_has_tooltip(enabled);
        #[cfg(test)]
        record_text(widget, self.text.borrow().clone());
    }
}

pub(crate) fn install(widget: &impl IsA<gtk4::Widget>, text: String) {
    let tooltip = LazyTooltip::install(widget);
    tooltip.set_text(widget, Some(text));
}

// Tests cannot read GTK's `tooltip-text` property back (it is deliberately
// never set — see the module doc comment), so `set_text` mirrors what each
// widget's `query-tooltip` handler would answer into this widget-keyed
// registry. `text_of` is the read side, used by contract tests that only
// have the bound widget, not the `LazyTooltip` that installed it.
#[cfg(test)]
thread_local! {
    static TEXT_BY_WIDGET: RefCell<HashMap<usize, Option<String>>> = RefCell::new(HashMap::new());
}

#[cfg(test)]
fn record_text(widget: &impl IsA<gtk4::Widget>, text: Option<String>) {
    TEXT_BY_WIDGET.with(|registry| {
        registry.borrow_mut().insert(widget.as_ptr() as usize, text);
    });
}

#[cfg(test)]
pub(crate) fn text_of(widget: &impl IsA<gtk4::Widget>) -> Option<String> {
    TEXT_BY_WIDGET.with(|registry| {
        registry
            .borrow()
            .get(&(widget.as_ptr() as usize))
            .cloned()
            .flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn lazy_tooltip_uses_query_state_instead_of_the_eager_text_property() {
        let _main_context = crate::ui::test_main_context::lock_main_context();
        gtk4::init().unwrap();
        let label = gtk4::Label::new(None);
        let tooltip = LazyTooltip::install(&label);

        tooltip.set_text(&label, Some("On demand".into()));
        assert!(label.has_tooltip());
        assert_eq!(label.tooltip_text(), None);

        tooltip.set_text(&label, None);
        assert!(!label.has_tooltip());
        assert_eq!(label.tooltip_text(), None);
    }
}
