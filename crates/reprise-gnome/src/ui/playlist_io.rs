//! M3U playlist import/export flow (Stage 3 Task 7): the glue between
//! `library::m3u`'s pure parse/serialize functions and the rest of the app —
//! reading/writing the actual file, resolving a parsed path line against the
//! library (exact match, with a best-effort canonicalize fallback), and
//! driving the two `gtk::FileDialog` flows (a global "Import playlist…"
//! headerbar button, wired here and called from `window.rs`; a per-playlist
//! "Export playlist…" sidebar context-menu action, wired in the sibling
//! `ui::sidebar_export` module and calling back into this one).
//!
//! ## Why the core functions take `&Rc<RefCell<Connection>>`, not `&Connection`
//!
//! [`import_playlist`]/[`export_playlist`] are the *same* functions both a
//! real dialog callback and the `REPRISE_SMOKE_M3U` dev hook call — see
//! [`arm_smoke_m3u`]'s doc comment. Taking the UI layer's shared connection
//! handle directly (rather than a borrowed `&Connection` the caller would
//! have to extract first) keeps both call sites identical, one line shorter,
//! and matches this project's existing seam for such dual-path functions
//! (e.g. `ui::sidebar_dnd::handle_playlist_drop`, `ui::track_list_context_
//! menu`'s `handle_*` functions called from both real actions and their
//! `REPRISE_SMOKE_MENU_ACTION` hook).
//!
//! ## Path resolution (import)
//!
//! `library::m3u::parse_m3u` returns each path line completely unresolved
//! (see that module's doc comment). [`resolve_and_match_path`] does the
//! resolution this task's brief calls for: an absolute path is used as-is;
//! a relative one is joined against the `.m3u` file's own parent directory
//! (never the current working directory, which would be meaningless for a
//! GUI app). The joined path is tried against `queries::track_id_for_path`
//! first *without* canonicalizing — matching the common case where the
//! scanner recorded exactly this same (already-absolute, not necessarily
//! symlink-resolved) string. Only if that exact match fails is `Path::
//! canonicalize` attempted as a best-effort fallback (requires the file to
//! actually exist on disk) and retried — this catches a relative path or a
//! path through a symlink that the scanner itself recorded in canonicalized
//! form. Either way, a path that still doesn't match anything is simply
//! left out of the new playlist — not an error, just one fewer "matched" in
//! the "N of M tracks matched" toast (see [`ImportOutcome`]).
//!
//! ## Encoding (import)
//!
//! The `.m3u` file is read as raw bytes and decoded as UTF-8; on invalid
//! UTF-8, [`import_playlist`] lossy-decodes instead (replacing invalid
//! sequences with U+FFFD) and logs a `warn!` — never fails the whole import
//! over one file's encoding, per the task's fault-tolerance requirement. In
//! practice this only ever affects path lines containing non-UTF-8 bytes
//! (rare, since most filesystems normalize to UTF-8 already); such a path
//! simply fails to match afterward like any other unmatched path.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use rusqlite::Connection;

use reprise_core::library::m3u::{self, M3uExportEntry};
use reprise_core::library::playlists;
use reprise_core::queries;

use super::playlist_import_navigation;
use super::playlist_io_names::{display_name, playlist_name_from_file};
use super::sidebar::Sidebar;
use super::strings;
use super::toasts;

/// Env var read by [`arm_smoke_m3u`] — see that function's doc comment for
/// the two accepted value forms.
const SMOKE_M3U_ENV_VAR: &str = "REPRISE_SMOKE_M3U";

/// Result of a successful [`import_playlist`] call.
#[derive(Debug, Clone)]
pub struct ImportOutcome {
    /// `None` when zero path lines matched a library track — no playlist is
    /// created in that case (see [`import_playlist`]'s doc comment), so
    /// there's nothing to switch the track list to.
    pub playlist_id: Option<i64>,
    pub name: String,
    /// How many of the file's path lines resolved to a track already in the
    /// library.
    pub matched: usize,
    /// Total path lines found by `library::m3u::parse_m3u` (comments/blank
    /// lines already excluded).
    pub total: usize,
}

/// Everything that can fail while importing a playlist — reading the file or
/// writing the result to the database. Never returned for an unmatched path
/// line (see the module doc's `## Path resolution` section) — that's a
/// counted outcome, not an error.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("could not read playlist file: {0}")]
    Io(#[from] std::io::Error),
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

/// Everything that can fail while exporting a playlist.
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("could not write playlist file: {0}")]
    Io(#[from] std::io::Error),
}

