//! Category-segmented storage bar for the device view (`MTP-27`, design 7a).
//!
//! Unlike `device_sync_storage_bar::StorageBar` (Music/After-sync/Other/Free
//! for the compact sync dialog, `MTP-7`), this bar breaks storage down by
//! content category — Music, YouTube audio, Podcasts, Other — and gives the
//! bytes this sync will write their own distinctly *hatched* segment rather
//! than a plain tint, so "about to change" reads differently from "already
//! there" at a glance. All of the segment math is
//! `reprise_core::device_sync::device_view::project_category_segments`
//! (`MTP-27`, unit-tested in core); this widget only draws the numbers it is
//! handed.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::device_sync::device_view::CategorySegments;

/// One rectangle of the bar: a byte width, a flat fill alpha, and whether it
/// is drawn hatched (the "Incoming this sync" segment) instead of flat.
struct Segment {
    bytes: u64,
    alpha: f64,
    hatched: bool,
}

fn segments(data: &CategorySegments) -> [Segment; 5] {
    [
        Segment {
            bytes: data.music_bytes,
            alpha: 0.82,
            hatched: false,
        },
        Segment {
            bytes: data.youtube_bytes,
            alpha: 0.62,
            hatched: false,
        },
        Segment {
            bytes: data.podcast_bytes,
            alpha: 0.42,
            hatched: false,
        },
        Segment {
            bytes: data.other_bytes,
            alpha: 0.22,
            hatched: false,
        },
        Segment {
            bytes: data.incoming_bytes,
            alpha: 0.82,
            hatched: true,
        },
    ]
}

pub(super) struct CategoryStorageBar {
    widget: gtk4::DrawingArea,
    data: Rc<Cell<Option<CategorySegments>>>,
}

impl CategoryStorageBar {
    pub(super) fn new() -> Self {
        let widget = gtk4::DrawingArea::new();
        widget.set_content_height(10);
        widget.set_hexpand(true);
        widget.set_visible(false);
        widget.update_property(&[gtk4::accessible::Property::Label(
            "Storage by category, including this sync's incoming bytes",
        )]);
        let data = Rc::new(Cell::new(None::<CategorySegments>));
        let draw_data = data.clone();
        widget.set_draw_func(move |widget, context, width, height| {
            let Some(data) = draw_data.get() else {
                return;
            };
            if data.total_bytes == 0 {
                return;
            }
            let foreground = widget.color();
            let width = f64::from(width.max(0));
            let height = f64::from(height.max(0));
            let mut x = 0.0;
            for segment in segments(&data) {
                let segment_width = width * segment.bytes as f64 / data.total_bytes as f64;
                if segment.hatched {
                    draw_hatched(context, x, segment_width, height, foreground);
                } else {
                    context.set_source_rgba(
                        f64::from(foreground.red()),
                        f64::from(foreground.green()),
                        f64::from(foreground.blue()),
                        segment.alpha,
                    );
                    context.rectangle(x, 0.0, segment_width, height);
                    let _ = context.fill();
                }
                x += segment_width;
            }
        });
        Self { widget, data }
    }

    pub(super) fn widget(&self) -> &gtk4::DrawingArea {
        &self.widget
    }

    pub(super) fn update(&self, data: Option<CategorySegments>) {
        self.data.set(data);
        self.widget.set_visible(data.is_some());
        self.widget.queue_draw();
    }
}

/// Diagonal-line fill for the "Incoming this sync" segment — deliberately
/// not just a different flat alpha, so the segment reads as "about to
/// change" rather than "another kind of content already there".
fn draw_hatched(
    context: &gtk4::cairo::Context,
    x: f64,
    width: f64,
    height: f64,
    color: gtk4::gdk::RGBA,
) {
    if width <= 0.0 {
        return;
    }
    let _ = context.save();
    context.rectangle(x, 0.0, width, height);
    context.clip();
    context.set_source_rgba(
        f64::from(color.red()),
        f64::from(color.green()),
        f64::from(color.blue()),
        0.9,
    );
    context.set_line_width(1.4);
    const STEP: f64 = 5.0;
    let mut offset = -height;
    while offset < width {
        context.move_to(x + offset, height);
        context.line_to(x + offset + height, 0.0);
        offset += STEP;
    }
    let _ = context.stroke();
    let _ = context.restore();
}
