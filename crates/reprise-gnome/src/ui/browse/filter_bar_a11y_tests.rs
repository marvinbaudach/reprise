use gtk4::prelude::*;

use super::tests::bar;
use super::FilterModel;
use crate::ui::test_accessible_label::{accessible_label_mismatch, has_labelled_by_relation};

fn visible_text(row: &gtk4::ListBoxRow) -> String {
    row.child()
        .expect("a chooser row has a child")
        .downcast::<gtk4::Label>()
        .expect("a chooser row child is its Label")
        .label()
        .to_string()
}

fn rows(list: &gtk4::ListBox) -> Vec<gtk4::ListBoxRow> {
    std::iter::successors(list.first_child(), gtk4::prelude::WidgetExt::next_sibling)
        .map(|child| {
            child
                .downcast::<gtk4::ListBoxRow>()
                .expect("the chooser list holds only rows")
        })
        .collect()
}

fn assert_rows_are_named(list: &gtk4::ListBox, page: &str) {
    let rows = rows(list);
    assert!(!rows.is_empty(), "the {page} page has rows to check");
    for row in rows {
        let text = visible_text(&row);
        assert_eq!(
            accessible_label_mismatch(&row, &text),
            None,
            "the {page} row drawn as {text:?} must be announced by that name"
        );
        assert!(
            !has_labelled_by_relation(&row),
            "the {page} row drawn as {text:?} must not be named through a labelled-by relation"
        );
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn gp_10_add_filter_popover_rows_are_named_by_their_text() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = bar();

    bar.rebuild_facets();
    assert_rows_are_named(&bar.facet_list, "facet");

    let facet = bar
        .model
        .facets()
        .into_iter()
        .next()
        .expect("the test model offers a facet");
    bar.show_values(facet, true);
    assert_rows_are_named(&bar.value_list, "value");
}
