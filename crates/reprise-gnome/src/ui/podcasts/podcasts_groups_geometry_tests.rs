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
            sync_to_phone: false,
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
            &[],
            &BTreeMap::new(),
            false,
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
