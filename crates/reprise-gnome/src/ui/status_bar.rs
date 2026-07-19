//! The compact, right-aligned status information in the track content's
//! bottom bar: `"{n} tracks · {total duration}"`, e.g.
//! `"1,704 tracks · 4 days, 6 hours and 28 minutes"`. The status line always
//! describes the whole library; the filter row owns restriction state. It
//! therefore only appears while the Library source is shown — in every
//! other source the filter row is the one count on screen (FIL-2's
//! role split: row = current view, bottom bar = library).
//! Storage size (e.g.
//! "43.4 GB") is out of scope: the schema has no file-size column yet (a
//! later stage).
//!
//! ## Refresh triggers
//!
//! `refresh` re-runs `queries::query_library_stats_browsed` without any
//! restriction and updates the label. `window.rs` calls it after every
//! `TrackList` reload — via the `on_reload` hook threaded into
//! `TrackList::new` (covers initial load, search, sort-header clicks, and the reload
//! after a scan completes, all in one place) — so the count/duration never
//! go stale relative to what's on screen.
//!
//! ## Empty library
//!
//! When the library has zero tracks, the complete status surface is hidden
//! rather than showing "0 tracks · 0 minutes" — the
//! empty-library placeholder in the track list already communicates that
//! state; a zeroed bar would be redundant clutter, and the player bar is
//! `set_sensitive(false)`/blank in that state anyway (see `window.rs`).

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::{glib, prelude::*};
use rusqlite::Connection;

use crate::ui::strings;
use reprise_core::format::{format_thousands, format_total_duration};
use reprise_core::queries::{self, BrowseFilter};

/// Shared handle to the complete status surface. Cloning only increments the
/// Rust `Rc`; the GTK parent and child remain one ownership unit while both
/// `window.rs` and the `TrackList` reload callback retain a handle.
#[derive(Clone)]
pub struct StatusBar {
    inner: Rc<StatusBarInner>,
}

struct StatusBarInner {
    surface: gtk4::Box,
    // The Box is the label's sole strong owner. StatusBar clones share this
    // Rust owner instead of independently ref'ing both sides of a GTK
    // parent-child relation during startup and shutdown.
    label: glib::WeakRef<gtk4::Label>,
    visibility: VisibilityState,
}

struct VisibilityState {
    enabled: Cell<bool>,
    library_source: Cell<bool>,
    has_content: Cell<bool>,
}

impl StatusBar {
    pub fn new() -> Self {
        let surface = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        surface.add_css_class("reprise-list-status-bar");
        surface.set_visible(false);
        let label = gtk4::Label::new(None);
        label.set_halign(gtk4::Align::End);
        label.set_xalign(1.0);
        label.set_hexpand(true);
        label.set_margin_top(4);
        label.set_margin_bottom(4);
        label.set_margin_end(12);
        label.add_css_class("reprise-text-secondary");
        label.add_css_class("caption");
        surface.append(&label);
        let label_weak = label.downgrade();
        Self {
            inner: Rc::new(StatusBarInner {
                surface,
                label: label_weak,
                visibility: VisibilityState {
                    enabled: Cell::new(true),
                    library_source: Cell::new(false),
                    has_content: Cell::new(false),
                },
            }),
        }
    }

    /// The complete bottom status surface placed below the track list.
    pub fn widget(&self) -> &gtk4::Box {
        &self.inner.surface
    }

    #[cfg(test)]
    pub(in crate::ui) fn label(&self) -> gtk4::Label {
        self.live_label()
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.inner.visibility.enabled.set(enabled);
        self.sync_visibility();
    }

    /// Re-queries `queries::query_library_stats` and updates the status text
    /// (or hides the surface, for an empty library — see the module comment).
    /// Query failures are logged and treated the same as an empty library:
    /// hide rather than show stale or partial text.
    pub fn refresh(&self, conn: &Rc<RefCell<Connection>>) {
        let stats = {
            let conn = conn.borrow();
            queries::query_library_stats_browsed(&conn, "", &BrowseFilter::default())
        };
        self.inner.visibility.library_source.set(true);
        let label = self.live_label();
        match stats {
            Ok(stats) if stats.track_count > 0 => {
                let text = format_status_text(stats.track_count, stats.total_duration_ms);
                tracing::debug!(text = %text, "status line updated");
                label.set_text(&text);
                self.inner.visibility.has_content.set(true);
            }
            Ok(_) => {
                label.set_text("");
                self.inner.visibility.has_content.set(false);
            }
            Err(error) => {
                tracing::error!(%error, "failed to load library stats for status line");
                label.set_text("");
                self.inner.visibility.has_content.set(false);
            }
        }
        self.sync_visibility();
    }

