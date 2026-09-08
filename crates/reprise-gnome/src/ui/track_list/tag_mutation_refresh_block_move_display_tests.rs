use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::library::tag_edit::{TagPatch, TrackEditPatch, TrackWrite};

use super::super::track_list_model_change::ModelChangeKind;
use super::super::track_list_reload::{capture_reload_anchor, ReloadViewport};
use super::super::TrackList;
use super::refresh_after_tag_mutation_with_model_change;

const FIRST_EDITED_POSITION: u32 = 40;
const EDITED: usize = 8;

fn viewport_labels(view: &gtk4::ColumnView) -> Vec<String> {
    fn collect(widget: &gtk4::Widget, view: &gtk4::ColumnView, labels: &mut Vec<String>) {
        if let Some(label) = widget.downcast_ref::<gtk4::Label>() {
            let width = view.width() as f32;
            let height = view.height() as f32;
            if widget.compute_bounds(view).is_some_and(|bounds| {
                bounds.x() < width
                    && bounds.x() + bounds.width() > 0.0
                    && bounds.y() < height
                    && bounds.y() + bounds.height() > 0.0
            }) {
                labels.push(label.text().to_string());
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            collect(&current, view, labels);
            child = current.next_sibling();
        }
    }
    let mut labels = Vec::new();
    collect(view.upcast_ref(), view, &mut labels);
    labels
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn tag_1_artist_save_on_contiguous_rows_emits_one_block_move() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    adw::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for position in 0..68_i64 {
        let title = if position < 40 {
            format!("Alpha {position:02}")
        } else if position < 48 {
            format!("Target {:02}", position - 40)
        } else {
            format!("Zulu {position:02}")
        };
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Middle Artist', 0)",
            (
                position + 1,
                format!("/synthetic/{position:02}.flac"),
                title,
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let track_list = TrackList::new(
        conn.clone(),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        super::super::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    *track_list.shared.sort.borrow_mut() = crate::ui::track_list_sort::SortState {
        field: "artist".into(),
        dir: "asc".into(),
    };
    super::super::track_list_reload::reload(&track_list.shared);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let table = track_list.widget();
    table.set_vexpand(true);
    content.append(table);
    let window = adw::Window::builder()
        .default_width(900)
        .default_height(600)
        .content(&content)
        .build();
    window.present();
    while gtk4::glib::MainContext::default().iteration(false) {}

    track_list.shared.selection.unselect_all();
    for offset in 0..EDITED as u32 {
        track_list
            .shared
            .selection
            .select_item(FIRST_EDITED_POSITION + offset, offset == 0);
    }
    track_list.shared.column_view.scroll_to(
        FIRST_EDITED_POSITION,
        None,
        gtk4::ListScrollFlags::FOCUS,
        None,
    );
    crate::ui::test_settle::settle_for(Duration::from_millis(150));
    let before_ids = track_list.shared.current_view_ids();
    let edited_ids = before_ids
        [FIRST_EDITED_POSITION as usize..FIRST_EDITED_POSITION as usize + EDITED]
        .to_vec();
    let captured = capture_reload_anchor(&track_list.shared);
    let writes = edited_ids
        .iter()
        .map(|id| TrackWrite {
            id: *id,
            path: PathBuf::from(format!("/synthetic/{id}.flac")),
            patch: TrackEditPatch {
                tags: TagPatch {
                    artist: Some("Aardvark Artist".into()),
                    ..TagPatch::default()
                },
                rating: None,
            },
        })
        .collect::<Vec<_>>();
    let anchor = crate::ui::tag_edit::tag_reload_anchor::post_save_reload_anchor(
        captured,
        &edited_ids,
        &writes,
        "artist",
        &before_ids,
        None,
    );
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in &edited_ids {
        tx.execute(
            "UPDATE tracks SET artist = 'Aardvark Artist' WHERE id = ?1",
            [id],
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let after_ids = track_list.shared.current_view_ids();
    let change = super::super::track_list_model_change::changed_range(
        &before_ids,
        &after_ids,
        &edited_ids,
        track_list.shared.model.generation(),
    )
    .expect("the contiguous resort must be a model change");
    assert_eq!(
        change.kind,
        ModelChangeKind::BlockMove {
            from: FIRST_EDITED_POSITION,
            to: 0,
            len: EDITED as u32,
        }
    );
    let trail = super::super::diagnostic_trail::handle();
    let trail_start = trail.snapshot().len();
    refresh_after_tag_mutation_with_model_change(
        &track_list.shared,
        &edited_ids,
        &[],
        anchor,
        ReloadViewport::PostSaveSortAnchor,
        Some(change),
        after_ids.clone(),
        false,
    );
    crate::ui::test_settle::settle_for(Duration::from_millis(500));

    let item_events = trail
        .snapshot()
        .into_iter()
        .skip(trail_start)
        .filter(|event| event.contains(" ItemsChanged "))
        .collect::<Vec<_>>();
    assert_eq!(item_events.len(), 2, "diagnostic trail: {item_events:?}");
    assert!(item_events[0].ends_with("position=40 removed=8 added=0"));
    assert!(item_events[1].ends_with("position=0 removed=0 added=8"));
    let selected_ids = after_ids
        .iter()
        .enumerate()
        .filter(|(position, _)| track_list.shared.selection.is_selected(*position as u32))
        .map(|(_, id)| *id)
        .collect::<Vec<_>>();
    assert_eq!(selected_ids, edited_ids);
    let labels = viewport_labels(&track_list.shared.column_view);
    for offset in 0..EDITED {
        let title = format!("Target {offset:02}");
        assert!(
            labels.contains(&title),
            "{title} is outside the viewport: {labels:?}"
        );
    }
    window.close();
}
