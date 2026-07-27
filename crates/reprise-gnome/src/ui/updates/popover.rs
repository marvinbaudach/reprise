//! Headerbar entry point and transient New Releases popover.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::concerts::{ConcertsRequest, ConcertsRuntime};
use crate::ui::{one_shot_task, strings};

use super::badge::{self, FeedBadgeInput};
use super::concerts_section::ConcertsSection;
use super::feed_snapshot;
use super::release_cover::fallback_accent_for_artist;
use super::release_row;
use super::shell;

/// Caps the scrolling release list's natural height before it scrolls.
pub(in crate::ui) const SCROLLER_MAX_HEIGHT: i32 = 288;
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

fn oldest_active_feed_timestamp(
    news_active: bool,
    news_latest: Option<i64>,
    concerts_active: bool,
    concerts_latest: Option<i64>,
) -> Option<i64> {
    match (news_active, concerts_active) {
        (false, false) => None,
        (true, false) => news_latest,
        (false, true) => concerts_latest,
        (true, true) => Some(news_latest?.min(concerts_latest?)),
    }
}

fn fetch_failure_text(news_failed: bool, concerts_failed: bool) -> String {
    match (news_failed, concerts_failed) {
        (false, false) => String::new(),
        (true, false) => strings::text(strings::FETCH_FAILED_INLINE),
        (false, true) => strings::text(strings::UPDATES_CONCERTS_FETCH_FAILED),
        (true, true) => format!(
            "{} · {}",
            strings::text(strings::FETCH_FAILED_INLINE),
            strings::text(strings::UPDATES_CONCERTS_FETCH_FAILED)
        ),
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

#[derive(Clone, Copy)]
enum FeedKind {
    News,
    Concerts,
}

pub(in crate::ui) type OnOpenView = Rc<dyn Fn(reprise_core::browser::navigation::SidebarTarget)>;

pub(in crate::ui) struct UpdatesCallbacks {
    pub on_show_album: release_row::OnShowAlbum,
    pub on_open_view: OnOpenView,
}

struct NewReleasesPopover {
    conn: Rc<RefCell<rusqlite::Connection>>,
    database_path: PathBuf,
    concerts_runtime: Rc<ConcertsRuntime>,
    button: gtk4::MenuButton,
    badge: gtk4::Label,
    popover: gtk4::Popover,
    news_section: gtk4::Box,
    concerts_section: ConcertsSection,
    list: gtk4::ListBox,
    empty: gtk4::Label,
    new_tag: gtk4::Label,
    releases_jump: gtk4::Button,
    releases_jump_label: gtk4::Label,
    concerts_jump: gtk4::Button,
    concerts_jump_label: gtk4::Label,
    fetch_button: gtk4::Button,
    fetch_stack: gtk4::Stack,
    spinner: gtk4::Spinner,
    updated: gtk4::Label,
    failure: gtk4::Label,
    fetching: Cell<bool>,
    pending_fetches: Cell<u8>,
    news_failed: Cell<bool>,
    concerts_failed: Cell<bool>,
    generation: Cell<u64>,
    /// The hourly background staleness timer (Beschluss 8), running only
    /// while the module is enabled. `SourceId` is move-only, so `Cell::take`
    /// is how `stop_refresh_timer` retrieves it to call `remove()`.
    refresh_timer: Cell<Option<gtk4::glib::SourceId>>,
    on_show_album: release_row::OnShowAlbum,
    on_open_view: OnOpenView,
}

impl NewReleasesPopover {
    fn new(
        conn: Rc<RefCell<rusqlite::Connection>>,
        database_path: PathBuf,
        concerts_runtime: Rc<ConcertsRuntime>,
        on_show_album: release_row::OnShowAlbum,
        on_open_view: OnOpenView,
    ) -> Rc<Self> {
        let shell::UpdatesShell {
            button,
            badge,
            popover,
            news_section,
            concerts_section,
            list,
            empty,
            new_tag,
            releases_jump,
            releases_jump_label,
            concerts_jump,
            concerts_jump_label,
            fetch_button,
            fetch_stack,
            spinner,
            updated,
            failure,
        } = shell::build();

        let state = Rc::new(Self {
            conn,
            database_path,
            concerts_runtime,
            button,
            badge,
            popover,
            news_section,
            concerts_section,
            list,
            empty,
            new_tag,
            releases_jump,
            releases_jump_label,
            concerts_jump,
            concerts_jump_label,
            fetch_button,
            fetch_stack,
            spinner,
            updated,
            failure,
            fetching: Cell::new(false),
            pending_fetches: Cell::new(0),
            news_failed: Cell::new(false),
            concerts_failed: Cell::new(false),
            generation: Cell::new(0),
            refresh_timer: Cell::new(None),
            on_show_album,
            on_open_view,
        });
        {
            let weak = Rc::downgrade(&state);
            state.concerts_section.set_on_open_url(Rc::new(move |url| {
                if let Some(state) = weak.upgrade() {
                    state.popover.popdown();
                }
                release_row::launch_uri(&url);
            }));
        }
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
                state.maybe_background_refresh();
            }
        });

        let weak = Rc::downgrade(self);
        self.fetch_button.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.start_fetch(true);
            }
        });

        self.wire_jump(
            &self.releases_jump,
            reprise_core::browser::navigation::SidebarTarget::Releases,
        );
        self.wire_jump(
            &self.concerts_jump,
            reprise_core::browser::navigation::SidebarTarget::Concerts,
        );
    }

    fn wire_jump(
        self: &Rc<Self>,
        button: &gtk4::Button,
        target: reprise_core::browser::navigation::SidebarTarget,
    ) {
        let weak = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.popover.popdown();
                (state.on_open_view)(target.clone());
            }
        });
    }

    fn render(self: &Rc<Self>, mark_seen: bool, failed: bool) {
        let news_enabled = reprise_core::modules::is_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::NEW_RELEASES_MODULE,
        )
        .unwrap_or(false);
        let concerts_enabled = reprise_core::modules::is_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::CONCERTS_MODULE,
        )
        .unwrap_or(false);
        let today = chrono::Local::now().date_naive();
        let all_releases = if news_enabled {
            match reprise_core::artist_news::query_releases(&self.conn.borrow(), true, today) {
                Ok(releases) => releases,
                Err(error) => {
                    tracing::warn!(%error, "could not query New Releases");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        let releases = all_releases
            .iter()
            .filter(|release| !release.hidden)
            .cloned()
            .collect::<Vec<_>>();
        let fetch_completed = self.fetch_completed();
        let effect = module_effect(
            news_enabled,
            !all_releases.is_empty(),
            fetch_completed,
            self.fetching.get(),
        );
        self.button
            .set_visible(effect.button_visible || concerts_enabled);
        self.news_section.set_visible(news_enabled);
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
        let concerts = feed_snapshot::concerts(&self.conn.borrow(), concerts_enabled, today);
        self.concerts_section.render(
            concerts_enabled,
            concerts.credentials,
            concerts.filter.radius_km.is_some(),
            &concerts.unseen,
            today,
        );
        self.concerts_jump.set_visible(concerts_enabled);
        self.concerts_jump_label
            .set_label(&strings::updates_show_all_concerts(concerts.count));
        let releases_count = feed_snapshot::releases_count(&self.conn.borrow(), today);
        self.releases_jump.set_visible(news_enabled);
        self.releases_jump_label
            .set_label(&strings::updates_show_all_releases(releases_count));
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
            if concerts_enabled {
                let conn = self.conn.borrow();
                let filter = reprise_core::concerts::config::persisted_filter(&conn);
                let location = reprise_core::concerts::config::location(&conn);
                match (filter, location) {
                    (Ok(filter), Ok(location)) => {
                        if let Err(error) = reprise_core::concerts::mark_scope_seen(
                            &conn,
                            &filter,
                            location.as_ref(),
                            today,
                            chrono::Utc::now().timestamp(),
                        ) {
                            tracing::warn!(%error, "could not mark Concerts updates seen");
                        }
                    }
                    (Err(error), _) | (_, Err(error)) => {
                        tracing::warn!(%error, "could not read Concerts scope while opening Updates");
                    }
                }
            }
        }
        let unseen_releases =
            reprise_core::artist_news::unseen_release_count(&self.conn.borrow(), today)
                .unwrap_or_default();
        let (_, concerts_ready, unseen_concerts, latest_concerts) =
            self.concerts_badge_state(today);
        match badge::updates_badge(
            FeedBadgeInput {
                enabled: news_enabled,
                ready: fetch_completed,
                unseen: unseen_releases,
            },
            FeedBadgeInput {
                enabled: concerts_enabled,
                ready: concerts_ready,
                unseen: unseen_concerts,
            },
        ) {
            Some(text) => {
                self.badge.set_label(&text);
                self.badge.set_visible(true);
            }
            _ => self.badge.set_visible(false),
        }
        if unseen_releases > 0 {
            self.new_tag
                .set_label(&strings::new_releases_new_count(unseen_releases));
            self.new_tag.set_visible(true);
        } else {
            self.new_tag.set_visible(false);
        }
        let latest_news = reprise_core::artist_news::latest_fetched_at(&self.conn.borrow())
            .ok()
            .flatten();
        let latest = oldest_active_feed_timestamp(
            news_enabled,
            latest_news,
            concerts_enabled && concerts.credentials,
            latest_concerts,
        );
        let footer = footer_presentation(latest, chrono::Utc::now().timestamp(), failed);
        self.updated.set_label(&footer.updated);
        self.updated.set_visible(latest.is_some());
        let failure_text = fetch_failure_text(failed, self.concerts_failed.get());
        self.failure.set_label(&failure_text);
        self.failure
            .set_visible(footer.show_cached_failure || self.concerts_failed.get());
    }

    fn concerts_badge_state(&self, today: chrono::NaiveDate) -> (bool, bool, i64, Option<i64>) {
        let conn = self.conn.borrow();
        let credentials = reprise_core::concerts::config::credentials(&conn)
            .is_ok_and(|credentials| !credentials.is_empty());
        let latest = reprise_core::concerts::latest_fetch_at(&conn)
            .ok()
            .flatten();
        if !credentials {
            return (false, false, 0, latest);
        }
        let unseen = reprise_core::concerts::config::persisted_filter(&conn)
            .and_then(|filter| {
                let location = reprise_core::concerts::config::location(&conn)?;
                reprise_core::concerts::count_unseen(&conn, &filter, location.as_ref(), today)
            })
            .unwrap_or_else(|error| {
                tracing::warn!(%error, "could not count unseen Concerts updates");
                0
            });
        (true, latest.is_some(), unseen, latest)
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
            self.start_fetch(false);
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
                self.start_fetch(false);
            } else {
                self.render(false, false);
            }
        } else {
            self.stop_refresh_timer();
            self.render(false, false);
        }
    }

    fn start_fetch(self: &Rc<Self>, include_concerts: bool) {
        if self.fetching.get() {
            return;
        }
        let news_enabled = reprise_core::modules::is_enabled(
            &self.conn.borrow(),
            &reprise_core::modules::NEW_RELEASES_MODULE,
        )
        .unwrap_or(false);
        let concerts_enabled = include_concerts
            && reprise_core::modules::is_enabled(
                &self.conn.borrow(),
                &reprise_core::modules::CONCERTS_MODULE,
            )
            .unwrap_or(false)
            && reprise_core::concerts::config::credentials(&self.conn.borrow())
                .is_ok_and(|credentials| !credentials.is_empty());
        let pending = u8::from(news_enabled) + u8::from(concerts_enabled);
        if pending == 0 {
            self.render(false, false);
            return;
        }
        self.fetching.set(true);
        self.pending_fetches.set(pending);
        self.news_failed.set(false);
        self.concerts_failed.set(false);
        self.fetch_stack.set_visible_child_name("spinner");
        self.spinner.start();
        self.fetch_button.set_sensitive(false);
        self.failure.set_visible(false);
        self.render(false, false);

        if news_enabled {
            self.start_news_fetch();
        }
        if concerts_enabled {
            self.start_concerts_fetch();
        }
    }

    fn start_news_fetch(self: &Rc<Self>) {
        let database_path = self.database_path.clone();
        let result = one_shot_task::spawn("reprise-new-releases", move || {
            fetch_from_database(&database_path)
        });
        let Ok(receiver) = result else {
            self.finish_feed(FeedKind::News, true);
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
                state.finish_feed(FeedKind::News, failed);
            }
        });
    }

    fn start_concerts_fetch(self: &Rc<Self>) {
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        let (sender, receiver) = async_channel::bounded(1);
        if !self.concerts_runtime.request(ConcertsRequest {
            generation,
            force: true,
            response: sender,
        }) {
            self.finish_feed(FeedKind::Concerts, true);
            return;
        }
        let weak = Rc::downgrade(self);
        gtk4::glib::spawn_future_local(async move {
            let failed = match receiver.recv().await {
                Ok(response) if response.generation == generation => match response.result {
                    Ok(summary) => summary.failed > 0,
                    Err(error) => {
                        tracing::warn!(%error, "could not refresh Concerts from Updates");
                        true
                    }
                },
                Ok(_) => true,
                Err(error) => {
                    tracing::warn!(%error, "Concerts worker closed without an Updates result");
                    true
                }
            };
            if let Some(state) = weak.upgrade() {
                state.finish_feed(FeedKind::Concerts, failed);
            }
        });
    }

    fn finish_feed(self: &Rc<Self>, feed: FeedKind, failed: bool) {
        if matches!(feed, FeedKind::News) && !failed {
            let result = {
                let conn = self.conn.borrow();
                reprise_core::library::settings::set_new_releases_fetch_completed(&conn, true)
            };
            if let Err(error) = result {
                tracing::warn!(%error, "could not save New Releases fetch state");
            }
        }
        match feed {
            FeedKind::News => self.news_failed.set(failed),
            FeedKind::Concerts => self.concerts_failed.set(failed),
        }
        let remaining = self.pending_fetches.get().saturating_sub(1);
        self.pending_fetches.set(remaining);
        if remaining > 0 {
            return;
        }
        self.fetching.set(false);
        self.spinner.stop();
        self.fetch_stack.set_visible_child_name("icon");
        self.fetch_button.set_sensitive(true);
        self.render(false, self.news_failed.get());
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

fn bind_concerts_runtime(state: &Rc<NewReleasesPopover>, runtime: &Rc<ConcertsRuntime>) {
    let alive = Rc::downgrade(state);
    let target = Rc::downgrade(state);
    runtime.subscribe_enabled(
        move || alive.upgrade().is_some(),
        move |_| {
            if let Some(state) = target.upgrade() {
                state.render(false, false);
            }
        },
    );
    let alive = Rc::downgrade(state);
    let target = Rc::downgrade(state);
    runtime.subscribe_settings(
        move || alive.upgrade().is_some(),
        move || {
            if let Some(state) = target.upgrade() {
                state.render(false, false);
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
    concerts_runtime: &Rc<ConcertsRuntime>,
    callbacks: UpdatesCallbacks,
) {
    let state = NewReleasesPopover::new(
        conn.clone(),
        database_path.to_path_buf(),
        concerts_runtime.clone(),
        callbacks.on_show_album,
        callbacks.on_open_view,
    );
    header.pack_end(&state.button);
    bind_runtime(&state, runtime);
    bind_concerts_runtime(&state, concerts_runtime);
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
    let scope = reprise_core::artist_news::configured_fetch_scope(&conn)
        .map_err(|error| reprise_core::artist_news::NewsError::Database(error.to_string()))?;
    reprise_core::artist_news::refresh(&conn, today, scope, true, fallback_accent_for_artist)
}

#[cfg(test)]
#[path = "popover_tests.rs"]
mod tests;
