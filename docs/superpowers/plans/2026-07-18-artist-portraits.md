# Artist Portraits (Deezer) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show real artist portraits fetched from Deezer in the artist master list, the detail hero, and the info panel — with the existing initials-gradient circle as the always-present fallback.

**Architecture:** A new blocking `reprise-core::artist_portrait` module mirrors `artist_news` (own HTTP + rate throttle, on-disk cache with TTL + negative markers, injectable fetch fns for offline tests). A new `ArtistPortraitRuntime` GTK worker (copy of `ArtistNewsRuntime`) runs fetches off the UI thread and is gated by a new opt-in `ARTIST_PORTRAIT_MODULE` (default OFF). The gradient avatar stays as the base layer; a `gtk::Picture` overlay is revealed only when a portrait texture is loaded.

**Tech Stack:** Rust, gtk4-rs / libadwaita, `ureq` v3, `serde_json`, `image` crate, `async_channel`, SQLite (`settings` table via `modules`).

## Global Constraints

- Deezer API: `https://api.deezer.com/search/artist?q=<urlencoded>&limit=5`, no API key.
- Deezer HTTP must NOT go through `musicbrainz::get` (wrong UA + wrong 1 req/s limiter). Use an own getter + own throttle (~300 ms min interval).
- Network opt-in module, `default_enabled: false`. Nothing hits the network unless enabled.
- Cache under `cover::cache_dir()/artist-portraits` (i.e. `<XDG cache>/reprise/artist-portraits`), keyed by `cover::hash_hex(normalize(name).as_bytes())`.
- TTL: positive 30 days, negative 7 days. Atomic temp-file + rename for all writes.
- Name matching: accept a Deezer candidate ONLY on exact normalized-name equality (lowercased, whitespace-collapsed). Otherwise `NotFound`.
- Deezer default-silhouette image (URL path contains `/artist//`, i.e. empty md5) counts as `NotFound` — never downloaded.
- Max downloaded image 20 MB; must decode as a supported image (`image::guess_format`).
- Immutability, small focused files, TDD, frequent commits.
- `normalize(value)` everywhere = `value.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()` (identical to `artist_news::normalize` / `cover_download` `norm`).

---

## File Structure

**New (core):**
- `crates/reprise-core/src/artist_portrait/mod.rs` — public API + orchestration (`load_or_fetch`, `PortraitOutcome`, `PortraitError`, image validation).
- `crates/reprise-core/src/artist_portrait/deezer.rs` — Deezer client: search URL, JSON parse, placeholder detection, image download, rate throttle.
- `crates/reprise-core/src/artist_portrait/cache.rs` — on-disk cache: keying, positive/negative paths, TTL freshness, atomic store.

**New (gnome):**
- `crates/reprise-gnome/src/ui/library_views/artist_portrait_worker.rs` — `ArtistPortraitRuntime` (copy of `ArtistNewsRuntime`).

**Modified (core):**
- `crates/reprise-core/src/lib.rs` — register `pub mod artist_portrait;`.
- `crates/reprise-core/src/modules.rs` — add `ARTIST_PORTRAIT_MODULE`, append to `ALL_MODULES`.
- `crates/reprise-core/src/cover_download.rs` — make `validated_image_extension` + `MAX_IMAGE_BYTES` `pub(crate)` for reuse.

