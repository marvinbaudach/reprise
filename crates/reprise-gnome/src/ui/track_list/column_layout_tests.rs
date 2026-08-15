use super::*;
use std::collections::HashSet;

use gtk4::gio::prelude::*;
use reprise_view::columns::ColumnKey;

#[test]
fn numeric_metadata_columns_are_classified_for_centering() {
    for id in [
        ColumnId::TrackNumber,
        ColumnId::Year,
        ColumnId::Duration,
        ColumnId::Rating,
        ColumnId::PlayCount,
        ColumnId::Added,
    ] {
        assert_eq!(cell_alignment(id), CellAlignment::Numeric);
    }
    for id in [
        ColumnId::Title,
        ColumnId::Artist,
        ColumnId::Album,
        ColumnId::Genre,
    ] {
        assert_eq!(cell_alignment(id), CellAlignment::Text);
    }
}

#[test]
fn every_non_cover_column_can_persist_its_width() {
    // Cover is not resizable (fixed 40px thumbnail) — never stored. Every
    // other column, Title included, can hold a user-set width; the production
    // width saver stores Title only once its fill-expand has been turned off.
    assert!(!is_width_persistable(ColumnId::Cover));
    for id in [
        ColumnId::Title,
        ColumnId::Artist,
        ColumnId::Album,
        ColumnId::Genre,
        ColumnId::Year,
        ColumnId::Duration,
        ColumnId::Rating,
        ColumnId::PlayCount,
        ColumnId::TrackNumber,
        ColumnId::Added,
    ] {
        assert!(is_width_persistable(id), "{id:?} should be persistable");
    }
}

// `gtk4::init()` acquires the default GLib main context and *leaks* the guard
// (gtk-rs-core#186), permanently pinning ownership to the libtest thread that
// happened to run this test — a thread that then exits. Every later test in the
// same process that touches a `*_local` GLib source therefore panics with
// "default main context already acquired by another thread". Marking this as a
// display test keeps the leak out of the default `cargo test` run and puts the
// assertion where it actually executes instead of early-returning.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn title_width_is_stored_only_after_its_fill_expand_is_turned_off() {
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
    let column = gtk4::ColumnViewColumn::builder().build();
    column.set_fixed_width(240);
    view.append_column(&column);
    let registry = GenericColumnRegistry::new(
        &view,
        conn.clone(),
        TableKeys {
            layout: "test.title-width-save.layout",
            widths: "test.title-width-save.widths",
        },
        vec![(ColumnId::Title, column.clone())],
    );
    let columns = [(ColumnId::Title, column.clone())];

    // While Title still fills (expand on), its width is not a real preference.
    column.set_expand(true);
    width_persistence::save_widths_now(&registry, &columns);
    assert_eq!(
        settings::get_setting(&conn, "test.title-width-save.widths").unwrap(),
        Some(String::new())
    );

    // A manual resize turns the fill off; the width becomes storable.
    column.set_expand(false);
    width_persistence::save_widths_now(&registry, &columns);
    assert_eq!(
        settings::get_setting(&conn, "test.title-width-save.widths").unwrap(),
        Some("title:240".to_owned())
    );
}

#[test]
fn rating_column_uses_the_compact_width() {
    assert_eq!(
        column_width_policy(ColumnId::Rating).fixed_width,
        crate::ui::rating::COMPACT_RATING_COLUMN_WIDTH
    );
}

#[test]
fn every_track_column_has_stable_width_and_only_title_expands() {
    for id in ColumnId::all() {
        let policy = column_width_policy(*id);
        if *id == ColumnId::Title {
            // Title uses expand with a low fixed_width so it absorbs
            // remaining space and shrinks when the info panel is open.
            assert!(policy.fixed_width > 0 && policy.fixed_width < 200);
            assert!(policy.expand);
        } else {
            assert!(policy.fixed_width > 0, "missing fixed width for {id:?}");
            assert!(!policy.expand);
        }
    }
}

#[test]
fn play_count_is_available_but_hidden_by_default() {
    let layout = ColumnLayout::default();
    let rating = layout
        .order
        .iter()
        .position(|id| *id == ColumnId::Rating)
        .unwrap();
    assert_eq!(layout.order[rating + 1], ColumnId::PlayCount);
    assert!(!layout.visible.contains(&ColumnId::PlayCount));
    assert_eq!(ColumnId::PlayCount.as_str(), "play-count");
    assert_eq!(
        ColumnId::from_sort_field("play_count"),
        Some(ColumnId::PlayCount)
    );
}

