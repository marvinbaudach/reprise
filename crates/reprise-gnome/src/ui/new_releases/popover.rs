//! Headerbar entry point and transient New Releases popover.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;

use crate::ui::artist_news_worker::ArtistNewsRuntime;
use crate::ui::{one_shot_task, popover_lifecycle, strings};

use super::release_cover::{fallback_accent_for_artist, LazyReleaseCover};

const POPOVER_LIMIT: usize = 5;
const HERO_COVER_EDGE: i32 = 56;
const ROW_COVER_EDGE: i32 = 34;

#[derive(Debug, PartialEq, Eq)]
struct OpeningEffect {
    seen_ids: Vec<String>,
    navigates: bool,
}

fn opening_effect(releases: &[reprise_core::artist_news::StoredRelease]) -> OpeningEffect {
    OpeningEffect {
        seen_ids: releases
            .iter()
            .take(POPOVER_LIMIT)
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

fn see_all_visible(total: usize, visible: usize, hidden: usize) -> bool {
    total > visible || hidden > 0
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

struct NewReleasesPopover {
    conn: Rc<RefCell<rusqlite::Connection>>,
    database_path: PathBuf,
    button: gtk4::MenuButton,
    badge: gtk4::Label,
    popover: gtk4::Popover,
    rows: gtk4::Box,
    empty: gtk4::Label,
    see_all: gtk4::Button,
    fetch_button: gtk4::Button,
    fetch_stack: gtk4::Stack,
    spinner: gtk4::Spinner,
    updated: gtk4::Label,
    failure: gtk4::Label,
    fetching: Cell<bool>,
    on_see_all: Rc<dyn Fn()>,
}

impl NewReleasesPopover {
    fn new(
        conn: Rc<RefCell<rusqlite::Connection>>,
        database_path: PathBuf,
        on_see_all: Rc<dyn Fn()>,
    ) -> Rc<Self> {
        let (button, badge) = build_button();
        let popover = gtk4::Popover::new();
        let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
        let empty = gtk4::Label::new(None);
        empty.set_wrap(true);
        empty.set_justify(gtk4::Justification::Center);
        empty.set_margin_top(12);
        empty.set_margin_bottom(12);
        let see_all = gtk4::Button::with_label(&strings::text(strings::SEE_ALL_RELEASES));
        see_all.add_css_class("flat");
        see_all.add_css_class("pill");
        see_all.set_halign(gtk4::Align::Center);
        see_all.set_visible(false);
        let (footer, fetch_button, fetch_stack, spinner, updated, failure) = build_footer();
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 8);
        content.set_margin_top(10);
        content.set_margin_bottom(10);
        content.set_margin_start(10);
        content.set_margin_end(10);
        content.append(&rows);
        content.append(&see_all);
        content.append(&gtk4::Separator::new(gtk4::Orientation::Horizontal));
        content.append(&footer);
        popover.set_child(Some(&content));
        popover_lifecycle::unparent_after_actions(&popover);
        button.set_popover(Some(&popover));

        let state = Rc::new(Self {
            conn,
            database_path,
            button,
            badge,
            popover,
            rows,
            empty,
            see_all,
            fetch_button,
            fetch_stack,
            spinner,
            updated,
            failure,
            fetching: Cell::new(false),
            on_see_all,
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
            }
        });

        let weak = Rc::downgrade(self);
        self.fetch_button.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.fetch_now();
            }
        });

        let weak = Rc::downgrade(self);
        self.see_all.connect_clicked(move |_| {
            if let Some(state) = weak.upgrade() {
                state.popover.popdown();
                (state.on_see_all)();
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
        let hidden = all_releases.iter().filter(|release| release.hidden).count();
        let fetch_completed = self.fetch_completed();
        let effect = module_effect(
            enabled,
            !all_releases.is_empty(),
            fetch_completed,
            self.fetching.get(),
        );
        self.button.set_visible(effect.button_visible);
        clear_box(&self.rows);
        match effect.empty {
            EmptyPresentation::Hidden => {}
            EmptyPresentation::Checking => {
                self.empty
                    .set_label(&strings::text(strings::NEW_RELEASES_CHECKING));
                self.rows.append(&self.empty);
            }
            EmptyPresentation::NoReleases => {
                self.empty
                    .set_label(&strings::text(strings::NEW_RELEASES_NONE));
                self.rows.append(&self.empty);
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
        for (index, release) in releases.iter().take(POPOVER_LIMIT).enumerate() {
            self.rows
                .append(&build_release_row(release, index == 0, &on_hide));
        }
        let visible = releases.len().min(POPOVER_LIMIT);
        self.see_all
            .set_visible(see_all_visible(releases.len(), visible, hidden));

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
        self.badge.set_visible(effect.badge_allowed && unseen > 0);
        let latest = all_releases.iter().map(|release| release.fetched_at).max();
        let footer = footer_presentation(latest, chrono::Utc::now().timestamp(), failed);
        self.updated.set_label(&footer.updated);
        self.updated.set_visible(latest.is_some());
        self.failure.set_visible(footer.show_cached_failure);
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
        if enabled && !self.fetch_completed() {
            self.fetch_now();
        } else {
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
    on_see_all: Rc<dyn Fn()>,
    runtime: &Rc<ArtistNewsRuntime>,
) {
    let state = NewReleasesPopover::new(conn.clone(), database_path.to_path_buf(), on_see_all);
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

fn build_button() -> (gtk4::MenuButton, gtk4::Label) {
    let glyph = gtk4::Label::new(Some("✦"));
    glyph.add_css_class("title-3");
    let badge = gtk4::Label::new(Some("•"));
    badge.add_css_class("accent");
    badge.set_halign(gtk4::Align::End);
    badge.set_valign(gtk4::Align::Start);
    badge.set_visible(false);
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&glyph));
    overlay.add_overlay(&badge);
    let button = gtk4::MenuButton::builder()
        .child(&overlay)
        .tooltip_text(strings::text(strings::NEW_RELEASES))
        .css_classes(["flat"])
        .visible(false)
        .build();
    button.update_property(&[gtk4::accessible::Property::Label(&strings::text(
        strings::NEW_RELEASES,
    ))]);
    (button, badge)
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

/// One popover entry. `on_hide` is what keeps NR-4 reachable: hiding used to
/// live only in the digest view, which is itself only reachable through
/// "See all" — and that button appears only when the list overflows or
/// something is already hidden. A user with a short list could therefore
/// never hide anything, and the digest's "N hidden · Show" footer could never
/// appear for them. The action belongs where the entries are.
fn build_release_row(
    release: &reprise_core::artist_news::StoredRelease,
    hero: bool,
    on_hide: &Rc<dyn Fn(&str)>,
) -> gtk4::Box {
    let cover = LazyReleaseCover::new(
        &release.release_group_mbid,
        &release.artist_name,
        &release.fallback_accent,
        if hero {
            HERO_COVER_EDGE
        } else {
            ROW_COVER_EDGE
        },
    );
    let title = gtk4::Label::new(Some(&release.title));
    title.set_xalign(0.0);
    title.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let meta = gtk4::Label::new(Some(&format!(
        "{} · {}",
        release.artist_name, release.first_release_date
    )));
    meta.set_xalign(0.0);
    meta.add_css_class("dim-label");
    let text = gtk4::Box::new(gtk4::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.append(&title);
    text.append(&meta);
    let hide = gtk4::Button::with_label(&strings::text(strings::HIDE_RELEASE));
    hide.add_css_class("flat");
    hide.add_css_class("pill");
    hide.set_valign(gtk4::Align::Center);
    let on_hide = on_hide.clone();
    let mbid = release.release_group_mbid.clone();
    hide.connect_clicked(move |_| on_hide(&mbid));

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    row.append(cover.widget());
    row.append(&text);
    row.append(&hide);
    row
}

fn clear_box(container: &gtk4::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

#[cfg(test)]
#[path = "popover_tests.rs"]
mod tests;