**Modified (gnome):**
- `crates/reprise-gnome/src/ui/library_views/mod.rs` (or the module's `mod` file) — register `artist_portrait_worker`.
- `crates/reprise-gnome/src/ui/preferences/preference_plugins.rs` — add `artist_portrait` to `plugin_applies_live`.
- `crates/reprise-gnome/src/ui/library_views/artist_detail_hero.rs` — portrait overlay + request on `update`.
- `crates/reprise-gnome/src/ui/library_views/artist_master_row.rs` — portrait overlay + prefetch on bind.
- `crates/reprise-gnome/src/ui/library_views/artist_view_css.rs` — circular Picture CSS class.
- `crates/reprise-gnome/src/ui/info_panel/…` — portrait in artist info section.
- Wiring/owner structs that construct the runtimes and view builders (see Tasks 5–8, filled from the wiring map).

---

## Task 1: Deezer client (`deezer.rs`) — search URL, parse, placeholder, throttle

**Files:**
- Create: `crates/reprise-core/src/artist_portrait/deezer.rs`
- Create (empty stub this task): `crates/reprise-core/src/artist_portrait/mod.rs` with `pub(crate) mod deezer;` and `pub(crate) mod cache;` (add `cache` in Task 2) — for now just `pub(crate) mod deezer;`
- Modify: `crates/reprise-core/src/lib.rs` — add `pub mod artist_portrait;`

**Interfaces:**
- Produces:
  - `pub(crate) struct DeezerArtist { pub name: String, pub picture_url: Option<String> }`
  - `pub(crate) fn search_url(name: &str) -> String`
  - `pub(crate) fn parse_best_artist(json: &str, name: &str) -> Option<DeezerArtist>` — exact normalized-name match; `picture_url` is `None` when the best `picture_xl` is Deezer's placeholder.
  - `pub(crate) fn search(name: &str) -> Result<String, crate::musicbrainz::FetchError>` — rate-throttled GET returning body text.
  - `pub(crate) fn download_image(url: &str) -> Result<Vec<u8>, crate::musicbrainz::FetchError>` — GET raw bytes (size-capped).

- [ ] **Step 1: Write the failing tests**

```rust
// at bottom of deezer.rs
#[cfg(test)]
mod tests {
    use super::*;

    const HIT: &str = r#"{"data":[
      {"id":1,"name":"Blessthefall","picture_xl":"https://e-cdns-images.dzcdn.net/images/artist/abc123/1000x1000-000000-80-0-0.jpg"}
    ],"total":1}"#;

    const PLACEHOLDER: &str = r#"{"data":[
      {"id":2,"name":"Before I Turn","picture_xl":"https://e-cdns-images.dzcdn.net/images/artist//1000x1000-000000-80-0-0.jpg"}
    ],"total":1}"#;

    const WRONG_NAME: &str = r#"{"data":[
      {"id":3,"name":"Blessthefall (Tribute)","picture_xl":"https://e-cdns-images.dzcdn.net/images/artist/def/1000x1000-000000-80-0-0.jpg"}
    ],"total":1}"#;

    #[test]
    fn search_url_encodes_query() {
        let url = search_url("Bring Me The Horizon");
        assert!(url.starts_with("https://api.deezer.com/search/artist?q="));
        assert!(url.contains("limit=5"));
        assert!(!url.contains(' '));
    }

    #[test]
    fn parse_accepts_exact_normalized_match_with_picture() {
        let artist = parse_best_artist(HIT, "  blessthefall ").unwrap();
        assert_eq!(artist.name, "Blessthefall");
        assert_eq!(
            artist.picture_url.as_deref(),
            Some("https://e-cdns-images.dzcdn.net/images/artist/abc123/1000x1000-000000-80-0-0.jpg")
        );
    }

    #[test]
    fn parse_treats_deezer_placeholder_as_no_picture() {
        let artist = parse_best_artist(PLACEHOLDER, "Before I Turn").unwrap();
        assert!(artist.picture_url.is_none());
    }

    #[test]
    fn parse_rejects_non_exact_name() {
        assert!(parse_best_artist(WRONG_NAME, "Blessthefall").is_none());
    }

    #[test]
    fn parse_handles_empty_and_garbage() {
        assert!(parse_best_artist(r#"{"data":[]}"#, "X").is_none());
        assert!(parse_best_artist("not json", "X").is_none());
    }

    #[test]
    fn is_placeholder_detects_empty_md5_segment() {
        assert!(is_placeholder_url("https://e-cdns-images.dzcdn.net/images/artist//1000x1000-000000-80-0-0.jpg"));
        assert!(!is_placeholder_url("https://e-cdns-images.dzcdn.net/images/artist/abc/1000x1000-000000-80-0-0.jpg"));
    }
}
```

- [ ] **Step 2: Run the tests, verify they fail**

Run: `cargo test -p reprise-core artist_portrait::deezer -- --nocapture`
Expected: FAIL (module/functions not defined).

- [ ] **Step 3: Implement `deezer.rs`**

```rust
//! Deezer public-API artist portrait client. Blocking; worker-thread only.
//! Own rate throttle and user-agent — deliberately NOT routed through
//! `musicbrainz::get` (that would apply the MusicBrainz UA and 1 req/s limit).

use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::musicbrainz::{self, FetchError};

const HTTP_TIMEOUT: Duration = Duration::from_secs(15);
const MIN_REQUEST_INTERVAL: Duration = Duration::from_millis(300);
const MAX_IMAGE_BYTES: u64 = crate::cover_download::MAX_IMAGE_BYTES;

static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);

pub(crate) struct DeezerArtist {
    pub name: String,
    pub picture_url: Option<String>,
}

pub(crate) fn search_url(name: &str) -> String {
    format!(
        "https://api.deezer.com/search/artist?q={}&limit=5",
        musicbrainz::urlencode(name.trim())
    )
}

/// Best exact-name match; `picture_url` is `None` for Deezer's placeholder image.
pub(crate) fn parse_best_artist(json: &str, name: &str) -> Option<DeezerArtist> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let data = value.get("data")?.as_array()?;
    let wanted = normalize(name);
    for candidate in data {
        let cand_name = candidate.get("name").and_then(serde_json::Value::as_str)?;
        if normalize(cand_name) != wanted {
            continue;
        }
        let picture = candidate
            .get("picture_xl")
            .or_else(|| candidate.get("picture_big"))
            .and_then(serde_json::Value::as_str)
            .filter(|url| !url.is_empty() && !is_placeholder_url(url))
            .map(str::to_owned);
        return Some(DeezerArtist {
            name: cand_name.to_string(),
            picture_url: picture,
        });
    }
    None
}

/// Deezer serves a default silhouette when an artist has no photo; its URL has
/// an empty md5 path segment (`/images/artist//…`).
fn is_placeholder_url(url: &str) -> bool {
    url.contains("/artist//")
}

/// GET a prebuilt Deezer URL (caller builds it via `search_url`) as text.
pub(crate) fn search(url: &str) -> Result<String, FetchError> {
    respect_rate_limit();
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(&musicbrainz::user_agent())
        .build()
        .new_agent();
    let mut response = agent.get(url).call().map_err(map_ureq_error)?;
    response.body_mut().read_to_string().map_err(|_| FetchError::Body)
}

pub(crate) fn download_image(url: &str) -> Result<Vec<u8>, FetchError> {
    respect_rate_limit();
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(HTTP_TIMEOUT))
        .user_agent(&musicbrainz::user_agent())
        .build()
        .new_agent();
    let response = agent.get(url).call().map_err(map_ureq_error)?;
    let mut bytes = Vec::new();
    use std::io::Read;
    response
        .into_body()
        .into_reader()
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| FetchError::Body)?;
    if bytes.len() as u64 > MAX_IMAGE_BYTES {
        return Err(FetchError::Body);
    }
    Ok(bytes)
}

fn map_ureq_error(error: ureq::Error) -> FetchError {
    match error {
        ureq::Error::StatusCode(status) => FetchError::HttpStatus(status),
        ureq::Error::Timeout(_) => FetchError::Timeout,
        _ => FetchError::Transport,
    }
}

