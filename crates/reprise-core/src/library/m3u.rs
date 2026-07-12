//! Pure M3U/M3U8 parsing and serialization (Stage 3 Task 7). No file I/O,
//! no database access, no path resolution — both directions are plain
//! string transforms so they can be unit-tested without a filesystem or a
//! GTK main loop. The import/export *flow* (reading/writing files, resolving
//! paths against the library, matching against the DB, driving the file
//! dialogs) lives in `ui::playlist_io`.
//!
//! ## Parsing decisions
//!
//! - **Tolerant, not validating**: every non-blank line that doesn't start
//!   with `#` is returned as a path entry, verbatim (trimmed of surrounding
//!   whitespace only). A garbage line that isn't actually a valid path is
//!   still returned — `parse_m3u` has no way to know that without touching
//!   the filesystem, and doing so here would make this "pure" module
//!   filesystem-dependent. The caller (`ui::playlist_io::import_playlist`)
//!   is the one that discovers a path doesn't match anything, and reports
//!   that in its "N of M matched" count.
//! - **`#EXTINF` is metadata for the path line that follows it, but this
//!   parser doesn't correlate the two**: an `#EXTINF:<secs>,<display>` line
//!   is simply skipped like any other `#`-prefixed comment. The duration and
//!   display name it carries describe the *next* path line by the M3U
//!   convention, but import only needs the path (it re-derives title/artist/
//!   duration from the already-scanned `tracks` row a path matches against),
//!   so capturing and correlating that metadata here would be complexity
//!   with no consumer. This also settles the "dangling `#EXTINF`" case from
//!   the task brief: an `#EXTINF` line with no path line after it (e.g. the
//!   last line in a truncated file) produces **no entry at all** — it's just
//!   a skipped comment line like any other, never a special "entry with a
//!   missing path" case. See `dangling_extinf_produces_no_entry` below.
//! - **CRLF and LF**: `str::lines()` already splits on both `\n` and
//!   `\r\n` (stripping the line terminator either way), so no special
//!   handling is needed here — see `crlf_and_lf_both_parse_the_same`.
//! - **Relative vs. absolute paths**: `parse_m3u` returns each path line
//!   completely unresolved — exactly the text found on that line, whether
//!   it looks relative (`Track 01.flac`, `../Other Album/x.flac`) or
//!   absolute (`/home/user/Music/x.flac`). Resolving a relative path against
//!   the `.m3u` file's own directory is an import-time concern (it needs the
//!   file's location, which this module never sees), not a parsing concern —
//!   see `ui::playlist_io::resolve_import_path`.

/// One parsed line from an M3U playlist: a path, exactly as written in the
/// file (not yet resolved to absolute, not yet matched against anything).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M3uEntry {
    pub path: String,
}

/// One track to write into an exported M3U file. `duration_secs` and
/// `display` are the caller's responsibility to derive (see `ui::
/// playlist_io::export_playlist`, which computes `duration_secs` from
/// `duration_ms / 1000` and `display` as `"Artist - Title"`, falling back to
/// just `Title` when there's no artist) — this module only knows how to lay
/// them out as `#EXTINF` lines, not where the numbers come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct M3uExportEntry {
    pub path: String,
    pub duration_secs: i64,
    pub display: String,
}

/// Parses M3U/M3U8 content into an ordered list of path entries. Tolerant of
/// blank lines and any `#`-prefixed line (`#EXTM3U`, `#EXTINF:...`, or any
/// other comment) — all skipped. Every remaining line becomes one
/// [`M3uEntry`], trimmed but otherwise unvalidated (no filesystem check, no
/// existence check — see the module doc's `## Parsing decisions` section).
pub fn parse_m3u(content: &str) -> Vec<M3uEntry> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| M3uEntry {
            path: line.to_string(),
        })
        .collect()
}

