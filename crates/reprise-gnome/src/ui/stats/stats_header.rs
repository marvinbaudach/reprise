//! My Stats page header with its optional new-history badge and period selector.

use gtk4::prelude::*;
use libadwaita as adw;

const HEADER_NATURAL_LINE_LENGTH: i32 = 720;

#[derive(Clone)]
pub(super) struct StatsHeader {
    pub(super) root: adw::WrapBox,
    pub(super) new_badge: gtk4::Label,
    pub(super) period_dropdown: gtk4::DropDown,
    pub(super) period_model: gtk4::StringList,
}

impl StatsHeader {
    pub(super) fn new() -> Self {
        let title = gtk4::Label::new(Some("My Stats"));
        title.add_css_class("stats-header-title");
        title.set_xalign(0.0);

        let new_badge = gtk4::Label::new(None);
        new_badge.add_css_class("stats-pill");
        new_badge.set_ellipsize(gtk4::pango::EllipsizeMode::None);
        new_badge.set_visible(false);

        let title_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
        title_row.set_hexpand(true);
        title_row.set_valign(gtk4::Align::Center);
        title_row.append(&title);
        title_row.append(&new_badge);

        let period_model = gtk4::StringList::new(&[]);
        let period_dropdown = gtk4::DropDown::builder().model(&period_model).build();
        period_dropdown.add_css_class("stats-period-dropdown");
        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        controls.set_valign(gtk4::Align::Center);
        controls.append(&period_dropdown);

        let root = adw::WrapBox::new();
        root.set_child_spacing(18);
        root.set_line_spacing(10);
        root.set_natural_line_length(HEADER_NATURAL_LINE_LENGTH);
        root.set_wrap_policy(adw::WrapPolicy::Natural);
        root.set_justify(adw::JustifyMode::Fill);
        root.set_justify_last_line(true);
        root.append(&title_row);
        root.append(&controls);

        Self {
            root,
            new_badge,
            period_dropdown,
            period_model,
        }
    }

    pub(super) fn show_new_badge(&self, label: &str, tooltip: &str) {
        self.new_badge.set_label(label);
        self.new_badge.set_tooltip_text(Some(tooltip));
        self.new_badge.set_visible(true);
    }

    pub(super) fn hide_new_badge(&self) {
        self.new_badge.set_visible(false);
        self.new_badge.set_tooltip_text(None);
    }
}