fn respect_rate_limit() {
    let mut guard = LAST_REQUEST.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(last) = *guard {
        let elapsed = last.elapsed();
        if elapsed < MIN_REQUEST_INTERVAL {
            std::thread::sleep(MIN_REQUEST_INTERVAL - elapsed);
        }
    }
    *guard = Some(Instant::now());
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
```

Note: verify the exact `ureq` v3 body-read API against `cover_download.rs::http_get_bytes` (which uses `response.into_body().into_reader()`); mirror it. For the search text read, if `body_mut().read_to_string()` differs in this ureq version, use the same `into_body().into_reader()` + `read_to_string` shape as elsewhere in the crate. Also add `use crate::musicbrainz::FetchError;` mapping — confirm `FetchError` variants (`Timeout`, `Transport`, `HttpStatus(u16)`, `Body`) in `musicbrainz.rs`.

- [ ] **Step 4: Run the tests, verify they pass**

Run: `cargo test -p reprise-core artist_portrait::deezer`
Expected: PASS (5+ tests).

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/artist_portrait/ crates/reprise-core/src/lib.rs
git commit -m "feat(core): add Deezer artist search client + parsing"
```

---

## Task 2: On-disk cache (`cache.rs`) — keying, paths, TTL, atomic store

**Files:**
- Create: `crates/reprise-core/src/artist_portrait/cache.rs`
- Modify: `crates/reprise-core/src/artist_portrait/mod.rs` — add `pub(crate) mod cache;`

**Interfaces:**
- Produces:
  - `pub(crate) fn cache_dir() -> std::path::PathBuf`
  - `pub(crate) fn key_for(name: &str) -> String`
  - `pub(crate) fn portrait_path_in(dir: &Path, name: &str) -> Option<PathBuf>` (existing image, any known ext)
  - `pub(crate) fn negative_marker_path(dir: &Path, name: &str) -> PathBuf`
  - `pub(crate) fn is_fresh(fetched_at: i64, now: i64, positive: bool) -> bool`
  - `pub(crate) fn file_epoch_secs(path: &Path) -> i64` (mtime → unix secs; 0 on error)
  - `pub(crate) fn store_image(dir: &Path, name: &str, bytes: &[u8], ext: &str) -> Option<PathBuf>`
  - `pub(crate) fn write_negative(dir: &Path, name: &str)`
  - `pub(crate) const IMAGE_EXTS: &[&str]`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_normalizes_case_and_whitespace() {
        assert_eq!(key_for("Bring Me The Horizon"), key_for("  bring  me the  horizon "));
    }

    #[test]
    fn cache_dir_is_under_xdg_cache_reprise() {
        assert!(cache_dir().ends_with("reprise/artist-portraits"));
    }

    #[test]
    fn portrait_path_finds_existing_and_none_otherwise() {
        let dir = std::env::temp_dir().join(format!("rp-portrait-{}", fastrand::u64(..)));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(portrait_path_in(&dir, "Solo").is_none());
        let f = dir.join(format!("{}.jpg", key_for("Solo")));
        std::fs::write(&f, b"x").unwrap();
        assert_eq!(portrait_path_in(&dir, "Solo"), Some(f));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn positive_ttl_is_30_days_negative_7_days() {
        let day = 24 * 60 * 60;
        assert!(is_fresh(1_000, 1_000 + 29 * day, true));
        assert!(!is_fresh(1_000, 1_000 + 31 * day, true));
        assert!(is_fresh(1_000, 1_000 + 6 * day, false));
        assert!(!is_fresh(1_000, 1_000 + 8 * day, false));
    }

    #[test]
    fn store_image_publishes_atomically_and_write_negative_marks() {
        let dir = std::env::temp_dir().join(format!("rp-portrait-{}", fastrand::u64(..)));
        let stored = store_image(&dir, "Band", b"img", "jpg").unwrap();
        assert!(stored.exists());
        assert_eq!(portrait_path_in(&dir, "Band"), Some(stored));
        write_negative(&dir, "Missing");
        assert!(negative_marker_path(&dir, "Missing").exists());
        std::fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p reprise-core artist_portrait::cache`
Expected: FAIL.

- [ ] **Step 3: Implement `cache.rs`**

```rust
//! On-disk artist-portrait cache under the XDG cover cache. Positive entries
//! are the image file `<key>.<ext>`; a `<key>.notfound` marker records a miss.
//! TTL is derived from the file mtime.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

const POSITIVE_TTL_SECONDS: i64 = 30 * 24 * 60 * 60;
const NEGATIVE_TTL_SECONDS: i64 = 7 * 24 * 60 * 60;

pub(crate) fn cache_dir() -> PathBuf {
    crate::cover::cache_dir().join("artist-portraits")
    // NOTE: cover::cache_dir() == <XDG cache>/reprise/covers, so this yields
    // <XDG cache>/reprise/covers/artist-portraits. If a sibling of `covers`
    // is preferred, use dirs::cache_dir()/"reprise/artist-portraits" instead —
    // pick one; the test `cache_dir_is_under_xdg_cache_reprise` only asserts the
    // suffix `reprise/artist-portraits`, so use the dirs form to satisfy it:
}

pub(crate) fn key_for(name: &str) -> String {
    crate::cover::hash_hex(normalize(name).as_bytes())
}

pub(crate) fn portrait_path_in(dir: &Path, name: &str) -> Option<PathBuf> {
    let key = key_for(name);
    IMAGE_EXTS
        .iter()
        .map(|ext| dir.join(format!("{key}.{ext}")))
        .find(|p| p.exists())
}

pub(crate) fn negative_marker_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{}.notfound", key_for(name)))
}

pub(crate) fn is_fresh(fetched_at: i64, now: i64, positive: bool) -> bool {
    let age = now.saturating_sub(fetched_at).max(0);
    let ttl = if positive { POSITIVE_TTL_SECONDS } else { NEGATIVE_TTL_SECONDS };
    age <= ttl
}

pub(crate) fn file_epoch_secs(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub(crate) fn store_image(dir: &Path, name: &str, bytes: &[u8], ext: &str) -> Option<PathBuf> {
    std::fs::create_dir_all(dir).ok()?;
    let key = key_for(name);
    let out = dir.join(format!("{key}.{ext}"));
    let tmp = dir.join(format!(".{key}-{}.{ext}.tmp", fastrand::u64(..)));
    if std::fs::write(&tmp, bytes).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return None;
    }
    if std::fs::rename(&tmp, &out).is_err() {
        let _ = std::fs::remove_file(&tmp);
        return portrait_path_in(dir, name);
    }
    Some(out)
}

pub(crate) fn write_negative(dir: &Path, name: &str) {
    let _ = std::fs::create_dir_all(dir);
    let _ = std::fs::write(negative_marker_path(dir, name), b"");
}

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
```

IMPORTANT: implement `cache_dir()` as `dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join("reprise/artist-portraits")` (the `artist_news.rs` shape) so it is a sibling of `covers`/`artist-news`, matching the test's suffix assertion. Remove the `cover::cache_dir()` variant from the note above.

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p reprise-core artist_portrait::cache`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/artist_portrait/
git commit -m "feat(core): add artist-portrait on-disk cache with TTL"
```

---

## Task 3: Orchestration (`mod.rs`) — `load_or_fetch` + image validation

**Files:**
- Modify: `crates/reprise-core/src/artist_portrait/mod.rs`
- Modify: `crates/reprise-core/src/cover_download.rs` — change `fn validated_image_extension` and `const MAX_IMAGE_BYTES` to `pub(crate)`.

**Interfaces:**
- Consumes: `deezer::{search, download_image, parse_best_artist}`, `cache::*`, `cover_download::validated_image_extension`.
- Produces:
  - `pub enum PortraitOutcome { Found(std::path::PathBuf), NotFound }`
  - `pub enum PortraitError { Fetch(crate::musicbrainz::FetchError), InvalidResponse }` (derive `thiserror::Error`)
  - `pub fn load_or_fetch(name: &str, force: bool) -> Result<PortraitOutcome, PortraitError>`
  - `pub(crate) fn load_or_fetch_with<S, D>(name, force, now, dir, search, download) -> Result<PortraitOutcome, PortraitError>`
    where `S: FnMut(&str) -> Result<String, FetchError>`, `D: FnMut(&str) -> Result<Vec<u8>, FetchError>`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::musicbrainz::FetchError;

    fn png_bytes() -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgb8(1, 1)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    const HIT: &str = r#"{"data":[{"id":1,"name":"Band","picture_xl":"https://e-cdns-images.dzcdn.net/images/artist/abc/1000x1000-000000-80-0-0.jpg"}]}"#;
    const PLACEHOLDER: &str = r#"{"data":[{"id":2,"name":"Band","picture_xl":"https://e-cdns-images.dzcdn.net/images/artist//1000x1000-000000-80-0-0.jpg"}]}"#;

    fn tmp() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("rp-portrait-mod-{}", fastrand::u64(..)))
    }

    #[test]
    fn found_downloads_and_stores_image() {
        let dir = tmp();
        let mut search = |_: &str| Ok(HIT.to_string());
        let mut download = |_: &str| Ok(png_bytes());
        let out = load_or_fetch_with("Band", false, 1_000, &dir, &mut search, &mut download).unwrap();
        match out {
            PortraitOutcome::Found(p) => assert!(p.exists()),
            _ => panic!("expected Found"),
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn placeholder_is_notfound_and_writes_marker_without_download() {
        let dir = tmp();
        let mut search = |_: &str| Ok(PLACEHOLDER.to_string());
        let mut download = |_: &str| -> Result<Vec<u8>, FetchError> { panic!("must not download") };
        let out = load_or_fetch_with("Band", false, 1_000, &dir, &mut search, &mut download).unwrap();
        assert!(matches!(out, PortraitOutcome::NotFound));
        assert!(cache::negative_marker_path(&dir, "Band").exists());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fresh_cached_image_short_circuits_without_fetch() {
        let dir = tmp();
        cache::store_image(&dir, "Band", b"img", "jpg").unwrap();
        let now = cache::file_epoch_secs(&cache::portrait_path_in(&dir, "Band").unwrap());
        let mut search = |_: &str| -> Result<String, FetchError> { panic!("must not search") };
        let mut download = |_: &str| -> Result<Vec<u8>, FetchError> { panic!("must not download") };
        let out = load_or_fetch_with("Band", false, now, &dir, &mut search, &mut download).unwrap();
        assert!(matches!(out, PortraitOutcome::Found(_)));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn fresh_negative_marker_short_circuits() {
        let dir = tmp();
        cache::write_negative(&dir, "Band");
        let now = cache::file_epoch_secs(&cache::negative_marker_path(&dir, "Band"));
        let mut search = |_: &str| -> Result<String, FetchError> { panic!("must not search") };
        let mut download = |_: &str| -> Result<Vec<u8>, FetchError> { panic!("must not download") };
        let out = load_or_fetch_with("Band", false, now, &dir, &mut search, &mut download).unwrap();
        assert!(matches!(out, PortraitOutcome::NotFound));
        std::fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 2: Run tests, verify they fail**

Run: `cargo test -p reprise-core artist_portrait::mod` (or `artist_portrait::tests`)
Expected: FAIL.

- [ ] **Step 3: Implement `mod.rs` + widen `cover_download` visibility**

In `cover_download.rs`: `pub(crate) const MAX_IMAGE_BYTES: u64 = …;` and `pub(crate) fn validated_image_extension(…)`.

```rust
//! Cached artist portraits from Deezer. Blocking; call from a worker thread.

pub(crate) mod cache;
pub(crate) mod deezer;

use std::path::{Path, PathBuf};

use crate::musicbrainz::FetchError;

#[derive(Debug)]
pub enum PortraitOutcome {
    Found(PathBuf),
    NotFound,
}

#[derive(Debug, thiserror::Error)]
pub enum PortraitError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("Deezer response was invalid")]
    InvalidResponse,
}

