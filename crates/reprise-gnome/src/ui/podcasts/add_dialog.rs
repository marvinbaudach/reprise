//! Podcast search-or-URL dialog.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::connectivity::Connectivity;
use reprise_core::podcasts::discovery::{
    active_source_keys, dialog_provider, filter_unsubscribed, source_is_subscribed, Candidate,
};
use reprise_core::podcasts::{self, PodcastKind};
use rusqlite::Connection;

use crate::ui::one_shot_task;
use crate::ui::source_add_action;
use crate::ui::strings;

use super::add_dialog_input::{
    classify_input, dialog_hint, dialog_status_hint, dialog_title, primary_action_for_connectivity,
    submit_refusal, AddInput,
};
use super::add_dialog_results::{clear, result_section, rss_candidate, youtube_candidate};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AddDialogPhase {
    Idle,
    Searching,
    Previewing,
    Results,
    Preview,
    Error,
}

#[derive(Clone)]
struct Preview {
    kind: PodcastKind,
    title: String,
    author: Option<String>,
    image_url: Option<String>,
    count: usize,
    url: String,
    guids: Vec<String>,
}

type OnAdded = Rc<dyn Fn(bool)>;

struct SearchContext<'a> {
    generation: &'a Rc<Cell<u64>>,
    status: &'a gtk4::Label,
    results: &'a gtk4::Box,
    conn: &'a Rc<RefCell<Connection>>,
    on_added: &'a OnAdded,
    preferred_kind: PodcastKind,
}

struct AddDialogSurface {
    dialog: adw::Dialog,
    entry: gtk4::SearchEntry,
    status: gtk4::Label,
    results: gtk4::Box,
    cancel: gtk4::Button,
    primary: gtk4::Button,
}

fn build_surface(kind: PodcastKind, connectivity: Connectivity) -> AddDialogSurface {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let entry = gtk4::SearchEntry::builder()
        .placeholder_text(strings::text(dialog_hint(kind)))
        .build();
    content.append(&entry);
    let status = gtk4::Label::new(None);
    status.add_css_class("dim-label");
    status.set_xalign(0.0);
    content.append(&status);
    let results = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
    // SRC-8: vertical scrolling only. Without this the widest result row adds a
    // horizontal scrollbar and pushes the row actions past the viewport edge.
    let scroller = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .vexpand(true)
        .child(&results)
        .build();
    // Keep the rows clear of the overlay scrollbar so no action sits under it.
    results.set_margin_end(6);
    content.append(&scroller);

    // SRC-7: say once why an added source stops appearing, instead of letting
    // it vanish unexplained on the next search.
    let footnote = gtk4::Label::new(Some(&strings::text(strings::SOURCE_SUBSCRIBED_DROP_OUT)));
    footnote.add_css_class("caption");
    footnote.add_css_class("dim-label");
    footnote.set_xalign(0.0);
    footnote.set_wrap(true);
    content.append(&footnote);

    let cancel = gtk4::Button::with_label(&strings::text(strings::PODCAST_CANCEL));
    let primary = gtk4::Button::with_label(&strings::text(strings::PODCAST_SEARCH));
    primary.add_css_class("suggested-action");
    primary.set_sensitive(false);
    let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    footer.set_halign(gtk4::Align::End);
    footer.append(&cancel);
    footer.append(&primary);
    content.append(&footer);

    // `NET-3` point 4: the reason offline search is unavailable is visible
    // immediately, before the user types anything — not only after a first
    // failed attempt.
    set_status_hint(&status, &AddInput::Empty, kind, connectivity);
    let primary_for_entry = primary.clone();
    let status_for_entry = status.clone();
    entry.connect_changed(move |entry| {
        let text = entry.text();
        let (label, sensitive) = primary_action_for_connectivity(&text, kind, connectivity);
        primary_for_entry.set_label(&strings::text(label));
        primary_for_entry.set_sensitive(sensitive);
        // SRC-6: name the mismatch while typing, not only on submit. `NET-3`
        // point 4 layers offline's search-needs-network reason on top.
        let parsed = classify_input(&text);
        set_status_hint(&status_for_entry, &parsed, kind, connectivity);
    });

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(
        &strings::text(dialog_title(kind)),
        "",
    )));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&content));
    let dialog = adw::Dialog::builder()
        .title(strings::text(dialog_title(kind)))
        .content_width(620)
        .content_height(560)
        .child(&toolbar)
        .build();

    AddDialogSurface {
        dialog,
        entry,
        status,
        results,
        cancel,
        primary,
    }
}