/// Reads and parses `file_path` as an M3U playlist, matches every path line
/// against the library (see the module doc's `## Path resolution` section),
/// and — if at least one path matched — creates a new playlist named after
/// the file's stem containing every match, in file order, via
/// `library::playlists::create_with_tracks` (playlist creation and track
/// insertion happen atomically: a failure partway rolls back both, never
/// leaving an orphaned empty playlist row). If *zero* path lines matched, no
/// playlist is created at all — an all-bogus or empty `.m3u` file shouldn't
/// leave a permanent, unremovable-by-undo empty playlist behind;
/// [`ImportOutcome::playlist_id`] is `None` in that case and the caller shows
/// a "0 of N matched" toast without switching the track list anywhere.
pub fn import_playlist(
    conn: &Rc<RefCell<Connection>>,
    file_path: &Path,
) -> Result<ImportOutcome, ImportError> {
    let bytes = std::fs::read(file_path)?;
    let content = String::from_utf8(bytes).unwrap_or_else(|error| {
        tracing::warn!(
            path = %file_path.display(),
            "playlist file is not valid UTF-8; lossy-decoding instead"
        );
        String::from_utf8_lossy(&error.into_bytes()).into_owned()
    });

    let entries = m3u::parse_m3u(&content);
    let total = entries.len();
    let base_dir = file_path.parent().unwrap_or_else(|| Path::new("."));

    let matched_ids: Vec<i64> = {
        let conn_ref = conn.borrow();
        entries
            .iter()
            .filter_map(|entry| resolve_and_match_path(&conn_ref, &entry.path, base_dir))
            .collect()
    };
    let matched = matched_ids.len();

    let name = playlist_name_from_file(file_path);
    let playlist_id = if matched_ids.is_empty() {
        None
    } else {
        let mut conn_ref = conn.borrow_mut();
        Some(playlists::create_with_tracks(
            &mut conn_ref,
            &name,
            &matched_ids,
        )?)
    };

    Ok(ImportOutcome {
        playlist_id,
        name,
        matched,
        total,
    })
}

/// Resolves one parsed M3U path line to a track id — see the module doc's
/// `## Path resolution` section for the exact-match-then-canonicalize
/// strategy. `None` for a path that doesn't match anything either way (a
/// query failure is logged and also treated as "no match" — this function's
/// contract is "matched or not", not "matched, not-found, or errored").
fn resolve_and_match_path(conn: &Connection, entry_path: &str, base_dir: &Path) -> Option<i64> {
    let raw = Path::new(entry_path);
    let resolved: PathBuf = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        base_dir.join(raw)
    };
    let resolved_str = resolved.to_string_lossy().to_string();

    match queries::track_id_for_path(conn, &resolved_str) {
        Ok(Some(id)) => return Some(id),
        Ok(None) => {}
        Err(error) => {
            tracing::error!(%error, path = %resolved_str, "track lookup by path failed during import");
            return None;
        }
    }

    let canonical = resolved.canonicalize().ok()?;
    let canonical_str = canonical.to_string_lossy().to_string();
    if canonical_str == resolved_str {
        // Canonicalizing changed nothing — the exact match above already
        // covered this case, no point re-querying with the same string.
        return None;
    }
    match queries::track_id_for_path(conn, &canonical_str) {
        Ok(id) => id,
        Err(error) => {
            tracing::error!(%error, path = %canonical_str, "track lookup by canonicalized path failed during import");
            None
        }
    }
}

/// Loads playlist `playlist_id`'s tracks (in playlist order) and writes them
/// to `file_path` as M3U8 — absolute paths, `#EXTINF:<duration_secs>,
/// <display>` per track (`display` is `"Artist - Title"`, or just `Title`
/// when there's no artist — see [`display_name`]). Returns the number of
/// tracks written (0 for an empty playlist, not an error).
pub fn export_playlist(
    conn: &Rc<RefCell<Connection>>,
    playlist_id: i64,
    file_path: &Path,
) -> Result<usize, ExportError> {
    let tracks = {
        let conn_ref = conn.borrow();
        queries::query_playlist_tracks_full(&conn_ref, playlist_id)?
    };

    let entries: Vec<M3uExportEntry> = tracks
        .iter()
        .map(|track| M3uExportEntry {
            path: track.path.clone(),
            duration_secs: track.duration_ms / 1000,
            display: display_name(track),
        })
        .collect();
    let count = entries.len();

    let content = m3u::serialize_m3u(&entries);
    std::fs::write(file_path, content)?;

    Ok(count)
}

