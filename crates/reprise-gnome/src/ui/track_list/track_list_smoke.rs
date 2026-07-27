//! Headless dev/verification hooks for `ui::track_list`: the permanent
//! `REPRISE_SMOKE_*` env-var arming functions (`arm_smoke_activate`/`_filter`/
//! `_source`/`_sort_column`) plus the `REPRISE_SMOKE_SOURCE` value parser
//! (`parse_smoke_source`) and its by-name playlist fallback
//! (`resolve_smoke_source_playlist_by_name`). Split out of `track_list.rs`;
//! each arm function is `pub(in crate::ui)` so `TrackList::new` can arm it as
//! `track_list_smoke::…`.

use std::rc::Rc;

use gtk4::glib;
use gtk4::prelude::*;

use crate::ui::track_list::{set_filter_and_reload, set_source_and_reload, Shared};
use crate::ui::track_list_activation::activate_track;
use reprise_core::view_source::ViewSource;

/// Dev/verification hook (permanent, like `REPRISE_SCAN_DIR` and
/// `REPRISE_SMOKE_QUIT`): when set, the first row is activated
/// programmatically — through the exact same `on_activate` path a
/// double-click takes — once the initial load has run and the main loop is
/// idle. Combined with `REPRISE_SCAN_DIR` (populate), `REPRISE_AUDIO_SINK=
/// fakesink` (no audio device) and `REPRISE_SMOKE_QUIT` (exit), this enables
/// the full headless play-a-track E2E:
///
/// `REPRISE_SCAN_DIR=… REPRISE_SMOKE_ACTIVATE=1 REPRISE_AUDIO_SINK=fakesink
///  REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`
const SMOKE_ACTIVATE_ENV_VAR: &str = "REPRISE_SMOKE_ACTIVATE";

/// Dev/verification hook (permanent, like `REPRISE_SMOKE_ACTIVATE`): when
/// set to a non-empty string, that string is applied as the search filter —
/// through `set_filter_and_reload`, the exact same filter-apply step
/// `TrackList::set_filter` (the typed-search path) ends in, just invoked
/// directly instead of via `window.rs`'s 200ms keystroke-debounce timer,
/// since there's no keystroke to debounce here — once the initial load has
/// run and the main loop is idle. Combined with `REPRISE_SCAN_DIR`
/// (populate) and `REPRISE_SMOKE_QUIT` (exit), this drives the `NoResults`
/// empty state and the filtered "N of M tracks" status line headlessly:
///
/// `REPRISE_SCAN_DIR=… REPRISE_SMOKE_FILTER=nomatch REPRISE_SMOKE_QUIT=1
///  xvfb-run -a cargo run`
const SMOKE_FILTER_ENV_VAR: &str = "REPRISE_SMOKE_FILTER";

/// Dev/verification hook (permanent, like the other `REPRISE_SMOKE_*` hooks
/// above): when set, switches the track list to the named `ViewSource` once
/// the initial load has run and the main loop is idle, through the exact
/// same source-routing path as the sidebar. Track-list sources continue
/// through `TrackList::set_source`; `my_stats` is delegated to the window
/// router because it opens a separate content view. Accepted values:
/// `library`, `missing`, `queue`, `import_errors`, `my_stats`, or
/// `playlist:<id>`/`smart:<id>` (Task 4 wires the sidebar UI for the latter
/// two; the query layer and this hook already support them). Track-list
/// targets log `"view source set"` plus the resulting row count; `my_stats`
/// logs that the separate view opened through sidebar routing.
///
/// Usage: `REPRISE_SCAN_DIR=… REPRISE_SMOKE_SOURCE=missing REPRISE_SMOKE_QUIT=1
///  xvfb-run -a cargo run`, or `REPRISE_SMOKE_SOURCE=my_stats
///  REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.
pub(in crate::ui) const SMOKE_SOURCE_ENV_VAR: &str = "REPRISE_SMOKE_SOURCE";

/// Dev/verification hook (permanent, like the other `REPRISE_SMOKE_*` hooks
/// above; added for the Task 5 Fix Round 1 "remove from playlist targets the
/// wrong row" data-loss fix): when set to `"title"` or `"artist"`,
/// programmatically calls `GtkColumnView::sort_by_column` on that column —
/// the exact same call a real column-header click triggers (see the initial
/// `column_view.sort_by_column(Some(&artist_column), …)` call in `TrackList::
/// new`) — so a headless E2E run can put the track list into a sort other
/// than a playlist source's own forced `"playlist_order"` default, and then
/// exercise `REPRISE_SMOKE_MENU_ACTION=remove-from-playlist` (`ui::track_
/// list_context_menu`) against the resulting divergent view: this is the
/// only way to drive "remove from a *sorted* playlist view" headlessly,
/// since there is no supported way to synthesize a real pointer click on a
/// column header. Registered to run *after* `REPRISE_SMOKE_SOURCE` (see the
/// arming order in `TrackList::new`), so a `playlist:<name>` switch's own
/// forced default sort has already applied before this overrides it —
/// matching what a real user does (open a playlist, then click a header).
///
/// Usage: `REPRISE_SCAN_DIR=… REPRISE_SMOKE_SOURCE=playlist:P
///  REPRISE_SMOKE_SORT_COLUMN=title
///  REPRISE_SMOKE_MENU_ACTION=remove-from-playlist REPRISE_SMOKE_QUIT=1
///  xvfb-run -a cargo run`.
const SMOKE_SORT_COLUMN_ENV_VAR: &str = "REPRISE_SMOKE_SORT_COLUMN";

