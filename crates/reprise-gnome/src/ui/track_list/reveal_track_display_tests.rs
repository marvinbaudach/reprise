//! BROWSE-4 and NAV-10b display tests for the player-bar title reveal.
//!
//! The legacy BROWSE-4 fixture proves an ordinary place restore still preserves
//! its anchored offset. The NAV-10b fixture drives the real player-bar button
//! through `MetadataNavigator` and proves that explicit reveal is the one
//! exception: its named anchor is centred in one viewport landing.

use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::AdwApplicationWindowExt;
use reprise_core::browser::navigation::NavigationIntent;
use reprise_core::browser::{BrowserPlace, LibraryScope, TrackAnchor, TrackCollection, TrackFocus};
use reprise_core::view_source::ViewSource;

use crate::ui::nav_history::NavHistory;
use crate::ui::track_list::current_track_selection::CurrentTrackChange;
use crate::ui::track_list::{reload_restore, TrackList};
use crate::ui::window::library_shell::ActiveContentFocus;
use crate::ui::window::metadata_navigation::MetadataNavigator;

/// Comfortably past `track_list_reload::SCROLL_ADJUSTMENT_HOLD`, so a hold
/// that is still guarding the old position has had every chance to pull the
/// viewport back before the assertion reads it.
const PAST_THE_SCROLL_HOLD: Duration = Duration::from_millis(500);

fn synthetic_track_list(rows: i64) -> (TrackList, gtk4::Window) {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=rows {
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
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        track_list
            .shared
            .column_view
            .vadjustment()
            .is_some_and(|adjustment| adjustment.upper() > adjustment.page_size())
    });
    (track_list, window)
}

/// The scroll value an ordinary preserved anchor asks for against live
/// geometry.
fn anchor_target(track_list: &TrackList, track_id: i64) -> Option<f64> {
    let adjustment = track_list.shared.column_view.vadjustment()?;
    let ids = track_list.shared.current_view_ids();
    let height = adjustment.upper() / ids.len() as f64;
    let layout = crate::ui::list_geometry_layout::ListLayout::rows_only(
        crate::ui::list_geometry::RowHeight::new(height)?,
    );
    reload_restore::scroll_target(Some((track_id, 0.0)), &ids, &layout, adjustment.page_size())
}

fn centered_target(track_list: &TrackList, track_id: i64) -> Option<(f64, f64)> {
    let adjustment = track_list.shared.column_view.vadjustment()?;
    let ids = track_list.shared.current_view_ids();
    let row_height = adjustment.upper() / ids.len() as f64;
    let position = ids.iter().position(|id| *id == track_id)?;
    let position = u32::try_from(position).ok()?;
    let layout = crate::ui::list_geometry_layout::ListLayout::rows_only(
        crate::ui::list_geometry::RowHeight::new(row_height)?,
    );
    let target = layout.centered_value(position, ids.len(), adjustment.page_size())?;
    Some((target, row_height))
}

struct PlayerBarRevealStage {
    _sidebar: Rc<crate::ui::sidebar::Sidebar>,
    track_list: Rc<TrackList>,
    player_bar: crate::ui::player_bar::PlayerBar,
    adjustment: gtk4::Adjustment,
    window: adw::ApplicationWindow,
    playing_id: i64,
    reveal_through_preserving_route: Rc<dyn Fn()>,
}

