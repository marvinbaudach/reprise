//! Display proof that both Queue section-header variants share one height.

use gtk4::prelude::*;
use reprise_core::browser::BrowserPlace;
use reprise_core::view_source::ViewSource;

use super::track_list_reload::queue_section_geometry_display_tests::{
    queue_model, rendered_queue_headers, sectioned_track_list,
};

fn list_header_heights(column_view: &gtk4::ColumnView) -> Vec<i32> {
    fn collect(widget: &gtk4::Widget, heights: &mut Vec<i32>) {
        if widget.type_().name().contains("ListHeader") {
            heights.push(widget.height());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(&current, heights);
            child = current.next_sibling();
        }
    }

    let mut heights = Vec::new();
    collect(column_view.upcast_ref(), &mut heights);
    heights
}

/// Queue section-header heights, split by whether the header carries the
/// real "Clear" button (Play Next: `queue_sections.rs` sets a `gtk4::Box`
/// child) or is the plain title alone (Now Playing: a bare `gtk4::Label`
/// child). QUE-1 requires the plain label to grow to the button row's floor,
/// never the button row to shrink toward the label's natural size, so the
/// two must be told apart rather than only checked for overall uniformity.
fn list_header_heights_by_kind(column_view: &gtk4::ColumnView) -> (Vec<i32>, Vec<i32>) {
    fn collect(widget: &gtk4::Widget, button_rows: &mut Vec<i32>, plain_rows: &mut Vec<i32>) {
        if widget.type_().name().contains("ListHeader") {
            let is_button_row = widget
                .first_child()
                .is_some_and(|child| child.downcast_ref::<gtk4::Box>().is_some());
            if is_button_row {
                button_rows.push(widget.height());
            } else {
                plain_rows.push(widget.height());
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(&current, button_rows, plain_rows);
            child = current.next_sibling();
        }
    }

    let mut button_rows = Vec::new();
    let mut plain_rows = Vec::new();
    collect(column_view.upcast_ref(), &mut button_rows, &mut plain_rows);
    (button_rows, plain_rows)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn que_1_queue_section_headers_share_one_height() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());
    let (track_list, sectioned, window) = sectioned_track_list();
    let queue = queue_model();
    sectioned.prepare_sections(super::queue_sections::section_ranges(&queue.sections));
    assert!(track_list.restore_browser_place(&BrowserPlace::from(ViewSource::Queue)));

    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        rendered_queue_headers(&track_list.shared.column_view).len() >= 2
            && list_header_heights(&track_list.shared.column_view)
                .iter()
                .all(|height| *height > 0)
    });

    let heights = list_header_heights(&track_list.shared.column_view);
    let measurement =
        crate::ui::list_geometry::RowMeasurement::from_widget_heights(heights.iter().copied());
    eprintln!(
        "HEADERPROBE settled ({heights:?}, {}, {:?})",
        measurement.is_uniform(),
        measurement
            .modal()
            .map(crate::ui::list_geometry::RowHeight::pixels)
    );
    assert!(
        heights.len() >= 2,
        "the Queue must allocate both header variants; got {heights:?}"
    );
    assert!(
        heights.iter().all(|height| *height > 0),
        "every Queue header must be allocated; got {heights:?}"
    );
    assert!(
        measurement.is_uniform(),
        "Queue section headers must share one height; got {heights:?}"
    );

    // Uniformity alone would pass identically if both header variants shrank
    // together, which is the opposite of what QUE-1 requires. GTK's default
    // Box child alignment is FILL, so the button row's own `.height()`
    // reflects its allocated height, not an unclamped natural size — that
    // natural size is not measurable with the helpers here without inventing
    // a new production measurement API. `SECTION_HEADER_MIN_HEIGHT` is the
    // documented floor both header variants are held to, so it stands in as
    // the lower bound: every header, plain or button-bearing, must reach it
    // rather than settle below it.
    let (button_row_heights, plain_row_heights) =
        list_header_heights_by_kind(&track_list.shared.column_view);
    assert!(
        !button_row_heights.is_empty(),
        "the Play Next (button) header must be allocated; got {heights:?}"
    );
    assert!(
        !plain_row_heights.is_empty(),
        "the Now Playing (plain label) header must be allocated; got {heights:?}"
    );
    for height in button_row_heights.iter().chain(plain_row_heights.iter()) {
        assert!(
            *height >= crate::ui::style::tokens::SECTION_HEADER_MIN_HEIGHT,
            "every Queue header must reach the design floor of {} px, not shrink toward it; got {heights:?}",
            crate::ui::style::tokens::SECTION_HEADER_MIN_HEIGHT
        );
    }

    window.close();
}