/// Arms the `REPRISE_SMOKE_ACTIVATE` hook (see `SMOKE_ACTIVATE_ENV_VAR`):
/// one idle callback, deferred so it runs once the main loop is up rather
/// than in the middle of window construction, that pushes the first row
/// through the same `on_activate` path as a real double-click.
pub(in crate::ui) fn arm_smoke_activate(shared: &Rc<Shared>) {
    if std::env::var(SMOKE_ACTIVATE_ENV_VAR).is_err() {
        return;
    }
    tracing::info!("{SMOKE_ACTIVATE_ENV_VAR} set: arming first-row activation");
    let shared = shared.clone();
    glib::idle_add_local_once(move || {
        let Some(track) = shared.model.track_at(0) else {
            tracing::warn!("{SMOKE_ACTIVATE_ENV_VAR}: track list is empty; nothing to activate");
            return;
        };
        tracing::info!(path = %track.path, "{SMOKE_ACTIVATE_ENV_VAR}: activating first row");
        activate_track(&shared, 0, &track);
    });
}

/// Arms the `REPRISE_SMOKE_FILTER` hook (see `SMOKE_FILTER_ENV_VAR`): one
/// idle callback, deferred so it runs once the main loop is up (matching
/// `arm_smoke_activate`), that applies the env var's value as the search
/// filter via `set_filter_and_reload`.
pub(in crate::ui) fn arm_smoke_filter(shared: &Rc<Shared>) {
    let Ok(text) = std::env::var(SMOKE_FILTER_ENV_VAR) else {
        return;
    };
    tracing::info!(filter = %text, "{SMOKE_FILTER_ENV_VAR} set: arming programmatic filter");
    let shared = shared.clone();
    glib::idle_add_local_once(move || {
        set_filter_and_reload(&shared, &text);
    });
}

/// Parses `REPRISE_SMOKE_SOURCE`'s value into a `ViewSource`. `None` for an
/// unrecognized value (caller logs and ignores) rather than silently
/// defaulting to `Library` — a typo in the env var should be visible, not
/// swallowed. Accepts `playlist:<id>`/`smart:<id>` too (Task 4's sidebar is
/// the eventual primary way to reach those, but the query layer and this
/// hook already support them).
pub(in crate::ui) fn parse_smoke_source(value: &str) -> Option<ViewSource> {
    match value {
        "library" => Some(ViewSource::Library),
        "missing" => Some(ViewSource::Missing),
        "queue" => Some(ViewSource::Queue),
        "import_errors" => Some(ViewSource::ImportErrors),
        "my_stats" => Some(ViewSource::MyStats),
        "concerts" => Some(ViewSource::Concerts),
        "releases" => Some(ViewSource::Releases),
        "podcasts" => Some(ViewSource::Podcasts),
        "radio" => Some(ViewSource::Radio),
        _ => value
            .strip_prefix("playlist:")
            .and_then(|id| id.parse::<i64>().ok())
            .map(ViewSource::Playlist)
            .or_else(|| {
                value
                    .strip_prefix("smart:")
                    .and_then(|id| id.parse::<i64>().ok())
                    .map(ViewSource::Smart)
            }),
    }
}

/// Fallback for `REPRISE_SMOKE_SOURCE=playlist:<name>` (Stage 3 Task 4):
/// playlist ids aren't stable across the scratch databases headless E2E runs
/// seed fresh each time, so once `parse_smoke_source` fails to parse the text
/// after `playlist:` as an id, this looks the playlist up by exact name via
/// `library::playlists::list` instead. Only tried for the `playlist:` prefix
/// — smart playlist ids ARE stable (the three seeds are created once, at
/// migration, never re-created by a test), so `smart:<id>` never needs a
/// name-based fallback. Returns `None` (caller warns and ignores) if the
/// prefix doesn't match, the lookup query fails, or no playlist has that
/// exact name. Names aren't required to be unique (`playlists::create`
/// doesn't enforce it) — if more than one playlist shares `name`, this logs
/// a warning and still picks the first one by `playlists::list`'s `ORDER BY
/// position ASC` (good enough for a headless-only smoke hook, but flagged so
/// duplicate names don't resolve silently and ambiguously).
fn resolve_smoke_source_playlist_by_name(shared: &Rc<Shared>, value: &str) -> Option<ViewSource> {
    let name = value.strip_prefix("playlist:")?;
    let conn = shared.conn.borrow();
    let playlists = reprise_core::library::playlists::list(&conn)
        .inspect_err(|error| {
            tracing::error!(%error, name, "failed to list playlists for smoke-source name lookup");
        })
        .ok()?;
    let mut matches = playlists.into_iter().filter(|p| p.name == name);
    let first = matches.next()?;
    let remaining = matches.count();
    if remaining > 0 {
        tracing::warn!(
            name,
            match_count = remaining + 1,
            "multiple playlists share this name; picking the first by position"
        );
    }
    Some(ViewSource::Playlist(first.id))
}

