//! Editorial listening-time area ribbon rendered with Cairo.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{NaiveDate, TimeZone};
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::stats_period::{Granularity, PeriodRange};
use reprise_core::library::stats_snapshot::BestWeek;

use super::stats_ribbon_math::{
    axis_ticks, bar_layout, best_week_bucket_index, bucket_at_x, reveal_clip_width, ribbon_layout,
    Point, RibbonLayout,
};
use crate::ui::strings;

pub(in crate::ui) const RIBBON_CSS_CLASS: &str = "stats-ribbon";
/// The secondary caption tone the axis labels borrow (CONTRAST: the accent
/// role belongs to data, not to axis descriptions).
const AXIS_LABEL_CSS_CLASS: &str = "stats-item-subtitle";
const BEST_WEEK_CSS_CLASS: &str = "stats-ribbon-best";
const BASELINE_CSS_CLASS: &str = "stats-ribbon-baseline";
const RIBBON_HEIGHT: i32 = 150;
const SPARSE_RIBBON_HEIGHT: i32 = 128;
const LABEL_HEIGHT: f64 = 24.0;
const MARKER_RADIUS: f64 = 4.0;

struct RibbonData {
    labels: Vec<String>,
    bucket_starts: Vec<Option<NaiveDate>>,
    values: Vec<i64>,
    granularity: Granularity,
    open_index: Option<usize>,
    best_week: Option<BestWeek>,
    sparse_weeks: bool,
    since_label: Option<String>,
    reveal_fraction: f64,
    marker_opacity: f64,
}

impl Default for RibbonData {
    fn default() -> Self {
        Self {
            labels: Vec::new(),
            bucket_starts: Vec::new(),
            values: Vec::new(),
            granularity: Granularity::Day,
            open_index: None,
            best_week: None,
            sparse_weeks: false,
            since_label: None,
            reveal_fraction: 1.0,
            marker_opacity: 1.0,
        }
    }
}

#[derive(Clone)]
pub(in crate::ui) struct StatsRibbon {
    root: gtk4::Box,
    area: gtk4::DrawingArea,
    /// Cairo can only read one colour off a widget, and the area's is the
    /// accent the data is drawn in. The axis labels are not data, so they take
    /// the secondary text tone — read off this hidden label, which carries the
    /// same CSS class as every other secondary caption on the page.
    #[cfg_attr(not(test), allow(dead_code))]
    axis_probe: gtk4::Label,
    #[cfg_attr(not(test), allow(dead_code))]
    best_week_probe: gtk4::Label,
    #[cfg_attr(not(test), allow(dead_code))]
    baseline_probe: gtk4::Label,
    data: Rc<RefCell<RibbonData>>,
}

