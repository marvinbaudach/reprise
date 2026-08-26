//! Tests for `radio_view.rs`, split out to keep that file under the
//! repository's 800-line ceiling.

use super::*;
use crate::ui::playback::external_media::{
    ExternalPlaybackSnapshot, RadioPresentation, StreamTags,
};
use crate::ui::playback::preview::PlaybackMode;
use reprise_core::source_error::FailureAction;

#[test]
fn rad_1_table_projects_only_connected_radio_snapshots() {
    let connected = live_state(Some(ExternalPlaybackSnapshot {
        mode: PlaybackMode::Radio,
        podcast_kind: None,
        media_category: None,
        media: ExternalMedia::Radio {
            station_id: 7,
            name: "Station".into(),
            stream_url: "https://radio.example/live".into(),
            uuid: None,
        },
        art_url: None,
        fallback_art_url: None,
        can_go_previous: false,
        can_go_next: false,
        stream_tags: StreamTags {
            title: Some("Artist — Song".into()),
            organization: None,
        },
        podcast_phase: None,
        restored: false,
        radio: Some(RadioPresentation::connected()),
        error: None,
    }));
    assert_eq!(connected.station_id, Some(7));
    assert!(connected.connected);
    assert_eq!(connected.title.as_deref(), Some("Artist — Song"));
}

#[test]
fn rad_3_dead_stream_actions_distinguish_retry_from_directory_reresolution() {
    assert_eq!(
        radio_failure_action(FailureAction::TryAgain, Some("station-uuid")),
        RadioFailureAction::RetryPlayback
    );
    assert_eq!(
        radio_failure_action(FailureAction::FindNewUrl, Some("station-uuid")),
        RadioFailureAction::ReresolveDirectoryUrl
    );
    assert_eq!(
        radio_failure_action(FailureAction::FindNewUrl, None),
        RadioFailureAction::OpenAddDialog
    );
}

fn add_station(conn: &Rc<Db>, name: &str) -> i64 {
    radio::station::add_or_restore(
        conn,
        &radio::station::NewStation {
            uuid: None,
            name: name.into(),
            stream_url: format!("https://example.invalid/{name}"),
            homepage: None,
            favicon_url: None,
            genre: None,
            codec: None,
            bitrate_kbps: None,
            country_code: None,
            votes: None,
        },
        0,
    )
    .unwrap()
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn net_6_radio_refreshes_only_mapped_artwork_cells_without_moving_selection() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, false)
        .unwrap();
    add_station(&conn, "Alpha");
    add_station(&conn, "Bravo");
    let view = RadioView::new(conn.clone(), None);
    view.shared.model.selection().set_selected(1);

    let hidden_before = source_images(view.root());
    view.refresh_visible_artwork();
    assert_eq!(source_images(view.root()), hidden_before);

    let window = gtk4::Window::new();
    window.set_default_size(900, 400);
    window.set_child(Some(view.root()));
    window.present();
    crate::ui::source_context_surface::settle_layout();
    let before = source_images(view.root());
    assert!(!before.is_empty());
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, true)
        .unwrap();
    crate::ui::podcasts::source_image::recompute_gate(&conn);

    view.refresh_visible_artwork();
    crate::ui::source_context_surface::settle_layout();

    let after = source_images(view.root());
    assert_eq!(after.len(), before.len());
    assert!(before.iter().zip(&after).all(|(left, right)| left != right));
    assert_eq!(view.shared.model.selection().selected(), 1);
}

fn source_images(widget: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut images = widget
        .has_css_class("reprise-source-image")
        .then(|| widget.clone())
        .into_iter()
        .collect::<Vec<_>>();
    let mut child = widget.first_child();
    while let Some(current) = child {
        images.extend(source_images(&current));
        child = current.next_sibling();
    }
    images
}

fn connected_snapshot(station_id: i64, title: &str) -> ExternalPlaybackSnapshot {
    ExternalPlaybackSnapshot {
        mode: PlaybackMode::Radio,
        podcast_kind: None,
        media_category: None,
        media: ExternalMedia::Radio {
            station_id,
            name: "Station".into(),
            stream_url: "https://example.invalid/stream".into(),
            uuid: None,
        },
        art_url: None,
        fallback_art_url: None,
        can_go_previous: false,
        can_go_next: false,
        stream_tags: StreamTags {
            title: Some(title.into()),
            organization: None,
        },
        podcast_phase: None,
        restored: false,
        radio: Some(RadioPresentation::connected()),
        error: None,
    }
}

fn playing_cells(view: &RadioView) -> usize {
    fn count(widget: &gtk4::Widget) -> usize {
        let here = usize::from(widget.has_css_class("reprise-radio-playing"));
        let mut child = widget.first_child();
        let mut total = here;
        while let Some(current) = child {
            total += count(&current);
            child = current.next_sibling();
        }
        total
    }
    count(&view.shared.root)
}

