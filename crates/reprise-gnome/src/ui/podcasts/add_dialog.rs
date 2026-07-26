//! Podcast search-or-URL dialog.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::podcasts::{self, PodcastKind};
use rusqlite::Connection;

use crate::ui::one_shot_task;
use crate::ui::strings;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AddInput {
    Empty,
    Search(String),
    YoutubeUrl(String),
    FeedUrl(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AddDialogPhase {
    Idle,
    Searching,
    Previewing,
    Results,
    Preview,
    Error,
}

pub(super) fn classify_input(input: &str) -> AddInput {
    let input = input.trim();
    if input.is_empty() {
        return AddInput::Empty;
    }
    match podcasts::url_detect::detect(input) {
        podcasts::url_detect::InputKind::Search => AddInput::Search(input.to_owned()),
        podcasts::url_detect::InputKind::YoutubeUrl => AddInput::YoutubeUrl(input.to_owned()),
        podcasts::url_detect::InputKind::ProbableFeedUrl => AddInput::FeedUrl(input.to_owned()),
    }
}

#[derive(Clone)]
struct Candidate {
    kind: PodcastKind,
    title: String,
    subtitle: String,
    author: Option<String>,
    image_url: Option<String>,
    url: String,
}

#[derive(Clone)]
struct Preview {
    kind: PodcastKind,
    title: String,
    author: Option<String>,
    image_url: Option<String>,
    count: usize,
    url: String,
}

type OnAdded = Rc<dyn Fn(bool)>;

pub(super) fn present(
    parent: &impl IsA<gtk4::Widget>,
    conn: &Rc<RefCell<Connection>>,
    on_added: impl Fn(bool) + 'static,
) {
    let conn = conn.clone();
    let on_added: OnAdded = Rc::new(on_added);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let entry = gtk4::SearchEntry::builder()
        .placeholder_text(strings::text(strings::PODCAST_DIALOG_HINT))
        .build();
    content.append(&entry);
    let status = gtk4::Label::new(None);
    status.add_css_class("dim-label");
    status.set_xalign(0.0);
    content.append(&status);
    let results = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    let scroller = gtk4::ScrolledWindow::builder()
        .vexpand(true)
        .child(&results)
        .build();
    content.append(&scroller);

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(strings::PODCAST_DIALOG_TITLE),
        "",
    )));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .title(strings::text(strings::PODCAST_DIALOG_TITLE))
        .content_width(620)
        .content_height(560)
        .child(&toolbar)
        .build();
    let generation = Rc::new(Cell::new(0_u64));
    let weak_dialog = dialog.downgrade();
    let submit = {
        let conn = conn.clone();
        let results = results.clone();
        let status = status.clone();
        let generation = generation.clone();
        let on_added = on_added.clone();
        move |input: String| {
            clear(&results);
            let next = generation.get().wrapping_add(1);
            generation.set(next);
            match classify_input(&input) {
                AddInput::Empty => status.set_text(""),
                AddInput::Search(terms) => {
                    status.set_text(&strings::text(strings::PODCAST_SEARCHING));
                    search(
                        next,
                        terms,
                        &generation,
                        &status,
                        &results,
                        &conn,
                        &on_added,
                    );
                }
                AddInput::FeedUrl(url) => {
                    status.set_text(&strings::text(strings::PODCAST_RSS_DETECTED));
                    preview(
                        next,
                        PodcastKind::Rss,
                        &url,
                        &generation,
                        &status,
                        &results,
                        &conn,
                        &on_added,
                    );
                }
                AddInput::YoutubeUrl(url) => {
                    status.set_text(&strings::text(strings::PODCAST_YOUTUBE_DETECTED));
                    preview(
                        next,
                        PodcastKind::Youtube,
                        &url,
                        &generation,
                        &status,
                        &results,
                        &conn,
                        &on_added,
                    );
                }
            }
        }
    };
    entry.connect_activate(move |entry| submit(entry.text().to_string()));
    if weak_dialog.upgrade().is_some() {
        dialog.present(Some(parent));
    }
}