impl PlayerBarRevealStage {
    fn new() -> Self {
        let conn = crate::test_db::open().unwrap();
        let fixture_conn = crate::test_db::connection(&conn);
        let tx = fixture_conn.unchecked_transaction().unwrap();
        for id in 1..=200 {
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

        let conn = Rc::new(conn);
        let app = adw::Application::builder()
            .application_id("io.github.marvinbaudach.Reprise.PlayerBarRevealTest")
            .build();
        app.register(None::<&gtk4::gio::Cancellable>).unwrap();
        let window = adw::ApplicationWindow::new(&app);
        window.set_default_size(900, 320);
        let sidebar = Rc::new(crate::ui::sidebar::Sidebar::new(
            conn.clone(),
            &window,
            || 0,
        ));
        let track_list = Rc::new(TrackList::new(
            conn,
            Box::new(|_, _, _, _| {}),
            |_, _, _, _| {},
            crate::ui::track_list::queue_sections::QueueViewModel::default,
            crate::ui::cover_download_worker::setup_for_test(),
        ));
        let content_stack = gtk4::Stack::new();
        content_stack.add_named(track_list.widget(), Some("library"));
        let player_bar = crate::ui::player_bar::PlayerBar::new();
        window.set_content(Some(&content_stack));
        window.present();
        let adjustment = track_list.shared.column_view.vadjustment().unwrap();
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            adjustment.upper() > adjustment.page_size()
        });

        let history = Rc::new(NavHistory::default());
        let library = BrowserPlace::from(ViewSource::Library);
        history.restore(library.clone(), library.clone());
        let navigator = MetadataNavigator::new(
            history.clone(),
            &sidebar,
            &track_list,
            adw::NavigationView::new(),
            content_stack.clone(),
            adw::WindowTitle::new("Music", ""),
            ActiveContentFocus::new(&content_stack, &track_list),
        );
        sidebar.set_on_select({
            let track_list = track_list.clone();
            move |source, _title| track_list.set_source(source)
        });

        let playing_id = track_list.shared.model.track_at(140).unwrap().id;
        track_list.update_current_track(playing_id, None, CurrentTrackChange::PlaybackStarted);
        crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);
        adjustment.set_value(0.0);
        crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
            adjustment.value() == 0.0
        });
        player_bar.set_track(
            "Track 141",
            "Synthetic Artist",
            crate::ui::playing_links::player_bar_labels(
                crate::ui::playback::preview::PlaybackMode::Queue,
                crate::ui::playing_links::LinkAvailability {
                    artist: true,
                    album: true,
                },
            ),
        );
        player_bar.set_on_title_click({
            let navigator = navigator.clone();
            let library = library.clone();
            move || {
                navigator.navigate(
                    NavigationIntent::RevealTrack {
                        origin: Box::new(library.clone()),
                        track_id: playing_id,
                    },
                    "player-bar title display test",
                );
            }
        });
        let reveal_through_preserving_route = Rc::new({
            let sidebar = sidebar.clone();
            let track_list = track_list.clone();
            let content_stack = content_stack.clone();
            move || {
                let place = history
                    .navigate_from(
                        NavigationIntent::RevealTrack {
                            origin: Box::new(library.clone()),
                            track_id: playing_id,
                        },
                        track_list.browser_place(),
                    )
                    .expect("the reveal must produce a navigation destination");
                crate::ui::window::library_shell::route_to_place(
                    &place,
                    &sidebar,
                    &track_list,
                    crate::ui::window::library_shell::ContentPages::new(
                        &adw::NavigationView::new(),
                        &content_stack,
                    ),
                    &adw::WindowTitle::new("Music", ""),
                    &ActiveContentFocus::new(&content_stack, &track_list),
                    "preserving reveal display test",
                );
            }
        });

        Self {
            _sidebar: sidebar,
            track_list,
            player_bar,
            adjustment,
            window,
            playing_id,
            reveal_through_preserving_route,
        }
    }

    fn click_title(&self) {
        let tooltip = crate::ui::strings::text(crate::ui::strings::JUMP_TO_NOW_PLAYING);
        let widget = self.player_bar.widget().clone().upcast::<gtk4::Widget>();
        let button = find_button_with_tooltip(&widget, &tooltip)
            .expect("the player bar must expose its title activation button");
        button.emit_clicked();
    }

    fn reveal_through_preserving_route(&self) {
        (self.reveal_through_preserving_route)();
    }
}