/// Builds the `.m3u`/`.m3u8` `gtk::FileFilter` shared by both the import
/// (open) and export (save) dialogs — `pub(super)` so `ui::sidebar_export`
/// can reuse it for the save dialog rather than duplicating the suffix list.
pub(super) fn m3u_file_filter() -> gtk4::FileFilter {
    let filter = gtk4::FileFilter::new();
    filter.set_name(Some(&strings::text(strings::M3U_FILE_FILTER_NAME)));
    filter.add_suffix("m3u");
    filter.add_suffix("m3u8");
    filter
}

/// Wires the headerbar's "Import playlist…" button: a click opens a
/// portal-friendly `gtk::FileDialog` file picker filtered to `.m3u`/`.m3u8`,
/// then runs [`import_playlist`] on the chosen file and applies the result
/// via [`apply_import_result`] — the exact same function `arm_smoke_m3u`'s
/// `import:<path>` form calls, so this callback is a thin dialog wrapper,
/// not a second implementation. Dismissing the dialog is a normal, expected
/// outcome (not an error) — logged at debug and otherwise ignored, matching
/// `window.rs`'s `wire_scan_button`.
pub fn wire_import_button(
    import_button: &gtk4::Button,
    window: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    conn: Rc<RefCell<Connection>>,
    sidebar: Rc<Sidebar>,
) {
    let window = window.clone();
    let toast_overlay = toast_overlay.clone();
    let import_button_handle = import_button.clone();

    import_button.connect_clicked(move |_| {
        // Disable synchronously, before the async dialog even opens — same
        // double-click guard `wire_scan_button` uses.
        import_button_handle.set_sensitive(false);

        let filter = m3u_file_filter();
        let filters = gio::ListStore::new::<gtk4::FileFilter>();
        filters.append(&filter);
        let dialog = gtk4::FileDialog::builder()
            .title(strings::text(strings::IMPORT_PLAYLIST_DIALOG_TITLE))
            .modal(true)
            .filters(&filters)
            .default_filter(&filter)
            .build();

        let window = window.clone();
        let toast_overlay = toast_overlay.clone();
        let conn = conn.clone();
        let sidebar = sidebar.clone();
        let import_button = import_button_handle.clone();

        glib::spawn_future_local(async move {
            let file = match dialog.open_future(Some(&window)).await {
                Ok(file) => file,
                Err(error) => {
                    if error.matches(gtk4::DialogError::Dismissed)
                        || error.matches(gtk4::DialogError::Cancelled)
                    {
                        tracing::debug!("import playlist dialog dismissed");
                    } else {
                        tracing::error!(%error, "import playlist dialog failed");
                    }
                    import_button.set_sensitive(true);
                    return;
                }
            };
            import_button.set_sensitive(true);
            let Some(path) = file.path() else {
                tracing::warn!(
                    "selected playlist file has no local filesystem path; cannot import"
                );
                return;
            };

            apply_import_result(import_playlist(&conn, &path), &toast_overlay, &sidebar);
        });
    });
}

/// Applies an [`import_playlist`] result to the UI. On success with at least
/// one matched track, rebuilds and selects the new sidebar row. The normal
/// sidebar selection callback then switches the track list, title, and
/// adaptive navigation as one synchronized operation before the "Imported N
/// of M" toast. On success with zero matched tracks, no playlist was created
/// (see [`import_playlist`]'s doc comment) — the sidebar/track-list/title
/// are left untouched and a "0 of N matched" toast is shown instead. On
/// failure, logs and shows a generic failure toast. Shared by the real
/// dialog callback ([`wire_import_button`]) and the `REPRISE_SMOKE_M3U=
/// import:<path>` hook ([`arm_smoke_m3u`]).
fn apply_import_result(
    result: Result<ImportOutcome, ImportError>,
    toast_overlay: &adw::ToastOverlay,
    sidebar: &Rc<Sidebar>,
) {
    match result {
        Ok(outcome) => {
            tracing::info!(
                playlist_id = outcome.playlist_id,
                name = outcome.name,
                matched = outcome.matched,
                total = outcome.total,
                "playlist imported: {} of {} tracks matched",
                outcome.matched,
                outcome.total
            );
            let Some(playlist_id) = outcome.playlist_id else {
                toasts::show(
                    toast_overlay,
                    &strings::playlist_import_zero_matched_toast(&outcome.name, outcome.total),
                );
                return;
            };
            sidebar.refresh_and_select(
                playlist_import_navigation::target_for_import(playlist_id),
                "playlist imported",
            );
            toasts::show(
                toast_overlay,
                &strings::playlist_imported_toast(&outcome.name, outcome.matched, outcome.total),
            );
        }
        Err(error) => {
            tracing::error!(%error, "playlist import failed");
            toasts::show(toast_overlay, &strings::playlist_import_failed_toast());
        }
    }
}

