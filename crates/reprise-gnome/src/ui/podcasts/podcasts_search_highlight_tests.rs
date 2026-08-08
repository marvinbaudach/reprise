//! FIL-5a display proofs for the shared Podcasts and YouTube episode row.

use super::*;

fn episode(kind: PodcastKind, title: &str) -> EpisodeRow {
    EpisodeRow {
        id: 1,
        subscription_id: 1,
        guid: "episode".into(),
        title: title.into(),
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
        is_new: false,
        media_category: None,
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

fn label_has_hit_tint(label: &gtk4::Label) -> bool {
    label.layout().attributes().is_some_and(|attributes| {
        attributes
            .iterator()
            .into_iter()
            .flatten()
            .any(|attribute| attribute.type_() == gtk4::pango::AttrType::BackgroundAlpha)
    })
}

fn rendered_episode_title(
    kind: PodcastKind,
    title: &str,
    parts: &TitleParts,
    query: &str,
) -> (gtk4::Widget, gtk4::Label) {
    let row = episode(kind, title);
    let mut widgets = RenderedRowWidgets {
        downloads: BTreeMap::new(),
        selection: BTreeMap::new(),
        channels: BTreeMap::new(),
    };
    let rendered = episode_row(
        &row,
        parts,
        &mut widgets,
        &EpisodeRenderContext {
            mark: None,
            download_state: &DownloadState::NotDownloaded,
            images_allowed: false,
            network: RowNetworkState {
                connectivity: Connectivity::Online,
                unavailable_now: false,
            },
            selection: &Rc::new(RefCell::new(PodcastSelection::default())),
            paths: &Rc::new(EpisodePaths::from_rows(&[])),
            unavailable_episode: None,
            query,
        },
    );
    let title = descendants(&rendered)
        .into_iter()
        .find(|widget| widget.has_css_class("reprise-source-row-title"))
        .and_downcast::<gtk4::Label>()
        .expect("rendered episode title");
    (rendered, title)
}

/// UX FIL-5a: the Podcasts row marks a matching episode title and does not
/// mark its unsearched detail line.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_5a_podcasts_marks_episode_title_but_not_details() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (row, title) = rendered_episode_title(
        PodcastKind::Rss,
        "A compact episode title",
        &TitleParts {
            distinct: "A compact episode title".into(),
            dimmed: None,
        },
        "compact",
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .child(&row)
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();

    assert!(label_has_hit_tint(&title), "episode-title hit has no tint");
    assert!(descendants(&row).into_iter().any(|widget| {
        widget
            .downcast::<gtk4::Label>()
            .is_ok_and(|label| label.has_css_class("dim-label") && !label_has_hit_tint(&label))
    }));
}

/// UX FIL-5a: YouTube's dimmed channel tail is still part of the searched
/// episode title, so a hit there is marked instead of being visually hidden.
#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn fil_5a_youtube_marks_a_hit_in_the_dimmed_channel_tail() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (row, title) = rendered_episode_title(
        PodcastKind::Youtube,
        "A compact subject | Werkbank",
        &TitleParts {
            distinct: "A compact subject".into(),
            dimmed: Some(" | Werkbank".into()),
        },
        "werk",
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .child(&row)
        .build();
    window.present();
    crate::ui::source_context_surface::settle_layout();

    assert_eq!(title.text(), "A compact subject | Werkbank");
    assert!(
        label_has_hit_tint(&title),
        "searched YouTube channel tail has no hit tint"
    );
}