pub fn load_or_fetch(name: &str, force: bool) -> Result<PortraitOutcome, PortraitError> {
    let now = chrono::Utc::now().timestamp();
    let dir = cache::cache_dir();
    load_or_fetch_with(name, force, now, &dir, &mut deezer::search, &mut deezer::download_image)
}

pub(crate) fn load_or_fetch_with<S, D>(
    name: &str,
    force: bool,
    now: i64,
    dir: &Path,
    search: &mut S,
    download: &mut D,
) -> Result<PortraitOutcome, PortraitError>
where
    S: FnMut(&str) -> Result<String, FetchError>,
    D: FnMut(&str) -> Result<Vec<u8>, FetchError>,
{
    let name = name.trim();
    if name.is_empty() {
        return Ok(PortraitOutcome::NotFound);
    }

    if !force {
        if let Some(path) = cache::portrait_path_in(dir, name) {
            if cache::is_fresh(cache::file_epoch_secs(&path), now, true) {
                return Ok(PortraitOutcome::Found(path));
            }
        }
        let marker = cache::negative_marker_path(dir, name);
        if marker.exists() && cache::is_fresh(cache::file_epoch_secs(&marker), now, false) {
            return Ok(PortraitOutcome::NotFound);
        }
    }

    let body = search(&deezer::search_url(name))?;
    let Some(artist) = deezer::parse_best_artist(&body, name) else {
        cache::write_negative(dir, name);
        return Ok(PortraitOutcome::NotFound);
    };
    let Some(url) = artist.picture_url else {
        cache::write_negative(dir, name);
        return Ok(PortraitOutcome::NotFound);
    };
    let bytes = download(&url)?;
    let Some(ext) = crate::cover_download::validated_image_extension(&bytes) else {
        cache::write_negative(dir, name);
        return Ok(PortraitOutcome::NotFound);
    };
    match cache::store_image(dir, name, &bytes, ext) {
        Some(path) => Ok(PortraitOutcome::Found(path)),
        None => Err(PortraitError::InvalidResponse),
    }
}
```

Note: `search` and `download` are injected as `&mut deezer::search` / `&mut deezer::download_image`; `load_or_fetch_with` builds the URL via `deezer::search_url(name)` and passes it to the injected `search`, matching Task 1's `search(url: &str)` signature.

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p reprise-core artist_portrait`
Expected: PASS (all deezer + cache + mod tests).

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/artist_portrait/ crates/reprise-core/src/cover_download.rs
git commit -m "feat(core): orchestrate artist-portrait fetch, cache, and validation"
```

---

## Task 4: Register the opt-in module

**Files:**
- Modify: `crates/reprise-core/src/modules.rs`
- Modify: `crates/reprise-gnome/src/ui/preferences/preference_plugins.rs`

**Interfaces:**
- Produces: `pub const ARTIST_PORTRAIT_MODULE: ModuleDescriptor` with `id: "artist_portrait"`, appended to `ALL_MODULES`.

- [ ] **Step 1: Write the failing test** (append to `modules.rs` tests)

```rust
#[test]
fn artist_portrait_is_listed_and_defaults_off() {
    let conn = migrated_conn();
    assert!(ALL_MODULES.iter().any(|m| m.id == ARTIST_PORTRAIT_MODULE.id));
    assert!(!is_enabled(&conn, &ARTIST_PORTRAIT_MODULE).unwrap());
    assert_eq!(enabled_key(&ARTIST_PORTRAIT_MODULE), "module.artist_portrait.enabled");
}
```

- [ ] **Step 2: Run test, verify it fails**

Run: `cargo test -p reprise-core modules::tests::artist_portrait_is_listed_and_defaults_off`
Expected: FAIL (const not defined).

- [ ] **Step 3: Implement**

In `modules.rs`, after `ARTIST_NEWS_MODULE`:

```rust
pub const ARTIST_PORTRAIT_MODULE: ModuleDescriptor = ModuleDescriptor {
    id: "artist_portrait",
    name: "Artist Portraits",
    description: "Show artist photos fetched from Deezer (network; off by default)",
    default_enabled: false,
};
```

Update `ALL_MODULES`:

```rust
pub const ALL_MODULES: &[&ModuleDescriptor] = &[
    &ARTIST_NEWS_MODULE,
    &ARTIST_PORTRAIT_MODULE,
    &LISTENBRAINZ_MODULE,
    &LASTFM_MODULE,
];
```

In `preference_plugins.rs`, add `artist_portrait` to the live-applies set:

```rust
pub(in crate::ui) fn plugin_applies_live(id: &str) -> bool {
    matches!(id, "artist_news" | "artist_portrait" | "listenbrainz" | "lastfm")
}
```

(The Plugins page renders the SwitchRow from `descriptor.name`/`descriptor.description` automatically; no `strings` entry is required for a non-scrobbling module — the `plugin_title`/`plugin_description` fallbacks return `descriptor.name`/`descriptor.description`.)

- [ ] **Step 4: Run tests, verify they pass**

Run: `cargo test -p reprise-core modules`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-core/src/modules.rs crates/reprise-gnome/src/ui/preferences/preference_plugins.rs
git commit -m "feat: register Artist Portraits opt-in module (default off)"
```

