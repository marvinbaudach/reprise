//! The slim, right-aligned status line shown above the player bar (design
//! mockup 7a): `"{n} tracks · {total duration}"`, e.g.
//! `"1,704 tracks · 4 days, 6 hours and 28 minutes"`.
//!
//! Always summarizes the *whole* library, never the active search filter —
//! `queries::LibraryStats::filtered_count` is reserved for a future
//! "N of M tracks" treatment and stays `None` for now, so filtering doesn't
//! change what this line shows. Storage size (e.g. "43.4 GB") is out of
//! scope: the schema has no file-size column yet (a later stage).
//!
//! ## Refresh triggers
//!
//! `refresh` re-runs `queries::query_library_stats` and updates the label.
//! `window.rs` calls it after every `TrackList` reload (via the `on_reload`
//! hook threaded into `TrackList::new` — covers initial load, search, and
//! sort-header clicks in one place) and again after a scan completes, so the
//! count/duration never go stale relative to what's on screen.
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

use crate::format::{format_thousands, format_total_duration};
use crate::queries;
use crate::ui::strings;

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
    pub fn refresh(&self, conn: &Rc<RefCell<Connection>>) {
        let stats = {
            let conn = conn.borrow();
            queries::query_library_stats(&conn)
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
}

impl Default for StatusBar {
    fn default() -> Self {
        Self::new()
    }
}

/// Pure text-formatting core of `refresh`, split out so the exact copy
/// (pluralization, separator placement) is unit-testable without a live GTK
/// widget — same pattern as `track_list::empty_state_for`.
fn format_status_text(track_count: i64, total_duration_ms: i64) -> String {
    let track_word = if track_count == 1 {
        strings::STATUS_TRACK_SINGULAR
    } else {
        strings::STATUS_TRACK_PLURAL
    };
    format!(
        "{} {track_word}{}{}",
        format_thousands(track_count),
        strings::STATUS_SEPARATOR,
        format_total_duration(total_duration_ms)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

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
