//! Display tests for the four Block B2 empty states. Split out of
//! `podcasts_view.rs` to keep it under the file-size gate.

use std::time::{Duration, Instant};

use gtk4::glib::variant::ToVariant;
use reprise_core::podcasts::feed::ParsedEpisode;
use reprise_core::podcasts::store::{self, NewSubscription};

use super::*;

/// Builds a `PodcastsView` over a freshly migrated in-memory-backed DB.
/// Callers seed subscriptions/episodes/persisted filter values on `conn`
/// *before* calling this, since `PodcastsFilterBar::new` and the initial
/// `refresh()` both read the DB at construction time.
fn view(conn: Db, kind: PodcastKind) -> Rc<PodcastsView> {
    let runtime = PodcastsRuntime::setup(&conn);
    let conn = Rc::new(conn);
    PodcastsView::install(conn, runtime, PodcastsCallbacks::default(), kind)
}

fn subscribe_with_one_episode(conn: &Db) -> i64 {
    subscribe_one_episode_of_kind(conn, PodcastKind::Rss)
}

fn subscribe_one_episode_of_kind(conn: &Db, kind: PodcastKind) -> i64 {
    let conn = &conn;
    let subscription_id = store::add_or_restore(
        conn,
        &NewSubscription {
            kind,
            feed_url: "https://example.test/feed".to_owned(),
            title: "Show".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    store::upsert_episode(
        conn,
        subscription_id,
        &ParsedEpisode {
            guid: "episode".to_owned(),
            title: "Episode".to_owned(),
            image_url: None,
            audio_url: "https://example.test/episode.mp3".to_owned(),
            page_url: None,
            published_at: None,
            duration_secs: None,
        },
        // Seen after the subscription was added (`added_at: 1`), so this
        // episode counts as new. An episode first seen at the moment of
        // subscribing is backlog and deliberately counts as `0 new`, which
        // would make every "the count reaches the header" assertion below pass
        // on a plain zero and stop proving anything.
        2,
    )
    .unwrap()
    .unwrap()
    .episode_id
}

/// A subscription with `count` episodes, plus a view showing them with the
/// group expanded — the state a user clicks rows in.
fn view_with_expanded_episodes(count: usize) -> (Rc<PodcastsView>, i64) {
    let conn = crate::test_db::open().unwrap();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://example.test/feed".to_owned(),
            title: "Show".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    for index in 0..count {
        store::upsert_episode(
            &conn,
            subscription_id,
            &ParsedEpisode {
                guid: format!("episode-{index}"),
                title: format!("Episode {index}"),
                image_url: None,
                audio_url: format!("https://example.test/{index}.mp3"),
                page_url: None,
                published_at: Some(1_000 + index as i64),
                duration_secs: None,
            },
            2,
        )
        .unwrap()
        .unwrap();
    }
    let view = view(conn, PodcastKind::Rss);
    view.expanded_sources.borrow_mut().insert(subscription_id);
    view.render();
    (view, subscription_id)
}

/// `SRC-14`: the row widget a user is pointing at (and may have focused) has
/// to survive being selected. Before this, every selection change went through
/// `render()`, which rebuilds every row — fine for a mouse, fatal for the
/// keyboard, where the second `Space` would have nothing left to act on.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_14_selecting_a_row_does_not_rebuild_it() {
    gtk4::init().unwrap();
    let (view, _) = view_with_expanded_episodes(3);
    let order = view.rendered_order();
    let episode_id = order[0];

    let before = view
        .selection_widgets
        .borrow()
        .get(&episode_id)
        .map(|widgets| widgets.row.clone());
    assert!(
        before.is_some(),
        "the row is registered for in-place updates"
    );

    view.select_row(episode_id, SelectMode::Toggle);

    let widgets = view.selection_widgets.borrow();
    let after = widgets.get(&episode_id).map(|widgets| widgets.row.clone());
    assert_eq!(before, after, "selecting must not rebuild the row");
    assert!(view.selection.borrow().contains(episode_id));
    let row = after.unwrap();
    assert!(row.has_css_class("reprise-podcast-episode-selected"));
    assert!(
        widgets[&episode_id].checkbox.is_active(),
        "the checkbox mirrors the selection it did not make"
    );
}

/// `SRC-14`: a range runs over the rendered order, and unselecting clears the
/// row's look again.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_14_a_range_selects_every_row_between_anchor_and_target() {
    gtk4::init().unwrap();
    let (view, _) = view_with_expanded_episodes(4);
    let order = view.rendered_order();
    assert_eq!(order.len(), 4, "all four rows are rendered");

    view.select_row(order[0], SelectMode::Only);
    view.select_row(order[2], SelectMode::Range);

    let mut expected = order[..3].to_vec();
    expected.sort_unstable();
    assert_eq!(view.selection.borrow().selected_ids(), expected);
    assert!(
        !view.selection.borrow().contains(order[3]),
        "the row past the target stays out"
    );

    view.select_row(order[3], SelectMode::Only);

    let widgets = view.selection_widgets.borrow();
    assert!(!widgets[&order[0]]
        .row
        .has_css_class("reprise-podcast-episode-selected"));
    assert!(!widgets[&order[0]].checkbox.is_active());
}