---

## Task 5: `ArtistPortraitRuntime` worker + construction + preferences toggle

Deliverable: the "Artist Portraits" switch appears on the Plugins page and persists; the off-UI-thread worker exists. No portraits are displayed yet.

**Files:**
- Create: `crates/reprise-gnome/src/ui/info_panel/artist_portrait_worker.rs`
- Modify: `crates/reprise-gnome/src/ui/info_panel/mod.rs` — declare `pub(in crate::ui) mod artist_portrait_worker;` next to `artist_news_worker`.
- Modify: `crates/reprise-gnome/src/ui/mod.rs` — re-export/alias mirroring `artist_news_worker` (see `ui/mod.rs:92-93`) so `super::artist_portrait_worker` resolves from `window.rs`.
- Modify: `crates/reprise-gnome/src/ui/window/window.rs:170` — construct the runtime.
- Modify: `crates/reprise-gnome/src/ui/preferences/preferences.rs` — add field + constructor param + toggle branch + subscribe branch.

**Interfaces:**
- Produces:
  - `pub(in crate::ui) struct ArtistPortraitRequest { pub generation: u64, pub artist: String, pub force: bool, pub response: async_channel::Sender<ArtistPortraitResponse> }`
  - `pub(in crate::ui) struct ArtistPortraitResponse { pub generation: u64, pub artist: String, pub result: Result<reprise_core::artist_portrait::PortraitOutcome, reprise_core::artist_portrait::PortraitError> }`
  - `pub(in crate::ui) struct ArtistPortraitRuntime` with `pub enabled: Rc<Cell<bool>>` and methods `setup(&Connection) -> Rc<Self>`, `set_enabled(&Connection, bool)`, `subscribe_enabled(is_alive, callback)`, `request(ArtistPortraitRequest)` — identical shapes to `ArtistNewsRuntime`.

- [ ] **Step 1: Create `artist_portrait_worker.rs` by copying `artist_news_worker.rs`**

Copy `crates/reprise-gnome/src/ui/info_panel/artist_news_worker.rs` verbatim, then apply these substitutions:
- `ArtistNewsRequest` → `ArtistPortraitRequest`; drop the `local_albums` field.
- `ArtistNewsResponse` → `ArtistPortraitResponse`; change `result` type to `Result<PortraitOutcome, PortraitError>`; add `pub artist: String`.
- `ArtistNewsRuntime` → `ArtistPortraitRuntime`.
- `ARTIST_NEWS_MODULE` → `ARTIST_PORTRAIT_MODULE` (two sites: `setup`, `set_enabled`).
- Thread name `"reprise-artist-news"` → `"reprise-artist-portrait"`.
- The `spawn()` body becomes:

```rust
fn spawn() -> async_channel::Sender<ArtistPortraitRequest> {
    let (sender, receiver) = async_channel::unbounded::<ArtistPortraitRequest>();
    let result = std::thread::Builder::new()
        .name("reprise-artist-portrait".into())
        .spawn(move || {
            while let Ok(request) = receiver.recv_blocking() {
                let result =
                    reprise_core::artist_portrait::load_or_fetch(&request.artist, request.force);
                let _ = request.response.try_send(ArtistPortraitResponse {
                    generation: request.generation,
                    artist: request.artist,
                    result,
                });
            }
        });
    if let Err(error) = result {
        tracing::warn!(%error, "could not start Artist Portrait worker");
    }
    sender
}
```

- Imports at top: `use reprise_core::artist_portrait::{PortraitError, PortraitOutcome};` (replace the `artist_news` import).
- Keep the `EnabledSubscriber(s)` machinery unchanged.
- Copy the three `#[cfg(test)]` tests, renaming types and using `ARTIST_PORTRAIT_MODULE`.

- [ ] **Step 2: Declare the module + re-export**

In `info_panel/mod.rs`, next to the `artist_news_worker` declaration, add `pub(in crate::ui) mod artist_portrait_worker;`. In `ui/mod.rs` mirror the existing `artist_news_worker` re-export line so `super::artist_portrait_worker::ArtistPortraitRuntime` resolves from `window.rs`.

- [ ] **Step 3: Run the worker unit tests**

Run: `cargo test -p reprise-gnome artist_portrait_worker`
Expected: PASS (3 tests: defaults off, activation persists, dead subscriber removed).

- [ ] **Step 4: Construct the runtime + wire the preferences toggle**

- In `window.rs` after line 170 (`let artist_news = …setup(…)`):

```rust
let artist_portrait = super::artist_portrait_worker::ArtistPortraitRuntime::setup(&conn.borrow());
```

