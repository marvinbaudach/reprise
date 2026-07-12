//! The slim, right-aligned status line shown above the player bar (design
//! mockup 7a): `"{n} tracks · {total duration}"` with no filter active, e.g.
//! `"1,704 tracks · 4 days, 6 hours and 28 minutes"`; `"{filtered} of {n}
//! tracks · {total duration}"` while a search filter is active, e.g. "42 of
//! 1,704 tracks · 4 days, 6 hours and 28 minutes" — the duration always
//! describes the *whole* library, never just the filtered rows (see
//! `queries::query_library_stats`'s doc comment). Storage size (e.g.
//! "43.4 GB") is out of scope: the schema has no file-size column yet (a
//! later stage).
//!
//! ## Refresh triggers
//!
//! `refresh` re-runs `queries::query_library_stats` (passing the current
//! search filter) and updates the label. `window.rs` calls it after every
//! `TrackList` reload — via the `on_reload` hook threaded into
//! `TrackList::new`, which now also carries the filter string that was just
//! applied (covers initial load, search, sort-header clicks, and the reload
//! after a scan completes, all in one place) — so the count/duration never
//! go stale relative to what's on screen.
//!
//! ## Empty library
//!
//! When the library has zero tracks, the label is hidden outright (`set_
//! visible(false)`) rather than showing "0 tracks · 0 minutes" — the
//! empty-library placeholder in the track list already communicates that
//! state; a zeroed status line above the player bar would be redundant
//! clutter, and the player bar is `set_sensitive(false)`/blank in that state
//! anyway (see `window.rs`).

use std::cell::RefCell;
use std::rc::Rc;

use gtk4::prelude::*;
use rusqlite::Connection;

use crate::ui::strings;
use reprise_core::format::{format_thousands, format_total_duration};
use reprise_core::queries;

/// Handle to the status label. Cheap to clone (clones the underlying
/// `gtk::Label`, a reference-counted GObject handle, not its contents) so
/// both `window.rs`'s scan-completion path and the `TrackList` `on_reload`
/// callback can each hold their own copy.
#[derive(Clone)]
pub struct StatusBar {
    label: gtk4::Label,
}

impl StatusBar {
    pub fn new() -> Self {
        let label = gtk4::Label::new(None);
        label.set_halign(gtk4::Align::End);
        label.set_margin_top(4);
        label.set_margin_bottom(4);
        label.set_margin_end(12);
        label.add_css_class("dim-label");
        label.add_css_class("caption");
        // Nothing to summarize until the first `refresh()` call resolves —
        // avoids a flash of "0 tracks · 0 minutes" before the initial track
        // list load completes.
        label.set_visible(false);
        Self { label }
    }

    /// The label widget to embed above the player bar in `window.rs`.
    pub fn widget(&self) -> &gtk4::Label {
        &self.label
    }

