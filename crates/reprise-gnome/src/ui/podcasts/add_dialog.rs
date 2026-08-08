//! Podcast search-or-URL dialog.

use std::cell::Cell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use libadwaita::prelude::*;
use reprise_core::connectivity::Connectivity;
use reprise_core::db::Db;
use reprise_core::podcasts::discovery::{
    active_source_keys, dialog_provider, filter_unsubscribed, source_is_subscribed, Candidate,
};
use reprise_core::podcasts::{self, PodcastKind};

use crate::ui::one_shot_task;
use crate::ui::strings;

use super::add_dialog_input::{
    classify_input, dialog_hint, dialog_status_hint, dialog_title, primary_action_for_connectivity,
    submit_refusal, AddInput,
};
use super::add_dialog_results::{clear, result_section, rss_candidate, youtube_candidate};
use super::add_dialog_rows::{append_candidate, append_heading, append_preview, Preview};
#[cfg(test)]
use super::add_dialog_rows::{candidate_row, images_allowed};
#[cfg(test)]
use super::add_dialog_subscription::baseline_for_import_choice;
use super::add_dialog_subscription::{configured_auto_download_default, subscribe_offline};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum AddDialogPhase {
    Idle,
    Searching,
    Previewing,
    Results,
    Preview,
    Error,
}

pub(super) type OnAdded = Rc<dyn Fn(bool)>;

/// `SRC-8`: the single size this dialog keeps, whatever a search returns.
/// `adw::Dialog` treats both as *natural* sizes, so a result label that does
/// not ellipsize raises the minimum width and widens the window instead.
const CONTENT_WIDTH: i32 = 620;
const CONTENT_HEIGHT: i32 = 560;

struct SearchContext<'a> {
    generation: &'a Rc<Cell<u64>>,
    status: &'a gtk4::Label,
    results: &'a gtk4::Box,
    conn: &'a Rc<Db>,
    on_added: &'a OnAdded,
    preferred_kind: PodcastKind,
}

struct AddDialogSurface {
    /// `SRC-15`: absent when the library has played nothing with a genre —
    /// the row is then not built at all rather than standing empty.
    library_chip: Option<gtk4::Button>,
    dialog: adw::Dialog,
    entry: gtk4::SearchEntry,
    status: gtk4::Label,
    results: gtk4::Box,
    cancel: gtk4::Button,
    primary: gtk4::Button,
}

