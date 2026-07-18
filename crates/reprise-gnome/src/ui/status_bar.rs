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
//! When the library has zero tracks, the label is hidden outright (`set_
//! visible(false)`) rather than showing "0 tracks · 0 minutes" — the
//! empty-library placeholder in the track list already communicates that
//! state; a zeroed bar would be redundant clutter, and the player bar is
//! `set_sensitive(false)`/blank in that state anyway (see `window.rs`).

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use rusqlite::Connection;

use crate::ui::strings;
use reprise_core::format::{format_thousands, format_total_duration};
use reprise_core::queries::{self, BrowseFilter};

/// Handle to the status label. Cheap to clone (clones the underlying
/// `gtk::Label`, a reference-counted GObject handle, not its contents) so
/// both `window.rs`'s scan-completion path and the `TrackList` `on_reload`
/// callback can each hold their own copy.
#[derive(Clone)]
pub struct StatusBar {
    label: gtk4::Label,
    enabled: Rc<std::cell::Cell<bool>>,
}

impl StatusBar {
    pub fn new() -> Self {
        let label = gtk4::Label::new(None);
        label.set_halign(gtk4::Align::End);
        label.set_xalign(1.0);
        label.set_margin_top(4);
        label.set_margin_bottom(4);
        label.set_margin_end(12);
        label.add_css_class("dim-label");
        label.add_css_class("caption");
        // Nothing to summarize until the first `refresh()` call resolves —
        // avoids a flash of "0 tracks · 0 minutes" before the initial track
        // list load completes.
        label.set_visible(false);
        Self {
            label,
            enabled: Rc::new(std::cell::Cell::new(true)),
        }
    }

    /// The label widget placed in the bottom status bar by `window.rs`.
    pub fn widget(&self) -> &gtk4::Label {
        &self.label
    }

    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.set(enabled);
        if !enabled {
            self.label.set_visible(false);
        }
    }

    /// Re-queries `queries::query_library_stats` and updates the label text
    /// (or hides it, for an empty library — see the module doc comment).
    /// Query failures are logged and treated the same as an empty library:
    /// hide rather than show stale or partial text.
    pub fn refresh(&self, conn: &Rc<RefCell<Connection>>) {
        if !self.enabled.get() {
            return;
        }
        let stats = {
            let conn = conn.borrow();
            queries::query_library_stats_browsed(&conn, "", &BrowseFilter::default())
        };
        match stats {
            Ok(stats) if stats.track_count > 0 => {
                let text = format_status_text(stats.track_count, stats.total_duration_ms);
                tracing::debug!(text = %text, "status line updated");
                self.label.set_text(&text);
                self.label.set_visible(true);
            }
            Ok(_) => {
                self.label.set_visible(false);
            }
            Err(error) => {
                tracing::error!(%error, "failed to load library stats for status line");
                self.label.set_visible(false);
            }
        }
    }

    /// Hides the status line. Called for every non-`Library` source: since
    /// the filter row became the permanent per-source count (FIL-2), a
    /// second "{n} tracks" overlay there was pure duplication — the library
    /// stats this widget exists for have no meaning outside the Library.
    pub fn hide(&self) {
        self.label.set_visible(false);
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
