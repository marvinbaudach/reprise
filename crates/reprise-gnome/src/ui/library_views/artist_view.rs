//! The Artists library view: a master/detail split.
//!
//! Composes the [`ArtistMaster`] list (fixed-width left pane) with the
//! [`ArtistDetailPane`] (flexible right pane) inside a horizontal `gtk4::Box`,
//! wrapped in an outer `gtk4::Stack` that swaps in an empty-library
//! `StatusPage` when there are no artists. Selecting an artist in the master
//! drives `ArtistDetailPane::show_artist`.

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use libadwaita as adw;
use reprise_core::queries::ArtistAlbum;
use rusqlite::Connection;

use crate::ui::artist_detail_pane::ArtistDetailPane;
use crate::ui::artist_master::ArtistMaster;
use crate::ui::artist_portrait_worker::ArtistPortraitRuntime;
use crate::ui::cover_loader::CoverLoader;
use crate::ui::strings;

const SPLIT_CHILD: &str = "split";
const EMPTY_CHILD: &str = "empty";

/// Owned, reference-counted view state. `ArtistView` keeps the sole strong
/// reference; `refresh_callback` hands out a `Weak` so a stale timer can never
/// keep the view alive.
struct Inner {
    stack: gtk4::Stack,
    master: ArtistMaster,
    detail: Rc<ArtistDetailPane>,
}

pub(in crate::ui) struct ArtistView {
    inner: Rc<Inner>,
}

impl ArtistView {
    pub(in crate::ui) fn new(
        conn: Rc<RefCell<Connection>>,
        cover_loader: Rc<CoverLoader>,
        portraits: Rc<ArtistPortraitRuntime>,
    ) -> Self {
        let master = ArtistMaster::new(conn.clone(), &portraits, &cover_loader);
        let detail = Rc::new(ArtistDetailPane::new(conn, cover_loader, portraits));

        // Master selection drives the detail pane. The GTK caller supplies the
        // reference "now" (core stays clock-free); a main-thread wall clock is
        // allowed here. Capturing a clone of `detail` (never `master`) keeps the
        // stored closure free of a master → closure → master cycle.
        master.set_on_select({
            let detail = detail.clone();
            move |artist| detail.show_artist(&artist, now_unix())
        });

        let detail_widget = detail.widget();
        detail_widget.set_hexpand(true);
        let split = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        split.add_css_class("artist-view");
        split.append(master.widget());
        split.append(detail_widget);

        let empty = adw::StatusPage::builder()
            .icon_name("avatar-default-symbolic")
            .title(strings::text(strings::ARTISTS_EMPTY_TITLE))
            .description(strings::text(strings::ARTISTS_EMPTY_DESCRIPTION))
            .build();

        let stack = gtk4::Stack::new();
        stack.add_named(&split, Some(SPLIT_CHILD));
        stack.add_named(&empty, Some(EMPTY_CHILD));

        let inner = Rc::new(Inner {
            stack,
            master,
            detail,
        });
        refresh_inner(&inner);
        Self { inner }
    }

    pub(in crate::ui) fn widget(&self) -> &gtk4::Widget {
        self.inner.stack.upcast_ref()
    }

    /// Re-runs the master query and re-evaluates the empty vs. populated state.
    // Consumed by later deep-link / refresh wiring; `refresh_callback` is the
    // path used today.
    #[allow(dead_code)]
    pub(in crate::ui) fn refresh(&self) {
        refresh_inner(&self.inner);
    }

    pub(in crate::ui) fn refresh_callback(&self) -> Rc<dyn Fn()> {
        let weak = Rc::downgrade(&self.inner);
        Rc::new(move || {
            if let Some(inner) = weak.upgrade() {
                refresh_inner(&inner);
            }
        })
    }

    // Task 9: consumed by deep-link routing (needs PlayerController).
    #[allow(dead_code)]
    pub(in crate::ui) fn select_artist(&self, artist: &str) {
        self.inner.master.select_artist(artist);
    }

    /// Task 9b: a self-contained `select_artist` callable for the player-bar
    /// artist deep-link, handed out so the player bar can select an artist
    /// without holding a strong reference to this view (see
    /// `ArtistMaster::select_callback` for the cycle-avoidance rationale).
    pub(in crate::ui) fn select_artist_callback(&self) -> Rc<dyn Fn(&str)> {
        self.inner.master.select_callback()
    }

    /// Lights the now-playing mini-EQ: the master row for `artist` and the
    /// detail pane's top-track row for `track_id`. Driven by the playback
    /// fan-out in `current_track_selection::wire` (`None`/`None` on stop).
    pub(in crate::ui) fn set_now_playing(&self, artist: Option<String>, track_id: Option<i64>) {
        self.inner.master.set_now_playing_artist(artist);
        self.inner.detail.set_now_playing_track(track_id);
    }