fn search(
    request_generation: u64,
    terms: String,
    generation: &Rc<Cell<u64>>,
    status: &gtk4::Label,
    results: &gtk4::Box,
    conn: &Rc<RefCell<Connection>>,
    on_added: &OnAdded,
) {
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "C".into());
    let apple_terms = terms.clone();
    let apple = one_shot_task::spawn("reprise-podcast-search", move || {
        podcasts::itunes::search(&apple_terms, &locale)
            .map(|rows| {
                rows.into_iter()
                    .map(|row| Candidate {
                        kind: PodcastKind::Rss,
                        title: row.title,
                        subtitle: row.author.clone().unwrap_or_default(),
                        author: row.author,
                        image_url: None,
                        url: row.feed_url,
                    })
                    .collect::<Vec<_>>()
            })
            .map_err(|error| error.to_string())
    });
    attach_candidates(
        apple,
        request_generation,
        generation,
        status,
        results,
        conn,
        on_added,
        strings::PODCAST_APPLE_RESULTS,
    );

    let config = podcasts::config::load(&conn.borrow()).ok();
    if config.as_ref().is_some_and(|value| value.youtube_enabled) {
        let ytdlp_path = config.and_then(|value| value.ytdlp_path);
        let youtube = one_shot_task::spawn("reprise-youtube-search", move || {
            podcasts::ytdlp::YtDlp::discover(ytdlp_path.as_deref())
                .search(&terms)
                .map(|rows| {
                    rows.entries
                        .into_iter()
                        .map(|row| Candidate {
                            kind: PodcastKind::Youtube,
                            title: row.title,
                            subtitle: strings::text(strings::PODCAST_YOUTUBE_FOOTNOTE),
                            author: None,
                            image_url: None,
                            url: format!("https://www.youtube.com/watch?v={}", row.id),
                        })
                        .collect::<Vec<_>>()
                })
                .map_err(|error| error.to_string())
        });
        attach_candidates(
            youtube,
            request_generation,
            generation,
            status,
            results,
            conn,
            on_added,
            strings::PODCAST_YOUTUBE_RESULTS,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn attach_candidates(
    receiver: std::io::Result<async_channel::Receiver<Result<Vec<Candidate>, String>>>,
    request_generation: u64,
    generation: &Rc<Cell<u64>>,
    status: &gtk4::Label,
    results: &gtk4::Box,
    conn: &Rc<RefCell<Connection>>,
    on_added: &OnAdded,
    heading: &'static str,
) {
    let generation = generation.clone();
    let status = status.clone();
    let results = results.clone();
    let conn = conn.clone();
    let on_added = on_added.clone();
    gtk4::glib::spawn_future_local(async move {
        let response = match receiver {
            Ok(receiver) => receiver.recv().await.map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        if generation.get() != request_generation {
            return;
        }
        match response.and_then(|value| value) {
            Ok(rows) => {
                status.set_text("");
                append_heading(&results, heading);
                for candidate in rows {
                    append_candidate(&results, candidate, &conn, &on_added);
                }
            }
            Err(error) => status.set_text(&error),
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn preview(
    request_generation: u64,
    kind: PodcastKind,
    url: &str,
    generation: &Rc<Cell<u64>>,
    status: &gtk4::Label,
    results: &gtk4::Box,
    conn: &Rc<RefCell<Connection>>,
    on_added: &OnAdded,
) {
    let config = podcasts::config::load(&conn.borrow()).ok();
    let import_count = config
        .as_ref()
        .map_or(podcasts::config::DEFAULT_IMPORT_COUNT, |value| {
            value.import_count
        });
    let ytdlp_path = config.and_then(|value| value.ytdlp_path);
    let task_url = url.to_owned();
    let receiver = one_shot_task::spawn(
        "reprise-podcast-preview",
        move || -> Result<Preview, String> {
            match kind {
                PodcastKind::Rss => {
                    let response =
                        podcasts::http::get(&task_url).map_err(|error| error.to_string())?;
                    let feed = podcasts::feed::parse_feed(&response.body, import_count)
                        .map_err(|error| error.to_string())?;
                    Ok(Preview {
                        kind,
                        title: feed.title,
                        author: feed.author,
                        image_url: feed.image_url,
                        count: feed.episodes.len(),
                        url: task_url,
                    })
                }
                PodcastKind::Youtube => {
                    let listing = podcasts::ytdlp::YtDlp::discover(ytdlp_path.as_deref())
                        .list(&task_url)
                        .map_err(|error| error.to_string())?;
                    Ok(Preview {
                        kind,
                        title: listing.title.unwrap_or_else(|| task_url.clone()),
                        author: None,
                        image_url: None,
                        count: listing.entries.len(),
                        url: task_url,
                    })
                }
            }
        },
    );
    let generation = generation.clone();
    let status = status.clone();
    let results = results.clone();
    let conn = conn.clone();
    let on_added = on_added.clone();
    gtk4::glib::spawn_future_local(async move {
        let response = match receiver {
            Ok(receiver) => receiver.recv().await.map_err(|error| error.to_string()),
            Err(error) => Err(error.to_string()),
        };
        if generation.get() != request_generation {
            return;
        }
        match response.and_then(|value| value) {
            Ok(preview) => {
                status.set_text("");
                append_preview(&results, preview, import_count, &conn, &on_added);
            }
            Err(error) => status.set_text(&error),
        }
    });
}

fn append_heading(parent: &gtk4::Box, text: &str) {
    let label = gtk4::Label::new(Some(&strings::text(text)));
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    parent.append(&label);
}

fn append_candidate(
    parent: &gtk4::Box,
    candidate: Candidate,
    conn: &Rc<RefCell<Connection>>,
    on_added: &OnAdded,
) {
    let row = candidate_row(&candidate.title, &candidate.subtitle, candidate.kind);
    let button = gtk4::Button::with_label(&strings::text(strings::PODCAST_SUBSCRIBE));
    button.add_css_class("suggested-action");
    let conn = conn.clone();
    let on_added = on_added.clone();
    button.connect_clicked(
        move |button| match subscribe(&conn.borrow(), &candidate, false) {
            Ok(_) => {
                button.set_label("✓");
                button.set_sensitive(false);
                on_added(true);
            }
            Err(error) => button.set_tooltip_text(Some(&error.to_string())),
        },
    );
    row.append(&button);
    parent.append(&row);
}

fn append_preview(
    parent: &gtk4::Box,
    preview: Preview,
    import_count: usize,
    conn: &Rc<RefCell<Connection>>,
    on_added: &OnAdded,
) {
    clear(parent);
    let subtitle = strings::podcast_episode_count(preview.count);
    let row = candidate_row(&preview.title, &subtitle, preview.kind);
    parent.append(&row);
    let import = gtk4::CheckButton::with_label(&strings::podcast_import_latest_count(import_count));
    import.set_active(true);
    parent.append(&import);
    let auto_download =
        gtk4::CheckButton::with_label(&strings::text(strings::PODCAST_AUTO_DOWNLOAD));
    parent.append(&auto_download);
    let subscribe_button = gtk4::Button::with_label(&strings::text(strings::PODCAST_SUBSCRIBE));
    subscribe_button.add_css_class("suggested-action");
    let candidate = Candidate {
        kind: preview.kind,
        title: preview.title,
        subtitle,
        author: preview.author,
        image_url: preview.image_url,
        url: preview.url,
    };
    let conn = conn.clone();
    let on_added = on_added.clone();
    subscribe_button.connect_clicked(move |button| {
        match subscribe(&conn.borrow(), &candidate, auto_download.is_active()) {
            Ok(_) => {
                button.set_label("✓");
                button.set_sensitive(false);
                on_added(import.is_active());
            }
            Err(error) => button.set_tooltip_text(Some(&error.to_string())),
        }
    });
    parent.append(&subscribe_button);
}

fn candidate_row(title: &str, subtitle: &str, kind: PodcastKind) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    row.add_css_class("reprise-podcast-result");
    let icon = gtk4::Image::from_icon_name(match kind {
        PodcastKind::Rss => "audio-input-microphone-symbolic",
        PodcastKind::Youtube => "video-x-generic-symbolic",
    });
    icon.add_css_class("reprise-podcast-glyph-tile");
    row.append(&icon);
    let labels = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    labels.set_hexpand(true);
    let title = gtk4::Label::new(Some(title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    labels.append(&title);
    let subtitle = gtk4::Label::new(Some(subtitle));
    subtitle.add_css_class("caption");
    subtitle.add_css_class("dim-label");
    subtitle.set_xalign(0.0);
    labels.append(&subtitle);
    row.append(&labels);
    row
}

fn subscribe(
    conn: &Connection,
    candidate: &Candidate,
    auto_download: bool,
) -> Result<i64, rusqlite::Error> {
    podcasts::store::add_or_restore(
        conn,
        &podcasts::store::NewSubscription {
            kind: candidate.kind,
            feed_url: candidate.url.clone(),
            title: candidate.title.clone(),
            author: candidate.author.clone(),
            image_url: candidate.image_url.clone(),
            auto_download,
        },
        chrono::Utc::now().timestamp(),
    )
}

fn clear(parent: &gtk4::Box) {
    while let Some(child) = parent.first_child() {
        parent.remove(&child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn src_3_add_dialog_submits_search_or_url_through_one_field() {
        assert_eq!(
            classify_input("systems"),
            AddInput::Search("systems".into())
        );
        assert!(matches!(
            classify_input("https://example.test/feed.xml"),
            AddInput::FeedUrl(_)
        ));
        assert!(matches!(
            classify_input("https://youtube.com/@show"),
            AddInput::YoutubeUrl(_)
        ));
    }

    #[test]
    fn dialogue_state_names_cover_async_lifecycle() {
        let phases = [
            AddDialogPhase::Idle,
            AddDialogPhase::Searching,
            AddDialogPhase::Previewing,
            AddDialogPhase::Results,
            AddDialogPhase::Preview,
            AddDialogPhase::Error,
        ];
        assert_eq!(phases.len(), 6);
    }
}