- In `preferences.rs`: add field `artist_portrait: Rc<ArtistPortraitRuntime>` (near `artist_news: Rc<ArtistNewsRuntime>`, `preferences.rs:115`); add constructor param `artist_portrait: &Rc<ArtistPortraitRuntime>` to `PreferencesContext::new` (`preferences.rs:124`), storing `artist_portrait: artist_portrait.clone()`.
- At the `window.rs` `PreferencesContext::new` call (`window.rs:475-493`), pass `&artist_portrait`.
- In the Plugins toggle handler (`preferences.rs:684-688`), add a branch mirroring artist_news:

```rust
} else if descriptor.id == "artist_portrait" {
    if let Err(error) = context.artist_portrait.set_enabled(&context.conn.borrow(), active) {
        tracing::warn!(%error, "could not persist Artist Portraits toggle");
    }
}
```

- In the subscribe block (`preferences.rs:703-711`), add an `artist_portrait` branch mirroring artist_news so the switch reflects live state:

```rust
} else if descriptor.id == "artist_portrait" {
    let row = row.downgrade();
    self.artist_portrait.subscribe_enabled(
        move || row.upgrade().is_some(),
        move |enabled| {
            if let Some(row) = row.upgrade() {
                row.set_active(enabled);
            }
        },
    );
}
```

(Match the exact closure shapes already used for `artist_news` at those lines — copy them and swap the runtime.)

- [ ] **Step 5: Build + verify the toggle**

Run: `cargo build -p reprise-gnome`
Expected: compiles with no warnings about `artist_portrait` being unused.

Headless smoke (per the gtk4 skill's triple-isolated harness — see `building-gtk4-rust-apps`): launch the app, open Preferences → Plugins, confirm an "Artist Portraits" row exists and toggling it persists across relaunch. If a preferences integration test exists, add one asserting the row is present; otherwise flag for the human tester.

- [ ] **Step 6: Commit**

```bash
git add crates/reprise-gnome/src/ui/info_panel/ crates/reprise-gnome/src/ui/mod.rs crates/reprise-gnome/src/ui/window/window.rs crates/reprise-gnome/src/ui/preferences/preferences.rs
git commit -m "feat(gnome): add ArtistPortraitRuntime worker + Plugins toggle"
```

---

## Task 6: Circular portrait CSS + detail-hero display

Deliverable: opening an artist with the module ON shows a real portrait in the 132px hero avatar; gradient shows while loading / when disabled / on miss.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/library_views/artist_view_css.rs` — add a reusable circular-image class.
- Modify: `crates/reprise-gnome/src/ui/library_views/artist_detail_hero.rs` — Picture overlay + `set_portrait`/`clear_portrait`.
- Modify: `crates/reprise-gnome/src/ui/library_views/artist_detail_pane.rs` — thread runtime, request on `show_artist`, reveal on `Found`.
- Modify: `crates/reprise-gnome/src/ui/library_views/artist_view.rs` — add runtime param to `ArtistView::new` and pass to the pane.
- Modify: `crates/reprise-gnome/src/ui/window/window.rs:340-343` — pass `artist_portrait.clone()` into `ArtistView::new`.

**Interfaces:**
- Consumes: `ArtistPortraitRuntime`, `ArtistPortraitRequest/Response`, `PortraitOutcome`.
- Produces: `Hero::set_portrait(&gdk::Texture)`, `Hero::clear_portrait()`; `ArtistView::new(conn, cover_loader, portraits: Rc<ArtistPortraitRuntime>)`.

- [ ] **Step 1: Add the circular-image CSS class**

In `artist_view_css.rs`, register a class reused by hero + rows + info panel. The class must clip its content to a circle:

```css
.artist-portrait-image {
    border-radius: 9999px;
    background-color: transparent;
}
```

Add it alongside the existing avatar classes (follow the file's existing registration style — string appended to the same stylesheet the module installs). The widget-side `set_overflow(gtk4::Overflow::Hidden)` does the actual clipping (set in code, Step 2).

- [ ] **Step 2: Add the Picture overlay to the Hero**

In `artist_detail_hero.rs`:
- Add `portrait: gtk4::Picture` to `struct Hero` (`:25-33`).
- Rewrite `build_avatar()` to wrap the gradient box + a hidden Picture in an `Overlay`:

```rust
/// Round gradient avatar with an initials label, plus a portrait Picture
/// overlaid on top (hidden until a portrait texture is set).
fn build_avatar() -> (gtk4::Widget, gtk4::Box, gtk4::Label, gtk4::Picture) {
    let avatar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    avatar.add_css_class("artist-hero-avatar");
    avatar.set_size_request(AVATAR_SIZE, AVATAR_SIZE);
    avatar.set_halign(gtk4::Align::Center);
    avatar.set_valign(gtk4::Align::Center);

    let initials = gtk4::Label::new(None);
    initials.set_halign(gtk4::Align::Center);
    initials.set_valign(gtk4::Align::Center);
    initials.add_css_class("artist-hero-initials");
    avatar.append(&initials);

    let portrait = gtk4::Picture::new();
    portrait.set_content_fit(gtk4::ContentFit::Cover);
    portrait.set_size_request(AVATAR_SIZE, AVATAR_SIZE);
    portrait.set_halign(gtk4::Align::Center);
    portrait.set_valign(gtk4::Align::Center);
    portrait.set_overflow(gtk4::Overflow::Hidden);
    portrait.add_css_class("artist-portrait-image");
    portrait.set_visible(false);

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&avatar));
    overlay.add_overlay(&portrait);
    overlay.set_halign(gtk4::Align::Center);
    overlay.set_valign(gtk4::Align::Center);

    (overlay.upcast(), avatar, initials, portrait)
}
```

- In `build_hero` (`:76`), update the call site: `let (avatar_container, avatar, initials, portrait) = build_avatar();` then `content.append(&avatar_container);` (replaces `content.append(&avatar)`), and store `portrait` in the `Hero { … }` literal.
- Add methods:

```rust
impl Hero {
    pub(in crate::ui) fn set_portrait(&self, texture: &gtk4::gdk::Texture) {
        self.portrait.set_paintable(Some(texture));
        self.portrait.set_visible(true);
    }