/// Dev/verification hook (permanent, like the project's other `REPRISE_
/// SMOKE_*` hooks): a portal `gtk::FileDialog` can't be driven headlessly
/// (open *or* save), so this lets headless E2E runs exercise the exact same
/// [`import_playlist`]/[`export_playlist`] functions a real dialog callback
/// calls, without a pointer. Two accepted forms:
///
/// - `import:<path>`: reads `<path>` as an M3U file and imports it — calls
///   [`apply_import_result`], the same function [`wire_import_button`]'s
///   dialog callback calls, so the sidebar/track-list/toast side effects are
///   identical to a real import.
/// - `export:<playlist_name>:<path>`: looks up the playlist by exact name
///   (playlist ids aren't stable across the scratch databases headless E2E
///   runs seed fresh each time — same reasoning as `ui::track_list_context_
///   menu`'s `resolve_smoke_menu_action_playlist`) and writes it to
///   `<path>` via [`export_playlist`], then shows the same toast a real
///   export would.
///
/// Usage: `REPRISE_SCAN_DIR=… REPRISE_SMOKE_M3U=import:/tmp/x.m3u
///  REPRISE_SMOKE_QUIT=1 xvfb-run -a cargo run`.
pub fn arm_smoke_m3u(
    conn: Rc<RefCell<Connection>>,
    toast_overlay: &adw::ToastOverlay,
    sidebar: Rc<Sidebar>,
) {
    let Ok(value) = std::env::var(SMOKE_M3U_ENV_VAR) else {
        return;
    };
    tracing::info!(value = %value, "{SMOKE_M3U_ENV_VAR} set: arming headless m3u import/export hook");
    let toast_overlay = toast_overlay.clone();
    glib::idle_add_local_once(move || {
        if let Some(path) = value.strip_prefix("import:") {
            apply_import_result(
                import_playlist(&conn, Path::new(path)),
                &toast_overlay,
                &sidebar,
            );
            return;
        }

        let Some(rest) = value.strip_prefix("export:") else {
            tracing::warn!(
                value = %value,
                "{SMOKE_M3U_ENV_VAR}: unrecognized value; expected import:<path> or \
                 export:<name>:<path>"
            );
            return;
        };
        let Some((playlist_name, path)) = rest.split_once(':') else {
            tracing::warn!(
                value = %value,
                "{SMOKE_M3U_ENV_VAR}: export form must be export:<name>:<path>"
            );
            return;
        };
        let Some(playlist_id) = find_playlist_id_by_name(&conn, playlist_name) else {
            tracing::warn!(
                playlist_name,
                "{SMOKE_M3U_ENV_VAR}: no playlist found with this name"
            );
            return;
        };
        match export_playlist(&conn, playlist_id, Path::new(path)) {
            Ok(count) => {
                tracing::info!(playlist_name, count, path, "playlist exported (smoke hook)");
                toasts::show(
                    &toast_overlay,
                    &strings::playlist_exported_toast(playlist_name),
                );
            }
            Err(error) => {
                tracing::error!(%error, playlist_name, "playlist export failed (smoke hook)");
                toasts::show(
                    &toast_overlay,
                    &strings::playlist_export_failed_toast(playlist_name),
                );
            }
        }
    });
}

