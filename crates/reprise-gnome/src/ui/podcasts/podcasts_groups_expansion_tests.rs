//! UX POD-25: expansion behaviour under an active search.
//!
//! Its own file so `podcasts_groups_tests` stays under the
//! repository's 800-line source-size gate.

use super::*;

fn episode() -> EpisodeRow {
    EpisodeRow {
        id: 1,
        subscription_id: 1,
        guid: "episode".into(),
        title: "Werkzeuge, die wir viel zu selten benutzen".into(),
        show: "Werkbank".into(),
        show_image_url: None,
        image_url: None,
        kind: PodcastKind::Rss,
        audio_url: "https://example.test/episode.mp3".into(),
        page_url: None,
        published_at: None,
        duration_secs: Some(3_180),
        downloaded_path: None,
        downloaded_bytes: None,
        played_at: None,
        position_ms: 0,
        first_seen_at: 1,
        is_new: false,
        media_category: None,
    }
}

/// UX POD-25: a search opens every show that survived it, without recording
/// that as a manual expansion — pull the query and the show is collapsed
/// again, exactly as the user last left it.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_25_a_query_expands_surviving_shows_without_overwriting_manual_state() {
    gtk4::init().unwrap();
    let rendered = RenderedSourceGroup {
        summary: SourceSummary {
            episode_count: 1,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        group: SourceGroup {
            subscription_id: 1,
            title: "Werkbank".into(),
            author: None,
            image_url: None,
            kind: PodcastKind::Rss,
            episodes: vec![episode()],
        },
    };
    // The show is collapsed by the user's own state: not in the set.
    let expanded_sources = Rc::new(RefCell::new(BTreeSet::new()));

    let render = |query: &str| {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        replace(
            &container,
            std::slice::from_ref(&rendered),
            None,
            &expanded_sources,
            &Rc::new(RefCell::new(BTreeSet::new())),
            &BTreeMap::new(),
            false,
            Connectivity::Online,
            None,
            &Rc::new(RefCell::new(PodcastSelection::default())),
            query,
        );
        container
            .first_child()
            .and_downcast::<gtk4::Expander>()
            .expect("one expander per show")
    };

    assert!(
        !render("").is_expanded(),
        "precondition: this show is collapsed"
    );
    assert!(
        render("wer").is_expanded(),
        "a query must open the show it matched in"
    );
    assert!(
        expanded_sources.borrow().is_empty(),
        "forcing a show open for a search must not be recorded as a manual expansion"
    );
    assert!(
        !render("").is_expanded(),
        "removing the query hands the show back its own collapsed state"
    );
}