#[test]
fn legacy_layout_gains_a_hidden_play_count_column() {
    let layout = parse_layout("cover,title,artist;cover,title,artist").unwrap();
    assert!(layout.order.contains(&ColumnId::PlayCount));
    assert!(!layout.visible.contains(&ColumnId::PlayCount));
}

#[test]
fn browse_9_added_is_selectable_sortable_persisted_and_hidden_by_default() {
    let layout = ColumnLayout::default();
    let year = layout
        .order
        .iter()
        .position(|id| *id == ColumnId::Year)
        .unwrap();
    assert_eq!(layout.order[year + 1], ColumnId::Added);
    assert!(!layout.visible.contains(&ColumnId::Added));
    assert_eq!(ColumnId::Added.as_str(), "added");
    assert_eq!(ColumnId::from_sort_field("added_at"), Some(ColumnId::Added));
    assert_eq!(column_label(ColumnId::Added), "Added");

    let restored = parse_layout(&serialize_layout(&ColumnLayout {
        order: layout.order,
        visible: HashSet::from([ColumnId::Cover, ColumnId::Title, ColumnId::Added]),
    }))
    .unwrap();
    assert!(restored.visible.contains(&ColumnId::Added));
}

#[test]
fn browse_9_legacy_layout_gains_a_hidden_added_column() {
    let layout = parse_layout("cover,title,artist;cover,title,artist").unwrap();
    assert!(layout.order.contains(&ColumnId::Added));
    assert!(!layout.visible.contains(&ColumnId::Added));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_9_track_list_builds_the_hidden_added_column() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let track_list = crate::ui::track_list::TrackList::new(
        conn,
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );

    let added = track_list
        .column_registry
        .column(ColumnId::Added)
        .expect("Added must be registered so the editor can reveal it");
    assert_eq!(added.title().as_deref(), Some("Added"));
    assert_eq!(added.id().as_deref(), Some("added_at"));
    assert!(!added.is_visible());
}

fn test_registry_with_conn(ids: &[ColumnId], conn: Rc<Db>) -> ColumnRegistry {
    let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
    let mut columns = Vec::new();
    for id in ids.iter().copied() {
        let column = gtk4::ColumnViewColumn::builder().title(id.as_str()).build();
        view.append_column(&column);
        columns.push((id, column));
    }
    let registry = GenericColumnRegistry::new(
        &view,
        conn,
        TableKeys {
            layout: COLUMN_LAYOUT_KEY,
            widths: COLUMN_WIDTHS_KEY,
        },
        columns,
    );
    width_persistence::wire(
        &registry,
        column_label,
        |id| column_width_policy(id).fixed_width,
        ColumnId::Title,
    );
    registry
}