    pub(in crate::ui) fn clear_portrait(&self) {
        self.portrait.set_paintable(gtk4::gdk::Paintable::NONE);
        self.portrait.set_visible(false);
    }
}
```

- In `Hero::update` (`:48-58`), call `self.clear_portrait();` at the end so every artist switch reverts to the gradient until (and unless) a portrait arrives.

- [ ] **Step 3: Thread the runtime into the pane and request on `show_artist`**

- `artist_view.rs:39`: change to `fn new(conn: Rc<RefCell<Connection>>, cover_loader: Rc<CoverLoader>, portraits: Rc<ArtistPortraitRuntime>) -> Self` and pass `portraits` into `ArtistDetailPane::new(conn, cover_loader, portraits)`.
- `window.rs:340-343`: pass `artist_portrait.clone()` as the third arg.
- `artist_detail_pane.rs:91` `ArtistDetailPane::new`: add `portraits: Rc<ArtistPortraitRuntime>` param; store it in `Inner` (add field `portraits: Rc<ArtistPortraitRuntime>`).
- In `show_artist` (`:187-211`), after `hero.update(artist, header)` and after the generation bump (`:189-190`), add a portrait request mirroring `spawn_accent` (`:366-384`). Use the pane's `generation` cell for the staleness guard:

```rust
// Fetch the artist portrait (module-gated; gradient stays until it arrives).
if self.inner.portraits.enabled.get() {
    let generation = self.inner.generation.get();
    let (sender, receiver) = async_channel::bounded(1);
    self.inner.portraits.request(ArtistPortraitRequest {
        generation,
        artist: artist.to_string(),
        force: false,
        response: sender,
    });
    let inner = self.inner.clone();
    glib::spawn_future_local(async move {
        let Ok(response) = receiver.recv().await else { return };
        if response.generation != inner.generation.get() {
            return; // a newer artist is showing
        }
        if let Ok(PortraitOutcome::Found(path)) = response.result {
            if let Ok(texture) = gtk4::gdk::Texture::from_filename(&path) {
                inner.hero.set_portrait(&texture);
            }
        }
    });
}
```

Match the exact `Inner` field accessors used by `spawn_accent` (e.g. whether it's `self.inner.generation` vs a local `Rc<Cell<u64>>` clone — read `:366-384` and mirror it exactly, including how `hero` is reached from `Inner`).

- Live toggle: in `ArtistDetailPane::new`, subscribe to the portrait runtime so enabling it while an artist is open loads the portrait, and disabling clears it:

```rust
{
    let inner = /* Weak or Rc-per the pane's existing self-ref pattern */;
    portraits.subscribe_enabled(
        move || /* alive check */ true,
        move |enabled| {
            if !enabled {
                /* inner.hero.clear_portrait(); */
            } else {
                /* re-request for the current artist if one is shown */
            }
        },
    );
}
```

Implement this only if the pane already keeps a reachable current-artist handle (it does — `HeroCallbacks.current_artist`, a `Rc<RefCell<String>>`; reuse that). Keep the disable path (clear) mandatory; the re-request-on-enable path may reuse a small helper that runs the Step-3 request for the current artist name. If the pane's self-reference story makes the alive-guard awkward, gate with a `Weak` upgrade like the other subscribers.

- [ ] **Step 4: Build + verify**

Run: `cargo build -p reprise-gnome`
Expected: compiles.

Verification (human — the gtk4 skill lists image rendering + circular clip as not headless-verifiable): with the module ON, open Blessthefall → a circular portrait replaces the gradient; open an artist Deezer lacks → gradient stays; toggle the module off → gradient returns.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/library_views/
git commit -m "feat(gnome): show Deezer portrait in the artist detail hero"
```

---

## Task 7: Master-list row portraits (visible-row prefetch)

Deliverable: scrolling the Artists list fills visible rows with portraits (module ON); recycled rows never show the wrong artist's photo.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/library_views/artist_master_row.rs` — Picture in `RowHandles`, prefetch on bind with identity guard.
- Modify: `crates/reprise-gnome/src/ui/library_views/artist_master.rs` — thread runtime into `new` + `build_row_factory`.
- Modify: `crates/reprise-gnome/src/ui/library_views/artist_view.rs` — pass runtime into `ArtistMaster::new`.

