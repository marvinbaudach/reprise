//! UX POD-25: expansion behaviour under an active search.
//!
//! Its own file so `podcasts_groups_tests` stays under the
//! repository's 800-line source-size gate.

use super::*;
use std::cell::Cell;

fn rendered_group(episode_count: i64) -> RenderedSourceGroup {
    let episodes = (1..=episode_count)
        .map(|id| {
            let mut row = episode();
            row.id = id;
            row.guid = format!("episode-{id}");
            row.title = format!("Episode {id}");
            row.image_url = Some(format!("https://images.test/episode-{id}.jpg"));
            row
        })
        .collect::<Vec<_>>();
    RenderedSourceGroup {
        summary: SourceSummary {
            episode_count: episodes.len(),
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
            episodes,
        },
    }
}

fn render_with_counted_episode_artwork(
    rendered: &RenderedSourceGroup,
    expanded_sources: &Rc<RefCell<BTreeSet<i64>>>,
    query: &str,
    cached_images_allowed: bool,
    submissions: &Rc<Cell<usize>>,
) -> gtk4::Expander {
    let conn = Rc::new(crate::test_db::open().unwrap());
    reprise_core::modules::set_enabled(&conn, &reprise_core::modules::ARTWORK_MODULE, true)
        .unwrap();
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let submissions_for_factory = submissions.clone();
    let artwork = Rc::new(
        move |_: &EpisodeRow,
              network_policy: ArtworkNetworkPolicy,
              load_policy: ArtworkLoadPolicy| {
            let label = gtk4::Label::new(None);
            if network_policy.is_allowed() && matches!(load_policy, ArtworkLoadPolicy::Load) {
                submissions_for_factory.set(submissions_for_factory.get() + 1);
                label.add_css_class("test-episode-cover");
            }
            (
                label.upcast::<gtk4::Widget>(),
                crate::ui::source_row::MediaShape::Square,
            )
        },
    );
    let groups = std::slice::from_ref(rendered);
    let expanded_episode_sources = Rc::new(RefCell::new(BTreeSet::new()));
    let download_states = BTreeMap::new();
    let selection = Rc::new(RefCell::new(PodcastSelection::default()));
    let syncing = HashMap::new();
    let paths = Rc::new(EpisodePaths::from_row_refs(snapshot_rows(groups)));
    let context = GroupRenderContext {
        playing_episode: None,
        expanded_sources,
        query,
        expanded_episode_sources: &expanded_episode_sources,
        download_states: &download_states,
        images_allowed: cached_images_allowed,
        conn: &conn,
        connectivity: Connectivity::Online,
        unavailable_episode: None,
        selection: &selection,
        paths: &paths,
        syncing: &syncing,
        episode_artwork: artwork,
    };
    replace_with_sync_and_artwork(&container, groups, &context);
    container
        .first_child()
        .and_downcast::<gtk4::Expander>()
        .expect("one expander per show")
}

fn descendant_count_with_class(root: &gtk4::Widget, class: &str) -> usize {
    let mut count = usize::from(root.has_css_class(class));
    let mut child = root.first_child();
    while let Some(widget) = child {
        count += descendant_count_with_class(&widget, class);
        child = widget.next_sibling();
    }
    count
}

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
            &Rc::new(crate::test_db::open().unwrap()),
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

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn collapsed_group_submits_episode_artwork_only_on_its_first_expansion() {
    gtk4::init().unwrap();
    let rendered = rendered_group(2);
    let submissions = Rc::new(Cell::new(0));
    let expander = render_with_counted_episode_artwork(
        &rendered,
        &Rc::new(RefCell::new(BTreeSet::new())),
        "",
        true,
        &submissions,
    );

    assert!(!expander.is_expanded());
    assert_eq!(submissions.get(), 0, "collapsed rows must submit nothing");

    expander.set_expanded(true);
    assert_eq!(submissions.get(), 2, "each row submits exactly once");
    assert_eq!(
        descendant_count_with_class(expander.upcast_ref(), "test-episode-cover"),
        2,
        "both rows show their loaded cover"
    );

    expander.set_expanded(false);
    expander.set_expanded(true);
    assert_eq!(
        submissions.get(),
        2,
        "re-expansion reuses the loaded covers"
    );
    assert_eq!(
        descendant_count_with_class(expander.upcast_ref(), "test-episode-cover"),
        2,
        "collapsing and re-expanding must not lose a cover"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn collapsed_group_uses_the_live_artwork_gate_on_its_first_expansion() {
    gtk4::init().unwrap();
    let rendered = rendered_group(1);
    let submissions = Rc::new(Cell::new(0));
    let expander = render_with_counted_episode_artwork(
        &rendered,
        &Rc::new(RefCell::new(BTreeSet::new())),
        "",
        false,
        &submissions,
    );

    assert!(!expander.is_expanded());
    assert_eq!(submissions.get(), 0, "collapsed rows must submit nothing");

    expander.set_expanded(true);
    assert_eq!(
        submissions.get(),
        1,
        "expansion must recompute the live gate instead of replaying the cached false value"
    );
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn search_expanded_group_submits_episode_artwork_during_render() {
    gtk4::init().unwrap();
    let rendered = rendered_group(2);
    let submissions = Rc::new(Cell::new(0));

    let expander = render_with_counted_episode_artwork(
        &rendered,
        &Rc::new(RefCell::new(BTreeSet::new())),
        "episode",
        true,
        &submissions,
    );

    assert!(expander.is_expanded());
    assert_eq!(submissions.get(), 2, "auto-expanded rows submit normally");
}