/// `SRC-10`: the genuine "nothing subscribed yet" empty state hides the
/// filter row and the footer — would go red if either stayed visible over
/// zero subscriptions.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_10_the_true_empty_state_hides_the_filter_row_and_the_footer() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    // Module on, nothing subscribed — the genuine `Empty` case. Modules
    // default to off, and an off module with zero subscriptions decides
    // `ModuleOff` instead (its own, separately tested, sibling state).
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE, true)
        .unwrap();
    let view = view(conn, PodcastKind::Rss);

    assert!(!view.filter_bar.widget().is_visible());
    assert!(!view.footer.is_visible());
    assert_eq!(view.stack.visible_child_name().as_deref(), Some(EMPTY_PAGE));
}

/// `SRC-10` addendum (Block B2): the filter-mismatch state is the exact
/// opposite of the true empty state — the filter row stays visible, with a
/// "Clear filters" action, because clearing the filter (not adding a show)
/// is the way out. This is the behaviour B2 was missing: before this
/// change `NoResults` reused the whole-page swap that also hides the
/// filter row.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_10_the_filter_mismatch_state_keeps_the_filter_row_visible_unlike_the_true_empty_state() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    let episode_id = subscribe_with_one_episode(&conn);
    store::mark_played(&conn, episode_id, 1).unwrap();
    // Persisted *before* construction: `PodcastsFilterBar::new` loads the
    // sticky filter value from the DB at build time.
    reprise_core::library::settings::set_bool(
        &conn,
        reprise_core::podcasts::config::FILTER_UNPLAYED_KEY,
        true,
    )
    .unwrap();

    let view = view(conn, PodcastKind::Rss);

    assert!(
        view.filter_bar.widget().is_visible(),
        "the filter row must stay visible so \"Clear filters\" is reachable"
    );
    assert_eq!(view.stack.visible_child_name().as_deref(), Some("status"));
    assert_eq!(view.status.title(), "Nothing matches these filters");
    assert_eq!(view.status_button.label().as_deref(), Some("Clear filters"));
}

/// `SRC-10` addendum (Block B2): the "Downloaded" filter matching nothing
/// gets its own copy, distinct from the generic filter-mismatch message —
/// would go red if it fell back to the shared `NoResults` title.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_10_the_downloads_only_view_names_nothing_downloaded_not_a_generic_mismatch() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    subscribe_with_one_episode(&conn);
    reprise_core::library::settings::set_bool(
        &conn,
        reprise_core::podcasts::config::FILTER_DOWNLOADED_KEY,
        true,
    )
    .unwrap();

    let view = view(conn, PodcastKind::Rss);

    assert!(view.filter_bar.widget().is_visible());
    assert_eq!(view.status.title(), "Nothing downloaded yet");
    assert_eq!(view.status_button.label().as_deref(), Some("Clear filters"));
}

/// `SRC-10` addendum (Block B2): a switched-off module with nothing
/// subscribed offers "Enable in Preferences" instead of the ordinary Add
/// button, and clicking it reaches the callback `window.rs` wires to
/// `Preferences::present_online_sources` — not the add dialog.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_10_the_module_off_state_offers_enable_in_preferences_and_never_opens_add() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    // Modules default to disabled — no explicit `set_enabled` call needed.
    let view = view(conn, PodcastKind::Youtube);

    assert_eq!(
        view.stack.visible_child_name().as_deref(),
        Some(MODULE_OFF_PAGE)
    );
    assert!(!view.filter_bar.widget().is_visible());
    assert_eq!(
        view.module_off_state.button_label_text().as_deref(),
        Some("Enable in Preferences")
    );
    assert_ne!(
        view.module_off_state.button_icon_name().as_deref(),
        Some("list-add-symbolic")
    );

    let opened = Rc::new(Cell::new(false));
    let flag = opened.clone();
    view.set_on_open_preferences(move || flag.set(true));
    view.module_off_state.button().emit_clicked();
    assert!(opened.get());
}

/// `SRC-10` addendum (Block B2): B2 only replaces the *empty* case's Add
/// button — an already-populated view must not be locked out just because
/// the module happens to be off right now.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_10_module_off_does_not_hide_an_already_populated_view() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    subscribe_with_one_episode(&conn);

    let view = view(conn, PodcastKind::Rss);

    assert_eq!(view.stack.visible_child_name().as_deref(), Some("list"));
    assert!(view.filter_bar.widget().is_visible());
}

