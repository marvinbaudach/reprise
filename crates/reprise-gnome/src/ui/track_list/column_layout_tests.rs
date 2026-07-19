use super::*;

#[test]
fn numeric_metadata_columns_are_classified_for_centering() {
    for id in [
        ColumnId::TrackNumber,
        ColumnId::Year,
        ColumnId::Duration,
        ColumnId::Rating,
        ColumnId::PlayCount,
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
    // other column, Title included, can hold a user-set width; Title only once
    // its fill-expand has been turned off (see `is_width_persistable_now`).
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
    let column = gtk4::ColumnViewColumn::builder().build();

    // While Title still fills (expand on), its width is not a real preference.
    column.set_expand(true);
    assert!(!is_width_persistable_now(ColumnId::Title, &column));

    // A manual resize turns the fill off; the width becomes storable.
    column.set_expand(false);
    assert!(is_width_persistable_now(ColumnId::Title, &column));

    // Cover is excluded regardless of expand state.
    assert!(!is_width_persistable_now(ColumnId::Cover, &column));
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
    for id in DEFAULT_ORDER {
        let policy = column_width_policy(id);
        if id == ColumnId::Title {
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

fn test_registry(ids: &[ColumnId]) -> ColumnRegistry {
    let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
    let mut columns = HashMap::new();
    for id in ids.iter().copied() {
        let column = gtk4::ColumnViewColumn::builder().title(id.as_str()).build();
        view.append_column(&column);
        columns.insert(id, column);
    }
    ColumnRegistry {
        view,
        columns,
        syncing_order: Rc::new(Cell::new(false)),
        syncing_width: Rc::new(Cell::new(false)),
    }
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
        .view
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
        .view
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
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    settings::set_setting(&conn, COLUMN_WIDTHS_KEY, "artist:333,cover:999").unwrap();
    let registry = test_registry(&[ColumnId::Cover, ColumnId::Artist]);

    restore_stored_widths(&registry.columns, &conn);

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

    registry.reset_widths();

    assert_eq!(
        registry.column(ColumnId::Artist).unwrap().fixed_width(),
        column_width_policy(ColumnId::Artist).fixed_width
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn width_policy_is_applied_to_gtk_columns() {
    gtk4::init().unwrap();
    for id in DEFAULT_ORDER {
        let column = gtk4::ColumnViewColumn::builder().build();
        apply_column_width_policy(&column, id);
        let policy = column_width_policy(id);
        assert_eq!(column.fixed_width(), policy.fixed_width);
        assert_eq!(column.expands(), policy.expand);
    }
}

#[test]
fn layout_round_trips_canonically() {
    let layout = ColumnLayout::default();
    assert_eq!(parse_layout(&serialize_layout(&layout)), Some(layout));
}

#[test]
fn duplicate_or_unknown_ids_are_rejected() {
    assert!(parse_layout("cover,title,title;cover,title").is_none());
    assert!(parse_layout("cover,title,banana;cover,title").is_none());
    assert!(parse_layout("cover,title;cover,banana").is_none());
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
fn rhythmbox_mapping_preserves_supported_order_and_ignores_unknown() {
    let tokens = [
        "rating",
        "play-count",
        "duration",
        "album",
        "artist",
        "date",
        "post-time",
    ]
    .map(str::to_string);
    let layout = import_rhythmbox_tokens(&tokens);
    assert_eq!(
        layout.order[..8],
        [
            ColumnId::Cover,
            ColumnId::Title,
            ColumnId::Rating,
            ColumnId::PlayCount,
            ColumnId::Duration,
            ColumnId::Album,
            ColumnId::Artist,
            ColumnId::Year,
        ]
    );
    assert_eq!(layout.visible.len(), 8);
}

#[test]
fn rhythmbox_mapping_stably_deduplicates_tokens() {
    let tokens = ["artist", "album", "artist", "genre"].map(str::to_string);
    let layout = import_rhythmbox_tokens(&tokens);
    assert_eq!(
        layout.order[..5],
        [
            ColumnId::Cover,
            ColumnId::Title,
            ColumnId::Artist,
            ColumnId::Album,
            ColumnId::Genre,
        ]
    );
}

#[test]
fn rhythmbox_empty_list_still_keeps_cover_and_title() {
    let layout = import_rhythmbox_tokens(&[]);
    assert_eq!(layout.order[..2], [ColumnId::Cover, ColumnId::Title]);
    assert_eq!(
        layout.visible,
        HashSet::from([ColumnId::Cover, ColumnId::Title])
    );
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

#[test]
fn rhythmbox_import_is_offered_exactly_when_available() {
    assert!(should_offer_rhythmbox_import(true));
    assert!(!should_offer_rhythmbox_import(false));
}