    // Task 9a: wired to PlayerController.
    pub(in crate::ui) fn set_on_play_all(&self, callback: impl Fn(String) + 'static) {
        self.inner.detail.set_on_play_all(callback);
    }

    // Task 9a: wired to PlayerController.
    pub(in crate::ui) fn set_on_shuffle(&self, callback: impl Fn(String) + 'static) {
        self.inner.detail.set_on_shuffle(callback);
    }

    // Task 9a: ⋮ menu "Add to queue" — wired to PlayerController.
    pub(in crate::ui) fn set_on_add_to_queue(&self, callback: impl Fn(String) + 'static) {
        self.inner.detail.set_on_add_to_queue(callback);
    }

    // Task 9a: ⋮ menu "Go to folder" — wired to the desktop file manager.
    pub(in crate::ui) fn set_on_go_to_folder(&self, callback: impl Fn(String) + 'static) {
        self.inner.detail.set_on_go_to_folder(callback);
    }

    pub(in crate::ui) fn set_on_show_all_tracks(&self, callback: impl Fn(String) + 'static) {
        self.inner.detail.set_on_show_all_tracks(callback);
    }

    pub(in crate::ui) fn set_on_album_activate(
        &self,
        callback: impl Fn(ArtistAlbum, String) + 'static,
    ) {
        self.inner.detail.set_on_album_activate(callback);
    }

    #[cfg(test)]
    fn master_count(&self) -> u32 {
        self.inner.master.count()
    }

    #[cfg(test)]
    fn select_index_for_test(&self, index: u32) {
        self.inner.master.select_index_for_test(index);
    }

    #[cfg(test)]
    fn hero_name(&self) -> String {
        self.inner.detail.hero_name()
    }
}

/// Reloads the master and shows the empty state when the library has no
/// artists, otherwise the master/detail split.
fn refresh_inner(inner: &Inner) {
    inner.master.reload();
    let child = if inner.master.count() == 0 {
        EMPTY_CHILD
    } else {
        SPLIT_CHILD
    };
    inner.stack.set_visible_child_name(child);
}

/// Main-thread wall-clock seconds since the Unix epoch, used as the reference
/// "now" for the detail pane's year-window stats.
fn now_unix() -> i64 {
    gtk4::glib::real_time() / 1_000_000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn selecting_a_master_artist_populates_the_detail_hero() {
        if gtk4::init().is_err() {
            return;
        }
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        conn.execute_batch(
            "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
             ('/one.flac','One','Artist A','First',0),
             ('/two.flac','Two','Artist B','Second',0);",
        )
        .unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let loader =
            crate::ui::cover_loader::CoverLoader::new(crate::ui::cover_download_worker::setup());

        let portraits = crate::ui::artist_portrait_worker::ArtistPortraitRuntime::setup();
        let view = ArtistView::new(conn, loader, portraits);
        assert_eq!(view.master_count(), 2);

        view.select_index_for_test(0);
        assert_eq!(view.hero_name(), "Artist A");
    }

    // Regression for Bug 2: `refresh_callback` hands out a `Weak<Inner>`, so it
    // only reloads while some strong owner keeps `Inner` alive. In production
    // that owner is the `Rc<ArtistView>` retained by `window::build` (captured
    // by the playback fan-out). Here we hold the view and prove the handed-out
    // callback still drives a reload after new rows land — before the fix the
    // view dropped at end of `build()` and the callback silently no-op'd.
    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn refresh_callback_reloads_master_while_view_is_retained() {
        if gtk4::init().is_err() {
            return;
        }
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        let conn = Rc::new(RefCell::new(conn));
        let loader =
            crate::ui::cover_loader::CoverLoader::new(crate::ui::cover_download_worker::setup());

        let portraits = crate::ui::artist_portrait_worker::ArtistPortraitRuntime::setup();
        let view = Rc::new(ArtistView::new(conn.clone(), loader, portraits));
        assert_eq!(view.master_count(), 0);

        // Grab the callback, then simulate a post-scan library change.
        let refresh = view.refresh_callback();
        conn.borrow()
            .execute_batch(
                "INSERT INTO tracks (path,title,artist,album,added_at) VALUES
                 ('/a.flac','A','Artist A','First',0),
                 ('/b.flac','B','Artist B','Second',0);",
            )
            .unwrap();

        refresh();
        assert_eq!(view.master_count(), 2);
    }
}
