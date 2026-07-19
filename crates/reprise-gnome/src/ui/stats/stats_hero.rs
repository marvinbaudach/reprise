//! Responsive composer for the My Stats hero.

use gtk4::prelude::*;
use libadwaita as adw;

use super::stats_customize::StatsCustomize;

const HERO_NATURAL_LINE_LENGTH: i32 = 720;
const HERO_COPY_NATURAL_LINE_LENGTH: i32 = 460;

pub(super) struct StatsHero {
    pub(super) root: adw::WrapBox,
    pub(super) time: gtk4::Label,
    pub(super) comparison: gtk4::Label,
    pub(super) subline: gtk4::Label,
    pub(super) period_dropdown: gtk4::DropDown,
    pub(super) period_model: gtk4::StringList,
    pub(super) row: adw::WrapBox,
    pub(super) time_row: adw::WrapBox,
}

impl StatsHero {
    pub(super) fn new(customize: &StatsCustomize) -> Self {
        let time = hero_label("0 h", "stats-headline-hours", false);
        let comparison = hero_label("", "stats-pill", false);
        comparison.set_halign(gtk4::Align::Start);
        comparison.set_visible(false);
        let subline = hero_label(
            "0 plays \u{00b7} \u{00d8} 0 min/day \u{00b7} 0 artists",
            "stats-headline-subtitle",
            true,
        );
        let hero_text = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        let time_row = wrapping_row(8, 8, HERO_COPY_NATURAL_LINE_LENGTH);
        time_row.append(&time);
        time_row.append(&comparison);
        hero_text.append(&time_row);
        hero_text.append(&subline);
        hero_text.set_hexpand(true);

        let period_model = gtk4::StringList::new(&[]);
        let period_dropdown = gtk4::DropDown::builder().model(&period_model).build();
        period_dropdown.add_css_class("stats-period-dropdown");
        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        controls.set_valign(gtk4::Align::Center);
        controls.append(&period_dropdown);
        controls.append(customize.widget());
        // WrapBox measures every resulting line itself. A BreakpointBin made
        // this row both its child and a breakpoint setter target, then needed
        // an artificial 1 px request to become narrow; inside a ScrolledWindow
        // that same request collapsed the row vertically.
        let row = wrapping_row(18, 12, HERO_NATURAL_LINE_LENGTH);
        row.set_valign(gtk4::Align::Center);
        row.set_hexpand(true);
        row.set_justify(adw::JustifyMode::Fill);
        row.set_justify_last_line(true);
        row.append(&hero_text);
        row.append(&controls);
        let root = row.clone();

        Self {
            root,
            time,
            comparison,
            subline,
            period_dropdown,
            period_model,
            row,
            time_row,
        }
    }
}

fn wrapping_row(child_spacing: i32, line_spacing: i32, natural_width: i32) -> adw::WrapBox {
    let row = adw::WrapBox::new();
    row.set_child_spacing(child_spacing);
    row.set_line_spacing(line_spacing);
    row.set_natural_line_length(natural_width);
    row.set_wrap_policy(adw::WrapPolicy::Natural);
    row
}

fn hero_label(text: &str, class: &str, ellipsize: bool) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class(class);
    label.set_xalign(0.0);
    label.set_ellipsize(if ellipsize {
        gtk4::pango::EllipsizeMode::End
    } else {
        gtk4::pango::EllipsizeMode::None
    });
    label
}
