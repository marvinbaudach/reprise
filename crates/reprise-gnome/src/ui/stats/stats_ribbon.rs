//! Editorial listening-time area ribbon rendered with Cairo.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;
use reprise_core::library::stats_period::PeriodRange;

use super::stats_ribbon_math::{bucket_at_x, ribbon_layout, Point, RibbonLayout};

pub(in crate::ui) const RIBBON_CSS_CLASS: &str = "stats-ribbon";
/// The secondary caption tone the axis labels borrow (CONTRAST: the accent
/// role belongs to data, not to axis descriptions).
const AXIS_LABEL_CSS_CLASS: &str = "stats-item-subtitle";
const RIBBON_HEIGHT: i32 = 150;
const LABEL_HEIGHT: f64 = 24.0;
const MARKER_RADIUS: f64 = 4.0;

#[derive(Default)]
struct RibbonData {
    labels: Vec<String>,
    values: Vec<i64>,
    open_index: Option<usize>,
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
                            "{}: {}",
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

    pub(in crate::ui) fn set_data(&self, period: &PeriodRange, values: &[i64]) {
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
            values,
            open_index: period.buckets.iter().position(|bucket| bucket.open),
        };
        self.area.queue_draw();
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
    draw_line(context, &layout, red, green, blue, alpha);
    draw_markers(context, &layout, red, green, blue, alpha);
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
    context.move_to(first.x, baseline);
    for point in &layout.points {
        context.line_to(point.x, point.y);
    }
    context.line_to(width, baseline);
    context.close_path();
    let gradient = gtk4::cairo::LinearGradient::new(0.0, 0.0, 0.0, baseline);
    gradient.add_color_stop_rgba(0.0, red, green, blue, alpha * 0.48);
    gradient.add_color_stop_rgba(1.0, red, green, blue, alpha * 0.04);
    let _ = context.set_source(&gradient);
    let _ = context.fill();
}

fn draw_line(
    context: &gtk4::cairo::Context,
    layout: &RibbonLayout,
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
    context.move_to(first.x, first.y);
    let solid_end = layout
        .open_index
        .filter(|index| *index > 0)
        .map_or(layout.points.len() - 1, |index| index - 1);
    for point in layout.points.iter().take(solid_end + 1).skip(1) {
        context.line_to(point.x, point.y);
    }
    let _ = context.stroke();

    if let Some(open_index) = layout.open_index.filter(|index| *index > 0) {
        let start = layout.points[open_index - 1];
        let end = layout.points[open_index];
        context.set_dash(&[5.0, 4.0], 0.0);
        context.move_to(start.x, start.y);
        context.line_to(end.x, end.y);
        let _ = context.stroke();
        context.set_dash(&[], 0.0);
    }
}

fn draw_markers(
    context: &gtk4::cairo::Context,
    layout: &RibbonLayout,
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
) {
    if let Some(point) = marker(layout, layout.peak_index) {
        context.set_source_rgba(red, green, blue, alpha);
        context.arc(point.x, point.y, MARKER_RADIUS, 0.0, std::f64::consts::TAU);
        let _ = context.fill();
    }
    if let Some(point) = marker(layout, layout.open_index) {
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
    let count = data.labels.len();
    let stride = count.div_ceil(8).max(1);
    context.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()),
    );
    context.set_font_size(9.0);
    for (index, label) in data.labels.iter().enumerate() {
        if index % stride != 0 && index + 1 != count {
            continue;
        }
        let x = if count <= 1 {
            0.0
        } else {
            index as f64 * width / (count - 1) as f64
        };
        context.move_to(x.min(width - 24.0).max(0.0), height - 5.0);
        let _ = context.show_text(label);
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
    use gtk4::prelude::*;

    use super::*;

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
