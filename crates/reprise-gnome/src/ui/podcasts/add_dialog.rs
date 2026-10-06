//! Podcast search-or-URL dialog.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

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
use crate::ui::source_add_dialog::chrome::{ChromeSpec, SourceAddChrome};
use crate::ui::source_add_dialog::generation::Generation;
use crate::ui::strings;

use super::add_dialog_chips::{chip_for, dialog_country, AddDialogChip};
use super::add_dialog_followers::{self, YoutubeFollowerRequest, YoutubeResults};
use super::add_dialog_input::{
    classify_input, dialog_hint, dialog_status_hint, dialog_title, primary_action_for_connectivity,
    submit_refusal, AddInput,
};
use super::add_dialog_results::{
    clear, partition_dormant_search_results, result_section, rss_candidate, youtube_candidate,
};
use super::add_dialog_rows::{append_candidate, append_heading, append_preview, Preview};
#[cfg(test)]
use super::add_dialog_rows::{candidate_row, images_allowed};
#[cfg(test)]
use super::add_dialog_subscription::{baseline_for_import_choice, subscribe};
use super::add_dialog_subscription::{configured_auto_download_default, subscribe_offline};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(super) enum AddDialogPhase {
    Idle,
    Searching,
    Previewing,
    Results,
    Preview,
    Error,
}

pub(super) type OnAdded = Rc<dyn Fn(i64, bool)>;

/// `SRC-8`: the single size this dialog keeps, whatever a search returns.
/// `adw::Dialog` treats both as *natural* sizes, so a result label that does
/// not ellipsize raises the minimum width and widens the window instead.
const CONTENT_WIDTH: i32 = 620;
const CONTENT_HEIGHT: i32 = 560;

struct SearchContext<'a> {
    generation: &'a Rc<Cell<Generation>>,
    status: &'a gtk4::Label,
    results: &'a gtk4::Box,
    conn: &'a Rc<Db>,
    on_added: &'a OnAdded,
    follower_cancel: &'a Rc<RefCell<Option<Arc<AtomicBool>>>>,
    preferred_kind: PodcastKind,
}

struct AddDialogSurface {
    /// `SRC-15a` / `SRC-19`: absent when the current suggestion is not useful
    /// — no played YouTube genre, or an offline Apple dialog.
    suggestion_chip: Option<gtk4::Button>,
    chip_action: Option<AddDialogChip>,
    chrome: SourceAddChrome,
    dialog: adw::Dialog,
    entry: gtk4::SearchEntry,
    status: gtk4::Label,
    results: gtk4::Box,
    #[cfg(test)]
    cancel: gtk4::Button,
    primary: gtk4::Button,
}

fn build_surface(
    kind: PodcastKind,
    connectivity: Connectivity,
    network_allowed: bool,
    country: &str,
    library_genre: Option<&str>,
) -> AddDialogSurface {
    // `SRC-15a` / `SRC-19`: the one suggestion slot is a library-genre query
    // for YouTube and the country chart for Apple Podcasts.
    let chip_action = chip_for(kind, connectivity, network_allowed, country, library_genre);
    let suggestion_chip = chip_action.as_ref().map(|action| {
        let chip = gtk4::Button::with_label(&action.label());
        chip.add_css_class("pill");
        // Left-aligned and only as wide as its text, like the radio chips —
        // a full-width button would read as a second primary action.
        chip.set_halign(gtk4::Align::Start);
        chip
    });
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

    // SRC-7: the footnote says once why an added source stops appearing, instead
    // of letting it vanish unexplained on the next search.
    let chrome = SourceAddChrome::build(
        ChromeSpec {
            title: strings::text(dialog_title(kind)),
            dialog_title: true,
            hint: strings::text(dialog_hint(kind)),
            footnote: strings::text(strings::SOURCE_SUBSCRIBED_DROP_OUT),
            cancel_label: strings::text(strings::PODCAST_CANCEL),
            primary_label: strings::text(strings::PODCAST_SEARCH),
            content_width: CONTENT_WIDTH,
            content_height: CONTENT_HEIGHT,
            margin: 18,
            status_wraps: false,
        },
        |content, status| {
            if let Some(chip) = &suggestion_chip {
                content.append(chip);
            }
            content.append(status);
            content.append(&scroller);
        },
    );
    let entry = chrome.entry.clone();
    let status = chrome.status.clone();
    let primary = chrome.primary.clone();

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

    AddDialogSurface {
        suggestion_chip,
        chip_action,
        dialog: chrome.dialog.clone(),
        entry,
        status,
        results,
        #[cfg(test)]
        cancel: chrome.cancel.clone(),
        primary,
        chrome,
    }
}

