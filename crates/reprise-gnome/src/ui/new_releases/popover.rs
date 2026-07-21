//! Headerbar entry point and transient New Releases popover.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::{one_shot_task, popover_lifecycle, strings};

use super::badge;
use super::release_cover::fallback_accent_for_artist;
use super::release_row;

/// Popover content width (NR-9 compact layout).
const POPOVER_WIDTH: i32 = 336;
/// Caps the scrolling release list's natural height before it scrolls.
const SCROLLER_MAX_HEIGHT: i32 = 288;
const LIST_PAGE: &str = "list";
const HISTORY_PAGE: &str = "history";
/// How often the background timer re-checks staleness while the module is
/// enabled (Beschluss 8). Deliberately coarse: `refresh_due`'s own 6 h+jitter
/// window is the real gate, this just samples it periodically.
const REFRESH_TIMER_SECONDS: u32 = 3600;

#[derive(Debug, PartialEq, Eq)]
struct OpeningEffect {
    seen_ids: Vec<String>,
    navigates: bool,
}

/// Every listed (already-filtered, non-hidden) release is stamped seen on
/// open — the list scrolls now instead of capping at a handful of rows, so
/// nothing should stay unseen just because it rendered below a fold.
fn opening_effect(releases: &[reprise_core::artist_news::StoredRelease]) -> OpeningEffect {
    OpeningEffect {
        seen_ids: releases
            .iter()
            .map(|release| release.release_group_mbid.clone())
            .collect(),
        navigates: false,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct FooterPresentation {
    updated: String,
    show_cached_failure: bool,
}

fn footer_presentation(latest: Option<i64>, now: i64, failed: bool) -> FooterPresentation {
    FooterPresentation {
        updated: latest.map_or_else(
            || strings::text(strings::UPDATED_JUST_NOW),
            |timestamp| strings::new_releases_updated_ago(timestamp, now),
        ),
        show_cached_failure: failed,
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ModuleEffect {
    button_visible: bool,
    fetch_allowed: bool,
    empty: EmptyPresentation,
    badge_allowed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EmptyPresentation {
    Hidden,
    Checking,
    NoReleases,
}

fn module_effect(
    enabled: bool,
    has_releases: bool,
    fetch_completed: bool,
    fetching: bool,
) -> ModuleEffect {
    let empty = if !enabled || has_releases {
        EmptyPresentation::Hidden
    } else if fetching {
        EmptyPresentation::Checking
    } else {
        EmptyPresentation::NoReleases
    };
    ModuleEffect {
        button_visible: enabled && (has_releases || !fetch_completed),
        fetch_allowed: enabled,
        empty,
        badge_allowed: enabled && has_releases && fetch_completed,
    }
}

/// Whether a background (non-user-initiated) fetch should run right now:
/// shared by the popover's open path (`trigger_staleness_refresh`) and the
/// hourly timer (`maybe_background_refresh`), so the two never drift apart
/// with their own copy of the same condition.
fn periodic_fetch_due(enabled: bool, fetching: bool, refresh_due: bool) -> bool {
    enabled && !fetching && refresh_due
}

struct NewReleasesPopover {
    conn: Rc<RefCell<rusqlite::Connection>>,
    database_path: PathBuf,
    button: gtk4::MenuButton,
    badge: gtk4::Label,
    popover: gtk4::Popover,
    #[allow(dead_code)] // Consumed by the list<->history navigation in C1.
    stack: gtk4::Stack,
    list: gtk4::ListBox,
    empty: gtk4::Label,
    new_tag: gtk4::Label,
    history_row_label: gtk4::Label,
    #[allow(dead_code)] // Filled in by history_page.rs in C1.
    history_page: gtk4::Box,
    fetch_button: gtk4::Button,
    fetch_stack: gtk4::Stack,
    spinner: gtk4::Spinner,
    updated: gtk4::Label,
    failure: gtk4::Label,
    fetching: Cell<bool>,
    /// The hourly background staleness timer (Beschluss 8), running only
    /// while the module is enabled. `SourceId` is move-only, so `Cell::take`
    /// is how `stop_refresh_timer` retrieves it to call `remove()`.
    refresh_timer: Cell<Option<gtk4::glib::SourceId>>,
    on_show_album: release_row::OnShowAlbum,
}

impl NewReleasesPopover {
    fn new(
        conn: Rc<RefCell<rusqlite::Connection>>,
        database_path: PathBuf,
        on_show_album: release_row::OnShowAlbum,
    ) -> Rc<Self> {
        let (button, badge) = badge::build_button();
        let popover = gtk4::Popover::new();

        let list = gtk4::ListBox::new();
        list.set_selection_mode(gtk4::SelectionMode::None);
        let empty = gtk4::Label::new(None);
        empty.set_wrap(true);
        empty.set_justify(gtk4::Justification::Center);
        empty.set_margin_top(12);
        empty.set_margin_bottom(12);
        let scroller = gtk4::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk4::PolicyType::Never)
            .vscrollbar_policy(gtk4::PolicyType::Automatic)
            .propagate_natural_height(true)
            .max_content_height(SCROLLER_MAX_HEIGHT)
            .build();

        let header_label = gtk4::Label::new(Some(&strings::text(strings::NEW_RELEASES_HEADER)));
        header_label.add_css_class("new-release-header");
        header_label.set_xalign(0.0);
        header_label.set_hexpand(true);
        let new_tag = gtk4::Label::new(None);
        new_tag.add_css_class("new-release-tag");
        new_tag.set_visible(false);
        let header_row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
        header_row.append(&header_label);
        header_row.append(&new_tag);

        let separator = gtk4::Separator::new(gtk4::Orientation::Horizontal);
        separator.add_css_class("new-release-separator");

        // The click handler is a stub: C1 wires the actual list<->history
        // navigation once the history sub-page has real content.
        let history_row_label = gtk4::Label::new(None);
        history_row_label.set_xalign(0.0);
        history_row_label.set_hexpand(true);
        let history_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
        history_content.append(&gtk4::Image::from_icon_name(
            "document-open-recent-symbolic",
        ));
        history_content.append(&history_row_label);
        history_content.append(&gtk4::Image::from_icon_name("go-next-symbolic"));
        let history_row = gtk4::Button::builder()
            .child(&history_content)
            .css_classes(["flat", "new-release-history-row"])
            .build();
        history_row.connect_clicked(|_| {
            // Stub: C1 wires list<->history navigation.
        });

        let (footer, fetch_button, fetch_stack, spinner, updated, failure) = build_footer();

        let list_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        list_page.append(&header_row);
        list_page.append(&scroller);
        list_page.append(&separator);
        list_page.append(&history_row);
        list_page.append(&footer);

        // C1 fills this with history_page.rs; B2 only lays out the stack.
        let history_page = gtk4::Box::new(gtk4::Orientation::Vertical, 8);

        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::SlideLeftRight);
        stack.set_transition_duration(crate::ui::motion::STANDARD_MS);
        stack.add_named(&list_page, Some(LIST_PAGE));
        stack.add_named(&history_page, Some(HISTORY_PAGE));
        stack.set_visible_child_name(LIST_PAGE);

        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.set_size_request(POPOVER_WIDTH, -1);
        content.set_margin_top(10);
        content.set_margin_bottom(10);
        content.set_margin_start(10);
        content.set_margin_end(10);
        content.append(&stack);
        popover.set_child(Some(&content));
        popover_lifecycle::unparent_after_actions(&popover);
        button.set_popover(Some(&popover));

        let state = Rc::new(Self {
            conn,
            database_path,
            button,
            badge,
            popover,
            stack,
            list,
            empty,
            new_tag,
            history_row_label,
            history_page,
            fetch_button,
            fetch_stack,
            spinner,
            updated,
            failure,
            fetching: Cell::new(false),
            refresh_timer: Cell::new(None),
            on_show_album,
        });
        state.wire();
        state.render(false, false);
        state
    }

    fn retain_for_window(self: &Rc<Self>, window: &adw::ApplicationWindow) {
        let state = self.clone();
        window.connect_destroy(move |_| {
            let _keep_alive_until_destroy = &state;
        });
    }

    fn wire(self: &Rc<Self>) {
        let weak = Rc::downgrade(self);
        self.popover.connect_show(move |_| {
            if let Some(state) = weak.upgrade() {
                state.render(true, false);
                state.trigger_staleness_refresh();
            }
        });

        let weak = Rc::downgrade(self);
        self.fetch_button.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.fetch_now();
            }
        });
    }

    fn render(self: &Rc<Self>, mark_seen: bool, failed: bool) {
        let enabled = reprise_core::modules::is_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::NEW_RELEASES_MODULE,
        )
        .unwrap_or(false);
        if !enabled {
            self.button.set_visible(false);
            self.badge.set_visible(false);
            return;
        }
        let today = chrono::Local::now().date_naive();
        let all_releases =
            match reprise_core::artist_news::query_releases(&self.conn.borrow(), true, today) {
                Ok(releases) => releases,
                Err(error) => {
                    tracing::warn!(%error, "could not query New Releases");
                    self.button.set_visible(false);
                    return;
                }
            };
        let releases = all_releases
            .iter()
            .filter(|release| !release.hidden)
            .cloned()
            .collect::<Vec<_>>();
        let fetch_completed = self.fetch_completed();
        let effect = module_effect(
            enabled,
            !all_releases.is_empty(),
            fetch_completed,
            self.fetching.get(),
        );
        self.button.set_visible(effect.button_visible);
        self.list.remove_all();
        match effect.empty {
            EmptyPresentation::Hidden => {}
            EmptyPresentation::Checking => {
                self.empty
                    .set_label(&strings::text(strings::NEW_RELEASES_CHECKING));
                self.list.append(&self.empty);
            }
            EmptyPresentation::NoReleases => {
                self.empty
                    .set_label(&strings::text(strings::NEW_RELEASES_NONE));
                self.list.append(&self.empty);
            }
        }
        let on_hide: Rc<dyn Fn(&str)> = {
            let weak = Rc::downgrade(self);
            Rc::new(move |mbid: &str| {
                let Some(state) = weak.upgrade() else { return };
                if let Err(error) =
                    reprise_core::artist_news::set_release_hidden(&state.conn.borrow(), mbid, true)
                {
                    tracing::warn!(%error, release_group_mbid = mbid, "could not hide New Release");
                    return;
                }
                state.render(false, false);
            })
        };
        let close_popover: Rc<dyn Fn()> = {
            let popover = self.popover.clone();
            Rc::new(move || popover.popdown())
        };
        for release in &releases {
            self.list.append(&release_row::build(
                release,
                today,
                &on_hide,
                &self.on_show_album,
                &close_popover,
            ));
        }
        if mark_seen {
            let effect = opening_effect(&releases);
            if !effect.seen_ids.is_empty() {
                let now = chrono::Utc::now().timestamp();
                if let Err(error) = reprise_core::artist_news::mark_releases_seen(
                    &self.conn.borrow(),
                    &effect.seen_ids,
                    now,
                ) {
                    tracing::warn!(%error, "could not mark New Releases seen");
                }
            }
        }
        let unseen = reprise_core::artist_news::unseen_release_count(&self.conn.borrow())
            .unwrap_or_default();
        match badge::badge_presentation(unseen) {
            Some(text) if effect.badge_allowed => {
                self.badge.set_label(&text);
                self.badge.set_visible(true);
            }
            _ => self.badge.set_visible(false),
        }
        if unseen > 0 {
            self.new_tag
                .set_label(&strings::new_releases_new_count(unseen));
            self.new_tag.set_visible(true);
        } else {
            self.new_tag.set_visible(false);
        }
        let history_count =
            reprise_core::artist_news_history::query_history(&self.conn.borrow(), today)
                .map_or_else(
                    |error| {
                        tracing::warn!(%error, "could not query New Releases history");
                        0
                    },
                    |entries| entries.len(),
                );
        self.history_row_label
            .set_label(&strings::new_releases_show_history_count(history_count));
        let latest = all_releases.iter().map(|release| release.fetched_at).max();
        let footer = footer_presentation(latest, chrono::Utc::now().timestamp(), failed);
        self.updated.set_label(&footer.updated);
        self.updated.set_visible(latest.is_some());
        self.failure.set_visible(footer.show_cached_failure);
    }

    /// A5's staleness policy: opening the popover is a natural moment to
    /// check whether a background refresh is due, without blocking the
    /// synchronous render above. B5 adds the hourly timer as a second entry
    /// point into the exact same check, so both share `maybe_background_refresh`
    /// instead of drifting apart with their own copy of the condition.
    fn trigger_staleness_refresh(self: &Rc<Self>) {
        self.maybe_background_refresh();
    }

    /// The shared background-refresh check behind both the popover's open
    /// path and the hourly timer (Beschluss 8). Skips while a fetch is
    /// already running or the module is disabled, so this never fights
    /// `fetch_now`'s own guard and never touches the network while the
    /// module is off.
    fn maybe_background_refresh(self: &Rc<Self>) {
        let enabled = reprise_core::modules::is_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::NEW_RELEASES_MODULE,
        )
        .unwrap_or(false);
        let latest = reprise_core::artist_news::latest_fetched_at(&self.conn.borrow())
            .ok()
            .flatten();
        let now = chrono::Utc::now().timestamp();
        let jitter =
            reprise_core::artist_news::jitter_seconds(&self.database_path.to_string_lossy());
        let due = reprise_core::artist_news::refresh_due(latest, now, jitter);
        if periodic_fetch_due(enabled, self.fetching.get(), due) {
            self.fetch_now();
        }
    }

    /// Starts the hourly background staleness timer if it is not already
    /// running. Coupled to `enabled_changed` so it only ever runs while the
    /// module is enabled — no timer, no network, while the module is off.
    fn start_refresh_timer(self: &Rc<Self>) {
        // `Cell<Option<SourceId>>` has no `Copy`-friendly peek, so `take` it
        // out to check, then put it straight back if one was already running.
        let existing = self.refresh_timer.take();
        if existing.is_some() {
            self.refresh_timer.set(existing);
            return;
        }
        let weak = Rc::downgrade(self);
        let id = gtk4::glib::timeout_add_seconds_local(REFRESH_TIMER_SECONDS, move || {
            let Some(state) = weak.upgrade() else {
                return gtk4::glib::ControlFlow::Break; // Popover gone: stop the timer.
            };
            state.maybe_background_refresh();
            gtk4::glib::ControlFlow::Continue
        });
        self.refresh_timer.set(Some(id));
    }

    /// Stops the hourly timer, if one is running. Called whenever the module
    /// is disabled so a disabled module never keeps a background timer alive.
    fn stop_refresh_timer(&self) {
        if let Some(id) = self.refresh_timer.take() {
            id.remove();
        }
    }

    /// Test-only peek at the timer field without consuming the `SourceId`
    /// (it is move-only, so a plain `Cell::get` is not available).
    #[cfg(test)]
    fn has_active_timer(&self) -> bool {
        let existing = self.refresh_timer.take();
        let active = existing.is_some();
        self.refresh_timer.set(existing);
        active
    }

    fn fetch_completed(&self) -> bool {
        let result = {
            let conn = self.conn.borrow();
            reprise_core::library::settings::get_new_releases_fetch_completed(&conn)
        };
        result.unwrap_or_else(|error| {
            tracing::warn!(%error, "could not read New Releases fetch state");
            false
        })
    }

    fn enabled_changed(self: &Rc<Self>, enabled: bool) {
        if enabled {
            self.start_refresh_timer();
            if !self.fetch_completed() {
                self.fetch_now();
            } else {
                self.render(false, false);
            }
        } else {
            self.stop_refresh_timer();
            self.render(false, false);
        }
    }

    fn fetch_now(self: &Rc<Self>) {
        let enabled = reprise_core::modules::is_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::NEW_RELEASES_MODULE,
        )
        .unwrap_or(false);
        if !module_effect(enabled, true, true, false).fetch_allowed {
            return;
        }
        if self.fetching.replace(true) {
            return;
        }
        self.fetch_stack.set_visible_child_name("spinner");
        self.spinner.start();
        self.fetch_button.set_sensitive(false);
        self.failure.set_visible(false);
        self.render(false, false);

        let database_path = self.database_path.clone();
        let result = one_shot_task::spawn("reprise-new-releases", move || {
            fetch_from_database(&database_path)
        });
        let Ok(receiver) = result else {
            self.finish_fetch(true);
            return;
        };
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let failed = match receiver.recv().await {
                Ok(Ok(report)) => report.failed > 0,
                Ok(Err(error)) => {
                    tracing::warn!(%error, "could not refresh New Releases");
                    true
                }
                Err(error) => {
                    tracing::warn!(%error, "New Releases worker closed without a result");
                    true
                }
            };
            if let Some(state) = weak.upgrade() {
                state.finish_fetch(failed);
            }
        });
    }

    fn finish_fetch(self: &Rc<Self>, failed: bool) {
        if !failed {
            let result = {
                let conn = self.conn.borrow();
                reprise_core::library::settings::set_new_releases_fetch_completed(&conn, true)
            };
            if let Err(error) = result {
                tracing::warn!(%error, "could not save New Releases fetch state");
            }
        }
        self.fetching.set(false);
        self.spinner.stop();
        self.fetch_stack.set_visible_child_name("icon");
        self.fetch_button.set_sensitive(true);
        self.render(false, failed);
    }
}

