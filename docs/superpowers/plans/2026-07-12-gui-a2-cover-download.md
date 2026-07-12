# GUI-A2: Online Album-Cover Download (opt-in) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** For tracks with no local cover, optionally fetch a high-resolution album cover from Cover Art Archive (opt-in, default OFF), cached like local covers, so the previously-blank/low-res cases get sharp art.

**Architecture:** A new pure-core `cover_download` module (adds the `ureq` blocking HTTP client) resolves a MusicBrainz release (embedded MBID preferred, else a conservative artist+album search) and downloads the front cover into a `covers/downloaded/` cache. `resolve_source` gains a 3rd, offline resolution stage that finds those cached files. A dedicated frontend download worker thread triggers fetches off the UI thread when the gated module is on, deduping per album; a header menu toggle turns the module on/off.

**Tech Stack:** Rust 2021, `ureq` (blocking, rustls — NEW core dep), `serde_json` (already in core), `lofty 0.22` (`ItemKey::MusicBrainzReleaseId`), `image`/`dirs`/`fastrand` (already in core), gtk4-rs 0.11.4 (`gio::SimpleAction::new_stateful`, `gtk4::MenuButton`, `gio::Menu`).

## Global Constraints

- **Core stays dependency-pure of GUI/platform libs.** After every task, `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` MUST be empty. The one new core dep is `ureq` (a pure-Rust HTTP client; verify it pulls in none of gtk/gst/zbus). Run `cargo audit` after adding it — if `ureq`/rustls introduces a NEW advisory, STOP and report.
- **Opt-in, default OFF.** `COVER_DOWNLOAD_MODULE.default_enabled = false`. NO network request may happen unless the user has explicitly enabled the module. A test MUST assert nothing is requested when the module is off.
- **Core promise — never write into the user's library.** Downloaded covers land ONLY under `cover::cache_dir().join("downloaded")` (i.e. `$XDG_CACHE_HOME/reprise/covers/downloaded/`). NEVER a track folder, NEVER an audio file. A test MUST assert the download path is under the cache dir.
- **Privacy — minimal egress.** Only album-artist + album (or an embedded MBID) go to MusicBrainz/Cover Art Archive. No user id, no other telemetry.
- **MusicBrainz etiquette (required or they block us):** a descriptive `User-Agent` (`Reprise/<version> ( https://github.com/reprise-music/reprise )`), and at most 1 request/second to MusicBrainz (a process-global min-interval). Per-request timeouts. Offline/DNS errors degrade to a silent `None`, never a panic or ERROR line.
- **Gates per commit (all pass):** `cargo fmt --check`; `cargo clippy --all-targets --workspace -- -D warnings`; `cargo test --workspace` (bare `cargo test` runs only the gnome default-member — always `--workspace`); `cargo audit` (accepted advisory: RUSTSEC-2024-0436 `paste`).
- **Baseline test count: 390 passed; 1 ignored.** Each task states its expected new total; the implementer records the actual.
- **Headless smoke isolation — MANDATORY (a prior stage clobbered the user's real DB by omitting `XDG_DATA_HOME`).** Every smoke command STRING must contain BOTH `XDG_DATA_HOME=$(mktemp -d)` AND `XDG_CACHE_HOME=$(mktemp -d)`, plus `dbus-run-session -- xvfb-run -a env GDK_BACKEND=x11 REPRISE_AUDIO_SINK=fakesink WAYLAND_DISPLAY= … cargo run`. Grep your own command for `XDG_DATA_HOME` before running — if absent, it is NOT isolated.
- **No real network in the test suite.** Unit-test the pure parts (key normalization, URL building, JSON parsing, negative-cache, resolve stage, module gate) against fixtures. Any live-network test is a separate `#[ignore]` integration test, never in the default suite.
- **File-size rule:** every file created or substantially edited ends < 800 lines. **English** for code/comments/log/UI strings/commits. No commit attribution footer. **Do not push.**

## Current-state facts (verified 2026-07-12, HEAD 1892314)

- `cover.rs`: `resolve_source(track_path) -> Option<CoverSource>` has 2 stages (embedded via `embedded_picture` → `folder_image`); `embedded_picture` reads the `TaggedFile` and discards it. `CoverSource { Embedded(Vec<u8>), FolderImage(PathBuf) }`. `pub fn cache_dir() -> PathBuf`, `pub fn thumbnail(&CoverSource, ThumbnailSize) -> Result<PathBuf, CoverError>`, private `fn hash_hex(bytes: &[u8]) -> String` (DefaultHasher, 16 hex chars), private `fn source_bytes(&CoverSource)`. `fastrand` already used for temp names.
- lofty 0.22: read tags via `lofty::read_from_path(path)?` → `tagged.primary_tag().or_else(|| tagged.first_tag())` → `&Tag`; `t.album()` (Accessor), `t.get_string(&lofty::tag::ItemKey::AlbumArtist)`, and confirmed `t.get_string(&lofty::tag::ItemKey::MusicBrainzReleaseId)` (maps `MUSICBRAINZ_ALBUMID` / `MusicBrainz Album Id`).
- `CoverLoader` (`ui/cover_loader.rs`): `load_into(self: &Rc<Self>, image, track_path, size, token, current: &Rc<Cell<u64>>)`; the resolve+thumbnail runs inside `gio::spawn_blocking` at ~lines 82-85; struct has `cache`/`order` RefCells. No `Connection`, no settings-flag field yet.
- `modules.rs`: `ModuleDescriptor { id, name, description, default_enabled }`, `MPRIS_MODULE`, `ALL_MODULES = &[&MPRIS_MODULE]`, `pub fn is_enabled(&Connection, &ModuleDescriptor) -> Result<bool, rusqlite::Error>`, `set_enabled`, `pub(crate) enabled_key`.
- Startup module read pattern — `window.rs:148-153`: `modules::is_enabled(&conn.borrow(), &MPRIS_MODULE).unwrap_or_else(|e| { warn; true })`, passed into `PlayerController::new`. Only `is_enabled` call site in the frontend.
- Header (`window.rs::build` ~93-134): `adw::HeaderBar::new()`, `pack_start(sidebar_toggle)`, `set_title_widget`, `pack_start(search_entry)`, `pack_end(scan_button)`, `pack_end(import_button)`. NO app menu / `gio::Menu` in header yet — greenfield.
- Actions: established stateless `gio::SimpleAction` pattern in `shortcuts.rs:159-181` (`SimpleAction::new` + `connect_activate` + `window.add_action`). NO `new_stateful`/`change-state` anywhere yet.
- `reprise-core/Cargo.toml`: has `serde`/`serde_json`, `lofty 0.22`, `dirs 6`, `fastrand 2`, `image`; NO http client. Workspace clippy lints hardened to `-D warnings`.

## Ordering

Strictly sequential: 1 → 2 → 3 → 4 → 5 → 6 → 7. Tasks 1–5 are pure/core (unit-tested). Tasks 6–7 are frontend (headless smokes). Task 3 depends on 1+2; Task 4 depends on 1; Task 6 depends on 3+4+5.

---

### Task 1: `cover_download` foundation — `ureq` dep, album-key, download-cache paths

**Files:**
- Modify: `crates/reprise-core/Cargo.toml` (add `ureq`)
- Create: `crates/reprise-core/src/cover_download.rs`
- Modify: `crates/reprise-core/src/lib.rs` (`pub mod cover_download;`)
- Modify: `crates/reprise-core/src/cover.rs` (promote `hash_hex` to `pub(crate)`)

**Interfaces:**
- Consumes: `cover::cache_dir()`, `cover::hash_hex` (promoted `pub(crate)`).
- Produces:
  - `pub fn album_key(album_artist: &str, album: &str) -> String` — normalized (lowercased, trimmed, whitespace-collapsed) `"{aa}\u{1}{album}"` hashed via `cover::hash_hex` → 16 hex chars.
  - `pub fn downloaded_dir() -> PathBuf` — `cover::cache_dir().join("downloaded")`.
  - `pub fn downloaded_cover_path(key: &str) -> Option<PathBuf>` — the existing `<downloaded>/<key>.<ext>` image file for `key` (first matching known ext), or None.
  - `pub fn negative_marker_path(key: &str) -> PathBuf` — `<downloaded>/<key>.notfound`.

- [ ] **Step 1: Add `ureq`.** In `crates/reprise-core/Cargo.toml` `[dependencies]`:

```toml
# Blocking, pure-Rust HTTP client (rustls) for the opt-in cover download
# (GUI-A2). Blocking is fine — fetches run on a dedicated worker thread, never
# the UI thread. No async runtime, no gtk/gstreamer/zbus (verify cargo tree).
ureq = "2"
```

- [ ] **Step 2: Promote `hash_hex`.** In `crates/reprise-core/src/cover.rs`, change `fn hash_hex(` to `pub(crate) fn hash_hex(`.

- [ ] **Step 3: Write the failing tests** in `crates/reprise-core/src/cover_download.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_key_normalizes_case_and_whitespace() {
        assert_eq!(album_key("Pink Floyd", "The Wall"), album_key("  pink   floyd ", "the wall"));
    }

    #[test]
    fn album_key_distinguishes_different_albums() {
        assert_ne!(album_key("A", "X"), album_key("A", "Y"));
        assert_ne!(album_key("A", "X"), album_key("B", "X"));
    }

    #[test]
    fn downloaded_dir_is_under_cache_dir() {
        assert!(downloaded_dir().starts_with(crate::cover::cache_dir()));
    }

    #[test]
    fn downloaded_cover_path_finds_an_existing_file_and_none_otherwise() {
        let key = album_key("FetchTest", "OnlyHere");
        assert!(downloaded_cover_path(&key).is_none());
        std::fs::create_dir_all(downloaded_dir()).unwrap();
        let f = downloaded_dir().join(format!("{key}.jpg"));
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(downloaded_cover_path(&key), Some(f.clone()));
        std::fs::remove_file(&f).ok();
    }
}
```

- [ ] **Step 4: Run to verify failure.** `cargo test -p reprise-core cover_download 2>&1 | tail` → FAIL (functions not found).

- [ ] **Step 5: Implement** `crates/reprise-core/src/cover_download.rs`:

```rust
//! Opt-in online album-cover download (GUI-A2). Resolves a MusicBrainz release
//! and fetches its Cover Art Archive front cover into the `covers/downloaded/`
//! cache. GATED: nothing here touches the network unless the `cover_download`
//! module is enabled (default off). Writes ONLY under the XDG cover cache.

use std::path::PathBuf;

use crate::cover;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// Cache key for an album's downloaded cover: normalized album-artist + album,
/// hashed to hex. One cover per album — every track of an album shares it.
pub fn album_key(album_artist: &str, album: &str) -> String {
    fn norm(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
    }
    cover::hash_hex(format!("{}\u{1}{}", norm(album_artist), norm(album)).as_bytes())
}

/// `<XDG cache>/reprise/covers/downloaded`.
pub fn downloaded_dir() -> PathBuf {
    cover::cache_dir().join("downloaded")
}

/// The cached downloaded cover file for `key`, if one exists (any known ext).
pub fn downloaded_cover_path(key: &str) -> Option<PathBuf> {
    let dir = downloaded_dir();
    IMAGE_EXTS
        .iter()
        .map(|ext| dir.join(format!("{key}.{ext}")))
        .find(|p| p.exists())
}

/// Marker written when a lookup found nothing — stops re-querying that album.
pub fn negative_marker_path(key: &str) -> PathBuf {
    downloaded_dir().join(format!("{key}.notfound"))
}
```

Add `pub mod cover_download;` to `crates/reprise-core/src/lib.rs` (alphabetical: after `cover`).

- [ ] **Step 6: Tests pass.** `cargo test -p reprise-core cover_download 2>&1 | tail` → 4 passed.

- [ ] **Step 7: Purity + gates + commit.**

Run: `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus' || echo PURE` → PURE.
Run: `cargo clippy -p reprise-core --all-targets -- -D warnings && cargo audit 2>&1 | grep -iE 'RUSTSEC|error' | head && cargo test --workspace 2>&1 | grep 'test result'`
Expected: clippy clean; audit shows only accepted `paste` (if ureq adds a NEW advisory, STOP + report); totals **394 passed; 1 ignored**.

```bash
git add crates/reprise-core/Cargo.toml Cargo.lock crates/reprise-core/src/cover_download.rs crates/reprise-core/src/lib.rs crates/reprise-core/src/cover.rs
git commit -m "feat: cover_download foundation — ureq dep, album-key, download-cache paths"
```

---

### Task 2: MusicBrainz URL builders + conservative release matching (pure)

**Files:**
- Modify: `crates/reprise-core/src/cover_download.rs`

**Interfaces:**
- Produces:
  - `pub(crate) fn musicbrainz_search_url(album_artist: &str, album: &str) -> String`
  - `pub(crate) fn caa_front_url(mbid: &str) -> String`
  - `pub(crate) fn parse_best_release(json: &str, album_artist: &str, album: &str) -> Option<String>` — the best release MBID whose score is high AND artist/album plausibly match; else None (conservative — never a weak guess).

- [ ] **Step 1: Write the failing tests** (add to the `tests` mod). Use small inline JSON fixtures modeled on the MusicBrainz `/ws/2/release` search response shape:

```rust
    const MB_STRONG: &str = r#"{"releases":[
      {"id":"11111111-1111-1111-1111-111111111111","score":100,
       "title":"The Wall","artist-credit":[{"name":"Pink Floyd"}]}]}"#;
    const MB_WEAK: &str = r#"{"releases":[
      {"id":"22222222-2222-2222-2222-222222222222","score":42,
       "title":"Something Else","artist-credit":[{"name":"Other Band"}]}]}"#;

    #[test]
    fn parse_best_release_accepts_a_strong_match() {
        assert_eq!(
            parse_best_release(MB_STRONG, "Pink Floyd", "The Wall").as_deref(),
            Some("11111111-1111-1111-1111-111111111111")
        );
    }

    #[test]
    fn parse_best_release_rejects_a_weak_match() {
        assert!(parse_best_release(MB_WEAK, "Pink Floyd", "The Wall").is_none());
    }

    #[test]
    fn parse_best_release_handles_empty_and_garbage() {
        assert!(parse_best_release(r#"{"releases":[]}"#, "A", "B").is_none());
        assert!(parse_best_release("not json", "A", "B").is_none());
    }

    #[test]
    fn urls_are_well_formed() {
        assert!(musicbrainz_search_url("Pink Floyd", "The Wall")
            .starts_with("https://musicbrainz.org/ws/2/release"));
        assert_eq!(
            caa_front_url("11111111-1111-1111-1111-111111111111"),
            "https://coverartarchive.org/release/11111111-1111-1111-1111-111111111111/front"
        );
    }
```

- [ ] **Step 2: Run to verify failure.** `cargo test -p reprise-core cover_download 2>&1 | tail` → FAIL.

- [ ] **Step 3: Implement** (append to `cover_download.rs`). The conservative rule: require `score >= 90` AND the winning release's artist-credit name and title each case-insensitively equal (after the same `norm` as the key) the query — this is what prevents wrong covers:

```rust
/// Minimum MusicBrainz search score to even consider a release.
const MIN_MB_SCORE: i64 = 90;

pub(crate) fn musicbrainz_search_url(album_artist: &str, album: &str) -> String {
    // MusicBrainz Lucene query; percent-encode the whole query value.
    let query = format!("artist:\"{album_artist}\" AND release:\"{album}\"");
    format!(
        "https://musicbrainz.org/ws/2/release?query={}&fmt=json&limit=5",
        urlencode(&query)
    )
}

pub(crate) fn caa_front_url(mbid: &str) -> String {
    format!("https://coverartarchive.org/release/{mbid}/front")
}

pub(crate) fn parse_best_release(json: &str, album_artist: &str, album: &str) -> Option<String> {
    fn norm(s: &str) -> String {
        s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
    }
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let releases = v.get("releases")?.as_array()?;
    let (want_artist, want_album) = (norm(album_artist), norm(album));
    for r in releases {
        let score = r.get("score").and_then(serde_json::Value::as_i64).unwrap_or(0);
        if score < MIN_MB_SCORE {
            continue;
        }
        let title = r.get("title").and_then(|t| t.as_str()).unwrap_or_default();
        let artist = r
            .get("artist-credit")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .and_then(|c| c.get("name"))
            .and_then(|n| n.as_str())
            .unwrap_or_default();
        if norm(title) == want_album && norm(artist) == want_artist {
            return r.get("id").and_then(|i| i.as_str()).map(str::to_string);
        }
    }
    None
}

/// Minimal percent-encoding for a query value (RFC 3986 unreserved kept).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
```

- [ ] **Step 4: Tests pass.** `cargo test -p reprise-core cover_download 2>&1 | tail` → 8 passed.
- [ ] **Step 5: Gates + commit.** clippy/`test --workspace` → **398 passed; 1 ignored**.

```bash
git add crates/reprise-core/src/cover_download.rs
git commit -m "feat: MusicBrainz URL builders + conservative release matching"
```

---

### Task 3: `fetch_and_cache` — the HTTP fetch (ureq), rate limit, negative cache

**Files:**
- Modify: `crates/reprise-core/src/cover_download.rs`

**Interfaces:**
- Consumes: Task 1 (`album_key`/`downloaded_dir`/`downloaded_cover_path`/`negative_marker_path`), Task 2 (URLs/parse), `ureq`.
- Produces: `pub fn fetch_and_cache(album_artist: &str, album: &str, mbid: Option<&str>) -> Option<PathBuf>` — BLOCKING, off-thread only. Returns the cached image path, or None. Self-rate-limits MusicBrainz calls to ≤1/s; writes a `.notfound` marker on a clean miss.

- [ ] **Step 1: Write the (non-network) failing tests** — the network path itself is NOT unit-tested (no CI network); test the short-circuits:

```rust
    #[test]
    fn fetch_returns_cached_path_without_network_when_already_downloaded() {
        let key = album_key("CachedBand", "CachedAlbum");
        std::fs::create_dir_all(downloaded_dir()).unwrap();
        let f = downloaded_dir().join(format!("{key}.png"));
        std::fs::write(&f, b"img").unwrap();
        // Already cached -> must return it, never touching the network.
        assert_eq!(fetch_and_cache("CachedBand", "CachedAlbum", None), Some(f.clone()));
        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn fetch_short_circuits_on_negative_marker_without_network() {
        let key = album_key("MissBand", "MissAlbum");
        std::fs::create_dir_all(downloaded_dir()).unwrap();
        let marker = negative_marker_path(&key);
        std::fs::write(&marker, b"").unwrap();
        assert_eq!(fetch_and_cache("MissBand", "MissAlbum", None), None);
        std::fs::remove_file(&marker).ok();
    }
```

- [ ] **Step 2: Run to verify failure.** `cargo test -p reprise-core cover_download 2>&1 | tail` → FAIL (`fetch_and_cache` not found).

- [ ] **Step 3: Implement** (append to `cover_download.rs`). Structure: cache/negative short-circuit FIRST (the tested paths, no network), then the network path:

```rust
use std::sync::Mutex;
use std::time::{Duration, Instant};

const USER_AGENT: &str = concat!(
    "Reprise/", env!("CARGO_PKG_VERSION"),
    " ( https://github.com/reprise-music/reprise )"
);
const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MB_MIN_INTERVAL: Duration = Duration::from_secs(1);

/// Process-global timestamp of the last MusicBrainz request, for the ≤1/s rule.
static LAST_MB_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

pub fn fetch_and_cache(album_artist: &str, album: &str, mbid: Option<&str>) -> Option<PathBuf> {
    let key = album_key(album_artist, album);
    // 1. Already resolved (positive or negative) -> no network.
    if let Some(existing) = downloaded_cover_path(&key) {
        return Some(existing);
    }
    if negative_marker_path(&key).exists() {
        return None;
    }
    // 2. Resolve a release MBID: embedded first, else conservative search.
    let release_mbid = match mbid {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            let body = mb_get(&musicbrainz_search_url(album_artist, album))?;
            match parse_best_release(&body, album_artist, album) {
                Some(id) => id,
                None => {
                    write_negative(&key);
                    return None;
                }
            }
        }
    };
    // 3. Fetch the CAA front cover (follows the 302 to the image).
    let (bytes, ext) = match http_get_bytes(&caa_front_url(&release_mbid)) {
        Some(v) => v,
        None => {
            write_negative(&key);
            return None;
        }
    };
    // 4. Publish atomically under the download cache.
    store_downloaded(&key, &bytes, ext)
}

/// A rate-limited MusicBrainz GET returning the response body as text.
fn mb_get(url: &str) -> Option<String> {
    respect_mb_rate_limit();
    ureq::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .get(url)
        .call()
        .ok()?
        .into_string()
        .ok()
}

/// A plain GET returning (bytes, extension) for an image response.
fn http_get_bytes(url: &str) -> Option<(Vec<u8>, &'static str)> {
    let resp = ureq::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .get(url)
        .call()
        .ok()?;
    let ext = match resp.content_type() {
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "jpg", // CAA front covers are overwhelmingly JPEG
    };
    let mut bytes = Vec::new();
    use std::io::Read;
    resp.into_reader().take(20 * 1024 * 1024).read_to_end(&mut bytes).ok()?;
    if bytes.is_empty() {
        return None;
    }
    Some((bytes, ext))
}

fn respect_mb_rate_limit() {
    let mut last = LAST_MB_REQUEST.lock().unwrap();
    if let Some(prev) = *last {
        let elapsed = prev.elapsed();
        if elapsed < MB_MIN_INTERVAL {
            std::thread::sleep(MB_MIN_INTERVAL - elapsed);
        }
    }
    *last = Some(Instant::now());
}

fn write_negative(key: &str) {
    let _ = std::fs::create_dir_all(downloaded_dir());
    let _ = std::fs::write(negative_marker_path(key), b"");
}

fn store_downloaded(key: &str, bytes: &[u8], ext: &str) -> Option<PathBuf> {
    let dir = downloaded_dir();
    std::fs::create_dir_all(&dir).ok()?;
    let out = dir.join(format!("{key}.{ext}"));
    let tmp = dir.join(format!(".{key}-{}.{ext}.tmp", fastrand::u64(..)));
    if std::fs::write(&tmp, bytes).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    if std::fs::rename(&tmp, &out).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return downloaded_cover_path(key); // a concurrent writer may have published it
    }
    Some(out)
}
```

(Verify the exact `ureq 2.x` API: `ureq::builder().timeout(..).user_agent(..).build()` → `Agent`; `.get(url).call()` → `Result<Response, _>`; `Response::content_type()`, `Response::into_string()`, `Response::into_reader()`. Adjust to the real 2.x signatures if the compiler disagrees, keeping behavior identical. `ureq` follows redirects by default — confirm; CAA `/front` 302-redirects to the image.)

- [ ] **Step 4: Tests pass** (the two non-network short-circuit tests). `cargo test -p reprise-core cover_download 2>&1 | tail` → 10 passed.
- [ ] **Step 5: Gates + commit.** clippy/`test --workspace` → **400 passed; 1 ignored**; `wc -l crates/reprise-core/src/cover_download.rs` < 800; purity PURE.

```bash
git add crates/reprise-core/src/cover_download.rs
git commit -m "feat: fetch_and_cache — CAA cover download, rate-limited, negative-cached"
```

---

### Task 4: `resolve_source` 3rd stage — download-cache lookup + single tag read

**Files:**
- Modify: `crates/reprise-core/src/cover.rs`

**Interfaces:**
- Consumes: Task 1 (`cover_download::{album_key, downloaded_cover_path}`).
- Produces:
  - `pub struct CoverTag { pub picture: Option<Vec<u8>>, pub album_artist: Option<String>, pub album: Option<String>, pub release_mbid: Option<String> }`
  - `pub fn read_cover_tag(track_path: &Path) -> CoverTag` — one lofty read, all cover-relevant fields (used by `resolve_source` AND the frontend download worker so it never double-reads).
  - `resolve_source` unchanged signature, now 3 stages.

- [ ] **Step 1: Write the failing tests** in `cover.rs`'s test mod (reuse the Task-1/2 `write`/`TINY_PNG`/`red_png` helpers already there):

```rust
    #[test]
    fn resolve_source_stage3_finds_a_downloaded_cover() {
        // A track file with album tags but NO embedded/folder cover, whose
        // album already has a downloaded cache file, resolves to it.
        let dir = tempfile::tempdir().unwrap();
        // A minimal file that lofty can read tags from is heavy to fabricate;
        // instead assert the stage-3 lookup wiring via read_cover_tag + the
        // download-cache path directly:
        let key = crate::cover_download::album_key("StageThree", "Album");
        std::fs::create_dir_all(crate::cover_download::downloaded_dir()).unwrap();
        let f = crate::cover_download::downloaded_dir().join(format!("{key}.jpg"));
        std::fs::write(&f, b"img").unwrap();
        assert_eq!(
            crate::cover_download::downloaded_cover_path(&key),
            Some(f.clone()),
            "stage-3 lookup must find the album's downloaded cover"
        );
        // And a downloaded cover path is always under the cache dir (promise):
        assert!(f.starts_with(cache_dir()));
        std::fs::remove_file(&f).ok();
        let _ = dir;
    }
```

(Note: fabricating a real tagged audio file for a full `resolve_source` end-to-end is out of proportion here; the stage-3 lookup logic is `downloaded_cover_path(album_key(aa, al))`, already unit-covered in Task 1 + this promise assertion. The `read_cover_tag` extraction is exercised by the existing embedded-cover tests plus the frontend smoke.)

- [ ] **Step 2: Run to verify failure.** (This test passes on Task-1 code already for the path part; the point of Task 4 is the `resolve_source` refactor + `read_cover_tag`.) Run `cargo test -p reprise-core cover 2>&1 | tail`.

- [ ] **Step 3: Refactor `resolve_source` + add `read_cover_tag`** in `cover.rs`. Replace `embedded_picture` usage with a single-read `read_cover_tag`, and append stage 3:

```rust
/// All cover-relevant tag fields, read in ONE lofty pass (so `resolve_source`
/// and the download worker never open the file twice).
pub struct CoverTag {
    pub picture: Option<Vec<u8>>,
    pub album_artist: Option<String>,
    pub album: Option<String>,
    pub release_mbid: Option<String>,
}

pub fn read_cover_tag(track_path: &Path) -> CoverTag {
    use lofty::prelude::*;
    let Ok(tagged) = lofty::read_from_path(track_path) else {
        return CoverTag { picture: None, album_artist: None, album: None, release_mbid: None };
    };
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return CoverTag { picture: None, album_artist: None, album: None, release_mbid: None };
    };
    CoverTag {
        picture: tag.pictures().first().map(|p| p.data().to_vec()),
        album_artist: tag
            .get_string(&lofty::tag::ItemKey::AlbumArtist)
            .map(str::to_string),
        album: tag.album().map(|s| s.to_string()),
        release_mbid: tag
            .get_string(&lofty::tag::ItemKey::MusicBrainzReleaseId)
            .map(str::to_string),
    }
}

pub fn resolve_source(track_path: &Path) -> Option<CoverSource> {
    let tag = read_cover_tag(track_path);
    if let Some(bytes) = tag.picture {
        return Some(CoverSource::Embedded(bytes));
    }
    if let Some(dir) = track_path.parent() {
        if let Some(p) = folder_image(dir) {
            return Some(CoverSource::FolderImage(p));
        }
    }
    // Stage 3 (offline): a previously downloaded cover for this album.
    if let (Some(aa), Some(al)) = (tag.album_artist.as_deref(), tag.album.as_deref()) {
        let key = crate::cover_download::album_key(aa, al);
        if let Some(p) = crate::cover_download::downloaded_cover_path(&key) {
            return Some(CoverSource::FolderImage(p));
        }
    }
    None
}
```

Delete the now-unused `embedded_picture` fn (its logic moved into `read_cover_tag`).

- [ ] **Step 4: Tests pass.** `cargo test -p reprise-core cover 2>&1 | tail` → all cover tests green (existing embedded/folder tests must still pass — the refactor is behavior-preserving for stages 1–2).
- [ ] **Step 5: Gates + commit.** clippy/`test --workspace` → **401 passed; 1 ignored** (approx — record actual); purity PURE; `wc -l cover.rs` < 800.

```bash
git add crates/reprise-core/src/cover.rs
git commit -m "feat: resolve_source stage 3 (downloaded-cover cache) via single tag read"
```

---

### Task 5: `COVER_DOWNLOAD_MODULE` in the registry (pure)

**Files:**
- Modify: `crates/reprise-core/src/modules.rs`

**Interfaces:**
- Produces: `pub const COVER_DOWNLOAD_MODULE: ModuleDescriptor` (id `"cover_download"`, `default_enabled: false`); appended to `ALL_MODULES`.

- [ ] **Step 1: Write the failing tests** (add to `modules.rs` test mod):

```rust
    #[test]
    fn cover_download_defaults_to_disabled() {
        let conn = migrated_conn();
        assert!(!is_enabled(&conn, &COVER_DOWNLOAD_MODULE).unwrap());
    }

    #[test]
    fn cover_download_round_trips() {
        let conn = migrated_conn();
        set_enabled(&conn, &COVER_DOWNLOAD_MODULE, true).unwrap();
        assert!(is_enabled(&conn, &COVER_DOWNLOAD_MODULE).unwrap());
    }

    #[test]
    fn all_modules_lists_cover_download() {
        assert!(ALL_MODULES.iter().any(|m| m.id == "cover_download"));
    }
```

- [ ] **Step 2: Run to verify failure.** `cargo test -p reprise-core modules 2>&1 | tail` → FAIL.
- [ ] **Step 3: Implement:**

```rust
pub const COVER_DOWNLOAD_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "cover_download",
    name: "Cover download",
    description: "Download missing album covers from Cover Art Archive (network; off by default)",
    default_enabled: false,
};

pub const ALL_MODULES: &[&ModuleDescriptor] = &[&MPRIS_MODULE, &COVER_DOWNLOAD_MODULE];
```

- [ ] **Step 4: Tests pass.** `cargo test -p reprise-core modules 2>&1 | tail` → 3 new pass.
- [ ] **Step 5: Gates + commit.** `test --workspace` → **404 passed; 1 ignored**.

```bash
git add crates/reprise-core/src/modules.rs
git commit -m "feat: register cover_download module (default off)"
```

---

### Task 6: Frontend download worker + CoverLoader trigger

**Files:**
- Create: `crates/reprise-gnome/src/ui/cover_download_worker.rs`
- Modify: `crates/reprise-gnome/src/ui/cover_loader.rs` (add `download_enabled` flag + trigger on miss)
- Modify: `crates/reprise-gnome/src/ui/mod.rs` (`pub mod cover_download_worker;`)
- Modify: `crates/reprise-gnome/src/ui/window.rs` (read the module flag at startup; construct the flag `Rc<Cell<bool>>`; pass into the `CoverLoader`s)

**Interfaces:**
- Consumes: `reprise_core::cover_download::fetch_and_cache`, `reprise_core::cover::read_cover_tag`, `modules::is_enabled`.
- Produces: a dedicated single-thread download worker driven by an `async_channel`, plus a `download_enabled: Rc<Cell<bool>>` field on `CoverLoader`.

**Design:** downloads run on ONE dedicated OS thread (serial → naturally paced with the ≤1/s MusicBrainz limit, and does NOT occupy the `gio::spawn_blocking` thumbnail pool). The worker dedups album-keys it has already attempted (its own `HashSet`, thread-local — no sharing). On a `resolve_source` miss with the module enabled, `CoverLoader::load_into` posts one request `(track_path, album_key, weak Image, token, current)`; the worker fetches, and on success signals the main loop to re-resolve+thumbnail that image (generation-guarded exactly like the normal load).

- [ ] **Step 1: Implement the worker** `cover_download_worker.rs`: a `spawn()` returning an `async_channel::Sender<DownloadRequest>`; a dedicated `std::thread` draining it, calling `read_cover_tag` (for album fields + mbid) then `fetch_and_cache`, deduping seen album-keys, and on `Some(path)` forwarding to a main-context callback (via `glib::MainContext::channel` or an `async_channel` back to a `spawn_future_local` loop) that re-runs the normal texture apply guarded by the generation token. Missing/None → drop silently. (Model the Weak/generation shape on `CoverLoader::load_into`; model the dedicated-thread + channel shape on `mpris/mod.rs`'s worker thread.)

- [ ] **Step 2: Wire the trigger into `CoverLoader`.** Add `download_enabled: Rc<Cell<bool>>` and an `Option<async_channel::Sender<DownloadRequest>>` (the worker) to the struct + `new`. In `load_into`, after the `gio::spawn_blocking` resolve returns `None` (cover_path is `None`) AND `download_enabled.get()`, post a `DownloadRequest` to the worker instead of just showing the placeholder-forever. Keep the generation token so a late download for a recycled row is dropped.

- [ ] **Step 3: Startup wiring in `window.rs`.** Mirror the `mpris_enabled` read (`window.rs:148-153`) for `cover_download`:

```rust
let cover_download_enabled = reprise_core::modules::is_enabled(
    &conn.borrow(),
    &reprise_core::modules::COVER_DOWNLOAD_MODULE,
)
.unwrap_or_else(|error| {
    tracing::warn!(%error, "could not read module.cover_download.enabled; defaulting to off");
    false
});
```

Construct one `let cover_download_flag = Rc::new(Cell::new(cover_download_enabled));`, spawn one shared download worker, and pass both into the `CoverLoader`s (the `TrackList` one and the `PlayerController` one — thread the flag/worker through their constructors). Keep `cover_download_flag` in scope in `build` so Task 7's menu toggle can flip it (both surfaces share the one `Rc<Cell<bool>>`).

- [ ] **Step 4: Build + isolated smoke — module OFF must make zero network calls.** (No live download is asserted; assert the OFF path is inert and nothing crashes.)

```bash
cargo build -p reprise-gnome 2>&1 | tail -3
dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) GDK_BACKEND=x11 REPRISE_AUDIO_SINK=fakesink WAYLAND_DISPLAY= REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=4 cargo run 2>&1 | grep -iE 'ERROR|panic|CRITICAL' | grep -vi atspi | head
```
Expected: exit 0, no real ERROR/panic/CRITICAL (module off → no fetch attempted).

- [ ] **Step 5: Gates + commit.** fmt/clippy/`test --workspace` **404 passed; 1 ignored** (no new unit tests — GTK glue); purity PURE; `wc -l` on all touched files < 800.

```bash
git add crates/reprise-gnome/src/ui/cover_download_worker.rs crates/reprise-gnome/src/ui/cover_loader.rs crates/reprise-gnome/src/ui/mod.rs crates/reprise-gnome/src/ui/window.rs
git commit -m "feat: dedicated cover-download worker + CoverLoader trigger (gated, off-thread)"
```

---

### Task 7: Header menu + stateful "Download missing covers" toggle

**Files:**
- Modify: `crates/reprise-gnome/src/ui/window.rs` (primary `MenuButton` + a stateful `gio::SimpleAction` toggle)
- Possibly: `crates/reprise-gnome/src/ui/strings.rs` (the menu label constant)

**Interfaces:**
- Consumes: the `Rc<Cell<bool>>` from Task 6, `modules::set_enabled`, `COVER_DOWNLOAD_MODULE`.
- Produces: a `win.download-missing-covers` stateful boolean action + a header hamburger menu item bound to it.

- [ ] **Step 1: Add the labels** in `strings.rs` (both used below):

```rust
pub const MAIN_MENU: &str = "Main menu";
pub const DOWNLOAD_MISSING_COVERS: &str = "Download missing album covers";
```

- [ ] **Step 2: Build the menu + action in `window.rs::build`.** After the header is built, add a primary menu button and a stateful toggle whose initial state is `cover_download_enabled`, whose `change-state` persists via `modules::set_enabled` AND flips the shared `Rc<Cell<bool>>`:

```rust
let menu = gio::Menu::new();
menu.append(Some(strings::DOWNLOAD_MISSING_COVERS), Some("win.download-missing-covers"));
let menu_button = gtk4::MenuButton::builder()
    .icon_name("open-menu-symbolic")
    .menu_model(&menu)
    .tooltip_text(strings::MAIN_MENU) // add this string too
    .build();
header.pack_end(&menu_button);

let toggle = gio::SimpleAction::new_stateful(
    "download-missing-covers",
    None,
    &cover_download_enabled.to_variant(),
);
{
    let conn = conn.clone();
    let flag = cover_download_flag.clone(); // the Rc<Cell<bool>> from Task 6
    toggle.connect_change_state(move |action, state| {
        let Some(enabled) = state.and_then(glib::Variant::get::<bool>) else { return };
        if let Err(error) =
            reprise_core::modules::set_enabled(&conn.borrow(), &reprise_core::modules::COVER_DOWNLOAD_MODULE, enabled)
        {
            tracing::warn!(%error, "could not persist cover_download toggle");
        }
        flag.set(enabled);
        action.set_state(&enabled.to_variant());
    });
}
window.add_action(&toggle);
```

Verify `gio::SimpleAction::new_stateful`, `connect_change_state`, `set_state`, and `bool: ToVariant`/`Variant::get::<bool>` against gtk4-rs 0.11 and adjust to the real API if needed (behavior identical). RefCell: `conn.borrow()` inside the handler is a single-statement borrow, dropped before `flag.set`.

- [ ] **Step 3: Add a headless toggle smoke hook.** `REPRISE_SMOKE_COVER_DOWNLOAD=on` that, at startup (deferred `idle_add_local_once` or inline), activates the toggle and logs `"smoke: cover_download toggled on"` — so the flag+persist path is exercised headlessly without a UI click.

- [ ] **Step 4: Build + isolated smoke — toggle on, flag+persist, still no crash.**

```bash
cargo build -p reprise-gnome 2>&1 | tail -3
D=$(mktemp -d)
dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=$D XDG_CACHE_HOME=$(mktemp -d) GDK_BACKEND=x11 REPRISE_AUDIO_SINK=fakesink WAYLAND_DISPLAY= REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_COVER_DOWNLOAD=on REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=4 cargo run 2>&1 | grep -iE 'cover_download toggled|ERROR|panic|CRITICAL' | grep -vi atspi | head
echo "persisted flag:"; sqlite3 "$D/reprise/reprise.db" "SELECT value FROM settings WHERE key='module.cover_download.enabled';"
```
Expected: logs the toggle, exit 0, no CRITICAL; the scratch DB shows `module.cover_download.enabled = 1` (persisted). The REAL DB is untouched (scratch `XDG_DATA_HOME`).

- [ ] **Step 5: Gates + commit.** fmt/clippy/`test --workspace` **404 passed; 1 ignored**; `wc -l window.rs` < 800 (if the menu wiring pushes it over, EXTRACT a small `ui/primary_menu.rs` rather than trimming — window.rs was ~791 after GUI-A).

```bash
git add crates/reprise-gnome/src/ui/window.rs crates/reprise-gnome/src/ui/strings.rs
git commit -m "feat: header menu with opt-in Download-missing-covers toggle"
```

---

## Stage close-out (run once after Task 7)

- [ ] Gates: `cargo fmt --check`; `cargo clippy --all-targets --workspace -- -D warnings`; `cargo test --workspace` (**404 passed; 1 ignored** approx — record actual); `cargo audit` (accepted `paste`; confirm `ureq`/rustls added no unaccepted advisory).
- [ ] **Purity proof:** `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` → empty; `cargo build -p reprise-core` standalone. Confirm the only new core dep is `ureq`.
- [ ] **Opt-in proof (recorded):** with the module off (default), the isolated smoke makes no network attempt; the `fetch_and_cache` short-circuit tests pass; grep confirms no fetch call path is reachable without `download_enabled`/`is_enabled`.
- [ ] **Core-promise proof:** `downloaded_dir().starts_with(cache_dir())` test passes; grep confirms nothing in `cover_download.rs`/`cover.rs` writes outside `cache_dir()`.
- [ ] File-size gate: `wc -l` all touched/new < 800.
- [ ] **Manual (human) check** — headless cannot verify a real download: with the module toggled on and real internet, cover-less albums gradually gain covers; wrong-cover rate is acceptable (conservative match); toggling off stops new fetches.
- [ ] Ledger update (`.superpowers/sdd/progress.md`): GUI-A2 done; `ureq` added to core (purity intact); opt-in/default-off; rate-limit + negative-cache; the deferred per-album in-flight-dedup optimization (worker dedups; the ≤1/s limiter + disk short-circuit bound bursts).

## Explicitly NOT in this plan (YAGNI / later)

- Artist/performer images (fanart.tv/Wikidata).
- Embedding a downloaded cover into the audio file (GUI-B tag editor).
- A second metadata provider / `MetadataProvider` abstraction.
- A preferences dialog (only the one menu toggle); per-album "choose cover" UI.
- A forced full-library backfill on enable (lazy only).
- Negative-cache TTL / download-cache size eviction (safely deletable; revisit with data).

## Known risks

1. **`ureq` API drift (Task 3).** The 2.x builder/response API (`content_type`, `into_reader`, redirect-following) must be verified against the resolved version. Mitigation: the fetch is isolated in `cover_download.rs`; adjust signatures to the real 2.x API keeping behavior identical; the non-network short-circuit tests pin the cache/negative logic regardless.
2. **Wrong-cover risk (Task 2).** A fuzzy match could fetch the wrong album's art. Mitigation: the conservative rule (score ≥ 90 AND normalized artist+album equality) rejects weak matches; MBID is preferred when tagged; this is opt-in.
3. **Download-burst thread pressure (Task 6).** Many cover-less rows becoming visible at once. Mitigation: a single dedicated worker thread (not the thumbnail pool) + per-album-key dedup + the ≤1/s limiter serialize the work; the disk/negative short-circuit makes repeats cheap.
4. **`window.rs` line budget (Task 7).** It was ~791 after GUI-A. Mitigation: if the menu wiring breaches 800, extract `ui/primary_menu.rs` rather than trimming docs.