pub(super) fn present(
    parent: &impl IsA<gtk4::Widget>,
    conn: &Rc<Db>,
    preferred_kind: PodcastKind,
    connectivity: Connectivity,
    on_added: impl Fn(i64, bool) + 'static,
) {
    let conn = conn.clone();
    let on_added: OnAdded = Rc::new(on_added);
    let locale = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_else(|_| "C".into());
    let location = reprise_core::location::app_location(&conn).ok().flatten();
    let country = dialog_country(location.as_ref(), &locale);
    // `SRC-15a`: YouTube keeps the same library fact the radio chip reads.
    // Apple Podcasts spends the slot on the country chart instead.
    let library_genre = (preferred_kind == PodcastKind::Youtube)
        .then(|| reprise_core::library::taste::top_genre(&conn))
        .transpose()
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read the library's top genre for the YouTube chip");
            None
        })
        .flatten();
    // `SRC-19` / `NET-1a`: the chip is a network action, so it needs the same
    // consent `submit_refusal` demands of a search — reachability alone is not
    // permission. A failed lookup counts as "not allowed", the safe direction
    // for a privacy promise.
    let network_allowed =
        podcasts::config::source_network_allowed(&conn, preferred_kind).unwrap_or(false);
    let surface = build_surface(
        preferred_kind,
        connectivity,
        network_allowed,
        &country,
        library_genre.as_ref().map(|genre| genre.name.as_str()),
    );
    let suggestion_chip = surface.suggestion_chip;
    let chip_action = surface.chip_action;
    let dialog = surface.dialog;
    let entry = surface.entry;
    let status = surface.status;
    let results = surface.results;
    let primary = surface.primary;
    let chrome = surface.chrome;
    let generation = Rc::new(Cell::new(Generation::default()));
    let follower_cancel = Rc::new(RefCell::new(None::<Arc<AtomicBool>>));
    let submit: Rc<dyn Fn(String)> = Rc::new({
        let conn = conn.clone();
        let results = results.clone();
        let status = status.clone();
        let generation = generation.clone();
        let on_added = on_added.clone();
        let country = country.clone();
        let follower_cancel = follower_cancel.clone();
        move |input: String| {
            if let Some(cancelled) = follower_cancel.borrow_mut().take() {
                cancelled.store(true, Ordering::Release);
            }
            clear(&results);
            let next = generation.get().next();
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
                        country.clone(),
                        &SearchContext {
                            generation: &generation,
                            status: &status,
                            results: &results,
                            conn: &conn,
                            on_added: &on_added,
                            follower_cancel: &follower_cancel,
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
                        ResultSurface {
                            status: &status,
                            results: &results,
                        },
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
                        ResultSurface {
                            status: &status,
                            results: &results,
                        },
                        &conn,
                        &on_added,
                    );
                }
            }
        }
    });
    let submit_on_activate = submit.clone();
    entry.connect_activate(move |entry| submit_on_activate(entry.text().to_string()));
    if let (Some(chip), Some(action)) = (suggestion_chip, chip_action) {
        match action {
            AddDialogChip::Charts { country } => {
                let generation = generation.clone();
                let status = status.clone();
                let results = results.clone();
                let conn = conn.clone();
                let on_added = on_added.clone();
                let follower_cancel = follower_cancel.clone();
                chip.connect_clicked(move |_| {
                    clear(&results);
                    let next = generation.get().next();
                    generation.set(next);
                    status.set_text(&strings::text(strings::PODCAST_SEARCHING));
                    load_charts(
                        next,
                        country.clone(),
                        &SearchContext {
                            generation: &generation,
                            status: &status,
                            results: &results,
                            conn: &conn,
                            on_added: &on_added,
                            follower_cancel: &follower_cancel,
                            preferred_kind,
                        },
                    );
                });
            }
            AddDialogChip::LibraryGenre { genre } => {
                // `SRC-15a`: a library chip fills the field it searches with,
                // keeping the run visible, editable and repeatable.
                let submit_on_chip = submit.clone();
                let entry_for_chip = entry.downgrade();
                chip.connect_clicked(move |_| {
                    if let Some(entry) = entry_for_chip.upgrade() {
                        entry.set_text(&genre);
                    }
                    submit_on_chip(genre.clone());
                });
            }
        }
    }
    let submit_on_click = submit.clone();
    let entry_for_click = entry.downgrade();
    primary.connect_clicked(move |_| {
        if let Some(entry) = entry_for_click.upgrade() {
            submit_on_click(entry.text().to_string());
        }
    });
    let follower_cancel_on_close = follower_cancel.clone();
    dialog.connect_closed(move |_| {
        if let Some(cancelled) = follower_cancel_on_close.borrow_mut().take() {
            cancelled.store(true, Ordering::Release);
        }
    });
    chrome.present(parent);
}

