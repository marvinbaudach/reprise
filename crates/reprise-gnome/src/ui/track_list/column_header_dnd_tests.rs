use super::*;

#[test]
fn header_click_hit_test_matches_only_the_header_band() {
    assert!(is_within_header(0.0, 25.0));
    assert!(is_within_header(25.0, 25.0));
    assert!(!is_within_header(25.1, 25.0));
    assert!(!is_within_header(200.0, 25.0));
    // No measurable header (not yet realized) never counts as a hit.
    assert!(!is_within_header(0.0, 0.0));
}

#[test]
fn resize_zone_covers_both_edges_but_not_the_middle() {
    // A 100px-wide title spanning [200, 300).
    assert!(is_in_resize_zone(200.0, 200.0, 300.0)); // exactly on the left edge
    assert!(is_in_resize_zone(205.9, 200.0, 300.0)); // just inside the left band
    assert!(is_in_resize_zone(300.0, 200.0, 300.0)); // exactly on the right edge
    assert!(is_in_resize_zone(294.1, 200.0, 300.0)); // just inside the right band
    assert!(!is_in_resize_zone(250.0, 200.0, 300.0)); // dead center
    assert!(!is_in_resize_zone(207.0, 200.0, 300.0)); // just past the left band
    assert!(!is_in_resize_zone(293.0, 200.0, 300.0)); // just before the right band
}

