//! Listening-time hero and its responsive KPI row.

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::stats_period::StatsPeriod;
use reprise_core::library::stats_snapshot::{ComparisonPresentation, StatsSnapshot};

use super::stats_header::StatsHeader;
use crate::ui::strings;

const HERO_NATURAL_LINE_LENGTH: i32 = 900;

#[derive(Clone)]
pub(super) struct StatsKpi {
    pub(super) root: gtk4::Box,
    pub(super) label: gtk4::Label,
    pub(super) value: gtk4::Label,
    pub(super) reference: gtk4::Label,
    pub(super) icon: gtk4::Image,
}

impl StatsKpi {
    fn new(label_text: &str, show_icon: bool) -> Self {
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
        root.add_css_class("stats-kpi");
        root.set_valign(gtk4::Align::End);

        let title = label(label_text, "stats-kpi-label");
        let value_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
        let icon = gtk4::Image::new();
        icon.add_css_class("stats-kpi-trend-icon");
        icon.set_visible(show_icon);
        let value = label("", "stats-kpi-value");
        value.set_ellipsize(gtk4::pango::EllipsizeMode::None);
        value_row.append(&icon);
        value_row.append(&value);
        let reference = label("", "stats-kpi-reference");

        root.append(&title);
        root.append(&value_row);
        root.append(&reference);
        Self {
            root,
            label: title,
            value,
            reference,
            icon,
        }
    }

    fn show(&self, value: &str, reference: Option<&str>, tooltip: Option<&str>) {
        self.value.set_label(value);
        self.reference.set_label(reference.unwrap_or_default());
        self.reference.set_visible(reference.is_some());
        self.root.set_tooltip_text(tooltip);
        self.root.set_visible(true);
    }

    fn hide(&self) {
        self.root.set_visible(false);
        self.root.set_tooltip_text(None);
    }
}

#[derive(Clone)]
pub(super) struct StatsKpis {
    pub(super) per_day: StatsKpi,
    pub(super) trend: StatsKpi,
    pub(super) pace: StatsKpi,
    pub(super) best_week: StatsKpi,
}

#[derive(Clone)]
pub(super) struct StatsHero {
    pub(super) root: adw::WrapBox,
    pub(super) time: gtk4::Label,
    pub(super) subline: gtk4::Label,
    pub(super) time_block: gtk4::Box,
    pub(super) kpis: StatsKpis,
}

impl StatsHero {
    pub(super) fn new() -> Self {
        let time = label("0 min", "stats-hero-number");
        time.set_ellipsize(gtk4::pango::EllipsizeMode::None);
        let subline = label("0 plays \u{00b7} 0 artists", "stats-headline-subtitle");
        let time_block = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        time_block.set_hexpand(true);
        time_block.set_valign(gtk4::Align::End);
        time_block.append(&time);
        time_block.append(&subline);

        let kpis = StatsKpis {
            per_day: StatsKpi::new("PER DAY", false),
            trend: StatsKpi::new("TREND", true),
            pace: StatsKpi::new("PACE", false),
            best_week: StatsKpi::new("BEST WEEK", false),
        };
        let kpi_row = adw::WrapBox::new();
        kpi_row.set_child_spacing(24);
        kpi_row.set_line_spacing(12);
        kpi_row.set_wrap_policy(adw::WrapPolicy::Natural);
        kpi_row.set_valign(gtk4::Align::End);
        for kpi in [&kpis.per_day, &kpis.trend, &kpis.pace, &kpis.best_week] {
            kpi_row.append(&kpi.root);
        }

        let root = adw::WrapBox::new();
        root.set_child_spacing(32);
        root.set_line_spacing(16);
        root.set_natural_line_length(HERO_NATURAL_LINE_LENGTH);
        root.set_wrap_policy(adw::WrapPolicy::Natural);
        root.set_justify(adw::JustifyMode::Fill);
        root.set_justify_last_line(true);
        root.set_valign(gtk4::Align::End);
        root.append(&time_block);
        root.append(&kpi_row);

        Self {
            root,
            time,
            subline,
            time_block,
            kpis,
        }
    }

    pub(super) fn set_data(
        &self,
        snapshot: &StatsSnapshot,
        period: StatsPeriod,
        header: &StatsHeader,
    ) {
        self.root.set_visible(true);
        self.time
            .set_label(&strings::stats_duration(snapshot.hero.total_ms));
        self.subline.set_label(&strings::stats_hero_subline(
            snapshot.hero.plays,
            snapshot.hero.artists,
        ));
        self.kpis.per_day.show(
            &strings::stats_per_day(snapshot.hero.average_ms_per_day),
            None,
            None,
        );

        self.render_comparison(snapshot, period, header);

        if let (Some(pace), StatsPeriod::YearToDate(year)) =
            (snapshot.hero.pace_projection_ms, period)
        {
            self.kpis
                .pace
                .label
                .set_label(&strings::stats_pace_label(year));
            self.kpis
                .pace
                .show(&strings::stats_duration(pace), None, None);
        } else {
            self.kpis.pace.hide();
        }

        if let Some(best_week) = &snapshot.best_week {
            self.kpis.best_week.show(
                &strings::stats_best_week(best_week.start, best_week.total_ms),
                None,
                None,
            );
        } else {
            self.kpis.best_week.hide();
        }
    }

    pub(super) fn clear(&self, header: &StatsHeader) {
        self.time.set_label("");
        self.subline.set_label("");
        self.kpis.per_day.hide();
        self.kpis.trend.hide();
        self.kpis.pace.hide();
        self.kpis.best_week.hide();
        header.hide_new_badge();
        self.root.set_visible(false);
    }

    fn render_comparison(
        &self,
        snapshot: &StatsSnapshot,
        period: StatsPeriod,
        header: &StatsHeader,
    ) {
        let copy = snapshot
            .hero
            .comparison_presentation
            .and_then(|presentation| strings::comparison_copy(presentation, period));
        if snapshot.hero.comparison_presentation == Some(ComparisonPresentation::New) {
            let copy = copy.expect("a new comparison period always has copy");
            header.show_new_badge(&strings::stats_new_badge(), &copy.tooltip);
            self.kpis.trend.hide();
            return;
        }
        header.hide_new_badge();

        let Some(previous_ms) = snapshot.hero.previous_ms else {
            self.kpis.trend.hide();
            return;
        };
        let Some(copy) = copy else {
            self.kpis.trend.hide();
            return;
        };
        let delta = snapshot.hero.total_ms - previous_ms;
        self.kpis.trend.icon.set_icon_name(Some(if delta >= 0 {
            "pan-up-symbolic"
        } else {
            "pan-down-symbolic"
        }));
        self.kpis.trend.show(
            &strings::stats_trend_delta(delta),
            strings::stats_trend_reference(period).as_deref(),
            Some(&copy.tooltip),
        );
    }
}

fn label(text: &str, class: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class(class);
    label.set_xalign(0.0);
    label
}