impl StatsRibbon {
    pub(in crate::ui) fn new() -> Self {
        let area = gtk4::DrawingArea::new();
        area.add_css_class(RIBBON_CSS_CLASS);
        area.set_hexpand(true);
        area.set_content_height(RIBBON_HEIGHT);

        let axis_probe = gtk4::Label::new(None);
        axis_probe.add_css_class(AXIS_LABEL_CSS_CLASS);
        axis_probe.set_visible(false);
        let best_week_probe = gtk4::Label::new(None);
        best_week_probe.add_css_class(BEST_WEEK_CSS_CLASS);
        best_week_probe.set_visible(false);
        let baseline_probe = gtk4::Label::new(None);
        baseline_probe.add_css_class(BASELINE_CSS_CLASS);
        baseline_probe.set_visible(false);

        let data = Rc::new(RefCell::new(RibbonData::default()));
        area.set_draw_func({
            let data = data.clone();
            let axis_probe = axis_probe.clone();
            let best_week_probe = best_week_probe.clone();
            let baseline_probe = baseline_probe.clone();
            move |area, context, width, height| {
                draw(
                    area,
                    context,
                    width,
                    height,
                    &data.borrow(),
                    DrawColors {
                        axis: axis_probe.color(),
                        best_week: best_week_probe.color(),
                        baseline: baseline_probe.color(),
                    },
                );
            }
        });

        let hovered = Rc::new(Cell::new(None));
        let motion = gtk4::EventControllerMotion::new();
        // The area owns the controller, so both closures have to hold the
        // area weakly — a strong clone would keep the widget alive forever.
        motion.connect_motion(glib::clone!(
            #[weak]
            area,
            #[strong]
            data,
            #[strong]
            hovered,
            move |_, x, _| {
                // The borrow ends before the setter: `set_tooltip_text` can
                // re-enter through a query-tooltip handler, and a live borrow
                // would turn that into a `BorrowMutError`.
                let tooltip = {
                    let data = data.borrow();
                    let index = bucket_at_x(x, f64::from(area.width()), data.values.len());
                    if index == hovered.get() {
                        return;
                    }
                    hovered.set(index);
                    index.and_then(|index| {
                        Some(format!(
                            "{}{} · {}",
                            if data.granularity == Granularity::Week {
                                "Week of "
                            } else {
                                ""
                            },
                            data.labels.get(index)?,
                            strings::stats_duration(*data.values.get(index)?)
                        ))
                    })
                };
                area.set_tooltip_text(tooltip.as_deref());
            }
        ));
        motion.connect_leave(glib::clone!(
            #[weak]
            area,
            move |_| {
                hovered.set(None);
                area.set_tooltip_text(None);
            }
        ));
        area.add_controller(motion);

        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        root.append(&area);
        root.append(&axis_probe);
        root.append(&best_week_probe);
        root.append(&baseline_probe);

        Self {
            root,
            area,
            axis_probe,
            best_week_probe,
            baseline_probe,
            data,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_data(
        &self,
        period: &PeriodRange,
        values: &[i64],
        best_week: Option<&BestWeek>,
    ) {
        let values = period
            .buckets
            .iter()
            .enumerate()
            .map(|(index, _)| values.get(index).copied().unwrap_or(0))
            .collect::<Vec<_>>();
        let sparse_weeks = period.sparse_weeks;
        self.area.set_content_height(if sparse_weeks {
            SPARSE_RIBBON_HEIGHT
        } else {
            RIBBON_HEIGHT
        });
        let since_label = (sparse_weeks && period.buckets.len() >= 10)
            .then(|| period.buckets.first())
            .flatten()
            .filter(|bucket| bucket.start_unix > period.start_unix)
            .and_then(|bucket| {
                chrono::Local
                    .timestamp_opt(bucket.start_unix, 0)
                    .earliest()
                    .map(|date| format!("since {}", date.format("%b %Y")))
            });
        *self.data.borrow_mut() = RibbonData {
            labels: period
                .buckets
                .iter()
                .map(|bucket| bucket.label.clone())
                .collect(),
            bucket_starts: period
                .buckets
                .iter()
                .map(|bucket| {
                    chrono::Local
                        .timestamp_opt(bucket.start_unix, 0)
                        .earliest()
                        .map(|date| date.date_naive())
                })
                .collect(),
            values,
            granularity: period.granularity,
            open_index: period.buckets.iter().position(|bucket| bucket.open),
            best_week: best_week.cloned(),
            sparse_weeks,
            since_label,
            reveal_fraction: 1.0,
            marker_opacity: 1.0,
        };
        self.set_reveal_fraction(1.0);
    }

    pub(in crate::ui) fn set_reveal_fraction(&self, fraction: f64) {
        self.data.borrow_mut().reveal_fraction = fraction.clamp(0.0, 1.0);
        self.area.queue_draw();
    }

    pub(in crate::ui) fn set_marker_opacity(&self, opacity: f64) {
        self.data.borrow_mut().marker_opacity = opacity.clamp(0.0, 1.0);
        self.area.queue_draw();
    }

    #[cfg(test)]
    pub(in crate::ui) fn reveal_fraction(&self) -> f64 {
        self.data.borrow().reveal_fraction
    }

    #[cfg(test)]
    pub(in crate::ui) fn marker_opacity(&self) -> f64 {
        self.data.borrow().marker_opacity
    }
}

#[derive(Clone, Copy)]
struct DrawColors {
    axis: gtk4::gdk::RGBA,
    best_week: gtk4::gdk::RGBA,
    baseline: gtk4::gdk::RGBA,
}

fn draw(
    area: &gtk4::DrawingArea,
    context: &gtk4::cairo::Context,
    width: i32,
    height: i32,
    data: &RibbonData,
    colors: DrawColors,
) {
    if data.values.is_empty() || width <= 0 || height <= 0 {
        return;
    }
    let plot_height = f64::from(height) - LABEL_HEIGHT;
    if plot_height <= 0.0 {
        return;
    }
    let width = f64::from(width);
    let layout = if data.sparse_weeks {
        bar_layout(&data.values, width, plot_height, data.open_index)
    } else {
        ribbon_layout(&data.values, width, plot_height, data.open_index)
    };
    let color = area.color();
    let (red, green, blue, alpha) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()),
    );
    let best_index = best_week_bucket_index(
        &data.bucket_starts,
        data.granularity,
        data.best_week.as_ref().map(|week| week.start),
    );

