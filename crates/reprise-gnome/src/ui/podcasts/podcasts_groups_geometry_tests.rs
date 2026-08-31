//! Isolated display evidence for podcast channel-header geometry.

use super::*;
use std::cell::Cell;

fn episode() -> EpisodeRow {
    EpisodeRow {
        id: 1,
        subscription_id: 1,
        guid: "episode".into(),
        title: "A compact episode title".into(),
        show: "Show".into(),
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

fn rendered_group(kind: PodcastKind, id: i64) -> RenderedSourceGroup {
    let mut row = episode();
    row.id = id;
    row.subscription_id = id;
    row.kind = kind;
    RenderedSourceGroup {
        summary: SourceSummary {
            episode_count: 1,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        group: SourceGroup {
            subscription_id: id,
            title: match kind {
                PodcastKind::Rss => "Show".into(),
                PodcastKind::Youtube => "Channel".into(),
            },
            author: Some("Publisher".into()),
            image_url: None,
            kind,
            episodes: vec![row],
        },
    }
}

fn descendant_with_class(widget: &gtk4::Widget, class: &str) -> Option<gtk4::Widget> {
    let mut child = widget.first_child();
    while let Some(current) = child {
        if current.has_css_class(class) {
            return Some(current);
        }
        if let Some(found) = descendant_with_class(&current, class) {
            return Some(found);
        }
        child = current.next_sibling();
    }
    None
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_13_replace_returns_one_channel_widget_per_group() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();

    let rendered = || RenderedSourceGroup {
        summary: SourceSummary {
            episode_count: 1,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        group: SourceGroup {
            subscription_id: 1,
            title: "Show".into(),
            author: Some("Publisher".into()),
            image_url: None,
            kind: PodcastKind::Rss,
            episodes: vec![episode()],
        },
    };
    let render = |expanded: bool| {
        let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        let widgets = replace(
            &container,
            &[rendered()],
            None,
            &Rc::new(RefCell::new(if expanded {
                BTreeSet::from([1])
            } else {
                BTreeSet::new()
            })),
            &Rc::new(RefCell::new(BTreeSet::new())),
            &BTreeMap::new(),
            false,
            &Rc::new(crate::test_db::open().unwrap()),
            Connectivity::Online,
            None,
            &Rc::new(RefCell::new(PodcastSelection::default())),
            "",
        );
        (container, widgets)
    };
    let (collapsed_container, collapsed) = render(false);
    let (expanded_container, expanded) = render(true);
    assert_eq!(collapsed.channels.len(), 1);
    assert_eq!(expanded.channels.len(), 1);

    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    root.append(&collapsed_container);
    root.append(&expanded_container);
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(700)
        .child(&root)
        .build();
    window.present();

    let collapsed = collapsed.channels[&1].clone();
    let expanded = expanded.channels[&1].clone();
    let measured = Rc::new(Cell::new(None::<(i32, i32)>));
    let main_loop = gtk4::glib::MainLoop::new(None, false);
    let timeout_loop = main_loop.clone();
    gtk4::glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
        timeout_loop.quit();
    });
    let tick_loop = main_loop.clone();
    let tick_window = window.clone();
    let tick_measured = measured.clone();
    gtk4::glib::idle_add_local_once(move || {
        tick_window.add_tick_callback(move |_, _| {
            let values = (collapsed.header.height(), expanded.header.height());
            println!(
                "podcast header geometry: collapsed header={}; expanded header={}",
                values.0, values.1
            );
            tick_measured.set(Some(values));
            tick_loop.quit();
            gtk4::glib::ControlFlow::Break
        });
    });
    main_loop.run();
    let (collapsed_header, expanded_header) = measured.get().expect("the post-idle tick must run");
    assert!(collapsed_header > 0);
    assert!(expanded_header > 0);
    window.close();
}

/// `SRC-16`: group and episode rows use one media grid, with the episode's
/// artwork beginning after the group's artwork in both source views.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_16_episode_media_starts_after_group_media_in_both_source_views() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());

    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    let groups = [
        rendered_group(PodcastKind::Rss, 1),
        rendered_group(PodcastKind::Youtube, 2),
    ];
    let widgets = replace(
        &container,
        &groups,
        None,
        &Rc::new(RefCell::new(BTreeSet::from([1, 2]))),
        &Rc::new(RefCell::new(BTreeSet::new())),
        &BTreeMap::new(),
        false,
        &Rc::new(crate::test_db::open().unwrap()),
        Connectivity::Online,
        None,
        &Rc::new(RefCell::new(PodcastSelection::default())),
        "",
    );
    let window = gtk4::Window::builder()
        .default_width(1_200)
        .default_height(500)
        .child(&container)
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();

    for (id, source) in [(1, "podcast"), (2, "YouTube")] {
        let group_artwork = descendant_with_class(
            &widgets.channels[&id].header,
            "reprise-podcast-group-artwork",
        )
        .expect("group artwork widget");
        let episode_artwork = descendant_with_class(
            widgets.selection[&id].row.upcast_ref(),
            "reprise-podcast-episode-thumbnail",
        )
        .expect("episode artwork widget");
        let group_bounds = group_artwork
            .compute_bounds(&window)
            .expect("group artwork has rendered bounds");
        let episode_bounds = episode_artwork
            .compute_bounds(&window)
            .expect("episode artwork has rendered bounds");
        let group_media_bounds = group_artwork
            .parent()
            .expect("group artwork has a media host")
            .compute_bounds(&window)
            .expect("group media host has rendered bounds");
        let episode_media_bounds = episode_artwork
            .parent()
            .expect("episode artwork has a media slot")
            .compute_bounds(&window)
            .expect("episode media slot has rendered bounds");
        assert_eq!(group_bounds.width(), 40.0, "{source} group artwork width");
        assert_eq!(group_bounds.height(), 40.0, "{source} group artwork height");
        assert_eq!(
            (group_media_bounds.width(), group_media_bounds.height()),
            (
                crate::ui::source_row::MEDIA_WIDTH as f32,
                crate::ui::source_row::MEDIA_HEIGHT as f32,
            ),
            "{source} group media host must use the shared bounds"
        );
        assert_eq!(
            (episode_media_bounds.width(), episode_media_bounds.height()),
            (
                crate::ui::source_row::MEDIA_WIDTH as f32,
                crate::ui::source_row::MEDIA_HEIGHT as f32,
            ),
            "{source} episode media slot must use the shared bounds"
        );
        assert_eq!(
            group_bounds.x() + group_bounds.width() / 2.0,
            group_media_bounds.x() + group_media_bounds.width() / 2.0,
            "{source} group artwork must be centred in the shared media host"
        );
        assert_eq!(
            episode_bounds.x() + episode_bounds.width() / 2.0,
            episode_media_bounds.x() + episode_media_bounds.width() / 2.0,
            "{source} episode artwork must be centred in the shared media slot"
        );
        assert!(
            episode_bounds.x() > group_bounds.x(),
            "{source} episode artwork must start after group artwork: group={group_bounds:?}, episode={episode_bounds:?}"
        );
    }
    window.close();
}