    /// Re-queries `queries::query_library_stats` and updates the label text
    /// (or hides it, for an empty library — see the module doc comment).
    /// Query failures are logged and treated the same as an empty library:
    /// hide rather than show stale or partial text.
    pub fn refresh(&self, conn: &Rc<RefCell<Connection>>, filter: &str) {
        let stats = {
            let conn = conn.borrow();
            queries::query_library_stats(&conn, filter)
        };
        match stats {
            Ok(stats) if stats.track_count > 0 => {
                let text = format_status_text(
                    stats.track_count,
                    stats.total_duration_ms,
                    stats.filtered_count,
                );
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

    /// Shows a simple "{n} tracks" line for a non-`Library` `ViewSource`
    /// (Stage 3 Task 3): no total-duration/"N of M" library context, since
    /// those describe the *whole* library (see `query_library_stats`'s doc
    /// comment), not e.g. one playlist or the missing-files view — full
    /// per-source stats (duration, etc.) are left to a later stage; this is
    /// the "simplest coherent behavior" the task calls for. `count <= 0`
    /// hides the label, matching `refresh`'s empty-library behavior: the
    /// empty-state placeholder already communicates "nothing here" for that
    /// case, so a zeroed status line would be redundant.
    pub fn refresh_for_source_count(&self, count: i64) {
        if count <= 0 {
            self.label.set_visible(false);
            return;
        }
        let text = format_source_status_text(count);
        tracing::debug!(text = %text, "status line updated (source count)");
        self.label.set_text(&text);
        self.label.set_visible(true);
    }
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Pure text-formatting core of `refresh`, split out so the exact copy
/// (pluralization, separator placement, filtered-count prefix) is
/// unit-testable without a live GTK widget — same pattern as `track_list::
/// empty_state_for`. `track_count`/`total_duration_ms` always describe the
/// whole library; `filtered_count` — `queries::LibraryStats::filtered_count`,
/// passed straight through — controls only whether an "N of " prefix is
/// shown ahead of the (always library-wide) track count, never which count
/// decides singular/plural wording (that stays keyed off `track_count`, so
/// "42 of 1,704 tracks" and "1,704 tracks" pluralize identically).
fn format_status_text(
    track_count: i64,
    total_duration_ms: i64,
    filtered_count: Option<i64>,
) -> String {
    let track_word = if track_count == 1 {
        strings::STATUS_TRACK_SINGULAR
    } else {
        strings::STATUS_TRACK_PLURAL
    };
    let count_text = match filtered_count {
        Some(filtered) => strings::status_filtered_of_total(
            &format_thousands(filtered),
            &format_thousands(track_count),
        ),
        None => format_thousands(track_count),
    };
    format!(
        "{count_text} {track_word}{}{}",
        strings::STATUS_SEPARATOR,
        format_total_duration(total_duration_ms)
    )
}

/// Pure text-formatting core of `refresh_for_source_count` — same
/// unit-testable-without-a-widget pattern as `format_status_text`, just
/// without the duration/"N of M" pieces that only make sense library-wide.
fn format_source_status_text(count: i64) -> String {
    let track_word = if count == 1 {
        strings::STATUS_TRACK_SINGULAR
    } else {
        strings::STATUS_TRACK_PLURAL
    };
    format!("{} {track_word}", format_thousands(count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_plural_track_count_with_separator() {
        let text = format_status_text(1_704, ((4 * 24 + 6) * 60 + 28) * 60 * 1000, None);
        assert_eq!(text, "1,704 tracks · 4 days, 6 hours and 28 minutes");
    }

    #[test]
    fn formats_singular_track_count() {
        let text = format_status_text(1, 90 * 60 * 1000, None);
        assert_eq!(text, "1 track · 1 hour and 30 minutes");
    }

    #[test]
    fn formats_filtered_count_as_n_of_m() {
        let text = format_status_text(1_704, ((4 * 24 + 6) * 60 + 28) * 60 * 1000, Some(42));
        assert_eq!(text, "42 of 1,704 tracks · 4 days, 6 hours and 28 minutes");
    }

    #[test]
    fn formats_zero_match_filter() {
        let text = format_status_text(1_704, ((4 * 24 + 6) * 60 + 28) * 60 * 1000, Some(0));
        assert_eq!(text, "0 of 1,704 tracks · 4 days, 6 hours and 28 minutes");
    }

    /// Stage 3 Task 1 backlog item (c): both halves of the "N of M" prefix
    /// must be comma-formatted once they cross 1,000 — the existing filtered-
    /// count tests above only exercise `filtered_count < 1000` (0, 42),
    /// leaving `format_thousands(filtered)`'s thousands-separator path on the
    /// *filtered* number untested.
    #[test]
    fn formats_filtered_count_over_a_thousand_with_comma() {
        let text = format_status_text(5_678, ((4 * 24 + 6) * 60 + 28) * 60 * 1000, Some(1_234));
        assert_eq!(
            text,
            "1,234 of 5,678 tracks · 4 days, 6 hours and 28 minutes"
        );
    }

    /// Stage 3 Task 3: the non-Library "{n} tracks" status line has none of
    /// the duration/"N of M" pieces `format_status_text` produces.
    #[test]
    fn source_status_text_formats_plural_and_singular() {
        assert_eq!(format_source_status_text(42), "42 tracks");
        assert_eq!(format_source_status_text(1), "1 track");
    }

    #[test]
    fn source_status_text_comma_formats_over_a_thousand() {
        assert_eq!(format_source_status_text(1_234), "1,234 tracks");
    }
}
