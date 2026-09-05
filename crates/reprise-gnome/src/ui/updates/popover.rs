//! Headerbar entry point and transient New Releases popover.

use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::concerts::ConcertsRuntime;
use crate::ui::strings;

use super::badge::{self, FeedBadgeInput};
use super::concerts_section::ConcertsSection;
#[cfg(test)]
use super::feed_row;
use super::feed_snapshot;
use super::footer_state::{aggregate as aggregate_footer_state, ActiveFeed};
use super::release_row;
use super::shell;
use crate::ui::feed_footer::FeedFooter;

#[path = "popover_fetch.rs"]
mod popover_fetch;

/// How often the background timer re-checks staleness while the module is
/// enabled (Beschluss 8). Deliberately coarse: `refresh_due`'s own 6 h+jitter
/// window is the real gate, this just samples it periodically.
const REFRESH_TIMER_SECONDS: u32 = 3600;

#[derive(Debug, PartialEq, Eq)]
struct OpeningEffect {
    seen_ids: Vec<String>,
    navigates: bool,
}

/// Every unseen candidate is stamped when Updates opens, including candidates
/// below the visible cap. Otherwise the badge could never clear; the section
/// count names the complete batch and the jump row leads to the remainder.
fn opening_effect(unseen_ids: &[String]) -> OpeningEffect {
    OpeningEffect {
        seen_ids: unseen_ids.to_vec(),
        navigates: false,
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
use reprise_core::updates::fetch_allowed as periodic_fetch_due;

use reprise_core::db::Db;
use reprise_core::updates::{Feed, FeedRefresh};

pub(in crate::ui) type OnOpenView = Rc<dyn Fn(reprise_core::browser::navigation::SidebarTarget)>;

pub(in crate::ui) struct UpdatesCallbacks {
    pub on_show_album: release_row::OnShowAlbum,
    pub on_open_view: OnOpenView,
}

struct NewReleasesPopover {
    conn: Rc<Db>,
    database_path: PathBuf,
    concerts_runtime: Rc<ConcertsRuntime>,
    button: gtk4::MenuButton,
    badge: gtk4::Label,
    popover: gtk4::Popover,
    news_section: gtk4::Box,
    concerts_section: ConcertsSection,
    list: gtk4::ListBox,
    empty: gtk4::Label,
    releases_header: gtk4::Button,
    new_tag: gtk4::Label,
    footer: FeedFooter,
    fetching: Cell<bool>,
    /// The fetch currently in flight across both feeds, or the finished one
    /// whose outcome the footer is still showing.
    run: RefCell<FeedRefresh>,
    news_loaded_this_visit: Cell<bool>,
    concerts_loaded_this_visit: Cell<bool>,
    dismissed_concert_ids: RefCell<HashSet<i64>>,
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
        conn: Rc<Db>,
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
            releases_header,
            new_tag,
            footer,
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
            releases_header,
            new_tag,
            footer,
            fetching: Cell::new(false),
            run: RefCell::new(FeedRefresh::start(&[])),
            news_loaded_this_visit: Cell::new(false),
            concerts_loaded_this_visit: Cell::new(false),
            dismissed_concert_ids: RefCell::new(HashSet::new()),
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
        {
            let weak = Rc::downgrade(&state);
            state
                .concerts_section
                .set_on_dismiss_event(Rc::new(move |id| {
                    let Some(state) = weak.upgrade() else { return };
                    if let Err(error) = reprise_core::concerts::mark_event_seen(
                        &state.conn,
                        id,
                        chrono::Utc::now().timestamp(),
                    ) {
                        tracing::warn!(%error, event_id = id, "could not dismiss Concerts update");
                        return;
                    }
                    // Opening already stamps the scope, but this idempotent call
                    // is still the correct persistence path if the row was not
                    // stamped yet. The held session delta is a separate concern.
                    state.dismissed_concert_ids.borrow_mut().insert(id);
                    state.render(false, false);
                }));
        }
        {
            let weak = Rc::downgrade(&state);
            state.concerts_section.set_on_open_view(Rc::new(move || {
                if let Some(state) = weak.upgrade() {
                    state.open_view(reprise_core::browser::navigation::SidebarTarget::Concerts);
                }
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
        self.footer.connect_reload(move || {
            if let Some(state) = weak.upgrade() {
                state.start_fetch(true);
            }
        });

        let weak = Rc::downgrade(self);
        self.releases_header.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.open_view(reprise_core::browser::navigation::SidebarTarget::Releases);
            }
        });
    }

    fn open_view(&self, target: reprise_core::browser::navigation::SidebarTarget) {
        self.popover.popdown();
        (self.on_open_view)(target);
    }

    fn render(self: &Rc<Self>, mark_seen: bool, failed: bool) {
        let news_enabled = reprise_core::modules::is_enabled(
            &self.conn,
            &reprise_core::modules::NEW_RELEASES_MODULE,
        )
        .unwrap_or(false);
        let concerts_enabled =
            reprise_core::modules::is_enabled(&self.conn, &reprise_core::modules::CONCERTS_MODULE)
                .unwrap_or(false);
        let today = chrono::Local::now().date_naive();
        let releases = feed_snapshot::releases(&self.conn, news_enabled, today);
        let mut concerts = feed_snapshot::concerts(&self.conn, concerts_enabled, today);
        concerts.delta.shown = {
            let dismissed = self.dismissed_concert_ids.borrow();
            visible_concert_rows(concerts.delta.shown, &dismissed)
        };
        let fetch_completed = self.fetch_completed();
        let effect = module_effect(
            news_enabled,
            releases.delta.total > 0,
            fetch_completed,
            self.fetching.get(),
        );
        self.button
            .set_visible(effect.button_visible || concerts_enabled);
        let news_visible = news_enabled;
        self.news_section.set_visible(news_visible);
        // Only an actually unseen batch is announced as new. A batch held over
        // from the last visit still renders, but without a count that would
        // contradict the badge (which has cleared by then).
        self.new_tag
            .set_label(&strings::updates_new_count(releases.delta.total));
        self.new_tag
            .set_visible(news_visible && releases.delta.unseen && releases.delta.total > 0);
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
                    .set_label(&strings::text(strings::UPDATES_NO_NEW_RELEASES));
                self.list.append(&self.empty);
            }
        }
        let on_hide: Rc<dyn Fn(&str)> = {
            let weak = Rc::downgrade(self);
            Rc::new(move |mbid: &str| {
                let Some(state) = weak.upgrade() else { return };
                if let Err(error) =
                    reprise_core::artist_news::set_release_hidden(&state.conn, mbid, true)
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
        for release in &releases.delta.shown {
            self.list.append(&release_row::build(
                release,
                today,
                &on_hide,
                &self.on_show_album,
                &close_popover,
            ));
        }
        self.concerts_section.render(
            concerts_enabled,
            concerts.credentials,
            concerts.delta.total,
            concerts.delta.unseen,
            &concerts.delta.shown,
            today,
            reprise_core::modules::is_enabled(&self.conn, &reprise_core::modules::ARTWORK_MODULE)
                .unwrap_or(false),
        );
        if mark_seen {
            let effect = opening_effect(&releases.unseen_ids);
            if !effect.seen_ids.is_empty() {
                let now = chrono::Utc::now().timestamp();
                if let Err(error) =
                    reprise_core::artist_news::mark_releases_seen(&self.conn, &effect.seen_ids, now)
                {
                    tracing::warn!(%error, "could not mark New Releases seen");
                }
            }
            if concerts_enabled {
                let conn = &self.conn;
                let location = reprise_core::concerts::config::location(conn);
                match location {
                    Ok(location) => {
                        if let Err(error) = reprise_core::concerts::mark_scope_seen(
                            conn,
                            &concerts.filter,
                            location.as_ref(),
                            today,
                            chrono::Utc::now().timestamp(),
                        ) {
                            tracing::warn!(%error, "could not mark Concerts updates seen");
                        }
                    }
                    Err(error) => {
                        tracing::warn!(%error, "could not read Concerts scope while opening Updates");
                    }
                }
            }
        }
        let unseen_releases =
            reprise_core::artist_news::unseen_release_count(&self.conn, today).unwrap_or_default();
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
        let latest_news = reprise_core::artist_news::latest_fetched_at(&self.conn)
            .ok()
            .flatten();
        let concerts_active = concerts_enabled && concerts.credentials;
        let run_failed = {
            let run = self.run.borrow();
            failed || run.has_failed(Feed::NewReleases) || run.has_failed(Feed::Concerts)
        };
        let footer_state = aggregate_footer_state(
            ActiveFeed {
                active: news_enabled,
                latest: latest_news,
                loaded_this_visit: self.news_loaded_this_visit.get(),
            },
            ActiveFeed {
                active: concerts_active,
                latest: latest_concerts,
                loaded_this_visit: self.concerts_loaded_this_visit.get(),
            },
            reprise_core::online_sources::is_enabled(&self.conn).unwrap_or(false),
            self.fetching.get(),
            run_failed,
            concerts_enabled && !concerts.credentials,
        );
        self.footer
            .apply_with_copy(footer_state, strings::updates_feed_footer_copy());
    }

    fn concerts_badge_state(&self, today: chrono::NaiveDate) -> (bool, bool, i64, Option<i64>) {
        let conn = &self.conn;
        let credentials = reprise_core::concerts::config::credentials(conn)
            .is_ok_and(|credentials| !credentials.is_empty());
        let latest = reprise_core::concerts::latest_fetch_at(conn).ok().flatten();
        if !credentials {
            return (false, false, 0, latest);
        }
        let unseen = reprise_core::concerts::config::persisted_filter(conn)
            .and_then(|filter| {
                let location = reprise_core::concerts::config::location(conn)?;
                reprise_core::concerts::count_unseen(conn, &filter, location.as_ref(), today)
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
            &self.conn,
            &reprise_core::modules::NEW_RELEASES_MODULE,
        )
        .unwrap_or(false);
        let latest = reprise_core::artist_news::last_check_started_at(&self.conn)
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
            let conn = &self.conn;
            reprise_core::library::settings::get_new_releases_fetch_completed(conn)
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
}

fn visible_concert_rows(
    rows: Vec<reprise_core::concerts::ConcertRow>,
    dismissed: &HashSet<i64>,
) -> Vec<reprise_core::concerts::ConcertRow> {
    rows.into_iter()
        .filter(|row| !dismissed.contains(&row.id))
        .collect()
}

fn bind_runtime(state: &Rc<NewReleasesPopover>, runtime: &Rc<ArtistNewsRuntime>) {
    let alive = Rc::downgrade(state);
    let target = Rc::downgrade(state);
    let initial = Rc::new(Cell::new(true));
    let current_enabled = runtime.enabled.clone();
    runtime.subscribe_enabled(
        move || alive.upgrade().is_some(),
        move |enabled| {
            let Some(state) = target.upgrade() else {
                return;
            };
            if initial.replace(false) {
                let current_enabled = current_enabled.clone();
                crate::ui::startup_quiet::run_after_quiet(move || {
                    // A user may toggle the module while the initial callback
                    // is waiting. Never replay that stale startup state.
                    if current_enabled.get() == enabled {
                        state.enabled_changed(enabled);
                    }
                });
            } else {
                // A live Preferences change is explicit and must not wait for
                // startup's gate.
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
    conn: &Rc<Db>,
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
    let conn = reprise_core::db::Db::open_migrated(Some(database_path))
        .map_err(|error| reprise_core::artist_news::NewsError::Database(error.to_string()))?;
    if !reprise_core::modules::is_enabled(&conn, &reprise_core::modules::NEW_RELEASES_MODULE)
        .map_err(|error| reprise_core::artist_news::NewsError::Database(error.to_string()))?
    {
        return Ok(reprise_core::artist_news::RefreshReport::default());
    }
    let today = chrono::Local::now().date_naive();
    let scope = reprise_core::artist_news::configured_fetch_scope(&conn)
        .map_err(|error| reprise_core::artist_news::NewsError::Database(error.to_string()))?;
    reprise_core::artist_news::refresh(&conn, today, scope, true)
}

#[cfg(test)]
#[path = "popover_tests.rs"]
mod tests;