    /// Hides the status line. Called for every non-`Library` source: since
    /// the filter row became the permanent per-source count (FIL-2), a
    /// second "{n} tracks" overlay there was pure duplication — the library
    /// stats this widget exists for have no meaning outside the Library.
    pub fn hide(&self) {
        self.inner.visibility.library_source.set(false);
        self.sync_visibility();
    }

    fn sync_visibility(&self) {
        self.inner.surface.set_visible(status_visibility(
            self.inner.visibility.enabled.get(),
            self.inner.visibility.library_source.get(),
            self.inner.visibility.has_content.get(),
        ));
    }

    fn live_label(&self) -> gtk4::Label {
        self.inner
            .label
            .upgrade()
            .expect("status surface owns its label")
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Pure text-formatting core of `refresh`, split out so the exact copy
/// (pluralization and separator placement) is
/// unit-testable without a live GTK widget — same pattern as `track_list::
/// empty_state_for`. `track_count`/`total_duration_ms` always describe the
/// whole library.
fn format_status_text(track_count: i64, total_duration_ms: i64) -> String {
    let track_word = if track_count == 1 {
        &strings::text(strings::STATUS_TRACK_SINGULAR)
    } else {
        &strings::text(strings::STATUS_TRACK_PLURAL)
    };
    let count_text = format_thousands(track_count);
    format!(
        "{count_text} {track_word}{}{}",
        strings::text(strings::STATUS_SEPARATOR),
        format_total_duration(total_duration_ms)
    )
}

fn status_visibility(enabled: bool, library_source: bool, has_content: bool) -> bool {
    enabled && library_source && has_content
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    #[test]
    fn status_visibility_requires_enabled_library_content() {
        assert!(status_visibility(true, true, true));
        assert!(!status_visibility(false, true, true));
        assert!(!status_visibility(true, false, true));
        assert!(!status_visibility(true, true, false));
    }

    // UX FIL-2: the status overlay always describes the whole library — the
    // "X of Y" variant is gone; the filter row owns restriction state.
    #[test]
    fn fil_2_status_line_copy_is_always_neutral() {
        let text = format_status_text(1_704, 4 * 24 * 3_600_000 + 6 * 3_600_000);
        assert!(text.starts_with("1,704 tracks"));
        assert!(!text.contains(" of "));
    }

    #[test]
    fn formats_plural_track_count_with_separator() {
        let text = format_status_text(1_704, ((4 * 24 + 6) * 60 + 28) * 60 * 1000);
        assert_eq!(text, "1,704 tracks · 4 days, 6 hours and 28 minutes");
    }

    #[test]
    fn formats_singular_track_count() {
        let text = format_status_text(1, 90 * 60 * 1000);
        assert_eq!(text, "1 track · 1 hour and 30 minutes");
    }

    #[test]
    #[ignore = "requires a display; run via xvfb-run"]
    fn status_bar_refresh_clone_and_teardown_emit_no_criticals() {
        gtk4::init().unwrap();
        let criticals = Arc::new(Mutex::new(Vec::new()));
        let handlers = ["Gtk", "GLib-GObject"].map(|domain| {
            let criticals = criticals.clone();
            glib::log_set_handler(
                Some(domain),
                glib::LogLevels::LEVEL_CRITICAL,
                false,
                false,
                move |domain, _, message| {
                    criticals
                        .lock()
                        .unwrap()
                        .push(format!("{}: {message}", domain.unwrap_or("unknown")));
                },
            )
        });

        {
            let conn = Rc::new(RefCell::new(reprise_core::db::open_migrated(None).unwrap()));
            conn.borrow()
                .execute(
                    "INSERT INTO tracks (path, title, artist, duration_ms, added_at) \
                     VALUES ('/tmp/status-lifecycle.ogg', 'Status', 'Artist', 90000, 0)",
                    [],
                )
                .unwrap();
            let status = StatusBar::new();
            let callback_copy = status.clone();
            let tracks = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            let content = crate::ui::track_content::build(&tracks, status.widget());
            let window = gtk4::Window::builder()
                .default_width(600)
                .default_height(400)
                .child(&content)
                .build();
            window.present();
            for _ in 0..5 {
                callback_copy.refresh(&conn);
            }
            while glib::MainContext::default().iteration(false) {}
            window.close();
            drop(window);
            drop(content);
            drop(callback_copy);
            drop(status);
            while glib::MainContext::default().iteration(false) {}
        }

        for (domain, handler) in ["Gtk", "GLib-GObject"].into_iter().zip(handlers) {
            glib::log_remove_handler(Some(domain), handler);
        }
        let criticals = criticals.lock().unwrap();
        assert!(criticals.is_empty(), "GTK criticals: {criticals:?}");
    }
}
