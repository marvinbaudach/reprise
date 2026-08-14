use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use crate::ui::strings;

type Callback = Rc<dyn Fn()>;

pub(super) struct ConcertsLocationBanner {
    root: gtk4::Revealer,
    title: gtk4::Label,
    on_open_location: Rc<RefCell<Option<Callback>>>,
}

impl ConcertsLocationBanner {
    pub(super) fn new() -> Self {
        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, 12);
        content.add_css_class("card");
        content.add_css_class("reprise-concerts-location-banner");
        content.set_margin_top(8);
        content.set_margin_bottom(8);
        content.set_margin_start(12);
        content.set_margin_end(12);
        let copy = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
        copy.set_hexpand(true);
        let title = gtk4::Label::new(None);
        title.add_css_class("heading");
        title.set_wrap(true);
        title.set_xalign(0.0);
        copy.append(&title);
        let description = gtk4::Label::new(Some(&strings::text(
            strings::CONCERTS_NO_LOCATION_DESCRIPTION,
        )));
        description.add_css_class("dim-label");
        description.set_wrap(true);
        description.set_xalign(0.0);
        copy.append(&description);
        content.append(&copy);
        let action = gtk4::Button::with_label(&strings::text(strings::LOCATION_SET_LOCATION));
        action.add_css_class("flat");
        action.set_valign(gtk4::Align::Center);
        content.append(&action);
        let root = gtk4::Revealer::new();
        root.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        root.set_child(Some(&content));
        let on_open_location: Rc<RefCell<Option<Callback>>> = Rc::new(RefCell::new(None));
        action.connect_clicked({
            let callback = on_open_location.clone();
            move |_| {
                if let Some(callback) = callback.borrow().clone() {
                    callback();
                }
            }
        });
        Self {
            root,
            title,
            on_open_location,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Widget {
        self.root.upcast_ref()
    }

    pub(super) fn set_on_open_location(&self, callback: impl Fn() + 'static) {
        self.on_open_location.replace(Some(Rc::new(callback)));
    }

    pub(super) fn show(&self, total: usize) {
        self.title
            .set_text(&strings::concerts_no_location_title(total));
        self.root.set_reveal_child(true);
    }

    pub(super) fn hide(&self) {
        self.root.set_reveal_child(false);
    }

    #[cfg(test)]
    pub(super) fn title(&self) -> &gtk4::Label {
        &self.title
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn missing_location_banner_uses_the_real_total() {
        gtk4::init().unwrap();
        let banner = ConcertsLocationBanner::new();
        banner.show(415);
        assert_eq!(
            banner.title().text(),
            "No location set — showing all 415 concerts worldwide"
        );
    }
}
