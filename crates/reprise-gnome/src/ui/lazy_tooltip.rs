//! Demand-driven GTK tooltips for hot widget construction and bind paths.
//!
//! `WidgetExt::set_tooltip_text` immediately asks GDK which surface is under
//! the pointer. On X11 that is a synchronous display roundtrip, so virtualized
//! row factories must only provide tooltip text from `query-tooltip` when GTK
//! is actually about to show one.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

#[derive(Clone)]
pub(crate) struct LazyTooltip {
    text: Rc<RefCell<Option<String>>>,
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
    }
}

pub(crate) fn install(widget: &impl IsA<gtk4::Widget>, text: String) {
    let tooltip = LazyTooltip::install(widget);
    tooltip.set_text(widget, Some(text));
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