fn bind_runtime(state: &Rc<NewReleasesPopover>, runtime: &Rc<ArtistNewsRuntime>) {
    let alive = Rc::downgrade(state);
    let target = Rc::downgrade(state);
    runtime.subscribe_enabled(
        move || alive.upgrade().is_some(),
        move |enabled| {
            if let Some(state) = target.upgrade() {
                state.enabled_changed(enabled);
            }
        },
    );
}

pub(in crate::ui) fn install(
    header: &adw::HeaderBar,
    window: &adw::ApplicationWindow,
    conn: &Rc<RefCell<rusqlite::Connection>>,
    database_path: &Path,
    runtime: &Rc<ArtistNewsRuntime>,
    on_show_album: release_row::OnShowAlbum,
) {
    let state = NewReleasesPopover::new(conn.clone(), database_path.to_path_buf(), on_show_album);
    header.pack_end(&state.button);
    bind_runtime(&state, runtime);
    state.retain_for_window(window);
}

fn fetch_from_database(
    database_path: &Path,
) -> Result<reprise_core::artist_news::RefreshReport, reprise_core::artist_news::NewsError> {
    let conn = reprise_core::db::open_migrated(Some(database_path))
        .map_err(|error| reprise_core::artist_news::NewsError::Database(error.to_string()))?;
    if !reprise_core::modules::is_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE)
        .map_err(|error| reprise_core::artist_news::NewsError::Database(error.to_string()))?
    {
        return Ok(reprise_core::artist_news::RefreshReport::default());
    }
    let today = chrono::Local::now().date_naive();
    let scope = reprise_core::artist_news::configured_fetch_scope(&conn, today)
        .map_err(|error| reprise_core::artist_news::NewsError::Database(error.to_string()))?;
    reprise_core::artist_news::refresh(&conn, today, scope, true, fallback_accent_for_artist)
}