/// `POD-9`: `pod_9_library_summary_*` (`podcasts_presentation.rs`) and
/// `pod_9_library_summary_*` (`strings_podcasts.rs`) pin the pure projection
/// and its string formatting, but neither one calls `podcasts_view::render`
/// — deleting the `self.filter_bar.set_context(...)` call in `render`
/// (`podcasts_view.rs` ~369) would leave both green. This closes that gap on
/// the real, fully wired view: one subscription with one unplayed episode
/// means `library_summary` resolves to `{shows: 1, episodes: 1, new: 1}`,
/// so the header must read exactly what `strings::podcast_library_summary`
/// renders for those numbers.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_9_the_library_summary_header_actually_reaches_the_filter_bar() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    subscribe_with_one_episode(&conn);

    let view = view(conn, PodcastKind::Rss);

    assert_eq!(
        view.filter_bar.result_text(),
        strings::podcast_library_summary(1, 1, 1),
        "render() must hand the real library summary to the filter bar, not leave it stale"
    );
}

/// `POD-15`: the same wired path on the YouTube page must name channels. The
/// string-level test in `strings_podcasts.rs` cannot see which formatter the
/// filter bar picks — hand the RSS one to both pages and this stays green
/// there while failing here.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_15_the_youtube_page_header_counts_channels_not_shows() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    subscribe_one_episode_of_kind(&conn, PodcastKind::Youtube);

    let view = view(conn, PodcastKind::Youtube);

    assert_eq!(
        view.filter_bar.result_text(),
        strings::youtube_library_summary(1, 1, 1),
        "the YouTube page subscribes to channels, so its header must count channels"
    );
}

/// Pumps the GLib main loop until `episode_id`'s recorded download state is
/// terminal (`Downloaded`/`Failed`) or `deadline` passes, returning whatever
/// was last observed. Bounded so a wiring regression that leaves the action
/// inert fails fast instead of hanging the test run.
fn pump_until_terminal(
    view: &PodcastsView,
    episode_id: i64,
    deadline: Instant,
) -> Option<DownloadState> {
    loop {
        let state = view.download_states.borrow().get(&episode_id).cloned();
        if matches!(
            state,
            Some(DownloadState::Downloaded { .. } | DownloadState::Failed { .. })
        ) {
            return state;
        }
        if Instant::now() >= deadline {
            return state;
        }
        gtk4::glib::MainContext::default().iteration(true);
    }
}