/// Looks up a playlist id by exact name — see [`arm_smoke_m3u`]'s doc
/// comment for why the hook takes a name instead of an id.
fn find_playlist_id_by_name(conn: &Rc<RefCell<Connection>>, name: &str) -> Option<i64> {
    let conn_ref = conn.borrow();
    playlists::list(&conn_ref)
        .inspect_err(|error| {
            tracing::error!(%error, name, "failed to list playlists for smoke m3u name lookup");
        })
        .ok()?
        .into_iter()
        .find(|p| p.name == name)
        .map(|p| p.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use reprise_core::models::Track;

    fn seeded_conn() -> Rc<RefCell<Connection>> {
        let conn = reprise_core::db::open(None).unwrap();
        reprise_core::db::migrate(&conn).unwrap();
        Rc::new(RefCell::new(conn))
    }

    #[test]
    fn playlist_name_from_file_uses_stem() {
        let name = playlist_name_from_file(Path::new("/x/Road Trip.m3u"));
        assert_eq!(name, "Road Trip");
    }

    #[test]
    fn playlist_name_from_file_falls_back_when_stem_missing() {
        // A path with no filename component at all (`Path::file_stem`
        // returns `None`, unlike a dotfile such as ".m3u" — which Rust
        // treats as a bare filename with no extension, so its "stem" is the
        // whole ".m3u" string, not a missing one).
        let name = playlist_name_from_file(Path::new("/"));
        assert_eq!(
            name,
            strings::text(strings::IMPORTED_PLAYLIST_FALLBACK_NAME)
        );
    }

    #[test]
    fn display_name_uses_artist_and_title() {
        let mut track = sample_track();
        track.artist = "Some Artist".to_string();
        track.title = "Some Title".to_string();
        assert_eq!(display_name(&track), "Some Artist - Some Title");
    }

    #[test]
    fn display_name_falls_back_to_title_only_when_artist_blank() {
        let mut track = sample_track();
        track.artist = "  ".to_string();
        track.title = "Some Title".to_string();
        assert_eq!(display_name(&track), "Some Title");
    }

    fn sample_track() -> Track {
        Track {
            id: 1,
            path: "/x/a.flac".to_string(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            album_artist: String::new(),
            year: None,
            track_no: None,
            genre: String::new(),
            duration_ms: 0,
            bitrate_kbps: None,
            rating: 0,
            play_count: 0,
            last_played_at: None,
            added_at: 0,
            file_mtime: 0,
            missing: false,
            file_size: 0,
            device: None,
            inode: None,
            playlist_position: None,
        }
    }

    #[test]
    fn import_playlist_matches_exact_absolute_paths_and_counts_unmatched() {
        let conn = seeded_conn();
        {
            let c = conn.borrow();
            c.execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
                 VALUES (1, '/music/a.flac', 'A', 'Artist A', 3000, 0)",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
                 VALUES (2, '/music/b.flac', 'B', 'Artist B', 4000, 0)",
                [],
            )
            .unwrap();
        }

        let dir = std::env::temp_dir().join(format!("reprise-m3u-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let m3u_path = dir.join("My Mix.m3u");
        std::fs::write(
            &m3u_path,
            "#EXTM3U\n/music/a.flac\n/music/nowhere.flac\n/music/b.flac\n",
        )
        .unwrap();

        let outcome = import_playlist(&conn, &m3u_path).unwrap();
        assert_eq!(outcome.name, "My Mix");
        assert_eq!(outcome.total, 3);
        assert_eq!(outcome.matched, 2);

        let track_ids: Vec<i64> = conn
            .borrow()
            .prepare(
                "SELECT track_id FROM playlist_tracks WHERE playlist_id = ?1 ORDER BY position",
            )
            .unwrap()
            .query_map(rusqlite::params![outcome.playlist_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(track_ids, vec![1, 2]);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn import_playlist_resolves_relative_paths_against_m3u_directory() {
        let conn = seeded_conn();
        let dir = std::env::temp_dir().join(format!("reprise-m3u-test-rel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let track_path = dir.join("song.flac");
        {
            let c = conn.borrow();
            c.execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
                 VALUES (1, ?1, 'S', 'Art', 1000, 0)",
                rusqlite::params![track_path.to_string_lossy().to_string()],
            )
            .unwrap();
        }
        let m3u_path = dir.join("rel.m3u");
        std::fs::write(&m3u_path, "#EXTM3U\nsong.flac\n").unwrap();

        let outcome = import_playlist(&conn, &m3u_path).unwrap();
        assert_eq!(outcome.matched, 1);
        assert_eq!(outcome.total, 1);

        std::fs::remove_dir_all(&dir).ok();
    }

    /// TDD regression for the "0-of-N-matched should not create a playlist"
    /// finding: an all-bogus `.m3u` file (no path line matches any library
    /// track) must not leave an empty, unremovable playlist behind.
    #[test]
    fn import_playlist_zero_matched_creates_no_playlist() {
        let conn = seeded_conn();
        {
            let c = conn.borrow();
            c.execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
                 VALUES (1, '/music/a.flac', 'A', 'Artist A', 3000, 0)",
                [],
            )
            .unwrap();
        }

        let dir =
            std::env::temp_dir().join(format!("reprise-m3u-test-zero-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let m3u_path = dir.join("Bogus.m3u");
        std::fs::write(
            &m3u_path,
            "#EXTM3U\n/music/nowhere.flac\n/music/also-nowhere.flac\n",
        )
        .unwrap();

        let before_count: i64 = conn
            .borrow()
            .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
            .unwrap();

        let outcome = import_playlist(&conn, &m3u_path).unwrap();
        assert_eq!(outcome.matched, 0);
        assert_eq!(outcome.total, 2);
        assert_eq!(outcome.playlist_id, None);

        let after_count: i64 = conn
            .borrow()
            .query_row("SELECT COUNT(*) FROM playlists", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            before_count, after_count,
            "zero-matched import must not create a playlist row"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    /// TDD regression for the "non-UTF-8 import" robustness gap: a `.m3u`
    /// file with one valid path line plus a trailing invalid-UTF-8 byte must
    /// still import successfully (lossy-decode, not panic/error) and still
    /// match the valid line.
    #[test]
    fn import_playlist_handles_non_utf8_bytes_via_lossy_decode() {
        let conn = seeded_conn();
        {
            let c = conn.borrow();
            c.execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
                 VALUES (1, '/music/a.flac', 'A', 'Artist A', 3000, 0)",
                [],
            )
            .unwrap();
        }

        let dir =
            std::env::temp_dir().join(format!("reprise-m3u-test-nonutf8-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let m3u_path = dir.join("bad-encoding.m3u");
        // Valid ASCII header + path line, then a lone 0xFF byte (invalid as
        // any UTF-8 sequence) on its own line — simulates a filesystem/tag
        // encoding glitch in one entry without corrupting the whole file.
        let mut bytes = b"#EXTM3U\n/music/a.flac\n".to_vec();
        bytes.push(0xFF);
        bytes.push(b'\n');
        std::fs::write(&m3u_path, &bytes).unwrap();

        let outcome = import_playlist(&conn, &m3u_path).unwrap();
        assert_eq!(outcome.matched, 1, "the valid path line should still match");
        assert!(outcome.playlist_id.is_some());

        std::fs::remove_dir_all(&dir).ok();
    }

    /// TDD regression for the "file-read error" robustness gap: a path that
    /// doesn't exist on disk must return `Err(ImportError::Io(_))`, not
    /// panic.
    #[test]
    fn import_playlist_nonexistent_path_returns_io_error() {
        let conn = seeded_conn();
        let path = std::env::temp_dir().join(format!(
            "reprise-m3u-does-not-exist-{}.m3u",
            std::process::id()
        ));

        let result = import_playlist(&conn, &path);
        assert!(matches!(result, Err(ImportError::Io(_))));
    }

    /// Same robustness gap, other failure shape: a path that exists but is a
    /// directory (not a regular file) must also return `Err(ImportError::
    /// Io(_))`, not panic — `std::fs::read` fails on a directory with an
    /// "Is a directory" `io::Error`.
    #[test]
    fn import_playlist_directory_path_returns_io_error() {
        let conn = seeded_conn();
        let dir =
            std::env::temp_dir().join(format!("reprise-m3u-test-dirpath-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let result = import_playlist(&conn, &dir);
        assert!(matches!(result, Err(ImportError::Io(_))));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn export_playlist_writes_absolute_paths_and_extinf() {
        let conn = seeded_conn();
        let playlist_id = {
            let mut c = conn.borrow_mut();
            c.execute(
                "INSERT INTO tracks (id, path, title, artist, duration_ms, added_at) \
                 VALUES (1, '/music/a.flac', 'Title A', 'Artist A', 125000, 0)",
                [],
            )
            .unwrap();
            let id = playlists::create(&c, "Exported").unwrap();
            playlists::add_tracks(&mut c, id, &[1]).unwrap();
            id
        };

        let dir = std::env::temp_dir().join(format!("reprise-m3u-test-exp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out_path = dir.join("out.m3u");

        let count = export_playlist(&conn, playlist_id, &out_path).unwrap();
        assert_eq!(count, 1);

        let content = std::fs::read_to_string(&out_path).unwrap();
        assert!(content.starts_with("#EXTM3U\n"));
        assert!(content.contains("#EXTINF:125,Artist A - Title A"));
        assert!(content.contains("/music/a.flac"));

        std::fs::remove_dir_all(&dir).ok();
    }
}