#[test]
fn next_sort_order_toggles_the_primary_column_and_resets_any_other() {
    assert_eq!(
        next_sort_order(true, gtk4::SortType::Ascending),
        gtk4::SortType::Descending
    );
    assert_eq!(
        next_sort_order(true, gtk4::SortType::Descending),
        gtk4::SortType::Ascending
    );
    // Clicking a column that is not currently primary always resets to
    // ascending, regardless of whatever direction the *other* primary
    // column was last sorted in.
    assert_eq!(
        next_sort_order(false, gtk4::SortType::Descending),
        gtk4::SortType::Ascending
    );
    assert_eq!(
        next_sort_order(false, gtk4::SortType::Ascending),
        gtk4::SortType::Ascending
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn sorting_a_new_column_replaces_the_previous_sort_key() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();

    let view = gtk4::ColumnView::new(None::<gtk4::SelectionModel>);
    let artist = gtk4::ColumnViewColumn::new(Some("Artist"), None::<gtk4::ListItemFactory>);
    artist.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
    let rating = gtk4::ColumnViewColumn::new(Some("Rating"), None::<gtk4::ListItemFactory>);
    rating.set_sorter(Some(&gtk4::CustomSorter::new(|_, _| gtk4::Ordering::Equal)));
    view.append_column(&artist);
    view.append_column(&rating);

    let store = gtk4::gio::ListStore::new::<gtk4::glib::Object>();
    let sort_model = gtk4::SortListModel::new(Some(store), view.sorter());
    let selection = gtk4::NoSelection::new(Some(sort_model));
    view.set_model(Some(&selection));

    activate_sort_click(&view, &artist);
    activate_sort_click(&view, &rating);

    let sorter = view
        .sorter()
        .and_downcast::<gtk4::ColumnViewSorter>()
        .expect("column view sorter");
    assert_eq!(
        sorter.n_sort_columns(),
        1,
        "switching columns must not retain a secondary sort key"
    );
    assert_eq!(sorter.primary_sort_column(), Some(rating));
}

fn span(visible: bool, left: f64, right: f64) -> TitleSpan {
    TitleSpan {
        visible,
        left,
        right,
    }
}

/// Five equal-width (100px) visible titles: A=[0,100) B=[100,200)
/// C=[200,300) D=[300,400) E=[400,500); midpoints at 50/150/250/350/450.
fn five_equal_spans() -> Vec<TitleSpan> {
    vec![
        span(true, 0.0, 100.0),
        span(true, 100.0, 200.0),
        span(true, 200.0, 300.0),
        span(true, 300.0, 400.0),
        span(true, 400.0, 500.0),
    ]
}

#[test]
fn insertion_slot_lands_before_the_first_title_whose_midpoint_is_past_the_pointer() {
    let spans = five_equal_spans();
    assert_eq!(
        insertion_slot_for_pointer(&spans, 10.0),
        InsertionSlot::Before(0)
    );
    // Between B and C's midpoints (150..250): still lands before C.
    assert_eq!(
        insertion_slot_for_pointer(&spans, 200.0),
        InsertionSlot::Before(2)
    );
}

#[test]
fn insertion_slot_is_end_past_the_last_titles_midpoint() {
    let spans = five_equal_spans();
    assert_eq!(
        insertion_slot_for_pointer(&spans, 460.0),
        InsertionSlot::End
    );
}

#[test]
fn insertion_slot_skips_hidden_titles_entirely() {
    // B is hidden with a midpoint (150) that IS past the pointer (90) — if
    // hidden titles weren't skipped, this would wrongly resolve to
    // `Before(1)` (B itself); the correct answer skips straight past it to
    // the next visible title, C.
    let spans = vec![
        span(true, 0.0, 100.0),
        span(false, 100.0, 200.0),
        span(true, 200.0, 300.0),
    ];
    assert_eq!(
        insertion_slot_for_pointer(&spans, 90.0),
        InsertionSlot::Before(2)
    );
}

/// Applies `resolve_drop` to a real `Vec` remove+insert pair (when it
/// resolves to a move at all) and checks the *resulting order*, not just the
/// returned index.
fn apply_drop(mut order: Vec<char>, dragged_index: usize, slot: InsertionSlot) -> Vec<char> {
    let Some(target_index) = resolve_drop(dragged_index, slot, order.len()) else {
        return order; // no-op: caller must not mutate
    };
    let dragged = order.remove(dragged_index);
    order.insert(target_index, dragged);
    order
}

#[test]
fn resolve_drop_moves_a_column_forward_to_land_before_its_target() {
    let order = vec!['A', 'B', 'C', 'D', 'E'];
    // Drag A (index 0) to land immediately before D (index 3).
    let result = apply_drop(order, 0, InsertionSlot::Before(3));
    assert_eq!(result, vec!['B', 'C', 'A', 'D', 'E']);
}

#[test]
fn resolve_drop_moves_a_column_backward_to_land_before_its_target() {
    let order = vec!['A', 'B', 'C', 'D', 'E'];
    // Drag E (index 4) to land immediately before B (index 1).
    let result = apply_drop(order, 4, InsertionSlot::Before(1));
    assert_eq!(result, vec!['A', 'E', 'B', 'C', 'D']);
}

#[test]
fn resolve_drop_moves_a_column_to_the_end() {
    let order = vec!['A', 'B', 'C', 'D', 'E'];
    let result = apply_drop(order, 0, InsertionSlot::End);
    assert_eq!(result, vec!['B', 'C', 'D', 'E', 'A']);
}

#[test]
fn resolve_drop_is_a_no_op_when_the_slot_names_the_dragged_column_itself() {
    let order = vec!['A', 'B', 'C', 'D', 'E'];
    assert_eq!(resolve_drop(2, InsertionSlot::Before(2), order.len()), None);
    let result = apply_drop(order, 2, InsertionSlot::Before(2));
    assert_eq!(result, vec!['A', 'B', 'C', 'D', 'E']);
}

#[test]
fn resolve_drop_is_a_no_op_when_the_slot_is_directly_after_an_adjacent_dragged_column() {
    // C (index 2) is already immediately before D (index 3): releasing
    // "before D" must not move anything.
    let order = vec!['A', 'B', 'C', 'D', 'E'];
    assert_eq!(resolve_drop(2, InsertionSlot::Before(3), order.len()), None);
    let result = apply_drop(order, 2, InsertionSlot::Before(3));
    assert_eq!(result, vec!['A', 'B', 'C', 'D', 'E']);
}

#[test]
fn resolve_drop_is_a_no_op_at_the_end_when_already_last() {
    let order = vec!['A', 'B', 'C', 'D', 'E'];
    assert_eq!(resolve_drop(4, InsertionSlot::End, order.len()), None);
    let result = apply_drop(order, 4, InsertionSlot::End);
    assert_eq!(result, vec!['A', 'B', 'C', 'D', 'E']);
}

#[test]
fn resolve_drop_is_not_a_no_op_across_a_hidden_column_gap() {
    // A hidden column sits between the dragged column and the next
    // *visible* title in the underlying model — "before the next visible
    // title" still moves the dragged column past the hidden one, which is a
    // real (if visually silent) order change.
    let order = vec!['A', 'B', 'C']; // A dragged (0), B hidden, C visible (2)
    assert_eq!(
        resolve_drop(0, InsertionSlot::Before(2), order.len()),
        Some(1)
    );
    let result = apply_drop(order, 0, InsertionSlot::Before(2));
    assert_eq!(result, vec!['B', 'A', 'C']);
}

#[test]
fn css_marks_insertion_edges_and_dims_the_drag_source() {
    let css = super::css();
    assert!(css.contains(".reprise-col-insert-before"));
    assert!(css.contains(".reprise-col-insert-after"));
    assert!(css.contains(".reprise-col-drag-source"));
    assert!(css.contains("box-shadow"));
    assert!(css.contains("@accent_color"));
    assert!(css.contains("opacity"));
    assert!(!css.contains(".dnd"));
}