/// `POD-13`: the row's retry action must actually retry, not just look
/// clickable. `pod_13_a_failed_download_offers_a_sensitive_retry_action`
/// (`podcasts_groups.rs`) pins the button's icon/tooltip/sensitivity from a
/// pure function and never activates anything, so it would stay green even
/// if `PodcastsView::toggle_download` — the production dispatch the button's
/// `"podcasts.toggle-download"` action name reaches — were deleted. This
/// test closes that gap: it seeds a *stale* failed attempt, activates the
/// real action on the fully wired view (the same one `install_actions`
/// installs and the row's button targets), and requires a *fresh* terminal
/// result distinct from the stale one.
///
/// The episode's `audio_url` points at a loopback port nothing listens on,
/// so the download fails fast and deterministically without depending on
/// this sandbox having outbound network access — `pipeline::download_episode`
/// (proven in `pipeline_refresh_tests.rs`) unconditionally walks
/// `Queued` → `Downloading` → a terminal state for every attempt; this test's
/// job is only to prove the button reaches that pipeline at all, which the
/// synchronous `Queued` set below already does, before either the terminal
/// state or the (frequently coalesced — the progress channel is
/// latest-wins by design, see `one_shot_task::spawn_with_progress`) transient
/// `Downloading` tick can arrive.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_13_activating_the_retry_action_runs_a_fresh_download_attempt() {
    gtk4::init().unwrap();
    let conn = crate::test_db::open().unwrap();
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::PODCASTS_MODULE, true)
        .unwrap();
    let subscription_id = store::add_or_restore(
        &conn,
        &NewSubscription {
            kind: PodcastKind::Rss,
            feed_url: "https://example.test/feed".to_owned(),
            title: "Show".to_owned(),
            author: None,
            image_url: None,
            auto_download: false,
        },
        1,
    )
    .unwrap();
    let episode_id = store::upsert_episode(
        &conn,
        subscription_id,
        &ParsedEpisode {
            guid: "episode".to_owned(),
            title: "Episode".to_owned(),
            image_url: None,
            // Loopback, nothing listening: a same-host connection refusal,
            // so this is fast and deterministic without touching the real
            // network.
            audio_url: "http://127.0.0.1:1/episode.mp3".to_owned(),
            page_url: None,
            published_at: None,
            duration_secs: None,
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id;

    let view = view(conn, PodcastKind::Rss);
    const STALE_MESSAGE: &str = "stale attempt from before this retry";
    view.download_states.borrow_mut().insert(
        episode_id,
        DownloadState::Failed {
            message: STALE_MESSAGE.to_owned(),
        },
    );

    let activated = view
        .root()
        .activate_action("podcasts.toggle-download", Some(&episode_id.to_variant()));
    assert!(
        activated.is_ok(),
        "the row's action must exist and accept an episode id target"
    );

    // `toggle_download` sets `Queued` synchronously, before the worker
    // thread can have replied — this alone proves the click reached the
    // production dispatch rather than doing nothing.
    assert_eq!(
        view.download_states.borrow().get(&episode_id).cloned(),
        Some(DownloadState::Queued),
        "activating the retry action must synchronously start a fresh attempt"
    );

    let terminal = pump_until_terminal(&view, episode_id, Instant::now() + Duration::from_secs(5));
    match terminal {
        Some(DownloadState::Failed { message }) => {
            assert_ne!(
                message, STALE_MESSAGE,
                "the retry must run a fresh attempt, not just redisplay the stale failure"
            );
        }
        other => panic!("expected a fresh terminal Failed state, got {other:?}"),
    }
}

/// `POD-19`: the footer carries the library's status line, and on a load
/// failure it used to carry `DbError`'s `Display` instead. For a rusqlite
/// input error that renders as the message plus the entire failing statement
/// and a byte offset, which is what a real installation showed on both the
/// Podcasts and the YouTube page: "no such column: sync_to_phone in SELECT id,
/// kind, feed_url, … at offset 150". Asserting only the replacement string
/// would still pass if the raw text were appended, so the absence of the
/// statement is asserted separately.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_19_an_unreadable_library_says_so_without_printing_the_query() {
    gtk4::init().unwrap();
    // A schema the subscription query cannot read — the shape the reported
    // failure had. Seeded through the fixture's own connection, since `Db`
    // deliberately exposes no raw SQL (ADR 002).
    //
    // `auto_download` and not `sync_to_phone`, although the report named the
    // latter: `migrate_v40` now repairs its own columns on open, so dropping
    // one of those is healed by `PodcastsRuntime::setup` before the view ever
    // reads anything — the fix in the preceding commit defeats the test.
    // `auto_download` belongs to the v32 table body, which no migration
    // re-adds, so the failure survives to the point this rule is about. The
    // error is the same class either way: a column the SELECT names and the
    // table does not have.
    let fixture = crate::test_db::open().unwrap();
    crate::test_db::connection(&fixture)
        .execute_batch("ALTER TABLE podcast_subscriptions DROP COLUMN auto_download;")
        .unwrap();
    let path = fixture
        .path()
        .expect("the fixture database must be file-backed");
    drop(fixture);
    let conn = Db::open_ready(&path).unwrap();

    let view = view(conn, PodcastKind::Rss);

    let status = view.footer_status.text().to_string();
    assert_eq!(
        status,
        strings::text(strings::PODCAST_LIBRARY_UNREADABLE),
        "a failed load must reach the user as a sentence"
    );
    assert!(
        !status.contains("SELECT") && !status.contains("auto_download"),
        "the failing statement must never reach the footer, got: {status}"
    );
}

/// `SRC-13`: the reveal must hang off the loaded-episode *identity*, not off
/// every external snapshot. Phase changes (Resolving → Playing → Paused) all
/// arrive through the same callback, and centering on each of them would move
/// the list under the reader on every pause tap.
#[test]
fn src_13_reveal_is_driven_by_the_episode_identity_not_the_snapshot() {
    let source = include_str!("podcasts_view_marker.rs");

    assert!(
        source.contains("episode_mark_requires_render"),
        "the reveal must reuse the identity predicate, not re-derive one"
    );
    assert!(
        source.contains("LoadedItemChange::ChangedElsewhere"),
        "an episode changed outside this view is the reveal case"
    );
    assert!(
        source.contains("source_reveal::reveal_policy"),
        "the view must ask the shared policy instead of deciding locally"
    );
}

/// `SRC-13`: revealing is the shared policy's call. A second predicate in the
/// view would be the same duplicated-decision class of bug that made a restart
/// pass for a toggle.
#[test]
fn src_13_the_view_holds_no_second_reveal_predicate() {
    let source = include_str!("podcasts_view_marker.rs");

    assert!(
        !source.contains("USER_SCROLL_GRACE"),
        "the grace period belongs to source_reveal, not to this view"
    );
}
