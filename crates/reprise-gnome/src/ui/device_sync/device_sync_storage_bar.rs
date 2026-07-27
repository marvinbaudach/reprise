//! Theme-aware storage composition bar for the compact sync dialog.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::device_sync::{DeviceStorageProjection, StorageProjectionState};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct StorageSegments {
    pub(super) music: u64,
    pub(super) after_sync: u64,
    pub(super) other: u64,
    pub(super) free: u64,
    pub(super) total: u64,
}

pub(super) fn segments(projection: &DeviceStorageProjection) -> Option<StorageSegments> {
    if projection.state != StorageProjectionState::Fits {
        return None;
    }
    let after = projection.after_sync.as_ref()?;
    let total = after.total_bytes.filter(|total| *total > 0)?;
    let current_music = projection
        .current
        .reprise_music_bytes
        .checked_add(projection.current.other_music_bytes)?;
    let after_music = after
        .reprise_music_bytes
        .checked_add(after.other_music_bytes)?;
    let other = after.other_used_bytes?;
    let free = after.free_bytes?;
    let music = current_music.min(after_music);
    let after_sync = after_music.saturating_sub(current_music);
    let represented = music
        .checked_add(after_sync)?
        .checked_add(other)?
        .checked_add(free)?;
    (represented == total).then_some(StorageSegments {
        music,
        after_sync,
        other,
        free,
        total,
    })
}

pub(super) struct StorageBar {
    widget: gtk4::DrawingArea,
    segments: Rc<Cell<Option<StorageSegments>>>,
}

impl StorageBar {
    pub(super) fn new() -> Self {
        let widget = gtk4::DrawingArea::new();
        widget.set_content_height(8);
        widget.set_hexpand(true);
        widget.set_visible(false);
        widget.update_property(&[gtk4::accessible::Property::Label(
            "Projected storage composition",
        )]);
        let current = Rc::new(Cell::new(None::<StorageSegments>));
        let draw_segments = current.clone();
        widget.set_draw_func(move |widget, context, width, height| {
            let Some(segments) = draw_segments.get() else {
                return;
            };
            let foreground = widget.color();
            let width = f64::from(width.max(0));
            let height = f64::from(height.max(0));
            let mut x = 0.0;
            for (bytes, color, alpha) in [
                (segments.music, foreground, 0.82),
                (segments.after_sync, foreground, 0.48),
                (segments.other, foreground, 0.28),
                (segments.free, foreground, 0.10),
            ] {
                let segment_width = width * bytes as f64 / segments.total as f64;
                context.set_source_rgba(
                    f64::from(color.red()),
                    f64::from(color.green()),
                    f64::from(color.blue()),
                    alpha,
                );
                context.rectangle(x, 0.0, segment_width, height);
                let _ = context.fill();
                x += segment_width;
            }
        });
        Self {
            widget,
            segments: current,
        }
    }

    pub(super) fn widget(&self) -> &gtk4::DrawingArea {
        &self.widget
    }

    pub(super) fn update(&self, projection: &DeviceStorageProjection) {
        let segments = segments(projection);
        self.segments.set(segments);
        self.widget.set_visible(segments.is_some());
        self.widget.queue_draw();
    }
}
