//! Display tests for the four Block B2 empty states. Split out of
//! `podcasts_view.rs` to keep it under the file-size gate.

use reprise_core::podcasts::feed::ParsedEpisode;
use reprise_core::podcasts::store::{self, NewSubscription};

use super::*;

/// Builds a `PodcastsView` over a freshly migrated in-memory-backed DB.
/// Callers seed subscriptions/episodes/persisted filter values on `conn`
/// *before* calling this, since `PodcastsFilterBar::new` and the initial
/// `refresh()` both read the DB at construction time.
fn view(conn: rusqlite::Connection, kind: PodcastKind) -> Rc<PodcastsView> {
    let runtime = PodcastsRuntime::setup(&conn);
    let conn = Rc::new(RefCell::new(conn));
    PodcastsView::install(conn, runtime, PodcastsCallbacks::default(), kind)
}

fn subscribe_with_one_episode(conn: &rusqlite::Connection) -> i64 {
    let subscription_id = store::add_or_restore(
        conn,
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
    store::upsert_episode(
        conn,
        subscription_id,
        &ParsedEpisode {
            guid: "episode".to_owned(),
            title: "Episode".to_owned(),
            audio_url: "https://example.test/episode.mp3".to_owned(),
            page_url: None,
            published_at: None,
            duration_secs: None,
        },
        1,
    )
    .unwrap()
    .unwrap()
    .episode_id
}

/// `SRC-10`: the genuine "nothing subscribed yet" empty state hides the
/// filter row and the footer — would go red if either stayed visible over
/// zero subscriptions.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_10_the_true_empty_state_hides_the_filter_row_and_the_footer() {
    gtk4::init().unwrap();
    let conn = reprise_core::db::open_migrated(None).unwrap();
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
    let conn = reprise_core::db::open_migrated(None).unwrap();
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
    let conn = reprise_core::db::open_migrated(None).unwrap();
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
    let conn = reprise_core::db::open_migrated(None).unwrap();
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
    let conn = reprise_core::db::open_migrated(None).unwrap();
    subscribe_with_one_episode(&conn);

    let view = view(conn, PodcastKind::Rss);

    assert_eq!(view.stack.visible_child_name().as_deref(), Some("list"));
    assert!(view.filter_bar.widget().is_visible());
}