fn build_surface(
    kind: PodcastKind,
    connectivity: Connectivity,
    library_genre: Option<&str>,
) -> AddDialogSurface {
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);

    let entry = gtk4::SearchEntry::builder()
        .placeholder_text(strings::text(dialog_hint(kind)))
        .build();
    content.append(&entry);
    // `SRC-15`: one suggestion from the user's own library, in the same flat
    // pill shape the radio dialog uses, so an empty search field is not the
    // only way in. Built only when the library has a genre to suggest — an
    // empty chip row would be worse than none.
    let library_chip = library_genre.map(|genre| {
        let label = match kind {
            PodcastKind::Rss => strings::podcast_chip_genre(genre),
            PodcastKind::Youtube => strings::youtube_chip_genre(genre),
        };
        let chip = gtk4::Button::with_label(&label);
        chip.add_css_class("pill");
        // Left-aligned and only as wide as its text, like the radio chips —
        // a full-width button would read as a second primary action.
        chip.set_halign(gtk4::Align::Start);
        content.append(&chip);
        chip
    });
    let status = gtk4::Label::new(None);
    status.add_css_class("reprise-text-secondary");
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
    footnote.add_css_class("reprise-text-secondary");
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
        .content_width(CONTENT_WIDTH)
        .content_height(CONTENT_HEIGHT)
        .child(&toolbar)
        .build();

    AddDialogSurface {
        library_chip,
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
    conn: &Rc<Db>,
    preferred_kind: PodcastKind,
    connectivity: Connectivity,
    on_added: impl Fn(bool) + 'static,
) {
    let conn = conn.clone();
    let on_added: OnAdded = Rc::new(on_added);
    // `SRC-15`: the same library fact the radio chip reads, so both dialogs
    // suggest the same genre instead of each inventing a rule of its own.
    // This dialog is rebuilt on every open, so the suggestion is always
    // current without a refresh path.
    let library_genre = reprise_core::library::taste::top_genre(&conn).unwrap_or_else(|error| {
        tracing::warn!(%error, "could not read the library's top genre for the podcast chip");
        None
    });
    let surface = build_surface(
        preferred_kind,
        connectivity,
        library_genre.as_ref().map(|genre| genre.name.as_str()),
    );
    let library_chip = surface.library_chip;
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
            let refusal = submit_refusal(&conn, preferred_kind, &parsed, connectivity);
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
    if let (Some(chip), Some(genre)) = (library_chip, library_genre) {
        // `SRC-15`: the chip fills the field it searches with, so the run is
        // visible, editable and repeatable — never a hidden query the user
        // cannot see or amend.
        let submit_on_chip = submit.clone();
        let entry_for_chip = entry.downgrade();
        chip.connect_clicked(move |_| {
            if let Some(entry) = entry_for_chip.upgrade() {
                entry.set_text(&genre.name);
            }
            submit_on_chip(genre.name.clone());
        });
    }
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
    let config = podcasts::config::load(context.conn).ok();
    let auto_download_default = configured_auto_download_default(config.as_ref());
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "C".into());
    // SRC-6: exactly one provider is queried — the one this dialog belongs to.
    let section = result_section();
    context.results.append(&section);

    match dialog_provider(context.preferred_kind) {
        PodcastKind::Rss => {
            let query = terms.clone();
            let task = one_shot_task::spawn("reprise-podcast-search", move || {
                podcasts::itunes::search(&terms, &locale)
                    .map(|rows| rows.into_iter().map(rss_candidate).collect::<Vec<_>>())
                    .map_err(|error| preview_error(&error))
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
                query,
            );
        }
        PodcastKind::Youtube => {
            let youtube_allowed = reprise_core::online_sources::network_allowed(
                context.conn,
                &reprise_core::modules::YOUTUBE_MODULE,
            )
            .unwrap_or(false);
            if !youtube_allowed {
                return;
            }
            let ytdlp_path = config.as_ref().and_then(|value| value.ytdlp_path.clone());
            let youtube_browser = config.and_then(|value| value.youtube_browser);
            let query = terms.clone();
            let task = one_shot_task::spawn("reprise-youtube-search", move || {
                super::metadata_ytdlp(ytdlp_path.as_deref(), youtube_browser)
                    .search_channels(&terms)
                    .map(|rows| rows.into_iter().map(youtube_candidate).collect::<Vec<_>>())
                    .map_err(|error| preview_error(&error))
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
                query,
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
    conn: &Rc<Db>,
    on_added: &OnAdded,
    heading: &'static str,
    auto_download_default: bool,
    query: String,
) {
    let generation = generation.clone();
    let status = status.clone();
    let results = results.clone();
    let conn = conn.clone();
    let on_added = on_added.clone();
    gtk4::glib::spawn_future_local(async move {
        let response = match receiver {
            Ok(receiver) => receiver
                .recv()
                .await
                .map_err(|_| strings::text(strings::PODCAST_SEARCH_FAILED)),
            Err(error) => {
                tracing::warn!(%error, "could not start podcast search task");
                Err(strings::text(strings::PODCAST_SEARCH_FAILED))
            }
        };
        if generation.get() != request_generation {
            return;
        }
        match response.and_then(|value| value) {
            Ok(rows) => {
                status.set_text("");
                let subscribed = active_source_keys(&conn);
                let rows = filter_unsubscribed(rows, &subscribed);
                if rows.is_empty() {
                    status.set_text(&strings::source_nothing_found(&query));
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
    conn: &Rc<Db>,
    on_added: &OnAdded,
) {
    let config = podcasts::config::load(conn).ok();
    let import_count = config
        .as_ref()
        .map_or(podcasts::config::DEFAULT_IMPORT_COUNT, |value| {
            value.import_count
        });
    let auto_download_default = configured_auto_download_default(config.as_ref());
    let ytdlp_path = config.as_ref().and_then(|value| value.ytdlp_path.clone());
    let youtube_browser = config.and_then(|value| value.youtube_browser);
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
                        title: feed.title.unwrap_or_else(|| task_url.clone()),
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
                    let listing = super::metadata_ytdlp(ytdlp_path.as_deref(), youtube_browser)
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
                        title: listing.channel.unwrap_or_else(|| task_url.clone()),
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
            Ok(receiver) => receiver
                .recv()
                .await
                .map_err(|_| strings::text(strings::PODCAST_PREVIEW_FAILED)),
            Err(error) => {
                tracing::warn!(%error, "could not start podcast preview task");
                Err(strings::text(strings::PODCAST_PREVIEW_FAILED))
            }
        };
        if generation.get() != request_generation {
            return;
        }
        match response.and_then(|value| value) {
            Ok(preview) => {
                let subscribed = active_source_keys(&conn);
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

#[cfg(test)]
#[path = "add_dialog_tests.rs"]
mod tests;
