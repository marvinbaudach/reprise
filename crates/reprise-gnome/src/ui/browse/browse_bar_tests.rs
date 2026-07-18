//! Tests for browse_bar.rs (extracted to keep the source under the 800-line gate).

use super::*;

// UX FIL-1a: chip order is search first, then the facet cascade.
#[test]
fn fil_1a_search_appears_as_chip_before_facet_chips() {
    let browse = BrowseFilter {
        genre: Some("Rock".into()),
        ..BrowseFilter::default()
    };
    let labels = chip_labels("falling", &browse, true);
    assert_eq!(
        labels,
        vec![
            "⌕ “falling” in any field".to_string(),
            "Genre: Rock".to_string()
        ]
    );
    assert!(chip_labels("  ", &BrowseFilter::default(), true).is_empty());
}

// UX FIL-1a: facet chips and "+ Add filter" stay Library-only — a facet
// set in Library must not render as a chip in a playlist, where the
// reload path does not apply it.
#[test]
fn fil_1a_facet_chips_are_library_only() {
    let browse = BrowseFilter {
        genre: Some("Rock".into()),
        ..BrowseFilter::default()
    };
    assert_eq!(
        chip_labels("falling", &browse, false),
        vec!["⌕ “falling” in any field".to_string()]
    );
    assert!(chip_labels("", &browse, false).is_empty());
}

fn full_filter() -> BrowseFilter {
    BrowseFilter {
        genre: Some("Rock".into()),
        artist: Some("A".into()),
        album: Some("Stage".into()),
        ..BrowseFilter::default()
    }
}

#[test]
fn genre_selection_resets_artist_and_album() {
    assert_eq!(
        apply_selection(&full_filter(), BrowseFacet::Genre, Some("Jazz".into())),
        BrowseFilter {
            genre: Some("Jazz".into()),
            ..BrowseFilter::default()
        }
    );
}

#[test]
fn artist_selection_keeps_genre_and_resets_album() {
    assert_eq!(
        apply_selection(&full_filter(), BrowseFacet::Artist, Some("B".into())),
        BrowseFilter {
            genre: Some("Rock".into()),
            artist: Some("B".into()),
            ..BrowseFilter::default()
        }
    );
}

#[test]
fn restored_filter_preserves_empty_unknown_values() {
    let filter = BrowseFilter {
        genre: Some(String::new()),
        artist: Some(String::new()),
        album: Some(String::new()),
        ..BrowseFilter::default()
    };
    assert_eq!(restored_filter(&filter), filter);
}

#[test]
fn browse_popup_minimum_height_does_not_collapse_with_zero_results() {
    assert_eq!(browse_popup_min_height(0), browse_popup_min_height(5));
}

#[test]
fn filter_chips_follow_cascade_order_and_render_unknown_values() {
    let filter = BrowseFilter {
        genre: Some(String::new()),
        artist: Some("Brand of Sacrifice".into()),
        album: Some(String::new()),
        ..BrowseFilter::default()
    };

    let chips = filter_chips(&filter);
    let projection: Vec<_> = chips
        .iter()
        .map(|chip| (chip.facet, chip.label.as_str()))
        .collect();
    assert_eq!(
        projection,
        vec![
            (BrowseFacet::Genre, "Genre: Unknown genre"),
            (BrowseFacet::Artist, "Artist: Brand of Sacrifice"),
            (BrowseFacet::Album, "Album: Unknown album"),
        ]
    );
    assert_eq!(
        chips[1].accessible_remove_label,
        "Remove Artist filter: Brand of Sacrifice"
    );
}

#[test]
fn available_facets_omit_filters_that_are_already_active() {
    let filter = BrowseFilter {
        genre: Some("Metal".into()),
        album: Some("Lifeblood".into()),
        ..BrowseFilter::default()
    };

    // Artist is the only open cascade facet; Year and Rating are always
    // available until set because they are standalone constraints.
    assert_eq!(
        available_facets(&filter),
        vec![BrowseFacet::Artist, BrowseFacet::Year, BrowseFacet::Rating]
    );
}