fn test_registry(ids: &[ColumnId]) -> ColumnRegistry {
    test_registry_with_conn(ids, Rc::new(crate::test_db::open().unwrap()))
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn visibility_only_apply_does_not_rebuild_the_column_list() {
    use std::cell::Cell;
    if gtk4::init().is_err() {
        return;
    }
    let ids = [
        ColumnId::Cover,
        ColumnId::Title,
        ColumnId::Artist,
        ColumnId::Album,
    ];
    let registry = test_registry(&ids);
    // Align the view order with the layout order first (this may rebuild).
    let mut visible: HashSet<ColumnId> = ids.iter().copied().collect();
    registry.apply(&ColumnLayout {
        order: ids.to_vec(),
        visible: visible.clone(),
    });

    let rebuilds = Rc::new(Cell::new(0u32));
    let counter = rebuilds.clone();
    registry
        .view()
        .columns()
        .connect_items_changed(move |_, _, _, _| counter.set(counter.get() + 1));

    // Hide Artist only — order is unchanged.
    visible.remove(&ColumnId::Artist);
    registry.apply(&ColumnLayout {
        order: ids.to_vec(),
        visible,
    });

    assert_eq!(
        rebuilds.get(),
        0,
        "a visibility-only change must not remove/re-append columns"
    );
    assert!(!registry.column(ColumnId::Artist).unwrap().is_visible());
    assert!(registry.column(ColumnId::Album).unwrap().is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn reordering_apply_rebuilds_the_column_list_once() {
    use std::cell::Cell;
    if gtk4::init().is_err() {
        return;
    }
    let ids = [
        ColumnId::Cover,
        ColumnId::Title,
        ColumnId::Artist,
        ColumnId::Album,
    ];
    let registry = test_registry(&ids);
    let visible: HashSet<ColumnId> = ids.iter().copied().collect();
    registry.apply(&ColumnLayout {
        order: ids.to_vec(),
        visible: visible.clone(),
    });

    let rebuilds = Rc::new(Cell::new(0u32));
    let counter = rebuilds.clone();
    registry
        .view()
        .columns()
        .connect_items_changed(move |_, _, _, _| counter.set(counter.get() + 1));

    // Move Album ahead of Artist — order genuinely changes.
    registry.apply(&ColumnLayout {
        order: vec![
            ColumnId::Cover,
            ColumnId::Title,
            ColumnId::Album,
            ColumnId::Artist,
        ],
        visible,
    });

    assert!(
        rebuilds.get() > 0,
        "a real reorder must update the column list"
    );
    assert_eq!(
        registry.current_order(),
        vec![
            ColumnId::Cover,
            ColumnId::Title,
            ColumnId::Album,
            ColumnId::Artist
        ]
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn restore_stored_widths_applies_persistable_columns_only() {
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    settings::set_setting(&conn, COLUMN_WIDTHS_KEY, "artist:333,cover:999").unwrap();
    let registry = test_registry_with_conn(&[ColumnId::Cover, ColumnId::Artist], conn);

    assert_eq!(
        registry.column(ColumnId::Artist).unwrap().fixed_width(),
        333
    );
    // Cover is not persistable, so its stored value is ignored.
    assert_ne!(registry.column(ColumnId::Cover).unwrap().fixed_width(), 999);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn reset_widths_restores_the_policy_default() {
    if gtk4::init().is_err() {
        return;
    }
    let registry = test_registry(&[ColumnId::Artist]);
    registry
        .column(ColumnId::Artist)
        .unwrap()
        .set_fixed_width(500);

    registry.reset();

    assert_eq!(
        registry.column(ColumnId::Artist).unwrap().fixed_width(),
        column_width_policy(ColumnId::Artist).fixed_width
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn width_policy_is_applied_to_gtk_columns() {
    gtk4::init().unwrap();
    for id in ColumnId::all() {
        let column = gtk4::ColumnViewColumn::builder().build();
        apply_column_width_policy(&column, *id);
        let policy = column_width_policy(*id);
        assert_eq!(column.fixed_width(), policy.fixed_width);
        assert_eq!(column.expands(), policy.expand);
    }
}

#[test]
fn layout_round_trips_canonically() {
    let layout = ColumnLayout::default();
    assert_eq!(parse_layout(&serialize_layout(&layout)), Some(layout));
}

/// STYLE-10: the music library is the table this concept came from. After
/// generalising it, its default layout, widths and filler must be
/// bit-identical — a silent shift here is a regression for every existing
/// user, whose stored layout was written against these defaults.
#[test]
fn style_13_the_music_defaults_are_unchanged() {
    let layout = reprise_view::columns::Layout::<ColumnId>::default();
    assert_eq!(
        reprise_view::columns::layout::serialize(&layout),
        "cover,title,artist,album,year,added,duration,rating,play-count,track-number,genre;\
cover,title,artist,album,year,duration,rating"
    );
}

#[test]
fn the_fixed_cover_is_absent_from_the_editable_music_band() {
    let editable = ColumnLayout::default()
        .order
        .into_iter()
        .filter(|id| id.pin().is_none())
        .collect::<Vec<_>>();
    assert!(!editable.contains(&ColumnId::Cover));
    assert!(editable.contains(&ColumnId::Title));
}

#[test]
fn duplicate_and_unknown_ids_keep_the_valid_layout() {
    let duplicate = parse_layout("cover,title,title;cover,title").unwrap();
    assert_eq!(duplicate.order[..2], [ColumnId::Cover, ColumnId::Title]);
    let unknown_order = parse_layout("cover,title,banana;cover,title").unwrap();
    assert_eq!(unknown_order.order[..2], [ColumnId::Cover, ColumnId::Title]);
    let unknown_visible = parse_layout("cover,title;cover,banana").unwrap();
    assert!(unknown_visible.visible.contains(&ColumnId::Cover));
    assert!(!unknown_visible.visible.contains(&ColumnId::Title));
}

#[test]
fn parse_layout_pins_cover_first_and_always_visible_but_respects_the_rest() {
    // Cover is pinned to the front AND forced visible regardless of the stored
    // layout (it is out of the editor, so it can't be toggled). Every other
    // column's order and visibility is honored verbatim (missing columns still
    // append); a stored layout that omits Title from the visible set keeps it
    // hidden.
    let layout = parse_layout("artist,album;artist,album").unwrap();
    assert_eq!(layout.order[0], ColumnId::Cover);
    assert_eq!(layout.order[1..3], [ColumnId::Artist, ColumnId::Album]);
    assert!(layout.visible.contains(&ColumnId::Cover));
    assert!(!layout.visible.contains(&ColumnId::Title));
    // Every known column is still present in the normalized order.
    assert!(layout.order.contains(&ColumnId::Title));
}

#[test]
fn corrupted_layout_can_fall_back_to_default() {
    let loaded = parse_layout("not a layout").unwrap_or_default();
    assert_eq!(loaded, ColumnLayout::default());
}

#[test]
fn optional_visibility_changes_without_changing_order() {
    let layout = ColumnLayout::default();
    let hidden = set_column_visible(&layout, ColumnId::Artist, false);
    assert_eq!(hidden.order, layout.order);
    assert!(!hidden.visible.contains(&ColumnId::Artist));
    let shown = set_column_visible(&hidden, ColumnId::TrackNumber, true);
    assert_eq!(shown.order, layout.order);
    assert!(shown.visible.contains(&ColumnId::TrackNumber));
}

#[test]
fn cover_cannot_be_hidden_but_other_columns_can() {
    let layout = ColumnLayout::default();
    // Cover is a fixed leading column — trying to hide it is a no-op.
    let cover_hidden = set_column_visible(&layout, ColumnId::Cover, false);
    assert!(cover_hidden.visible.contains(&ColumnId::Cover));
    assert_eq!(cover_hidden.order, layout.order);
    // Title (like every other listed column) can still be hidden.
    let title_hidden = set_column_visible(&layout, ColumnId::Title, false);
    assert!(!title_hidden.visible.contains(&ColumnId::Title));
    assert_eq!(title_hidden.order, layout.order);
}

#[test]
fn movable_column_is_inserted_before_the_target() {
    let layout = ColumnLayout::default();
    let moved = move_column(&layout, ColumnId::Rating, ColumnId::Artist);
    assert_eq!(
        moved.order[..5],
        [
            ColumnId::Cover,
            ColumnId::Title,
            ColumnId::Rating,
            ColumnId::Artist,
            ColumnId::Album,
        ]
    );
    assert_eq!(moved.visible, layout.visible);
}

#[test]
fn movable_column_can_be_inserted_after_the_target() {
    let layout = ColumnLayout::default();
    let moved = move_column_after(&layout, ColumnId::Artist, ColumnId::Rating);
    let rating = moved
        .order
        .iter()
        .position(|id| *id == ColumnId::Rating)
        .unwrap();
    assert_eq!(moved.order[rating + 1], ColumnId::Artist);
    assert_eq!(moved.visible, layout.visible);
}

#[test]
fn cover_stays_pinned_first_while_other_columns_move_freely() {
    let layout = ColumnLayout::default();
    // Self-move stays a no-op.
    assert_eq!(
        move_column(&layout, ColumnId::Artist, ColumnId::Artist),
        layout
    );
    // Cover cannot be moved off the leading position: even an explicit
    // "move Cover after Title" normalizes right back to Cover first.
    let moved = move_column_after(&layout, ColumnId::Cover, ColumnId::Title);
    assert_eq!(moved.order[0], ColumnId::Cover);
    // Nor can another column be placed before Cover: "move Artist before
    // Cover" lands it right after the pinned Cover, not ahead of it.
    let moved = move_column(&layout, ColumnId::Artist, ColumnId::Cover);
    assert_eq!(moved.order[0], ColumnId::Cover);
    assert_eq!(moved.order[1], ColumnId::Artist);
    // Title (no longer an anchor) still moves freely among the rest.
    let moved = move_column(&layout, ColumnId::Artist, ColumnId::Title);
    let title_index = moved
        .order
        .iter()
        .position(|id| *id == ColumnId::Title)
        .unwrap();
    assert_eq!(moved.order[title_index - 1], ColumnId::Artist);
}
