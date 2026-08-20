//! NAV-10b display proof for Glide centering in a three-section Queue.

use std::rc::Rc;

use gtk4::prelude::*;
use reprise_core::up_next::QueueItem;

use super::{descendant_track_title, rendered_queue_headers, SectionedTrackModel};
use crate::ui::track_list::{queue_sections, TrackList};

const ROWS: i64 = 2_276;
const TARGET_POSITION: u32 = 1_100;
const TARGET_TITLE: &str = "Track 1101";
const EXPECTED_HEADERS_ABOVE: usize = 3;

fn three_section_queue() -> (queue_sections::QueueViewModel, Vec<i64>) {
    let play_next = (2..=11).map(QueueItem::Track).collect::<Vec<_>>();
    let up_next = (12..=ROWS).collect::<Vec<_>>();
    let queue = queue_sections::compose(
        Some(QueueItem::Track(1)),
        &play_next,
        &up_next,
        Some("Synthetic Queue"),
    );
    (queue, up_next)
}

fn build_track_list() -> (TrackList, gtk4::Window) {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=ROWS {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();

    let track_list = TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let sectioned = SectionedTrackModel::new(&track_list.shared.model);
    track_list.shared.selection.set_model(Some(&sectioned));

    let (queue, up_next) = three_section_queue();
    let ranges = queue_sections::section_ranges(&queue.sections);
    assert_eq!(ranges.len(), EXPECTED_HEADERS_ABOVE);
    sectioned.prepare_sections(ranges.clone());
    track_list
        .shared
        .queue_sections
        .replace(queue.sections.clone());
    track_list
        .shared
        .model
        .set_queue_snapshot(&queue, Rc::new(up_next), ranges);
    queue_sections::apply_queue_header_factory(&track_list.shared, true);

    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    let settled =
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            rendered_queue_headers(&track_list.shared.column_view).len() == EXPECTED_HEADERS_ABOVE
                && track_list
                    .shared
                    .column_view
                    .vadjustment()
                    .is_some_and(|adjustment| adjustment.upper() > adjustment.page_size())
                && crate::ui::track_list::track_list_geometry::remember_after_layout(
                    &track_list.shared,
                    ROWS as usize,
                )
        });
    assert!(settled, "the three-section Queue geometry did not settle");
    (track_list, window)
}

fn target_row_and_viewport_centres(track_list: &TrackList) -> Option<(f32, f32, f32)> {
    fn collect(
        widget: &gtk4::Widget,
        scrolled: &gtk4::ScrolledWindow,
        rows: &mut Vec<gtk4::graphene::Rect>,
        viewport: &mut Option<gtk4::graphene::Rect>,
    ) {
        let type_name = widget.type_().name();
        if let Some(bounds) = widget.compute_bounds(scrolled) {
            if type_name.contains("ColumnViewRow")
                && bounds.height() > 0.0
                && descendant_track_title(widget).as_deref() == Some(TARGET_TITLE)
            {
                rows.push(bounds);
            } else if type_name.contains("ListView") && bounds.height() > 0.0 {
                *viewport = Some(bounds);
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(&current, scrolled, rows, viewport);
            child = current.next_sibling();
        }
    }

    let mut rows = Vec::new();
    let mut viewport = None;
    collect(
        track_list.shared.column_view.upcast_ref(),
        &track_list.shared.scrolled,
        &mut rows,
        &mut viewport,
    );
    let viewport = viewport?;
    let viewport_bottom = viewport.y() + viewport.height();
    let row = rows.into_iter().find(|bounds| {
        bounds.y() < viewport_bottom && bounds.y() + bounds.height() > viewport.y()
    })?;
    let row_centre = row.y() + row.height() / 2.0;
    let viewport_centre = viewport.y() + viewport.height() / 2.0;
    Some((row_centre, viewport_centre, row.height()))
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_glide_centres_a_queue_row_after_all_section_headers() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    gtk4::Settings::default()
        .unwrap()
        .set_gtk_enable_animations(false);
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());
    let (track_list, window) = build_track_list();
    let headers = rendered_queue_headers(&track_list.shared.column_view);
    assert_eq!(
        headers.len(),
        EXPECTED_HEADERS_ABOVE,
        "precondition: the target row must follow all three Queue headers; got {headers:?}"
    );

    let layout =
        crate::ui::track_list::track_list_geometry::layout(&track_list.shared, None, ROWS as usize)
            .expect("the settled Queue must expose layout geometry");
    let (adjustment, target) = crate::ui::scroll_center::centered_scroll_target(
        &track_list.shared.column_view,
        ROWS as u32,
        (TARGET_POSITION, layout),
    )
    .expect("the settled Queue must expose Glide geometry");
    adjustment.set_value(target - adjustment.page_size());

    assert!(crate::ui::track_list::track_reveal::reveal_position(
        &track_list.shared,
        TARGET_POSITION,
        8,
        crate::ui::track_list::track_reveal::RevealMotion::Glide,
    ));
    crate::ui::test_settle::settle_for(std::time::Duration::from_millis(200));
    let (row_centre, viewport_centre, row_height) = target_row_and_viewport_centres(&track_list)
        .unwrap_or_else(|| {
            panic!(
                "the target row did not settle into the viewport: value={}, target={old_target}",
                adjustment.value(),
                old_target = target,
            )
        });
    let offset = row_centre - viewport_centre;
    let header_height = crate::ui::style::tokens::SECTION_HEADER_MIN_HEIGHT as f32;
    let omitted_header_offset = EXPECTED_HEADERS_ABOVE as f32 * header_height;
    assert!(
        offset.abs() <= 0.5,
        "three-section Queue Glide missed the viewport centre: measured offset={offset:.1}px, \
         headers_above={EXPECTED_HEADERS_ABOVE}, header_height={header_height:.1}px, \
         omitted_header_offset={omitted_header_offset:.1}px, row_height={row_height:.1}px, \
         row_centre={row_centre:.1}px, viewport_centre={viewport_centre:.1}px"
    );

    window.close();
}
