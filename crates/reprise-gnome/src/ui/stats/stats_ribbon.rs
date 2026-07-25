//! Editorial listening-time area ribbon rendered with Cairo.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use chrono::{NaiveDate, TimeZone};
use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::stats_period::{Granularity, PeriodRange};
use reprise_core::library::stats_snapshot::BestWeek;

use super::stats_ribbon_math::{
    best_week_bucket_index, bucket_at_x, month_ticks, reveal_clip_width, ribbon_layout, Point,
    RibbonLayout,
};

pub(in crate::ui) const RIBBON_CSS_CLASS: &str = "stats-ribbon";
/// The secondary caption tone the axis labels borrow (CONTRAST: the accent
/// role belongs to data, not to axis descriptions).
const AXIS_LABEL_CSS_CLASS: &str = "stats-item-subtitle";
const RIBBON_HEIGHT: i32 = 150;
const LABEL_HEIGHT: f64 = 24.0;
const MARKER_RADIUS: f64 = 4.0;

struct RibbonData {
    labels: Vec<String>,
    bucket_starts: Vec<Option<NaiveDate>>,
    values: Vec<i64>,
    granularity: Granularity,
    open_index: Option<usize>,
    best_week: Option<BestWeek>,
    reveal_fraction: f64,
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
            reveal_fraction: 1.0,
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