#[test]
fn removing_a_parent_filter_clears_dependent_filters() {
    assert_eq!(
        remove_filter(&full_filter(), BrowseFacet::Genre),
        BrowseFilter::default()
    );
    assert_eq!(
        remove_filter(&full_filter(), BrowseFacet::Artist),
        BrowseFilter {
            genre: Some("Rock".into()),
            ..BrowseFilter::default()
        }
    );
    assert_eq!(
        remove_filter(&full_filter(), BrowseFacet::Album),
        BrowseFilter {
            genre: Some("Rock".into()),
            artist: Some("A".into()),
            ..BrowseFilter::default()
        }
    );
}

#[test]
fn year_and_rating_are_standalone_and_leave_the_cascade_untouched() {
    let base = full_filter();
    let with_year = apply_selection(&base, BrowseFacet::Year, Some("2001".into()));
    assert_eq!(
        with_year,
        BrowseFilter {
            year: Some("2001".into()),
            ..full_filter()
        }
    );
    let with_rating = apply_selection(&with_year, BrowseFacet::Rating, Some("5".into()));
    assert_eq!(
        with_rating,
        BrowseFilter {
            year: Some("2001".into()),
            rating: Some("5".into()),
            ..full_filter()
        }
    );
}

#[test]
fn cascade_selection_preserves_active_year_and_rating() {
    let filter = BrowseFilter {
        year: Some("2001".into()),
        rating: Some("5".into()),
        ..full_filter()
    };
    // Re-selecting Genre resets Artist/Album but keeps Year/Rating.
    assert_eq!(
        apply_selection(&filter, BrowseFacet::Genre, Some("Jazz".into())),
        BrowseFilter {
            genre: Some("Jazz".into()),
            year: Some("2001".into()),
            rating: Some("5".into()),
            ..BrowseFilter::default()
        }
    );
}

#[test]
fn removing_year_or_rating_leaves_every_other_facet_intact() {
    let filter = BrowseFilter {
        year: Some("2001".into()),
        rating: Some("5".into()),
        ..full_filter()
    };
    assert_eq!(
        remove_filter(&filter, BrowseFacet::Year),
        BrowseFilter {
            rating: Some("5".into()),
            ..full_filter()
        }
    );
}

#[test]
fn the_single_value_search_is_case_insensitive_and_matches_substrings() {
    assert!(value_matches_search("Brand of Sacrifice", "SACRI"));
    assert!(value_matches_search("Brand of Sacrifice", ""));
    assert!(!value_matches_search("Brand of Sacrifice", "Chelsea"));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn widget_projects_removable_chips_without_a_redundant_reset_button() {
    if gtk4::init().is_err() {
        return;
    }
    let conn = Connection::open_in_memory().unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    let bar = BrowseBar::new(Rc::new(RefCell::new(conn)));

    // QA #8: the bar keeps a constant height across empty/active states.
    assert_eq!(bar.root.height_request(), FILTER_BAR_MIN_HEIGHT);
    bar.restore_filter(&full_filter());
    assert_eq!(bar.root.height_request(), FILTER_BAR_MIN_HEIGHT);
    assert_eq!(bar.chips.observe_children().n_items(), 4);
    assert_eq!(bar.root.observe_children().n_items(), 4);
    assert_eq!(bar.root.last_child(), Some(bar.clear_all.clone().upcast()));

    let genre_chip = bar.chips.child_at_index(0).unwrap().child().unwrap();
    genre_chip
        .downcast::<gtk4::Button>()
        .unwrap()
        .emit_clicked();
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
    assert_eq!(bar.filter(), BrowseFilter::default());
    assert_eq!(bar.chips.observe_children().n_items(), 1);
    assert!(!bar.add_filter.has_css_class("flat"));
    assert_eq!(
        bar.add_filter
            .child()
            .and_then(|child| child.downcast::<gtk4::Label>().ok())
            .map(|label| label.text().to_string()),
        Some("+ Add filter".into())
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_filter_button_stays_attached_when_chips_rebuild() {
    if gtk4::init().is_err() {
        return;
    }
    let conn = Connection::open_in_memory().unwrap();
    reprise_core::db::migrate(&conn).unwrap();
    let bar = BrowseBar::new(Rc::new(RefCell::new(conn)));

    bar.refresh();

    let wrapper = bar
        .add_filter
        .parent()
        .and_downcast::<gtk4::FlowBoxChild>()
        .expect("add-filter button must have a FlowBoxChild wrapper");
    assert_eq!(wrapper.parent(), Some(bar.chips.clone().upcast()));
    assert!(!wrapper.is_focusable());
}