    let _ = context.save();
    context.rectangle(
        0.0,
        0.0,
        reveal_clip_width(width, data.reveal_fraction),
        plot_height,
    );
    context.clip();
    if data.sparse_weeks {
        draw_bars(
            context,
            &layout,
            width,
            plot_height,
            best_index,
            BarColors {
                standard: color,
                best: colors.best_week,
                baseline: colors.baseline,
            },
            data.marker_opacity,
        );
    } else {
        draw_fill(
            context,
            &layout,
            width,
            plot_height,
            red,
            green,
            blue,
            alpha,
        );
        draw_line(context, &layout, width, red, green, blue, alpha);
    }
    draw_best_week_highlight(
        context,
        &layout,
        data,
        width,
        plot_height,
        red,
        green,
        blue,
        alpha,
        colors.best_week,
    );
    draw_open_marker(context, &layout, red, green, blue, alpha);
    let _ = context.restore();
    draw_labels(context, data, width, f64::from(height), colors.axis);
}

#[allow(clippy::too_many_arguments)]
fn draw_fill(
    context: &gtk4::cairo::Context,
    layout: &RibbonLayout,
    width: f64,
    baseline: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let Some(first) = layout.points.first() else {
        return;
    };
    if layout.points.len() == 1 {
        context.move_to(0.0, baseline);
        context.line_to(0.0, first.y);
        context.line_to(width, first.y);
    } else {
        context.move_to(first.x, baseline);
        for point in &layout.points {
            context.line_to(point.x, point.y);
        }
    }
    context.line_to(width, baseline);
    context.close_path();
    let gradient = gtk4::cairo::LinearGradient::new(0.0, 0.0, 0.0, baseline);
    gradient.add_color_stop_rgba(0.0, red, green, blue, alpha * 0.30);
    gradient.add_color_stop_rgba(1.0, red, green, blue, 0.0);
    let _ = context.set_source(&gradient);
    let _ = context.fill();
}

fn draw_line(
    context: &gtk4::cairo::Context,
    layout: &RibbonLayout,
    width: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let Some(first) = layout.points.first() else {
        return;
    };
    context.set_source_rgba(red, green, blue, alpha);
    context.set_line_width(2.0);
    if layout.points.len() == 1 {
        context.move_to(0.0, first.y);
        context.line_to(width, first.y);
    } else {
        context.move_to(first.x, first.y);
        for point in layout.points.iter().skip(1) {
            context.line_to(point.x, point.y);
        }
    }
    let _ = context.stroke();
}

#[allow(clippy::too_many_arguments)]
fn draw_bars(
    context: &gtk4::cairo::Context,
    layout: &RibbonLayout,
    plot_width: f64,
    baseline: f64,
    best_index: Option<usize>,
    colors: BarColors,
    highlight_opacity: f64,
) {
    if layout.points.is_empty() {
        return;
    }
    let slot_width = plot_width / layout.points.len() as f64;
    let bar_width = (slot_width * 0.62).clamp(2.0, 28.0).min(slot_width);
    set_source_color(context, colors.baseline, 1.0);
    context.set_line_width(1.0);
    context.move_to(0.0, baseline - 0.5);
    context.line_to(plot_width, baseline - 0.5);
    let _ = context.stroke();
    for (index, point) in layout.points.iter().enumerate() {
        let color = if Some(index) == best_index {
            blend_color(colors.standard, colors.best, highlight_opacity)
        } else {
            colors.standard
        };
        set_source_color(context, color, 0.82);
        let height = (baseline - point.y).max(0.0);
        let (top, height) = if height == 0.0 {
            (baseline - 2.0, 2.0)
        } else {
            (point.y, height)
        };
        context.rectangle(point.x - bar_width / 2.0, top, bar_width, height);
        let _ = context.fill();
    }
}

#[derive(Clone, Copy)]
struct BarColors {
    standard: gtk4::gdk::RGBA,
    best: gtk4::gdk::RGBA,
    baseline: gtk4::gdk::RGBA,
}

fn set_source_color(context: &gtk4::cairo::Context, color: gtk4::gdk::RGBA, opacity: f64) {
    context.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()) * opacity,
    );
}