/// Arms the `REPRISE_SMOKE_SOURCE` hook (see `SMOKE_SOURCE_ENV_VAR`): one
/// idle callback, deferred so it runs once the main loop is up (matching
/// `arm_smoke_activate`/`arm_smoke_filter`), that switches the track list to
/// the parsed `ViewSource` via `set_source_and_reload` and logs the
/// resulting row count. Registered last in `TrackList::new` (after `arm_
/// smoke_activate`), so if both hooks are set together (e.g. verifying
/// `source=queue` after an activation), the queue is already populated by
/// the time this callback runs — GLib dispatches same-priority idle
/// callbacks in the order they were registered.
///
/// Values `parse_smoke_source` can't parse directly (today: only
/// `playlist:<name>`, since ids aren't stable across scratch DBs — see
/// `resolve_smoke_source_playlist_by_name`) fall back to a by-name playlist
/// lookup before giving up. `my_stats` is recognized here but left untouched
/// for the window router armed by `library_shell::wire_source_routing`.
pub(in crate::ui) fn arm_smoke_source(shared: &Rc<Shared>) {
    let Ok(text) = std::env::var(SMOKE_SOURCE_ENV_VAR) else {
        return;
    };
    let shared = shared.clone();
    glib::idle_add_local_once(move || {
        let source = parse_smoke_source(&text)
            .or_else(|| resolve_smoke_source_playlist_by_name(&shared, &text));
        let Some(source) = source else {
            tracing::warn!(
                value = %text,
                "{SMOKE_SOURCE_ENV_VAR} set to an unrecognized value; ignoring"
            );
            return;
        };
        if matches!(
            source,
            ViewSource::MyStats
                | ViewSource::Concerts
                | ViewSource::Releases
                | ViewSource::Podcasts
                | ViewSource::Radio
        ) {
            tracing::debug!(
                "{SMOKE_SOURCE_ENV_VAR} detail source delegated to the window source router"
            );
            return;
        }
        tracing::info!(value = %text, "{SMOKE_SOURCE_ENV_VAR} set: applying programmatic view-source switch");
        set_source_and_reload(&shared, &source);
        let label = shared.source.borrow().label();
        // Stage 3 Task 8: the ImportErrors source's rows live in `import_
        // errors_view`, not `shared.model` (which is always empty for this
        // source — see `reload`'s own branch) — mirror that here so this
        // log line reports the real row count instead of a stale 0.
        let rows = if matches!(*shared.source.borrow(), ViewSource::ImportErrors) {
            shared.import_errors_view.refresh() as u32
        } else {
            shared.model.n_items()
        };
        tracing::info!(source = %label, rows, "view source set to {label} ({rows} rows)");
    });
}

/// Arms the `REPRISE_SMOKE_SORT_COLUMN` hook (see `SMOKE_SORT_COLUMN_ENV_
/// VAR`): one idle callback that calls `GtkColumnView::sort_by_column` on
/// the matching column, exactly like a real column-header click. Registered
/// after `arm_smoke_source` (see the arming order in `TrackList::new`) so a
/// prior `REPRISE_SMOKE_SOURCE=playlist:<name>` switch's own forced default
/// sort has already landed before this overrides it. Only `"title"`/
/// `"artist"` are recognized today — the two columns `TrackList::new` already
/// keeps a handle to (`artist_column` for the initial-sort call above); an
/// unrecognized value is logged and ignored rather than silently doing
/// nothing.
pub(in crate::ui) fn arm_smoke_sort_column(
    column_view: &gtk4::ColumnView,
    title_column: &gtk4::ColumnViewColumn,
    artist_column: &gtk4::ColumnViewColumn,
) {
    let Ok(field) = std::env::var(SMOKE_SORT_COLUMN_ENV_VAR) else {
        return;
    };
    let column = match field.as_str() {
        "title" => title_column.clone(),
        "artist" => artist_column.clone(),
        _ => {
            tracing::warn!(
                field,
                "{SMOKE_SORT_COLUMN_ENV_VAR} set to an unrecognized column id; ignoring"
            );
            return;
        }
    };
    let column_view = column_view.clone();
    glib::idle_add_local_once(move || {
        tracing::info!(
            field,
            "{SMOKE_SORT_COLUMN_ENV_VAR} set: applying programmatic column sort"
        );
        super::track_list_sort::sort_by_column(&column_view, &column, gtk4::SortType::Ascending);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_views_are_supported_smoke_sources() {
        assert_eq!(parse_smoke_source("my_stats"), Some(ViewSource::MyStats));
        assert_eq!(parse_smoke_source("concerts"), Some(ViewSource::Concerts));
        assert_eq!(parse_smoke_source("releases"), Some(ViewSource::Releases));
        assert_eq!(parse_smoke_source("podcasts"), Some(ViewSource::Podcasts));
        assert_eq!(parse_smoke_source("radio"), Some(ViewSource::Radio));
    }
}