        let data = Rc::new(RefCell::new(RibbonData::default()));
        area.set_draw_func({
            let data = data.clone();
            let axis_probe = axis_probe.clone();
            move |area, context, width, height| {
                draw(
                    area,
                    context,
                    width,
                    height,
                    &data.borrow(),
                    axis_probe.color(),
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
                            format_duration(*data.values.get(index)?)
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

        Self {
            root,
            area,
            axis_probe,
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
            reveal_fraction: 1.0,
        };
        self.set_reveal_fraction(1.0);
    }

    pub(in crate::ui) fn set_reveal_fraction(&self, fraction: f64) {
        self.data.borrow_mut().reveal_fraction = fraction.clamp(0.0, 1.0);
        self.area.queue_draw();
    }

    #[cfg(test)]
    pub(in crate::ui) fn reveal_fraction(&self) -> f64 {
        self.data.borrow().reveal_fraction
    }
}

fn draw(
    area: &gtk4::DrawingArea,
    context: &gtk4::cairo::Context,
    width: i32,
    height: i32,
    data: &RibbonData,
    axis_color: gtk4::gdk::RGBA,
) {
    if data.values.is_empty() || width <= 0 || height <= 0 {
        return;
    }
    let plot_height = f64::from(height) - LABEL_HEIGHT;
    if plot_height <= 0.0 {
        return;
    }
    let width = f64::from(width);
    let layout = ribbon_layout(&data.values, width, plot_height, data.open_index);
    let color = area.color();
    let (red, green, blue, alpha) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()),
    );

    let _ = context.save();
    context.rectangle(
        0.0,
        0.0,
        reveal_clip_width(width, data.reveal_fraction),
        plot_height,
    );
    context.clip();
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
    draw_best_week_marker(
        context,
        &layout,
        data,
        width,
        plot_height,
        red,
        green,
        blue,
        alpha,
    );
    draw_open_marker(context, &layout, red, green, blue, alpha);
    let _ = context.restore();
    draw_labels(context, data, width, f64::from(height), axis_color);
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
fn draw_best_week_marker(
    context: &gtk4::cairo::Context,
    layout: &RibbonLayout,
    data: &RibbonData,
    plot_width: f64,
    plot_height: f64,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    let index = best_week_bucket_index(
        &data.bucket_starts,
        data.granularity,
        data.best_week.as_ref().map(|week| week.start),
    );
    let Some(point) = marker(layout, index) else {
        return;
    };
    context.set_source_rgba(red, green, blue, alpha);
    context.set_line_width(1.0);
    context.set_dash(&[4.0, 4.0], 0.0);
    context.move_to(point.x, 0.0);
    context.line_to(point.x, plot_height);
    let _ = context.stroke();
    context.set_dash(&[], 0.0);
    if let Some(best_week) = &data.best_week {
        context.move_to((point.x + 5.0).min((plot_width - 110.0).max(0.0)), 12.0);
        let _ = context.show_text(&format!(
            "best week · {}",
            format_duration(best_week.total_ms)
        ));
        context.new_path();
    }
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
    for tick in month_ticks(&data.bucket_starts) {
        let x = if count <= 1 {
            0.0
        } else {
            tick.index as f64 * width / (count - 1) as f64
        };
        context.move_to(x.min(width - 24.0).max(0.0), height - 5.0);
        let _ = context.show_text(&tick.label);
        context.new_path();
    }
}

fn marker(layout: &RibbonLayout, index: Option<usize>) -> Option<Point> {
    layout.points.get(index?).copied()
}

fn format_duration(milliseconds: i64) -> String {
    let minutes = milliseconds.max(0) / 60_000;
    format!("{} h {} min", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use gtk4::cairo::{Format, ImageSurface};
    use gtk4::prelude::*;

    use super::*;

    fn pixel_has_ink(surface: &mut ImageSurface, x: i32, y: i32) -> bool {
        surface.flush();
        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        let offset = y as usize * stride + x as usize * 4;
        u32::from_ne_bytes(data[offset..offset + 4].try_into().unwrap()) >> 24 != 0
    }

    #[test]
    fn open_marker_starts_a_fresh_cairo_path() {
        let mut surface = ImageSurface::create(Format::ARgb32, 100, 100).unwrap();
        {
            let context = gtk4::cairo::Context::new(&surface).unwrap();
            context.move_to(0.0, 0.0);
            let layout = RibbonLayout {
                points: vec![Point { x: 80.0, y: 80.0 }],
                open_index: Some(0),
            };

            draw_open_marker(&context, &layout, 1.0, 1.0, 1.0, 1.0);
        }

        assert!(
            !(38..=42).any(|x| (38..=42).any(|y| pixel_has_ink(&mut surface, x, y))),
            "the marker must not stroke a line from Cairo's previous current point"
        );
    }

    #[test]
    fn one_bucket_draws_a_full_width_fill_and_line() {
        let layout = RibbonLayout {
            points: vec![Point { x: 50.0, y: 25.0 }],
            open_index: None,
        };
        let mut fill_surface = ImageSurface::create(Format::ARgb32, 100, 100).unwrap();
        {
            let context = gtk4::cairo::Context::new(&fill_surface).unwrap();
            draw_fill(&context, &layout, 100.0, 90.0, 1.0, 1.0, 1.0, 1.0);
        }
        let mut line_surface = ImageSurface::create(Format::ARgb32, 100, 100).unwrap();
        {
            let context = gtk4::cairo::Context::new(&line_surface).unwrap();
            draw_line(&context, &layout, 100.0, 1.0, 1.0, 1.0, 1.0);
        }

        assert!(
            pixel_has_ink(&mut fill_surface, 10, 50),
            "the fill must extend left of the sole bucket point"
        );
        assert!(
            pixel_has_ink(&mut line_surface, 10, 25),
            "the sole bucket must render as a line, not an invisible move-to"
        );
    }

    /// CONTRAST: the accent belongs to the data. The axis descriptions take the
    /// same secondary tone as every other caption, not the teal of the ribbon.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn ribbon_axis_labels_do_not_borrow_the_data_accent() {
        gtk4::init().unwrap();
        crate::ui::style::install();
        let ribbon = StatsRibbon::new();
        let window = gtk4::Window::new();
        window.set_child(Some(ribbon.widget()));
        window.present();
        while gtk4::glib::MainContext::default().iteration(false) {}

        let data_color = ribbon.area.color();
        let axis_color = ribbon.axis_probe.color();

        assert_ne!(
            (data_color.red(), data_color.green(), data_color.blue()),
            (axis_color.red(), axis_color.green(), axis_color.blue()),
            "axis labels resolved to the data accent {data_color}"
        );
        window.close();
    }
}