fn search(
    request_generation: Generation,
    terms: String,
    country: String,
    context: &SearchContext<'_>,
) {
    let config = podcasts::config::load(context.conn).ok();
    let auto_download_default = configured_auto_download_default(config.as_ref());
    // SRC-6: exactly one provider is queried — the one this dialog belongs to.
    let section = result_section();
    context.results.append(&section);

    match dialog_provider(context.preferred_kind) {
        PodcastKind::Rss => {
            let query = terms.clone();
            let empty_status = strings::source_nothing_found(&query);
            let task = one_shot_task::spawn("reprise-podcast-search", move || {
                podcasts::itunes::search_in_country(&terms, &country)
                    .map(|mut rows| {
                        partition_dormant_search_results(&mut rows, chrono::Utc::now().timestamp());
                        rows.into_iter().map(rss_candidate).collect::<Vec<_>>()
                    })
                    .map_err(|error| preview_error(&error))
            });
            attach_candidates(
                task,
                request_generation,
                context.generation,
                ResultSurface {
                    status: context.status,
                    results: &section,
                },
                context.conn,
                context.on_added,
                AddOptions {
                    heading: strings::text(strings::PODCAST_APPLE_RESULTS),
                    query: Some(query),
                    auto_download_default,
                    empty_status,
                    follower_request: None,
                },
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
            let follower_cancel = Arc::new(AtomicBool::new(false));
            *context.follower_cancel.borrow_mut() = Some(follower_cancel.clone());
            let follower_request = YoutubeFollowerRequest {
                ytdlp_path: ytdlp_path.clone(),
                youtube_browser,
                cancelled: follower_cancel,
            };
            let query = terms.clone();
            let empty_status = strings::source_nothing_found(&query);
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
                ResultSurface {
                    status: context.status,
                    results: &section,
                },
                context.conn,
                context.on_added,
                AddOptions {
                    heading: strings::text(strings::PODCAST_YOUTUBE_RESULTS),
                    query: Some(query),
                    auto_download_default,
                    empty_status,
                    follower_request: Some(follower_request),
                },
            );
        }
    }
}

fn load_charts(request_generation: Generation, country: String, context: &SearchContext<'_>) {
    let config = podcasts::config::load(context.conn).ok();
    let auto_download_default = configured_auto_download_default(config.as_ref());
    let section = result_section();
    context.results.append(&section);
    let heading = strings::podcast_charts_heading(&country);
    // `SRC-19`: an empty chart is not a search that missed, and the country
    // label is not a search term — so it never borrows `SOURCE_NOTHING_FOUND`.
    let empty_status = strings::podcast_charts_empty(&country);
    let task = one_shot_task::spawn("reprise-podcast-charts", move || {
        podcasts::itunes_charts::top_podcasts(&country)
            .map(|rows| rows.into_iter().map(rss_candidate).collect::<Vec<_>>())
            .map_err(|error| preview_error(&error))
    });
    attach_candidates(
        task,
        request_generation,
        context.generation,
        ResultSurface {
            status: context.status,
            results: &section,
        },
        context.conn,
        context.on_added,
        AddOptions {
            heading,
            query: None,
            auto_download_default,
            empty_status,
            follower_request: None,
        },
    );
}

/// The two widgets a candidate list is rendered into.
#[derive(Clone, Copy)]
struct ResultSurface<'a> {
    status: &'a gtk4::Label,
    results: &'a gtk4::Box,
}

/// How the candidates of one request are offered: the list heading, the query that produced
/// them, the auto-download default, the empty-list status, and the YouTube follower request.
struct AddOptions {
    heading: String,
    query: Option<String>,
    auto_download_default: bool,
    empty_status: String,
    follower_request: Option<YoutubeFollowerRequest>,
}

fn attach_candidates(
    receiver: std::io::Result<async_channel::Receiver<Result<Vec<Candidate>, String>>>,
    request_generation: Generation,
    generation: &Rc<Cell<Generation>>,
    surface: ResultSurface<'_>,
    conn: &Rc<Db>,
    on_added: &OnAdded,
    options: AddOptions,
) {
    let AddOptions {
        heading,
        query,
        auto_download_default,
        empty_status,
        follower_request,
    } = options;
    let generation = generation.clone();
    let status = surface.status.clone();
    let results = surface.results.clone();
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
                    status.set_text(&empty_status);
                    return;
                }
                if let Some(follower_request) = follower_request {
                    let youtube_results = YoutubeResults::new(&results, &heading, query.clone());
                    for candidate in rows {
                        let row = append_candidate(
                            &results,
                            candidate.clone(),
                            query.as_deref(),
                            &conn,
                            &on_added,
                            auto_download_default,
                        );
                        youtube_results.push(candidate, row);
                    }
                    add_dialog_followers::start(
                        youtube_results,
                        follower_request,
                        &conn,
                        generation,
                        request_generation,
                    );
                } else {
                    append_heading(&results, &heading);
                    for candidate in rows {
                        append_candidate(
                            &results,
                            candidate,
                            query.as_deref(),
                            &conn,
                            &on_added,
                            auto_download_default,
                        );
                    }
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

fn preview(
    request_generation: Generation,
    kind: PodcastKind,
    url: &str,
    generation: &Rc<Cell<Generation>>,
    surface: ResultSurface<'_>,
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
                    let response = podcasts::http::get_feed(&task_url)
                        .map_err(|error| preview_error(&error))?;
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
    let status = surface.status.clone();
    let results = surface.results.clone();
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

#[cfg(test)]
#[path = "add_dialog_chrome_tests.rs"]
mod chrome_tests;