/// The reported radio bug: double-clicking a station moved the highlight
/// off it — every external snapshot (the play itself, the phase change,
/// and later every new ICY title) rebuilt the whole store with
/// `remove_all()`, and `GtkSingleSelection` answers that by autoselecting
/// row 0. The same rebuild emptied the store for an instant, which reset
/// the scroll offset — the "the rows keep switching around" half of the
/// report. Nothing about the station list changed here, so nothing in the
/// table may move.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn rad_1_a_live_state_update_never_moves_the_selection() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    add_station(&conn, "Alpha");
    let bravo = add_station(&conn, "Bravo");
    add_station(&conn, "Charlie");

    let view = RadioView::new(conn, None);
    let window = gtk4::Window::new();
    window.set_default_size(900, 400);
    window.set_child(Some(view.root()));
    window.present();
    crate::ui::source_context_surface::settle_layout();

    view.shared.model.selection().set_selected(1);
    assert_eq!(view.shared.model.selection().selected(), 1);

    on_external_snapshot(
        &view.shared,
        Some(connected_snapshot(bravo, "Artist — Song")),
    );
    crate::ui::source_context_surface::settle_layout();

    assert_eq!(
        view.shared.model.selection().selected(),
        1,
        "a live-state snapshot must leave the selected station selected"
    );
    assert!(
        playing_cells(&view) > 0,
        "the connected station must still pick up its playing marker"
    );

    // A second snapshot carrying only a new title — the every-song case.
    on_external_snapshot(&view.shared, Some(connected_snapshot(bravo, "Next — Song")));
    crate::ui::source_context_surface::settle_layout();
    assert_eq!(view.shared.model.selection().selected(), 1);
}

fn list_vadjustment(view: &RadioView) -> gtk4::Adjustment {
    view.shared
        .stack
        .child_by_name(LIST_PAGE)
        .and_downcast::<gtk4::Overlay>()
        .and_then(|overlay| overlay.child())
        .and_downcast::<gtk4::ScrolledWindow>()
        .expect("the list overlay owns a ScrolledWindow")
        .vadjustment()
}

/// The other half of the report — "the rows keep switching around". A
/// snapshot used to empty the store for an instant, which collapsed the
/// scrolled window's content height and reset the offset to the top; and
/// a station activated *here* was still classified as a change from
/// elsewhere, so the reveal centred the row the user had just clicked.
/// `SRC-13`: an activated row was visible by definition, so nothing moves.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_13_activating_a_station_here_leaves_the_viewport_where_the_user_put_it() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let ids: Vec<i64> = (0..40)
        .map(|index| add_station(&conn, &format!("Station {index:02}")))
        .collect();

    let view = RadioView::new(conn, None);
    let window = gtk4::Window::new();
    window.set_default_size(900, 300);
    window.set_child(Some(view.root()));
    window.present();
    crate::ui::source_context_surface::settle_layout();

    let adjustment = list_vadjustment(&view);
    adjustment.set_value(adjustment.upper() / 2.0);
    crate::ui::source_context_surface::settle_layout();
    let scrolled_to = adjustment.value();
    assert!(scrolled_to > 0.0, "the table must be scrollable for this");

    // A double-click on a station of this table. Scrolling the table above
    // counts as user activity, which would hold off *any* reveal for the
    // next 1.5 seconds and make this pass for the wrong reason.
    view.shared.reveal.forget_scroll_activity();
    let station = radio::station::get(&view.shared.conn, ids[35])
        .unwrap()
        .unwrap();
    activate_station(&view.shared, &station);
    // The stream connects asynchronously: the activation itself is long
    // over by the time the `Connected` snapshot — the one the reveal acts
    // on — arrives.
    on_external_snapshot(&view.shared, Some(connected_snapshot(ids[35], "Song")));
    crate::ui::source_context_surface::settle_layout();

    assert_eq!(
        adjustment.value(),
        scrolled_to,
        "activating a station here must not move the table"
    );

    // The same change arriving from elsewhere — the player bar, MPRIS — is
    // still revealed, which is what `SRC-13` promises. Without this the
    // assertion above would prove nothing: a reveal that never runs at all
    // also never moves the viewport.
    view.shared.reveal.forget_scroll_activity();
    on_external_snapshot(&view.shared, Some(connected_snapshot(ids[2], "Song")));
    crate::ui::source_context_surface::settle_layout();
    assert_ne!(
        adjustment.value(),
        scrolled_to,
        "a station connected elsewhere is still revealed"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_1a_radio_empty_state_offers_add_station_without_playback() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let view = RadioView::new(conn, None);
    // `SRC-10` moved this action onto the shared empty-state page's own
    // button (`empty_page`) rather than the still-existing
    // `status_button`, which now serves only `NoResults`.
    assert_eq!(
        view.shared.empty_page.button_label_text().as_deref(),
        Some("Add station")
    );
    assert_eq!(view.shared.empty_state.get(), RadioEmptyState::Empty);
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_2_radio_add_action_is_the_leftmost_footer_child_not_a_filter_control() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    add_station(&conn, "Radio Nova");
    let view = RadioView::new(conn, None);

    assert_eq!(
        view.shared.footer.first_child(),
        Some(view.shared.footer_add.clone().upcast::<gtk4::Widget>())
    );
    assert_eq!(
        view.shared.footer_add.action_name().as_deref(),
        Some("radio.open-add")
    );
    assert!(view
        .shared
        .footer_add
        .has_css_class(crate::ui::style::buttons::ADD_ACTION_CLASS));
    assert!(!descendant_buttons(view.shared.filter_bar.widget())
        .iter()
        .any(|button| button.action_name().as_deref() == Some("radio.open-add")));
}

