//! 24-hour listening activity bar chart rendered in a `gtk4::DrawingArea`.
//!
//! Structurally identical to `StatsChart` (12-month chart) but draws 24
//! bars (one per hour of the day) with axis labels at 0, 6, 12, 18, 24
//! and an optional "peak HH:00" annotation in the top-right corner.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;

use reprise_core::library::stats_snapshot::ClockSection;

use super::stats_chart_math::{expand_hourly, normalize_bars, BAR_GAP_FRACTION, MIN_BAR_FRACTION};

pub(in crate::ui) const HOURLY_CHART_CSS_CLASS: &str = "stats-chart";
const CHART_HEIGHT: i32 = 160;
const LABEL_AREA_HEIGHT: f64 = 20.0;
const LABEL_FONT_SIZE: f64 = 9.0;

/// The hour values that get a label on the x-axis.
const LABELED_HOURS: [usize; 5] = [0, 6, 12, 18, 23];

struct HourlyData {
    /// Normalised 0..1 bar heights (always 24 entries).
    bars: Vec<f64>,
    peak_hours: [bool; 24],
}

#[derive(Clone)]
pub(in crate::ui) struct HourlyChart {
    root: gtk4::Box,
    area: gtk4::DrawingArea,
    caption: gtk4::Label,
    data: Rc<RefCell<HourlyData>>,
}

impl HourlyChart {
    pub(in crate::ui) fn new() -> Self {
        let area = gtk4::DrawingArea::new();
        area.add_css_class(HOURLY_CHART_CSS_CLASS);
        area.set_hexpand(true);
        area.set_content_height(CHART_HEIGHT);
        area.set_valign(gtk4::Align::Fill);

        let data = Rc::new(RefCell::new(HourlyData {
            bars: Vec::new(),
            peak_hours: [false; 24],
        }));

        area.set_draw_func({
            let data = data.clone();
            move |area, cr, width, height| draw(area, cr, width, height, &data.borrow())
        });

        let caption = gtk4::Label::new(None);
        caption.add_css_class("stats-item-subtitle");
        caption.set_xalign(0.0);
        let root = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        root.append(&area);
        root.append(&caption);

        Self {
            root,
            area,
            caption,
            data,
        }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Box {
        &self.root
    }

    pub(in crate::ui) fn set_data(&self, section: &ClockSection) {
        let sparse = section
            .hours
            .iter()
            .filter_map(|hour| {
                u8::try_from(hour.hour)
                    .ok()
                    .map(|value| (value, hour.total_ms))
            })
            .collect::<Vec<_>>();
        let full = expand_hourly(&sparse);
        let bars = normalize_bars(&full);
        *self.data.borrow_mut() = HourlyData {
            bars,
            peak_hours: peak_hour_mask(section),
        };
        self.caption.set_label(&section.caption);
        // The caption already names the peak — and it names *every* peak hour,
        // which a single "peak H:00" annotation next to it would contradict as
        // soon as two hours tie.
        self.area.set_tooltip_text(None);
        self.area.queue_draw();
    }
}

fn peak_hour_mask(section: &ClockSection) -> [bool; 24] {
    let mut peaks = [false; 24];
    for hour in &section.peak_hours {
        if let Some(peak) = peaks.get_mut(*hour as usize) {
            *peak = true;
        }
    }
    peaks
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

    for (i, &magnitude) in data.bars.iter().enumerate() {
        let effective = if magnitude > 0.0 {
            magnitude.max(MIN_BAR_FRACTION)
        } else {
            0.0
        };
        let bar_h = effective * bar_area_height;
        let x = i as f64 * slot + (slot - bar_w) / 2.0;
        let y = bar_area_height - bar_h;

        let alpha = if data.peak_hours.get(i).copied().unwrap_or(false) {
            0.95
        } else {
            0.38
        };
        cr.set_source_rgba(r, g, b, base_alpha * alpha);

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

#[cfg(test)]
mod tests {
    use reprise_core::library::stats_screen::HourlyListens;
    use reprise_core::library::stats_snapshot::ClockSection;

    use super::*;

    #[test]
    fn hourly_chart_highlights_peak_hours() {
        let section = ClockSection {
            hours: (0..24)
                .map(|hour| HourlyListens {
                    hour,
                    listens: i64::from(hour == 22 || hour == 23),
                    total_ms: if hour == 22 || hour == 23 { 500 } else { 0 },
                })
                .collect(),
            peak_hours: vec![22, 23],
            caption: "Peak 10 PM\u{2013}11 PM \u{00b7} night owl".to_string(),
        };

        let peaks = peak_hour_mask(&section);
        assert!(peaks[22]);
        assert!(peaks[23]);
        assert_eq!(peaks.iter().filter(|peak| **peak).count(), 2);
    }
}