/// Serializes `entries` into M3U8 text: a leading `#EXTM3U` header, then per
/// entry an `#EXTINF:<duration_secs>,<display>` line followed by the path
/// line. Callers are expected to pass absolute paths (see `ui::playlist_io::
/// export_playlist`) — this function writes whatever `path` string it's
/// given verbatim, without checking or normalizing it.
pub fn serialize_m3u(entries: &[M3uExportEntry]) -> String {
    let mut out = String::from("#EXTM3U\n");
    for entry in entries {
        out.push_str(&format!(
            "#EXTINF:{},{}\n{}\n",
            entry.duration_secs, entry.display, entry.path
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_serialize_then_parse_preserves_paths() {
        let entries = vec![
            M3uExportEntry {
                path: "/music/Artist/Album/01 Track.flac".to_string(),
                duration_secs: 215,
                display: "Artist - Track".to_string(),
            },
            M3uExportEntry {
                path: "/music/Other/02 Song.mp3".to_string(),
                duration_secs: 180,
                display: "Other - Song".to_string(),
            },
        ];
        let text = serialize_m3u(&entries);
        let parsed = parse_m3u(&text);
        let paths: Vec<&str> = parsed.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                "/music/Artist/Album/01 Track.flac",
                "/music/Other/02 Song.mp3",
            ]
        );
    }

    #[test]
    fn serialize_writes_extm3u_header_and_extinf_lines() {
        let entries = vec![M3uExportEntry {
            path: "/a/b.flac".to_string(),
            duration_secs: 42,
            display: "Someone - Something".to_string(),
        }];
        let text = serialize_m3u(&entries);
        assert_eq!(text, "#EXTM3U\n#EXTINF:42,Someone - Something\n/a/b.flac\n");
    }

    #[test]
    fn parse_is_tolerant_of_blank_and_comment_lines() {
        let content = "\
#EXTM3U
#EXTINF:100,Artist - Title

/music/a.flac
# a plain comment, not EXTINF
/music/b.flac

";
        let entries = parse_m3u(content);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].path, "/music/a.flac");
        assert_eq!(entries[1].path, "/music/b.flac");
    }

    #[test]
    fn crlf_and_lf_both_parse_the_same() {
        let lf = "#EXTM3U\n/music/a.flac\n/music/b.flac\n";
        let crlf = "#EXTM3U\r\n/music/a.flac\r\n/music/b.flac\r\n";
        assert_eq!(parse_m3u(lf), parse_m3u(crlf));
        assert_eq!(parse_m3u(crlf).len(), 2);
    }

    #[test]
    fn parses_paths_with_spaces_and_unicode() {
        let content = "#EXTM3U\n/music/Ünïcödé Ärtist/Track Nr. 1 (live).flac\n";
        let entries = parse_m3u(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(
            entries[0].path,
            "/music/Ünïcödé Ärtist/Track Nr. 1 (live).flac"
        );
    }

    #[test]
    fn dangling_extinf_produces_no_entry() {
        // An #EXTINF line with nothing after it (e.g. a truncated file) is
        // just a skipped comment line, per the module doc's decision — it
        // never becomes an entry with a missing/empty path.
        let content = "#EXTM3U\n/music/a.flac\n#EXTINF:120,Orphaned Title";
        let entries = parse_m3u(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "/music/a.flac");
    }

    #[test]
    fn parse_returns_relative_and_absolute_paths_unresolved() {
        let content = "#EXTM3U\n../Other Album/x.flac\n/abs/path/y.flac\nplain.flac\n";
        let entries = parse_m3u(content);
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        // Returned exactly as written — no join against a base directory,
        // no absolute-ness check. Resolution is an import-time concern.
        assert_eq!(
            paths,
            vec!["../Other Album/x.flac", "/abs/path/y.flac", "plain.flac"]
        );
    }

    #[test]
    fn parse_returns_garbage_lines_as_entries_unvalidated() {
        // Not a real path in any meaningful sense, but parse_m3u has no way
        // (and no business) to know that — see the module doc.
        let content = "#EXTM3U\nthis is not a path??\n";
        let entries = parse_m3u(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "this is not a path??");
    }

    #[test]
    fn parse_empty_content_returns_no_entries() {
        assert_eq!(parse_m3u(""), Vec::new());
        assert_eq!(parse_m3u("#EXTM3U\n"), Vec::new());
    }

    #[test]
    fn serialize_empty_entries_writes_only_header() {
        assert_eq!(serialize_m3u(&[]), "#EXTM3U\n");
    }
}