fn build_footer() -> (
    gtk4::Box,
    gtk4::Button,
    gtk4::Stack,
    gtk4::Spinner,
    gtk4::Label,
    gtk4::Label,
) {
    let icon = gtk4::Image::from_icon_name("view-refresh-symbolic");
    let spinner = gtk4::Spinner::new();
    let stack = gtk4::Stack::new();
    stack.add_named(&icon, Some("icon"));
    stack.add_named(&spinner, Some("spinner"));
    stack.set_visible_child_name("icon");
    let fetch_label = gtk4::Label::new(Some(&strings::text(strings::FETCH_NOW)));
    let fetch_content = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    fetch_content.append(&stack);
    fetch_content.append(&fetch_label);
    let fetch_button = gtk4::Button::builder()
        .child(&fetch_content)
        .css_classes(["flat"])
        .build();
    let updated = gtk4::Label::new(None);
    updated.add_css_class("dim-label");
    let failure = gtk4::Label::new(Some(&strings::text(strings::FETCH_FAILED_INLINE)));
    failure.add_css_class("dim-label");
    failure.set_visible(false);
    let status = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    status.set_hexpand(true);
    status.set_halign(gtk4::Align::End);
    status.append(&updated);
    status.append(&failure);
    let footer = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    footer.append(&fetch_button);
    footer.append(&status);
    (footer, fetch_button, stack, spinner, updated, failure)
}

#[cfg(test)]
#[path = "popover_tests.rs"]
mod tests;
