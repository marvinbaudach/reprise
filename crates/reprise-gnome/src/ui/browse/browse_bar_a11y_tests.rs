use super::*;
use crate::ui::test_accessible_label::{accessible_label_mismatch, has_labelled_by_relation};

fn rows(list: &gtk4::ListBox) -> Vec<gtk4::ListBoxRow> {
    std::iter::successors(list.first_child(), gtk4::prelude::WidgetExt::next_sibling)
        .map(|child| {
            child
                .downcast::<gtk4::ListBoxRow>()
                .expect("the chooser list holds only rows")
        })
        .collect()
}

fn assert_row_names(list: &gtk4::ListBox, expected: &[&str], page: &str) {
    let rows = rows(list);
    assert_eq!(rows.len(), expected.len(), "the {page} page row count");
    for (row, name) in rows.iter().zip(expected) {
        assert_eq!(
            accessible_label_mismatch(row, name),
            None,
            "the {page} row must be announced as {name:?}"
        );
        assert!(
            !has_labelled_by_relation(row),
            "the {page} row {name:?} must not be named through a labelled-by relation"
        );
    }
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn gp_10_browse_chooser_rows_are_named_by_their_text() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let bar = BrowseBar::new(conn);

    bar.rebuild_facet_page(&bar.filter());
    let facets = rows(&bar.facet_list);
    assert!(!facets.is_empty(), "the facet page has rows to check");
    for (row, facet) in facets.iter().zip(available_facets(&bar.filter())) {
        let name = facet_label(facet);
        assert_eq!(
            accessible_label_mismatch(row, &name),
            None,
            "the facet row {name:?} must be announced by that name"
        );
        assert!(
            !has_labelled_by_relation(row),
            "the facet row {name:?} must not be named through a labelled-by relation"
        );
    }

    bar.chooser_facet.set(Some(BrowseFacet::Genre));
    *bar.chooser_values.borrow_mut() = vec![
        BrowseValue {
            value: "Rock".into(),
            count: 1_704,
        },
        BrowseValue {
            value: "Jazz".into(),
            count: 3,
        },
    ];
    bar.rebuild_value_rows();
    assert_row_names(&bar.value_list, &["Rock (1,704)", "Jazz (3)"], "value");
}
