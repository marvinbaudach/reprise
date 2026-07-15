//! 24-hour listening activity bar chart rendered in a `gtk4::DrawingArea`.
//!
//! Structurally identical to `StatsChart` (12-month chart) but draws 24
//! bars (one per hour of the day) with axis labels at 0, 6, 12, 18, 24
//! and an optional "peak HH:00" annotation in the top-right corner.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::stats_chart_math::{
    expand_hourly, format_peak_hour, normalize_bars, peak_hour, BAR_GAP_FRACTION,
    MIN_BAR_FRACTION,
};

pub(super) const HOURLY_CHART_CSS_CLASS: &str = "stats-chart";
const CHART_HEIGHT: i32 = 160;
const LABEL_AREA_HEIGHT: f64 = 20.0;
const LABEL_FONT_SIZE: f64 = 9.0;
const ANNOTATION_FONT_SIZE: f64 = 10.0;

/// The hour values that get a label on the x-axis.
const LABELED_HOURS: [usize; 5] = [0, 6, 12, 18, 23];

struct HourlyData {
    /// Normalised 0..1 bar heights (always 24 entries).
    bars: Vec<f64>,
    /// "peak HH:00" annotation text (empty when all bars are zero).
    peak_annotation: String,
}

pub(super) struct HourlyChart {
    area: gtk4::DrawingArea,
    data: Rc<RefCell<HourlyData>>,
}

impl HourlyChart {
    pub(super) fn new() -> Self {
        let area = gtk4::DrawingArea::new();
        area.add_css_class(HOURLY_CHART_CSS_CLASS);
        area.set_hexpand(true);
        area.set_content_height(CHART_HEIGHT);
        area.set_valign(gtk4::Align::Fill);

        let data = Rc::new(RefCell::new(HourlyData {
            bars: Vec::new(),
            peak_annotation: String::new(),
        }));

        area.set_draw_func({
            let data = data.clone();
            move |area, cr, width, height| draw(area, cr, width, height, &data.borrow())
        });

        Self { area, data }
    }

    pub(super) fn widget(&self) -> &gtk4::DrawingArea {
        &self.area
    }

    /// Updates the chart with sparse hourly data from `listening_by_hour`.
    /// `sparse` is a list of `(hour, listens)` pairs — only hours with
    /// events are present; this method expands to a full 24-slot array.
    pub(super) fn set_data(&self, sparse: &[(u8, i64)]) {
        let full = expand_hourly(sparse);
        let peak = peak_hour(&full);
        let has_data = full.iter().any(|&v| v > 0);
        let peak_annotation = if has_data {
            format!("peak {}", format_peak_hour(peak))
        } else {
            String::new()
        };
        let bars = normalize_bars(&full);
        *self.data.borrow_mut() = HourlyData {
            bars,
            peak_annotation,
        };
        self.area.queue_draw();
    }
}

fn draw(
    area: &gtk4::DrawingArea,
    cr: &gtk4::cairo::Context,
    width: i32,
    height: i32,
    data: &HourlyData,
) {
    let count = data.bars.len();
    if count == 0 || width <= 0 || height <= 0 {
        return;
    }
    let color = area.color();
    let (r, g, b, base_alpha) = (
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        f64::from(color.alpha()),
    );
    let w = f64::from(width);
    let h = f64::from(height);
    let bar_area_height = h - LABEL_AREA_HEIGHT;
    if bar_area_height <= 0.0 {
        return;
    }
    let slot = w / count as f64;
    let bar_w = slot * (1.0 - BAR_GAP_FRACTION);
    let radius = (bar_w * 0.3).min(4.0);

    // Draw peak annotation in top-right corner.
    if !data.peak_annotation.is_empty() {
        cr.set_source_rgba(r, g, b, base_alpha * 0.6);
        cr.set_font_size(ANNOTATION_FONT_SIZE);
        if let Ok(extents) = cr.text_extents(&data.peak_annotation) {
            let ann_x = w - extents.width() - 4.0;
            let ann_y = ANNOTATION_FONT_SIZE + 2.0;
            cr.move_to(ann_x, ann_y);
            let _ = cr.show_text(&data.peak_annotation);
        }
    }

    for (i, &magnitude) in data.bars.iter().enumerate() {
        let effective = if magnitude > 0.0 {
            magnitude.max(MIN_BAR_FRACTION)
        } else {
            0.0
        };
        let bar_h = effective * bar_area_height;
        let x = i as f64 * slot + (slot - bar_w) / 2.0;
        let y = bar_area_height - bar_h;

        // Bars use uniform alpha (no "current" highlight for hourly).
        cr.set_source_rgba(r, g, b, base_alpha * 0.75);

        if bar_h > 0.0 {
            rounded_rect(cr, x, y, bar_w, bar_h, radius);
            let _ = cr.fill();
        }

        // Hour label below the bar — only for labeled positions.
        if LABELED_HOURS.contains(&i) {
            let label = if i == 23 { "24" } else { &i.to_string() };
            cr.set_source_rgba(r, g, b, base_alpha * 0.45);
            cr.set_font_size(LABEL_FONT_SIZE);
            if let Ok(extents) = cr.text_extents(label) {
                let label_x = x + (bar_w - extents.width()) / 2.0;
                let label_y = h - 4.0;
                cr.move_to(label_x, label_y);
                let _ = cr.show_text(label);
            }
        }
    }
}

/// Draws a rectangle with rounded top corners (bottom corners stay square,
/// sitting on the label baseline).
fn rounded_rect(cr: &gtk4::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.new_path();
    cr.move_to(x, y + h);
    cr.line_to(x, y + r);
    cr.arc(
        x + r,
        y + r,
        r,
        std::f64::consts::PI,
        1.5 * std::f64::consts::PI,
    );
    cr.arc(
        x + w - r,
        y + r,
        r,
        1.5 * std::f64::consts::PI,
        2.0 * std::f64::consts::PI,
    );
    cr.line_to(x + w, y + h);
    cr.close_path();
}