pub(super) fn present(
    parent: &impl IsA<gtk4::Widget>,
    conn: &Rc<RefCell<Connection>>,
    preferred_kind: PodcastKind,
    connectivity: Connectivity,
    on_added: impl Fn(bool) + 'static,
) {
    let conn = conn.clone();
    let on_added: OnAdded = Rc::new(on_added);
    let surface = build_surface(preferred_kind, connectivity);
    let dialog = surface.dialog;
    let entry = surface.entry;
    let status = surface.status;
    let results = surface.results;
    let cancel = surface.cancel;
    let primary = surface.primary;
    let generation = Rc::new(Cell::new(0_u64));
    let submit: Rc<dyn Fn(String)> = Rc::new({
        let conn = conn.clone();
        let results = results.clone();
        let status = status.clone();
        let generation = generation.clone();
        let on_added = on_added.clone();
        move |input: String| {
            clear(&results);
            let next = generation.get().wrapping_add(1);
            generation.set(next);
            let parsed = classify_input(&input);
            // SRC-6, NET-1a and NET-3 point 4 decided in one place, before
            // any provider work.
            let refusal = submit_refusal(&conn.borrow(), preferred_kind, &parsed, connectivity);
            if let Some(reason) = refusal {
                status.set_text(&strings::text(reason));
                return;
            }
            match parsed {
                AddInput::Empty => status.set_text(""),
                AddInput::Search(terms) => {
                    status.set_text(&strings::text(strings::PODCAST_SEARCHING));
                    search(
                        next,
                        terms,
                        &SearchContext {
                            generation: &generation,
                            status: &status,
                            results: &results,
                            conn: &conn,
                            on_added: &on_added,
                            preferred_kind,
                        },
                    );
                }
                AddInput::FeedUrl(url) => {
                    if connectivity.is_offline() {
                        subscribe_offline(PodcastKind::Rss, &url, &conn, &status, &on_added);
                        return;
                    }
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
                    if connectivity.is_offline() {
                        subscribe_offline(PodcastKind::Youtube, &url, &conn, &status, &on_added);
                        return;
                    }
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
    });
    let submit_on_activate = submit.clone();
    entry.connect_activate(move |entry| submit_on_activate(entry.text().to_string()));
    let submit_on_click = submit.clone();
    let entry_for_click = entry.downgrade();
    primary.connect_clicked(move |_| {
        if let Some(entry) = entry_for_click.upgrade() {
            submit_on_click(entry.text().to_string());
        }
    });
    let dialog_for_cancel = dialog.downgrade();
    cancel.connect_clicked(move |_| {
        if let Some(dialog) = dialog_for_cancel.upgrade() {
            dialog.close();
        }
    });
    dialog.present(Some(parent));
    entry.grab_focus();
}

fn search(request_generation: u64, terms: String, context: &SearchContext<'_>) {
    let config = podcasts::config::load(&context.conn.borrow()).ok();
    let auto_download_default = configured_auto_download_default(config.as_ref());
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "C".into());
    // SRC-6: exactly one provider is queried — the one this dialog belongs to.
    let section = result_section();
    context.results.append(&section);

    match dialog_provider(context.preferred_kind) {
        PodcastKind::Rss => {
            let task = one_shot_task::spawn("reprise-podcast-search", move || {
                podcasts::itunes::search(&terms, &locale)
                    .map(|rows| rows.into_iter().map(rss_candidate).collect::<Vec<_>>())
                    .map_err(|error| error.to_string())
            });
            attach_candidates(
                task,
                request_generation,
                context.generation,
                context.status,
                &section,
                context.conn,
                context.on_added,
                strings::PODCAST_APPLE_RESULTS,
                auto_download_default,
            );
        }
        PodcastKind::Youtube => {
            let youtube_allowed = reprise_core::online_sources::network_allowed(
                &context.conn.borrow(),
                &reprise_core::modules::YOUTUBE_MODULE,
            )
            .unwrap_or(false);
            if !youtube_allowed {
                return;
            }
            let ytdlp_path = config.and_then(|value| value.ytdlp_path);
            let task = one_shot_task::spawn("reprise-youtube-search", move || {
                podcasts::ytdlp::YtDlp::discover(ytdlp_path.as_deref())
                    .search_channels(&terms)
                    .map(|rows| rows.into_iter().map(youtube_candidate).collect::<Vec<_>>())
                    .map_err(|error| error.to_string())
            });
            attach_candidates(
                task,
                request_generation,
                context.generation,
                context.status,
                &section,
                context.conn,
                context.on_added,
                strings::PODCAST_YOUTUBE_RESULTS,
                auto_download_default,
            );
        }
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
    auto_download_default: bool,
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
                let subscribed = active_source_keys(&conn.borrow());
                let rows = filter_unsubscribed(rows, &subscribed);
                if rows.is_empty() {
                    return;
                }
                append_heading(&results, heading);
                for candidate in rows {
                    append_candidate(&results, candidate, &conn, &on_added, auto_download_default);
                }
            }
            Err(error) => status.set_text(&error),
        }
    });
}

