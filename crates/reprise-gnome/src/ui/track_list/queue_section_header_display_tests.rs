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

    window.close();
}
