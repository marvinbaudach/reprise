//! Tests for browse_bar.rs (extracted to keep the source under the 800-line gate).

use super::*;

#[cfg(test)]
fn test_bar() -> Rc<BrowseBar> {
    let conn = Rc::new(crate::test_db::open().unwrap());
    BrowseBar::new(conn)
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn place_pill_is_outlined_and_carries_no_remove_cross() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = test_bar();
    bar.set_source_context(&ViewSource::Artist("Alpha Artist".into()));

    let pill = bar.place_button().expect("an artist place shows a pill");
    let label = pill.label().expect("the pill is labelled").to_string();
    assert!(label.contains("Alpha Artist"), "label was {label}");
    assert!(
        !label.contains('×'),
        "a place is left, not removed: {label}"
    );
    assert!(pill.has_css_class(PLACE_PILL_CSS_CLASS));
    assert!(!pill.has_css_class(filter_bar_layout::CHIP_CSS_CLASS));
    assert!(
        pill.tooltip_text()
            .is_some_and(|tooltip| tooltip.contains("Leave")),
        "the tooltip names leaving, not removing"
    );
    assert!(pill.width_request() >= 20 && pill.height_request() >= 20);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_2a_music_has_no_filter_caption_or_zone_separator() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = test_bar();
    bar.set_source_context(&ViewSource::Artist("Alpha Artist".into()));

    assert!(bar.widget().is_visible(), "the pill forces the row visible");
    let labels = descendant_labels(bar.widget());
    assert!(!labels.iter().any(|label| label == "FILTER"));
    assert!(descendants::<gtk4::Separator>(bar.widget()).is_empty());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn sidebar_places_show_no_place_pill() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let bar = test_bar();

    for source in [
        ViewSource::Library,
        ViewSource::RecentlyAdded,
        ViewSource::Playlist(7),
        ViewSource::Queue,
    ] {
        bar.set_source_context(&source);
        assert!(
            bar.place_button().is_none(),
            "{source:?} is named by its sidebar row"
        );
    }
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
fn fil_2a_music_fills_place_filters_count_and_clear_slots() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());
    let bar = BrowseBar::new(conn);

    // QA #8: the bar keeps a constant height across empty/active states.
    assert_eq!(
        bar.root.height_request(),
        filter_bar_layout::FILTER_BAR_MIN_HEIGHT
    );
    bar.restore_filter(&full_filter());
    bar.set_search("falling");
    bar.set_committed_query("falling");
    assert_eq!(
        bar.root.height_request(),
        filter_bar_layout::FILTER_BAR_MIN_HEIGHT
    );
    assert_eq!(bar.chips.observe_children().n_items(), 3);
    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::Place,
        &bar.place_zone
    ));
    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::Facets,
        &bar.chips
    ));
    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::AddFilter,
        &bar.add_filter
    ));
    assert_eq!(
        bar.layout
            .slot_child(crate::ui::filter_bar_layout::FilterBarSlot::Search)
            .and_then(|widget| widget.downcast::<gtk4::Button>().ok())
            .and_then(|button| button.label())
            .as_deref(),
        Some("⌕ “falling” in track, artist and album  ×")
    );
    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::Count,
        &bar.result_label
    ));
    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::ClearAll,
        &bar.clear_all
    ));

    let genre_chip = bar.chips.first_child().unwrap();
    genre_chip
        .downcast::<gtk4::Button>()
        .unwrap()
        .emit_clicked();
    let context = glib::MainContext::default();
    while context.pending() {
        context.iteration(false);
    }
    assert_eq!(bar.filter(), BrowseFilter::default());
    assert_eq!(bar.chips.observe_children().n_items(), 0);
    assert!(!bar.add_filter.has_css_class("flat"));
    assert_eq!(
        bar.add_filter
            .child()
            .and_then(|child| child.downcast::<gtk4::Label>().ok())
            .map(|label| label.text().to_string()),
        Some("+ Add filter".into())
    );
}

fn descendant_labels(widget: &impl IsA<gtk4::Widget>) -> Vec<String> {
    descendants::<gtk4::Label>(widget)
        .into_iter()
        .map(|label| label.text().to_string())
        .collect()
}

fn descendants<T: IsA<gtk4::Widget> + Clone + 'static>(widget: &impl IsA<gtk4::Widget>) -> Vec<T> {
    let mut found = Vec::new();
    let mut child = widget.as_ref().first_child();
    while let Some(current) = child {
        if let Ok(value) = current.clone().downcast::<T>() {
            found.push(value);
        }
        found.extend(descendants::<T>(&current));
        child = current.next_sibling();
    }
    found
}

// UX FIL-7: the "Hide AI music" filter state is sticky across sessions — it
// persists to settings and a freshly-built bar reads it back, like other view
// state (Beschluss 17). Verifies decision 17's stickiness.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_7_hide_ai_filter_state_is_sticky_across_sessions() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    if gtk4::init().is_err() {
        return;
    }
    let conn = Rc::new(crate::test_db::open().unwrap());

    // A fresh bar defaults to off — the filter is opt-in (AI tracks visible).
    let bar = BrowseBar::new(conn.clone());
    assert!(!bar.exclude_ai(), "the filter is opt-in: off by default");

    // Turning it on persists it to settings.
    bar.set_exclude_ai(true);
    assert!(bar.exclude_ai());
    assert!(
        reprise_core::library::settings::get_bool(&conn, EXCLUDE_AI_KEY, false).unwrap(),
        "the on state is written to settings"
    );

    // A new session (a freshly-built bar on the same DB) reads it back on.
    let next_session = BrowseBar::new(conn.clone());
    assert!(
        next_session.exclude_ai(),
        "the sticky state survives into the next session"
    );

    // Clearing it persists off again, and the following session reads off.
    next_session.clear_exclude_ai();
    assert!(!BrowseBar::new(conn.clone()).exclude_ai());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn rebuilding_chips_keeps_the_persistent_filter_button_in_its_slot() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let bar = BrowseBar::new(conn);

    bar.refresh();

    assert!(bar.layout.slot_contains(
        crate::ui::filter_bar_layout::FilterBarSlot::AddFilter,
        &bar.add_filter
    ));
    assert!(!bar.add_filter.is_focusable());
}