/// `POD-13`: turn a provider failure into the fixed, classified reason the
/// download path (`pipeline::download_episode`) and the MCP path
/// (`source_actions::podcast_source_error`) already use, instead of a raw
/// `PodcastError::to_string()` — yt-dlp's first stderr line in particular can
/// echo a URL, a query token, a credential-like value or a local filesystem
/// path, and none of that belongs in a dialog the user reads.
fn preview_error(error: &podcasts::PodcastError) -> String {
    error.classify().to_owned()
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
    let auto_download_default = configured_auto_download_default(config.as_ref());
    let ytdlp_path = config.and_then(|value| value.ytdlp_path);
    let task_url = url.to_owned();
    let receiver = one_shot_task::spawn(
        "reprise-podcast-preview",
        move || -> Result<Preview, String> {
            match kind {
                PodcastKind::Rss => {
                    // POD-13: classify rather than forward `PodcastError`'s
                    // `Display` text — the same classifier the download path
                    // (`pipeline::download_episode`) and the MCP path
                    // (`source_actions::podcast_source_error`) already use, so
                    // this preview never becomes a second, drifting sanitiser.
                    let response =
                        podcasts::http::get(&task_url).map_err(|error| preview_error(&error))?;
                    let feed = podcasts::feed::parse_feed(&response.body, import_count)
                        .map_err(|error| preview_error(&error))?;
                    let count = feed.episodes.len();
                    let guids = feed
                        .episodes
                        .iter()
                        .map(|episode| episode.guid.clone())
                        .collect();
                    Ok(Preview {
                        kind,
                        title: feed.title,
                        author: feed.author,
                        image_url: feed.image_url,
                        count,
                        url: task_url,
                        guids,
                    })
                }
                PodcastKind::Youtube => {
                    // POD-13: yt-dlp's raw stderr line can carry a URL, a
                    // query token or a local path — classify it the same way
                    // the download and MCP paths do rather than showing it.
                    let listing = podcasts::ytdlp::YtDlp::discover(ytdlp_path.as_deref())
                        .list(&task_url)
                        .map_err(|error| preview_error(&error))?;
                    let count = listing.entries.len();
                    let guids = listing
                        .entries
                        .iter()
                        .map(|entry| entry.id.clone())
                        .collect();
                    Ok(Preview {
                        kind,
                        title: listing.title.unwrap_or_else(|| task_url.clone()),
                        author: None,
                        image_url: listing.image_url,
                        count,
                        url: listing.source_url.unwrap_or(task_url),
                        guids,
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
                let subscribed = active_source_keys(&conn.borrow());
                if source_is_subscribed(preview.kind, &preview.url, &preview.guids, &subscribed) {
                    status.set_text(&strings::text(strings::PODCAST_ALREADY_SUBSCRIBED));
                    return;
                }
                status.set_text("");
                append_preview(
                    &results,
                    preview,
                    import_count,
                    auto_download_default,
                    &conn,
                    &on_added,
                );
            }
            Err(error) => status.set_text(&error),
        }
    });
}

/// `dialog_status_hint` returns `""` for "nothing to say" — routed straight
/// to `set_text`, never through `strings::text`, since `gettext("")` is a
/// well-known trap that returns the PO file's header metadata instead of an
/// empty string.
fn set_status_hint(
    status: &gtk4::Label,
    input: &AddInput,
    kind: PodcastKind,
    connectivity: Connectivity,
) {
    let hint = dialog_status_hint(input, kind, connectivity);
    if hint.is_empty() {
        status.set_text("");
    } else {
        status.set_text(&strings::text(hint));
    }
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
    auto_download_default: bool,
) {
    let row = candidate_row(
        &candidate.title,
        &candidate.subtitle,
        candidate.kind,
        candidate.image_url.as_deref(),
        images_allowed(&conn.borrow()),
    );
    // SRC-7: the same compact action every discovery row uses.
    let title = candidate.title.clone();
    let button = source_add_action::add_button(source_add_action::AddActionKind::Subscribe, &title);
    let conn = conn.clone();
    let on_added = on_added.clone();
    button.connect_clicked(move |button| {
        let result = {
            let conn = conn.borrow();
            subscribe(&conn, &candidate, auto_download_default, None)
        };
        match result {
            Ok(_) => {
                on_added(true);
                // SRC-5/SRC-7: acknowledge in place; only the next submitted
                // search drops the row.
                source_add_action::mark_added(
                    button,
                    source_add_action::AddActionKind::Subscribe,
                    &title,
                );
            }
            Err(error) => button.set_tooltip_text(Some(&error.to_string())),
        }
    });
    row.append(&button);
    parent.append(&row);
}

fn append_preview(
    parent: &gtk4::Box,
    preview: Preview,
    import_count: usize,
    auto_download_default: bool,
    conn: &Rc<RefCell<Connection>>,
    on_added: &OnAdded,
) {
    clear(parent);
    let subtitle = strings::podcast_episode_count(preview.count);
    let row = candidate_row(
        &preview.title,
        &subtitle,
        preview.kind,
        preview.image_url.as_deref(),
        images_allowed(&conn.borrow()),
    );
    parent.append(&row);
    let import = gtk4::CheckButton::with_label(&strings::podcast_import_latest_count(import_count));
    import.set_active(true);
    parent.append(&import);
    let auto_download =
        gtk4::CheckButton::with_label(&strings::text(strings::PODCAST_AUTO_DOWNLOAD));
    auto_download.set_active(auto_download_default);
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
        identity_guids: preview.guids.clone(),
    };
    let conn = conn.clone();
    let on_added = on_added.clone();
    let preview_guids = preview.guids;
    let parent_weak = parent.downgrade();
    subscribe_button.connect_clicked(move |button| {
        let baseline = baseline_for_import_choice(import.is_active(), &preview_guids);
        let result = {
            let conn = conn.borrow();
            subscribe(
                &conn,
                &candidate,
                auto_download.is_active(),
                baseline.as_deref(),
            )
        };
        match result {
            Ok(_) => {
                on_added(import.is_active());
                if let Some(parent) = parent_weak.upgrade() {
                    clear(&parent);
                }
            }
            Err(error) => button.set_tooltip_text(Some(&error.to_string())),
        }
    });
    parent.append(&subscribe_button);
}

/// `NET-1a` / `C1`: `online_sources::network_allowed(conn,
/// &modules::SOURCE_IMAGES_MODULE)`, computed once by each caller of
/// [`candidate_row`] — this dialog never lets the widget read settings
/// itself.
fn images_allowed(conn: &Connection) -> bool {
    reprise_core::online_sources::network_allowed(
        conn,
        &reprise_core::modules::SOURCE_IMAGES_MODULE,
    )
    .unwrap_or(false)
}

fn candidate_row(
    title: &str,
    subtitle: &str,
    kind: PodcastKind,
    image_url: Option<&str>,
    images_allowed: bool,
) -> gtk4::Box {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    row.add_css_class("reprise-podcast-result");
    let image = super::source_image::SourceImage::new(
        image_url,
        match kind {
            PodcastKind::Rss => "audio-input-microphone-symbolic",
            PodcastKind::Youtube => "video-x-generic-symbolic",
        },
        40,
        images_allowed,
    );
    row.append(image.widget());
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

/// `NET-3` point 4: the URL path while offline — no preview fetch, the
/// subscription is created straight away with the URL itself as a
/// placeholder title, and the next successful refresh (already scheduled
/// independently of this dialog) fills in the real title and episodes.
/// This only translates [`podcasts::offline_add::offline_subscribe`]'s
/// outcome into status text and the `on_added` callback; the decision and
/// the one DB write both live in core, where they are testable without a
/// GTK dialog.
fn subscribe_offline(
    kind: PodcastKind,
    url: &str,
    conn: &Rc<RefCell<Connection>>,
    status: &gtk4::Label,
    on_added: &OnAdded,
) {
    let auto_download_default = {
        let conn = conn.borrow();
        podcasts::config::load(&conn)
            .ok()
            .is_some_and(|config| config.auto_download_default)
    };
    let outcome = {
        let conn = conn.borrow();
        podcasts::offline_add::offline_subscribe(&conn, kind, url, auto_download_default)
    };
    match outcome {
        Ok(podcasts::offline_add::OfflineSubscribeOutcome::AlreadySubscribed) => {
            status.set_text(&strings::text(strings::PODCAST_ALREADY_SUBSCRIBED));
        }
        Ok(podcasts::offline_add::OfflineSubscribeOutcome::Added { .. }) => {
            status.set_text(&strings::text(strings::PODCAST_ADDED_OFFLINE));
            // `import_latest = false`: there is nothing to import yet while
            // offline, and forcing an immediate refresh attempt now would
            // just fail loudly over the network this dialog just avoided.
            on_added(false);
        }
        Err(error) => status.set_text(&error.to_string()),
    }
}

fn subscribe(
    conn: &Connection,
    candidate: &Candidate,
    auto_download: bool,
    future_only_baseline: Option<&[String]>,
) -> Result<i64, rusqlite::Error> {
    podcasts::store::add_or_restore_with_baseline(
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
        future_only_baseline,
    )
}

fn baseline_for_import_choice(import: bool, preview_guids: &[String]) -> Option<Vec<String>> {
    (!import).then(|| preview_guids.to_vec())
}

fn configured_auto_download_default(config: Option<&podcasts::config::PodcastConfig>) -> bool {
    config.is_some_and(|value| value.auto_download_default)
}

#[cfg(test)]
#[path = "add_dialog_tests.rs"]
mod tests;
