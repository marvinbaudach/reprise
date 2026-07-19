//! Fixed three-toggle customization menu for optional My Stats sections.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::library::settings::StatsLayout;

type LayoutCallback = Rc<RefCell<Option<Rc<dyn Fn(StatsLayout)>>>>;

#[derive(Clone)]
pub(in crate::ui) struct StatsCustomize {
    button: gtk4::MenuButton,
    clock: gtk4::CheckButton,
    genres: gtk4::CheckButton,
    highlights: gtk4::CheckButton,
    updating: Rc<Cell<bool>>,
    on_changed: LayoutCallback,
}

impl StatsCustomize {
    pub(in crate::ui) fn new() -> Self {
        let button = gtk4::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .tooltip_text("Customize")
            .build();
        button.add_css_class("flat");

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        content.set_margin_top(12);
        content.set_margin_bottom(12);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let title = gtk4::Label::new(Some("Customize"));
        title.add_css_class("heading");
        title.set_xalign(0.0);
        content.append(&title);
        let clock = gtk4::CheckButton::with_label("Clock");
        let genres = gtk4::CheckButton::with_label("Genres");
        let highlights = gtk4::CheckButton::with_label("Highlights");
        content.append(&clock);
        content.append(&genres);
        content.append(&highlights);
        let popover = gtk4::Popover::builder().child(&content).build();
        button.set_popover(Some(&popover));

        let updating = Rc::new(Cell::new(false));
        let on_changed: LayoutCallback = Rc::new(RefCell::new(None));
        for check in [&clock, &genres, &highlights] {
            check.connect_toggled({
                let clock = clock.clone();
                let genres = genres.clone();
                let highlights = highlights.clone();
                let updating = updating.clone();
                let on_changed = on_changed.clone();
                move |_| {
                    if updating.get() {
                        return;
                    }
                    let callback = on_changed.borrow().clone();
                    if let Some(callback) = callback {
                        callback(StatsLayout {
                            clock: clock.is_active(),
                            genres: genres.is_active(),
                            highlights: highlights.is_active(),
                        });
                    }
                }
            });
        }

        let customize = Self {
            button,
            clock,
            genres,
            highlights,
            updating,
            on_changed,
        };
        customize.set_layout(StatsLayout {
            clock: true,
            genres: true,
            highlights: true,
        });
        customize
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::MenuButton {
        &self.button
    }

    pub(in crate::ui) fn set_layout(&self, layout: StatsLayout) {
        self.updating.set(true);
        self.clock.set_active(layout.clock);
        self.genres.set_active(layout.genres);
        self.highlights.set_active(layout.highlights);
        self.updating.set(false);
    }

    pub(in crate::ui) fn set_on_changed(&self, callback: impl Fn(StatsLayout) + 'static) {
        *self.on_changed.borrow_mut() = Some(Rc::new(callback));
    }

    #[cfg(test)]
    pub(in crate::ui) fn check_count(&self) -> usize {
        let Some(popover) = self.button.popover() else {
            return 0;
        };
        let Some(content) = popover.child() else {
            return 0;
        };
        let mut count = 0;
        let mut child = content.first_child();
        while let Some(widget) = child {
            if widget.is::<gtk4::CheckButton>() {
                count += 1;
            }
            child = widget.next_sibling();
        }
        count
    }
}