fn descendant_buttons(widget: &impl IsA<gtk4::Widget>) -> Vec<gtk4::Button> {
    let mut buttons = Vec::new();
    let mut child = widget.as_ref().first_child();
    while let Some(current) = child {
        if let Ok(button) = current.clone().downcast::<gtk4::Button>() {
            buttons.push(button);
        }
        buttons.extend(descendant_buttons(&current));
        child = current.next_sibling();
    }
    buttons
}

fn descendant_with_class<T: IsA<gtk4::Widget> + Clone + 'static>(
    widget: &gtk4::Widget,
    class: &str,
) -> Option<T> {
    if widget.has_css_class(class) {
        if let Ok(found) = widget.clone().downcast::<T>() {
            return Some(found);
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        if let Some(found) = descendant_with_class(&current, class) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_3a_radio_end_line_counts_stations_and_recovers_with_clear_all() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    add_station(&conn, "Afd Radio");
    add_station(&conn, "Different Radio");
    let view = RadioView::new(conn, None);
    let window = gtk4::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(view.root()));
    window.present();

    view.set_search_query("afd");
    crate::ui::source_context_surface::settle_layout();

    let line = descendant_with_class::<gtk4::Label>(
        view.root(),
        crate::ui::end_of_results::LINE_CSS_CLASS,
    )
    .expect("Radio owns the shared end-of-results line");
    assert_eq!(
        line.text(),
        "End of results — 1 station hidden by search “afd”"
    );
    assert!(line.is_visible());
    let recovery = descendant_with_class::<gtk4::Button>(
        view.root(),
        crate::ui::end_of_results::RECOVERY_CSS_CLASS,
    )
    .expect("Radio owns the shared recovery pill");
    assert_eq!(recovery.label().as_deref(), Some("Show all 2 stations"));
    recovery.emit_clicked();
    crate::ui::source_context_surface::settle_layout();
    assert_eq!(view.shared.filter_bar.filter().query, "");
    assert!(!line.is_visible());
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_10_radio_empty_state_hides_the_toolbar_and_the_first_station_restores_it() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let view = RadioView::new(conn.clone(), None);

    assert!(!view.shared.filter_bar.widget().is_visible());
    assert_eq!(
        view.shared.stack.visible_child_name().as_deref(),
        Some(EMPTY_PAGE)
    );

    radio::station::add_or_restore(
        &conn,
        &radio::station::NewStation {
            uuid: None,
            name: "Test Station".into(),
            stream_url: "https://example.invalid/stream".into(),
            homepage: None,
            favicon_url: None,
            genre: None,
            codec: None,
            bitrate_kbps: None,
            country_code: None,
            votes: None,
        },
        0,
    )
    .unwrap();
    view.refresh();

    assert!(view.shared.filter_bar.widget().is_visible());
    assert_eq!(
        view.shared.stack.visible_child_name().as_deref(),
        Some(LIST_PAGE)
    );
}

/// `SRC-10` addendum (Block B2): the filter-mismatch state is the
/// opposite of the genuine empty state — the filter row stays visible,
/// with a "Clear filters" action, because clearing the filter (not
/// adding a station) is the way out. Would go red if `NoResults` hid
/// the toolbar the same way `Empty` does.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_10_the_filter_mismatch_state_keeps_the_filter_row_visible_unlike_the_true_empty_state() {
    gtk4::init().unwrap();
    let conn = Rc::new(crate::test_db::open().unwrap());
    let view = RadioView::new(conn, None);

    apply_empty_state(&view.shared, RadioEmptyState::Empty);
    assert!(!view.shared.filter_bar.widget().is_visible());

    apply_empty_state(&view.shared, RadioEmptyState::NoResults);
    assert!(view.shared.filter_bar.widget().is_visible());
    assert_eq!(view.shared.status.title(), "Nothing matches these filters");
    assert_eq!(
        view.shared.status_button.label().as_deref(),
        Some("Clear filters")
    );

    // The button's click handler reads `empty_state` (set above) to
    // decide whether to clear filters — clicking it here must not
    // panic and must route through `clear_all` rather than a refresh.
    view.shared.status_button.emit_clicked();
}