fn blend_color(from: gtk4::gdk::RGBA, to: gtk4::gdk::RGBA, fraction: f64) -> gtk4::gdk::RGBA {
    let fraction = fraction.clamp(0.0, 1.0) as f32;
    gtk4::gdk::RGBA::new(
        from.red() + (to.red() - from.red()) * fraction,
        from.green() + (to.green() - from.green()) * fraction,
        from.blue() + (to.blue() - from.blue()) * fraction,
        from.alpha() + (to.alpha() - from.alpha()) * fraction,
    )
}

#[allow(clippy::too_many_arguments)]
fn draw_best_week_highlight(
    context: &gtk4::cairo::Context,
    layout: &RibbonLayout,
    data: &RibbonData,
    plot_width: f64,
    _plot_height: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
    best_week_color: gtk4::gdk::RGBA,
) {
    let index = best_week_bucket_index(
        &data.bucket_starts,
        data.granularity,
        data.best_week.as_ref().map(|week| week.start),
    );
    let Some(point) = marker(layout, index) else {
        return;
    };
    let base_color = gtk4::gdk::RGBA::new(red as f32, green as f32, blue as f32, alpha as f32);
    set_source_color(
        context,
        blend_color(base_color, best_week_color, data.marker_opacity),
        data.marker_opacity,
    );
    if !data.sparse_weeks {
        context.new_path();
        context.arc(point.x, point.y, 3.0, 0.0, std::f64::consts::TAU);
        let _ = context.fill();
    }
    if let Some(best_week) = &data.best_week {
        let copy = format!(
            "best week · {}",
            strings::stats_duration(best_week.total_ms)
        );
        context.set_font_size(10.0);
        context.move_to(
            best_week_label_x(context, point.x, plot_width, &copy),
            best_week_label_y(point.y),
        );
        let _ = context.show_text(&copy);
        context.new_path();
    }
}

fn best_week_label_x(
    context: &gtk4::cairo::Context,
    marker_x: f64,
    plot_width: f64,
    copy: &str,
) -> f64 {
    let Ok(extents) = context.text_extents(copy) else {
        return marker_x.max(4.0);
    };
    let x = marker_x - extents.x_bearing() - extents.width() / 2.0;
    x.clamp(4.0, (plot_width - extents.width() - 4.0).max(4.0))
}

fn best_week_label_y(bar_top: f64) -> f64 {
    (bar_top - 2.0).max(10.0)
}

fn draw_open_marker(
    context: &gtk4::cairo::Context,
    layout: &RibbonLayout,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    if let Some(point) = marker(layout, layout.open_index) {
        context.new_path();
        context.set_source_rgba(red, green, blue, alpha);
        context.set_line_width(2.0);
        context.arc(point.x, point.y, MARKER_RADIUS, 0.0, std::f64::consts::TAU);
        let _ = context.stroke();
    }
}

fn draw_labels(
    context: &gtk4::cairo::Context,
    data: &RibbonData,
    width: f64,
    height: f64,
    color: gtk4::gdk::RGBA,
) {
    let count = data.values.len();
    context.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()),
    );
    context.set_font_size(9.0);
    let short_week_axis = data.granularity == Granularity::Week && count < 10;
    for tick in axis_ticks(&data.bucket_starts, data.granularity) {
        let x = if data.sparse_weeks && count > 0 {
            (tick.index as f64 + 0.5) * width / count as f64
        } else if count <= 1 {
            0.0
        } else {
            tick.index as f64 * width / (count - 1) as f64
        };
        let x = if short_week_axis {
            centered_label_x(context, x, width, &tick.label)
        } else {
            x.min(width - 24.0).max(0.0)
        };
        context.move_to(x, height - 5.0);
        let _ = context.show_text(&tick.label);
        context.new_path();
    }
    if let Some(label) = &data.since_label {
        let label_width = context
            .text_extents(label)
            .map_or(0.0, |extents| extents.width());
        context.move_to((width - label_width).max(0.0), height - 5.0);
        let _ = context.show_text(label);
        context.new_path();
    }
}

fn centered_label_x(context: &gtk4::cairo::Context, center: f64, width: f64, label: &str) -> f64 {
    let Ok(extents) = context.text_extents(label) else {
        return center.max(0.0);
    };
    (center - extents.x_bearing() - extents.width() / 2.0)
        .clamp(0.0, (width - extents.width()).max(0.0))
}

fn marker(layout: &RibbonLayout, index: Option<usize>) -> Option<Point> {
    layout.points.get(index?).copied()
}

#[cfg(test)]
#[path = "stats_ribbon_tests.rs"]
mod tests;
