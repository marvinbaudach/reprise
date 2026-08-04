//! Display-backed geometry tests for the shared podcast and YouTube row.

use super::*;

fn episode(kind: PodcastKind) -> EpisodeRow {
    EpisodeRow {
        id: 1,
        subscription_id: 1,
        guid: "episode".into(),
        title: "A compact episode title".into(),
        show: "Show".into(),
        show_image_url: None,
        image_url: None,
        kind,
        audio_url: "https://example.test/episode.mp3".into(),
        page_url: None,
        published_at: None,
        duration_secs: Some(3_180),
        downloaded_path: None,
        downloaded_bytes: None,
        played_at: None,
        position_ms: 0,
        first_seen_at: 1,
        is_new: true,
    }
}

fn descendants(widget: &gtk4::Widget) -> Vec<gtk4::Widget> {
    let mut found = Vec::new();
    let mut child = widget.first_child();
    while let Some(current) = child {
        found.push(current.clone());
        found.extend(descendants(&current));
        child = current.next_sibling();
    }
    found
}

struct RenderedEpisode {
    _window: gtk4::Window,
    row: gtk4::Box,
}

fn render_single_group(kind: PodcastKind) -> RenderedEpisode {
    crate::ui::style::install_css_string_for_test(&crate::ui::style::app_css_for_test());
    let rendered = RenderedSourceGroup {
        summary: SourceSummary {
            episode_count: 1,
            new_count: 1,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        group: SourceGroup {
            subscription_id: 1,
            title: "Source".into(),
            author: None,
            image_url: None,
            kind,
            sync_to_phone: false,
            episodes: vec![episode(kind)],
        },
    };
    let container = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    replace(
        &container,
        &[rendered],
        None,
        &Rc::new(RefCell::new(BTreeSet::from([1]))),
        &Rc::new(RefCell::new(BTreeSet::new())),
        &BTreeMap::new(),
        &[],
        &BTreeMap::new(),
        false,
        Connectivity::Online,
        None,
        &Rc::new(RefCell::new(PodcastSelection::default())),
    );
    let window = gtk4::Window::builder()
        .default_width(1_200)
        .child(&container)
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();
    let row = descendants(container.upcast_ref())
        .into_iter()
        .find_map(|widget| {
            widget
                .has_css_class("reprise-podcast-episode-row")
                .then(|| widget.downcast::<gtk4::Box>().ok())
                .flatten()
        })
        .expect("one rendered episode row");
    RenderedEpisode {
        _window: window,
        row,
    }
}

fn title_label(rendered: &RenderedEpisode) -> gtk4::Label {
    descendants(rendered.row.upcast_ref())
        .into_iter()
        .find(|widget| widget.has_css_class("reprise-source-row-title"))
        .and_downcast::<gtk4::Label>()
        .expect("episode title")
}

fn menu_button(rendered: &RenderedEpisode) -> gtk4::MenuButton {
    descendants(rendered.row.upcast_ref())
        .into_iter()
        .find_map(|widget| widget.downcast::<gtk4::MenuButton>().ok())
        .expect("episode row menu")
}

fn render_group_header(kind: PodcastKind, author: Option<&str>) -> gtk4::Widget {
    group_header(
        &SourceGroup {
            subscription_id: 1,
            title: "Source".into(),
            author: author.map(str::to_owned),
            image_url: None,
            kind,
            sync_to_phone: false,
            episodes: Vec::new(),
        },
        &SourceSummary {
            episode_count: 0,
            new_count: 0,
            downloaded_bytes: 0,
            latest_published_at: None,
        },
        &[],
        &[],
        false,
    )
}

/// `POD-10`: expansion and opening a page must not compete on the header.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn pod_10_the_channel_header_has_no_arrow_button() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let header = render_group_header(PodcastKind::Youtube, None);
    let icon_names = descendants(&header)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Image>().ok())
        .filter_map(|image| image.icon_name())
        .collect::<Vec<_>>();
    assert!(
        icon_names.iter().all(|name| name != "go-next-symbolic"),
        "the channel header still carries an arrow"
    );
}

/// `SRC-16`: RSS's available second-line data uses the shared quiet identity
/// typography. YouTube has no stored handle field yet, so its half is planned.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_16_the_rss_source_header_types_its_second_line_like_the_shared_grammar() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let header = render_group_header(PodcastKind::Rss, Some("Author"));
    let subtitle = descendants(&header)
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Label>().ok())
        .find(|label| label.text() == "Author")
        .expect("RSS author subtitle");
    assert!(subtitle.has_css_class("caption"));
    assert!(subtitle.has_css_class("dim-label"));
    assert_eq!(subtitle.xalign(), 0.0);
}

/// `SRC-16`: 16:9 and square artwork leave the title at the same x position.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_16_the_title_starts_at_the_same_x_in_both_source_views() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let rss = render_single_group(PodcastKind::Rss);
    let youtube = render_single_group(PodcastKind::Youtube);
    let x = |rendered: &RenderedEpisode| {
        title_label(rendered)
            .compute_bounds(&rendered.row)
            .expect("title bounds")
            .x()
    };
    assert_eq!(x(&rss), x(&youtube));
}

/// `SRC-16`: the shared skeleton also fixes the other axis.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_16_rows_have_the_same_height_in_both_source_views() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    assert_eq!(
        render_single_group(PodcastKind::Rss).row.height(),
        render_single_group(PodcastKind::Youtube).row.height()
    );
}

/// `SRC-16`: status is one chip, not a third item in the detail chain.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_16_a_row_renders_exactly_one_status_chip() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let row = render_single_group(PodcastKind::Rss);
    let descendants = descendants(row.row.upcast_ref());
    assert_eq!(
        descendants
            .iter()
            .filter(|widget| widget.has_css_class("reprise-source-row-chip"))
            .count(),
        1
    );
    let detail = descendants
        .into_iter()
        .filter_map(|widget| widget.downcast::<gtk4::Label>().ok())
        .find(|label| label.has_css_class("dim-label"))
        .expect("episode facts");
    assert!(!detail.text().contains("New"));
}

/// `SRC-17`: the row menu is reserved but transparent at rest.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_17_the_row_menu_button_is_transparent_until_hover_focus_or_selection() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    assert_eq!(
        menu_button(&render_single_group(PodcastKind::Rss)).opacity(),
        0.0
    );
}

/// `SRC-17`: revealing the reserved menu slot moves no identity content.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn src_17_revealing_the_row_menu_button_does_not_move_the_title() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let row = render_single_group(PodcastKind::Rss);
    let before = title_label(&row).width();
    menu_button(&row).set_opacity(1.0);
    crate::ui::source_context_surface::settle_layout();
    assert_eq!(before, title_label(&row).width());
}
