//! 12-month listening activity bar chart rendered in a `gtk4::DrawingArea`.
//!
//! Follows the same `set_draw_func` + cairo pattern as
//! `player_bar/waveform_seek.rs`: colours come from the widget's own CSS
//! `color` (set to `@accent_color` via `.stats-chart`), so the chart
//! recolors with the active theme.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use super::stats_chart_math::{
    is_current_month, normalize_bars, short_month_label, BAR_GAP_FRACTION, MIN_BAR_FRACTION,
    PAST_MONTH_ALPHA,
};

pub(super) const CHART_CSS_CLASS: &str = "stats-chart";
const CHART_HEIGHT: i32 = 160;
const LABEL_AREA_HEIGHT: f64 = 20.0;
const LABEL_FONT_SIZE: f64 = 9.0;

struct ChartData {
    /// Normalised 0..1 bar heights (one per month bucket).
    bars: Vec<f64>,
    /// Short month labels (e.g. "Jan", "Feb").
    labels: Vec<String>,
}

pub(super) struct StatsChart {
    area: gtk4::DrawingArea,
    data: Rc<RefCell<ChartData>>,
}

impl StatsChart {
    pub(super) fn new() -> Self {
        let area = gtk4::DrawingArea::new();
        area.add_css_class(CHART_CSS_CLASS);
        area.set_hexpand(true);
        area.set_content_height(CHART_HEIGHT);
        area.set_valign(gtk4::Align::Fill);

        let data = Rc::new(RefCell::new(ChartData {
            bars: Vec::new(),
            labels: Vec::new(),
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

    /// Updates the chart with fresh timeseries data. `year_months` are
    /// `"YYYY-MM"` strings; `values` are the per-month totals (ms or listen
    /// count — the chart only cares about relative heights).
    pub(super) fn set_data(&self, year_months: &[String], values: &[i64]) {
        let bars = normalize_bars(values);
        let labels = year_months
            .iter()
            .map(|ym| short_month_label(ym).to_string())
            .collect();
        *self.data.borrow_mut() = ChartData { bars, labels };
        self.area.queue_draw();
    }
}

fn draw(
    area: &gtk4::DrawingArea,
    cr: &gtk4::cairo::Context,
    width: i32,
    height: i32,
    data: &ChartData,
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

    for (i, &magnitude) in data.bars.iter().enumerate() {
        let effective = if magnitude > 0.0 {
            magnitude.max(MIN_BAR_FRACTION)
        } else {
            0.0
        };
        let bar_h = effective * bar_area_height;
        let x = i as f64 * slot + (slot - bar_w) / 2.0;
        let y = bar_area_height - bar_h;

        let alpha = if is_current_month(i, count) {
            base_alpha
        } else {
            base_alpha * PAST_MONTH_ALPHA
        };
        cr.set_source_rgba(r, g, b, alpha);

        if bar_h > 0.0 {
            rounded_rect(cr, x, y, bar_w, bar_h, radius);
            let _ = cr.fill();
        }

        // Month label below the bar
        if let Some(label) = data.labels.get(i) {
            let label_alpha = if is_current_month(i, count) {
                base_alpha * 0.9
            } else {
                base_alpha * 0.45
            };
            cr.set_source_rgba(r, g, b, label_alpha);
            cr.set_font_size(LABEL_FONT_SIZE);
            let extents = match cr.text_extents(label) {
                Ok(e) => e,
                Err(_) => continue,
            };
            let label_x = x + (bar_w - extents.width()) / 2.0;
            let label_y = h - 4.0;
            cr.move_to(label_x, label_y);
            let _ = cr.show_text(label);
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
