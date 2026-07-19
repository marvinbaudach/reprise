//! Responsive composer for the My Stats hero.

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;

use super::stats_customize::StatsCustomize;

const HERO_REFLOW_BREAKPOINT: f64 = 720.0;
const HERO_COPY_REFLOW_BREAKPOINT: f64 = 460.0;

pub(super) struct StatsHero {
    pub(super) root: adw::BreakpointBin,
    pub(super) time: gtk4::Label,
    pub(super) comparison: gtk4::Label,
    pub(super) subline: gtk4::Label,
    pub(super) period_dropdown: gtk4::DropDown,
    pub(super) period_model: gtk4::StringList,
    pub(super) row: gtk4::Box,
    pub(super) time_row: gtk4::Box,
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
        let time_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
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
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 18);
        row.set_valign(gtk4::Align::Center);
        row.append(&hero_text);
        row.append(&controls);

        let root = adw::BreakpointBin::new();
        // Without a small request, the natural width of the unellipsized hero
        // can prevent BreakpointBin from ever receiving a narrow allocation.
        root.set_width_request(1);
        root.set_height_request(1);
        root.set_child(Some(&row));
        let hero_reflow = breakpoint(HERO_REFLOW_BREAKPOINT);
        set_vertical(&hero_reflow, &row);
        root.add_breakpoint(hero_reflow);
        let copy_reflow = breakpoint(HERO_COPY_REFLOW_BREAKPOINT);
        // BreakpointBin exposes one current breakpoint. The narrower state
        // therefore repeats the outer reflow as well as stacking the copy.
        set_vertical(&copy_reflow, &row);
        set_vertical(&copy_reflow, &time_row);
        root.add_breakpoint(copy_reflow);

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

fn breakpoint(max_width: f64) -> adw::Breakpoint {
    adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        max_width,
        adw::LengthUnit::Px,
    ))
}

fn set_vertical(breakpoint: &adw::Breakpoint, box_: &gtk4::Box) {
    breakpoint.add_setter(
        box_,
        "orientation",
        Some(&gtk4::Orientation::Vertical.to_value()),
    );
}