**Interfaces:**
- Consumes: `ArtistPortraitRuntime` (from Task 6's `ArtistView::new` param — already available in `ArtistView`).
- Produces: `build_row_factory(registry, now_playing, portraits: &Rc<ArtistPortraitRuntime>)`.

- [ ] **Step 1: Add a Picture to the row + build it**

In `artist_master_row.rs`:
- Add `portrait: gtk4::Picture` to `RowHandles` (`:36-48`).
- In `build_row` (`:126-175`), wrap the avatar box + a hidden Picture in an `Overlay` exactly as the hero does (38px), append the overlay to `root` in the avatar's place, and store the Picture:

```rust
let portrait = gtk4::Picture::new();
portrait.set_content_fit(gtk4::ContentFit::Cover);
portrait.set_size_request(AVATAR_SIZE, AVATAR_SIZE);
portrait.set_overflow(gtk4::Overflow::Hidden);
portrait.add_css_class("artist-portrait-image");
portrait.set_visible(false);

let avatar_overlay = gtk4::Overlay::new();
avatar_overlay.set_child(Some(&avatar));
avatar_overlay.add_overlay(&portrait);
root.append(&avatar_overlay); // replaces `root.append(&avatar)`
```

Add `portrait` to the `RowHandles { … }` literal (`:166-174`).

- [ ] **Step 2: Prefetch + reveal on bind with identity guard**

- Change `build_row_factory` signature (`:68-71`) to add `portraits: &Rc<ArtistPortraitRuntime>`; clone it into the `connect_bind` closure (`:88-109`) beside `registry`/`now_playing`.
- In `bind_row` (`:178-192`), after setting initials/gradient, hide any stale portrait immediately, then (module ON) request. Pass the runtime + the cell's `ListItem` identity so the response can be dropped if the row was recycled to another artist:

```rust
handles.portrait.set_visible(false);
handles.portrait.set_paintable(gtk4::gdk::Paintable::NONE);
if portraits.enabled.get() {
    let artist = summary.artist.clone();
    let (sender, receiver) = async_channel::bounded(1);
    portraits.request(ArtistPortraitRequest {
        generation: 0,
        artist: artist.clone(),
        force: false,
        response: sender,
    });
    let handles_artist = handles.artist.clone(); // Rc<RefCell<String>> already on RowHandles
    let portrait = handles.portrait.clone();
    glib::spawn_future_local(async move {
        let Ok(response) = receiver.recv().await else { return };
        // Recycling guard: only paint if this row is STILL bound to this artist.
        if handles_artist.borrow().as_str() != response.artist {
            return;
        }
        if let Ok(PortraitOutcome::Found(path)) = response.result {
            if let Ok(texture) = gtk4::gdk::Texture::from_filename(&path) {
                portrait.set_paintable(Some(&texture));
                portrait.set_visible(true);
            }
        }
    });
}
```

Note: `RowHandles.artist` is `RefCell<String>`; to move it into the async closure it must be shareable. Change the field to `Rc<RefCell<String>>` (update `set_now_playing` `:53-56` and `bind_row`'s `*handles.artist.borrow_mut() = …` accordingly), OR compare against a captured `String` snapshot plus re-check via the registry. Preferred: make `artist: Rc<RefCell<String>>` — smallest, and the identity check reads the row's currently-bound artist at response time. `bind_row` already overwrites it at `:188`, so a recycled row will have a different value and the guard fails safely.

`generation` is unused for rows (identity guard replaces it); pass `0`.

- [ ] **Step 3: Thread the runtime through the master**

- `artist_master.rs:84` `ArtistMaster::new`: add `portraits: Rc<ArtistPortraitRuntime>`; at the `build_row_factory` call (`:103`) pass `&portraits`.
- `artist_view.rs:40`: pass the `portraits` param (already on `ArtistView::new` from Task 6) into `ArtistMaster::new`.

- [ ] **Step 4: Build + verify**

Run: `cargo build -p reprise-gnome`
Expected: compiles.

Verification (human): with the module ON, scroll the Artists list — visible rows fill with circular portraits within a second or two; fast-scroll then stop shows correct photos per row (no smearing onto the wrong artist).

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/library_views/
git commit -m "feat(gnome): prefetch + show artist portraits in the master list"
```

---

## Task 8: Info-panel artist portrait

Deliverable: selecting a track shows the artist's portrait at the top of the info panel's artist section (module ON), gradient/empty otherwise.

**Files:**
- Modify: `crates/reprise-gnome/src/ui/info_panel/info_panel.rs` — Picture in `PanelWidgets`, request on `dispatch`, reveal on `Found`.
- Modify: `crates/reprise-gnome/src/ui/info_panel/…` constructor chain — thread the runtime.
- Modify: `crates/reprise-gnome/src/ui/window/library_shell.rs:238` — add runtime param to `library_shell::build`.
- Modify: `crates/reprise-gnome/src/ui/window/window.rs:409-417` — pass `&artist_portrait` into `library_shell::build`.

**Interfaces:**
- Consumes: `ArtistPortraitRuntime`. `InfoPanel::new` gains a `portraits: Rc<ArtistPortraitRuntime>` param.

- [ ] **Step 1: Add a persistent portrait Image to the panel**

In `info_panel.rs` `build_widgets` (`:96-198`): build a circular portrait `gtk4::Picture` (sized to the panel's artist header; reuse the `.artist-portrait-image` class + `Overflow::Hidden`), store it in `PanelWidgets` (`:78-94`), and `body.append` it at/above the `local` card (`:123-128`) so it sits ABOVE the dynamic news area that `clear_body_after_local` (`:688-694`) wipes. Start hidden.

- [ ] **Step 2: Thread the runtime**

- `InfoPanel::new` (`:225-231`): add `portraits: Rc<ArtistPortraitRuntime>` param; store as a field (near `runtime: Rc<ArtistNewsRuntime>`, `:214`).
- `library_shell::build` (`library_shell.rs:238`): add `portraits: &Rc<ArtistPortraitRuntime>` param; pass `portraits.clone()` into `InfoPanel::new` (`library_shell.rs:251-257`).
- `window.rs:409-417`: pass `&artist_portrait`.

- [ ] **Step 3: Request on dispatch + reveal**

In `dispatch` (`:485-521`), alongside the existing artist-news request, clear the portrait and (module ON) send a portrait request tied to the same `generation`; apply on response mirroring `apply_response`'s generation gate (`:524-530`):

```rust
self.portrait_image.set_visible(false);
self.portrait_image.set_paintable(gtk4::gdk::Paintable::NONE);
if self.portraits.enabled.get() {
    let (sender, receiver) = async_channel::bounded(1);
    self.portraits.request(ArtistPortraitRequest {
        generation,
        artist: artist.clone(),
        force: false,
        response: sender,
    });
    let panel = /* weak self, per the existing spawn_future_local pattern at :513-520 */;
    glib::spawn_future_local(async move {
        let Ok(response) = receiver.recv().await else { return };
        // reuse the same generation-accept check apply_response uses
        …
        if let Ok(PortraitOutcome::Found(path)) = response.result {
            if let Ok(texture) = gtk4::gdk::Texture::from_filename(&path) {
                /* panel.portrait_image.set_paintable(Some(&texture)); set_visible(true) */
            }
        }
    });
}
```

Mirror the exact weak-self / generation-accept mechanism the existing news `spawn_future_local` uses (`:513-530`) — do not invent a new one.

- Live toggle: extend the existing `subscribe_enabled` handling in `info_panel.rs` (`:460-472`) to also clear/re-request the portrait when the portrait module flips (add a second `subscribe_enabled` on `self.portraits`).

- [ ] **Step 4: Build + verify**

Run: `cargo build -p reprise-gnome && cargo test -p reprise-gnome`
Expected: compiles; existing info-panel tests still pass.

Verification (human): select a track by an artist Deezer has → portrait shows atop the info panel; switch quickly between artists → no stale portrait; toggle module off → portrait clears.

- [ ] **Step 5: Commit**

```bash
git add crates/reprise-gnome/src/ui/info_panel/ crates/reprise-gnome/src/ui/window/
git commit -m "feat(gnome): show artist portrait in the info panel"
```

---

## Task 9: Full workspace gate

- [ ] **Step 1:** `cargo fmt --all`
- [ ] **Step 2:** `cargo clippy --workspace --all-targets -- -D warnings` → fix any lints.
- [ ] **Step 3:** `cargo test --workspace` → all pass.
- [ ] **Step 4:** Headless app smoke via the `building-gtk4-rust-apps` triple-isolated harness (`dbus-run-session -- env XDG_DATA_HOME=$SCRATCH APP_AUDIO_SINK=fakesink GDK_BACKEND=x11` … `xvfb-run -a cargo run`): app launches, Artists view renders, no panic on rapid artist switching with the module toggled on. Confirm the Deezer network path is never hit while the module is OFF (default).
- [ ] **Step 5: Commit** any fmt/clippy fixups.

```bash
git add -A
git commit -m "chore: fmt + clippy for artist portraits"
```

## Self-Review Notes (coverage)

- Source = Deezer, no key → Task 1. Own throttle/UA (not `musicbrainz::get`) → Task 1 `respect_rate_limit` + own agent.
- Exact-name match + placeholder guard → Task 1 `parse_best_artist` / `is_placeholder_url`.
- Cache dir/key/TTL(30d/7d)/negative markers/atomic → Task 2.
- Orchestration + image validation reuse → Task 3.
- Opt-in module default OFF + Plugins live-apply → Task 4 + Task 5.
- Off-UI-thread worker + generation/identity guards → Tasks 5–8.
- Three display surfaces (hero, list, info panel) with gradient fallback → Tasks 6, 7, 8.
- Privacy: nothing fetches unless `enabled.get()` is checked before every `request(...)` and inside `ArtistPortraitRuntime::request` → Tasks 5–8.