fn find_button_with_tooltip(widget: &gtk4::Widget, tooltip: &str) -> Option<gtk4::Button> {
    if let Ok(button) = widget.clone().downcast::<gtk4::Button>() {
        if button.tooltip_text().as_deref() == Some(tooltip) {
            return Some(button);
        }
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        child = current.next_sibling();
        if let Some(button) = find_button_with_tooltip(&current, tooltip) {
            return Some(button);
        }
    }
    None
}

fn emit_user_scroll(track_list: &TrackList) {
    let controllers = track_list.shared.scrolled.observe_controllers();
    let controller = (0..controllers.n_items())
        .find_map(|index| {
            controllers
                .item(index)?
                .downcast::<gtk4::EventControllerScroll>()
                .ok()
        })
        .expect("the track scroller must expose its capture-phase scroll witness");
    assert!(!controller.emit_by_name::<bool>("scroll", &[&0.0_f64, &1.0_f64]));
}

/// Restores a place through the default viewport contract. This is the control
/// occasion for NAV-10b: it must preserve an anchored offset, not centre it.
fn restore_revealed_anchor_with_default_viewport(track_list: &TrackList, track_id: i64) {
    track_list.set_source(ViewSource::Library);
    let mut state = track_list
        .browser_place()
        .track_state()
        .expect("the library place must carry track view state")
        .clone();
    state.anchor = Some(TrackAnchor::new(track_id, 0.0));
    state.selected_ids = vec![track_id];
    state.focus = TrackFocus::Track(track_id);
    assert!(track_list.restore_browser_place(&BrowserPlace::tracks(
        TrackCollection::Library(LibraryScope::All),
        state,
    )));
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn browse_4_the_title_link_leaves_the_viewport_at_the_revealed_track() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = synthetic_track_list(200);

    let position = 150;
    track_list
        .shared
        .column_view
        .scroll_to(position, None, gtk4::ListScrollFlags::FOCUS, None);
    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        adjustment.value() > adjustment.page_size() * 2.0
    });
    let before = adjustment.value();
    assert!(
        before > 0.0,
        "precondition: the user is somewhere else in the list"
    );

    let revealed_id = track_list.shared.model.track_at(10).unwrap().id;
    restore_revealed_anchor_with_default_viewport(&track_list, revealed_id);
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);

    let expected = anchor_target(&track_list, revealed_id)
        .expect("a 200-row list in a 320px window must have scrollable geometry");
    assert!(
        (adjustment.value() - expected).abs() < 1.0,
        "the reveal was pulled back: actual {}, expected {expected}, came from {before}",
        adjustment.value()
    );
    let revealed_position = track_list
        .shared
        .current_view_ids()
        .iter()
        .position(|id| *id == revealed_id)
        .unwrap() as u32;
    assert!(
        track_list.shared.selection.is_selected(revealed_position),
        "the revealed track must stay selected"
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_player_bar_title_centers_the_revealed_track() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let stage = PlayerBarRevealStage::new();

    stage.click_title();
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);

    let (expected, row_height) = centered_target(&stage.track_list, stage.playing_id)
        .expect("the revealed track must have a centered target");
    assert!(
        (stage.adjustment.value() - expected).abs() <= row_height / 2.0,
        "the title reveal must center its track: actual {}, expected {expected} (within {})",
        stage.adjustment.value(),
        row_height / 2.0
    );
    stage.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_player_bar_title_centers_in_one_viewport_step() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let stage = PlayerBarRevealStage::new();
    let handler = super::search_viewport_display_tests::record_viewport_steps(&stage.adjustment);

    stage.click_title();
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);
    stage.adjustment.disconnect(handler);
    let steps = super::search_viewport_display_tests::viewport_steps(
        crate::ui::scroll_probe::trail::take(),
    );

    let (expected, row_height) = centered_target(&stage.track_list, stage.playing_id)
        .expect("the revealed track must have a centered target");
    assert_eq!(
        steps.len(),
        1,
        "the player-bar title reveal must move the viewport once: {steps:?}"
    );
    assert!(
        steps[0].writer.starts_with("anchor."),
        "the existing anchor writer must own the only landing: {steps:?}"
    );
    assert!(
        (steps[0].value - expected).abs() <= row_height / 2.0,
        "the only landing must center the revealed track at {expected}: {steps:?}"
    );
    stage.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_reveal_intent_outranks_later_restore_writers() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let stage = PlayerBarRevealStage::new();
    stage.track_list.set_source(ViewSource::Queue);
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);
    let handler = super::search_viewport_display_tests::record_viewport_steps(&stage.adjustment);

    stage.reveal_through_preserving_route();
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);
    stage.adjustment.disconnect(handler);
    let trail = crate::ui::scroll_probe::trail::take();
    let (reveal_entry, reveal_value) = trail
        .iter()
        .enumerate()
        .find_map(|(index, entry)| match entry {
            crate::ui::scroll_probe::trail::Entry::Write { writer, value }
                if writer == "centered.reveal.instant" =>
            {
                Some((index, *value))
            }
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!("the source switch must deliberately reveal the playing track: {trail:?}")
        });
    for entry in &trail[reveal_entry + 1..] {
        if let crate::ui::scroll_probe::trail::Entry::Write { writer, value } = entry {
            if matches!(
                writer.as_str(),
                "anchor.initial.hold_target" | "view_state_restore"
            ) {
                assert!(
                    (*value - reveal_value).abs() < 1.0,
                    "a later restore must not contradict the reveal; ordered trail: {trail:?}"
                );
            }
        }
    }
    let steps = super::search_viewport_display_tests::viewport_steps(trail);
    assert_eq!(
        steps.last().map(|step| step.value),
        Some(reveal_value),
        "the reveal must remain the final visible destination: {steps:?}"
    );
    let stand_downs = stage
        .track_list
        .shared
        .diagnostic_trail
        .snapshot()
        .into_iter()
        .filter(|line| line.contains("ScrollRestoreStandDown"))
        .collect::<Vec<_>>();
    assert!(
        stand_downs
            .iter()
            .any(|line| line.contains("writer=anchor.initial.hold_target")),
        "the anchor restore must name its stand-down: {stand_downs:?}"
    );
    assert!(
        stand_downs
            .iter()
            .any(|line| line.contains("writer=view_state_restore")),
        "the view-state restore must name its stand-down: {stand_downs:?}"
    );
    assert!(
        stage
            .track_list
            .shared
            .scroll_glide
            .deliberate_destination()
            .is_some(),
        "the reveal must leave a durable destination after settling"
    );

    stage.window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn nav_10b_user_scroll_releases_the_reveal_before_a_reload() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let stage = PlayerBarRevealStage::new();
    stage.track_list.set_source(ViewSource::Queue);
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);
    stage.reveal_through_preserving_route();
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);
    let reveal = stage
        .track_list
        .shared
        .scroll_glide
        .deliberate_destination()
        .expect("the reveal must leave a durable destination");

    emit_user_scroll(&stage.track_list);
    assert_eq!(
        stage
            .track_list
            .shared
            .scroll_glide
            .deliberate_destination(),
        None,
        "direct user input must take ownership from the reveal"
    );
    let row_height = stage.adjustment.upper() / 200.0;
    let user_target = row_height * 20.0;
    stage.adjustment.set_value(user_target);
    stage.track_list.reload();
    crate::ui::test_settle::settle_for(PAST_THE_SCROLL_HOLD);

    assert!(
        (stage.adjustment.value() - user_target).abs() <= row_height / 2.0,
        "reload must restore the user's position {user_target}, not the reveal {reveal}; actual {}",
        stage.adjustment.value()
    );
    assert!(
        (stage.adjustment.value() - reveal).abs() > row_height,
        "the revealed destination must not freeze the viewport after user input"
    );
    stage.window.close();
}
