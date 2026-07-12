# GUI-A: Cover Pipeline, Player Bar & Now Playing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show album covers in the track list, player bar, and a new Now-Playing full view, fed by a portable, disk-friendly thumbnail pipeline; and make the player bar position (top/bottom) a persisted setting.

**Architecture:** Cover extraction + thumbnailing + on-disk cache live in the pure `reprise-core::cover` module (portable; consumed later by Android/iOS too). The GTK frontend gets a thin async `CoverLoader` that offloads decode/cache work via `gio::spawn_blocking` and sets `gdk::Texture`s on widgets, guarded by a per-widget generation token so recycled rows never show a stale cover. Player-bar position and the Now-Playing page are frontend-only, wired through the existing `PlayerController` (one playback state, no second path).

**Tech Stack:** Rust 2021, rusqlite (bundled), lofty 0.22 (embedded picture reads), the `image` crate 0.25 (decode/resize, NEW — pure Rust, cross-platform), gtk4-rs 0.11.4 (v4_22) / libadwaita 0.9.2 (v1_9) — `gdk::Texture`, `gtk::Image`, `adw::NavigationView`, `adw::ToolbarView`. `dirs 6` (already present) for the XDG cache dir.

## Global Constraints

- **Core stays dependency-pure.** After every task, `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` MUST be empty. The only new core dependency this plan adds is `image` (pure Rust; verify it pulls in none of the above).
- **Core promise — never write into the user's library.** The cover cache lives ONLY under `dirs::cache_dir().join("reprise/covers")` (honors `$XDG_CACHE_HOME`). Covers are read-only from the user's files (embedded picture or a sidecar image in the album folder). A test MUST assert the produced cache path is under the cache dir, never under the track's folder.
- **Gates per commit (all must pass):** `cargo fmt --check`; `cargo clippy --all-targets --workspace -- -D warnings`; `cargo test --workspace` (the bare `cargo test` runs only the gnome default-member — always use `--workspace`); `cargo audit` (known-accepted advisory: RUSTSEC-2024-0436 `paste` via lofty).
- **Baseline test count: 378 passed; 1 ignored.** Each task states its expected new total. Never edit an existing test's expectations.
- **Headless smoke isolation (Wayland host — mandatory).** Never run the app bare. Use:
  `dbus-run-session -- xvfb-run -a env GDK_BACKEND=x11 REPRISE_AUDIO_SINK=fakesink WAYLAND_DISPLAY= <hooks> cargo run` — a leaked session bus hijacks the user's real launches; a Wayland backend opens a window on the real desktop.
- **File-size rule:** every file created or substantially edited ends < 800 lines.
- **English** for all code, comments, log/UI strings, and commit messages. No commit attribution footer. **Do not push.**
- **Workspace clippy lints** (enforced `-D warnings`): `needless_pass_by_value`, `redundant_closure_for_method_calls`, `semicolon_if_nothing_returned`, `uninlined_format_args`, `map_unwrap_or`, `unnested_or_patterns`.

## Current-state facts (verified 2026-07-12, HEAD 99dadee)

- `Track` (`crates/reprise-core/src/models.rs`): on-disk path is `pub path: String`; also `title`/`artist`/`album`/`album_artist`/`duration_ms`. No cover field.
- lofty is used ONLY in `crates/reprise-core/src/library/scanner.rs::read_meta` (`lofty::read_from_path` → `primary_tag().or_else(first_tag)`). Embedded pictures are NOT read anywhere yet. lofty 0.22 picture API: `tag.pictures() -> &[lofty::picture::Picture]`, `Picture::data() -> &[u8]`.
- Settings façade (`crates/reprise-core/src/library/settings.rs`): `get_setting`/`set_setting`/`get_bool`/`set_bool`/`get_library_root`/`set_library_root`; test helper `fn migrated_conn() -> Connection`.
- Player bar (`crates/reprise-gnome/src/ui/player_bar.rs`): root is a `gtk4::ActionBar`; `set_track(&self, title, artist)` / `clear_track` / `widget() -> &gtk4::ActionBar`; NO cover widget yet.
- Controller (`crates/reprise-gnome/src/ui/player_controller.rs`): `play_track_id(&self, id)` resolves `queries::query_track_summary` (has `.path`), calls `self.bar.set_track(...)` then `self.player.play(&summary.path)` (~lines 442–443). `NowPlaying { id, title, artist, album, duration_ms }` (no `path`).
- Track-list columns (`crates/reprise-gnome/src/ui/track_list_columns.rs`): `append_column(...)` (Label cells) and `append_rating_column(...)` (the precedent for a non-Label cell widget). Columns are appended in `TrackList::new` (`track_list.rs`) in call order. Model item wraps `Track` via `glib::BoxedAnyObject`.
- Shell (`crates/reprise-gnome/src/ui/window.rs::build`): `adw::NavigationSplitView` (sidebar + content). Content is an `adw::ToolbarView`; the bar sits in a vertical `bottom_box` (status bar + `player.bar_widget()`) attached via `toolbar_view.add_bottom_bar(&bottom_box)`. No `adw::NavigationView`/`ViewStack` yet.
- Cache dirs: `dirs` is available in core; `db::default_path` uses `dirs::data_dir()`. Use `dirs::cache_dir()` for covers. `db::open` shows the `std::fs::create_dir_all` pattern.
- Smoke-hook convention: `const REPRISE_SMOKE_* ` + an `arm_smoke_*` fn guarded by `std::env::var`, deferred via `glib::idle_add_local_once`, wired from `window::build` or `PlayerController::new` (closest model: `arm_smoke_repeat` in `player_controller_wiring.rs`).

## Ordering

Strictly sequential (each builds on the prior): 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8 → 9. Tasks 1–3 are pure `reprise-core` (unit-tested, no display). Tasks 4–9 are frontend (headless smokes). Task 4 (the loader) is the shared substrate Tasks 5/6/8/9 consume.

---

### Task 1: `cover::resolve_source` — find the best cover source (pure core)

**Files:**
- Create: `crates/reprise-core/src/cover.rs`
- Modify: `crates/reprise-core/src/lib.rs` (add `pub mod cover;`)

**Interfaces:**
- Consumes: `lofty` (already a dep), `Track.path` (a `String`, but this fn takes a `&Path`).
- Produces:
  - `pub enum CoverSource { Embedded(Vec<u8>), FolderImage(std::path::PathBuf) }`
  - `pub fn resolve_source(track_path: &std::path::Path) -> Option<CoverSource>`

- [ ] **Step 1: Write the failing tests** in `crates/reprise-core/src/cover.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // A 1x1 PNG, enough for source-resolution tests (no decode here).
    const TINY_PNG: &[u8] = &[
        0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F,
        0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x62, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
    ];

    fn write(dir: &std::path::Path, name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[test]
    fn returns_none_when_no_embedded_and_no_folder_image() {
        let dir = tempfile::tempdir().unwrap();
        let track = write(dir.path(), "song.flac", b"not audio, no tag");
        assert!(resolve_source(&track).is_none());
    }

    #[test]
    fn falls_back_to_folder_image_cover_jpg() {
        let dir = tempfile::tempdir().unwrap();
        let track = write(dir.path(), "song.flac", b"not audio");
        let cover = write(dir.path(), "cover.jpg", TINY_PNG); // content irrelevant here
        match resolve_source(&track) {
            Some(CoverSource::FolderImage(p)) => assert_eq!(p, cover),
            other => panic!("expected FolderImage, got {other:?}"),
        }
    }

    #[test]
    fn folder_image_matches_are_case_insensitive_and_prioritized() {
        // "Folder.PNG" must be found; the first canonical name wins deterministically.
        let dir = tempfile::tempdir().unwrap();
        let track = write(dir.path(), "song.flac", b"x");
        let _ = write(dir.path(), "Folder.PNG", TINY_PNG);
        assert!(matches!(
            resolve_source(&track),
            Some(CoverSource::FolderImage(_))
        ));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p reprise-core cover::tests 2>&1 | tail -5`
Expected: FAIL — `cannot find function resolve_source` / `CoverSource`.

- [ ] **Step 3: Implement `resolve_source`** at the top of `crates/reprise-core/src/cover.rs`:

```rust
//! Cover-art resolution and thumbnailing (portable, GUI-free). Reads covers
//! from the user's files (embedded picture, or a sidecar image in the album
//! folder) and produces cached thumbnails under the XDG cache dir. NEVER
//! writes into the user's library — see `thumbnail`'s cache path.

use std::path::{Path, PathBuf};

/// Where a cover for a track comes from.
#[derive(Debug)]
pub enum CoverSource {
    /// A picture embedded in the audio file (via lofty).
    Embedded(Vec<u8>),
    /// An image file sitting in the album folder (cover.*, folder.*).
    FolderImage(PathBuf),
}

/// Canonical sidecar cover file stems and extensions, in priority order.
const FOLDER_STEMS: &[&str] = &["cover", "folder", "front", "album"];
const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "webp", "gif", "bmp"];

/// Resolves the best available cover source for a track: first an embedded
/// picture, otherwise the first matching sidecar image in the track's folder,
/// otherwise None. Pure read — touches no cache and writes nothing.
pub fn resolve_source(track_path: &Path) -> Option<CoverSource> {
    if let Some(bytes) = embedded_picture(track_path) {
        return Some(CoverSource::Embedded(bytes));
    }
    let dir = track_path.parent()?;
    folder_image(dir).map(CoverSource::FolderImage)
}

/// Reads the first embedded picture's bytes from an audio file, if any.
fn embedded_picture(track_path: &Path) -> Option<Vec<u8>> {
    use lofty::prelude::*;
    let tagged = lofty::read_from_path(track_path).ok()?;
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag())?;
    let picture = tag.pictures().first()?;
    Some(picture.data().to_vec())
}

/// Finds a sidecar cover image in `dir` by canonical stem + known extension,
/// case-insensitively, deterministically (stem-then-ext priority).
fn folder_image(dir: &Path) -> Option<PathBuf> {
    let entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    for stem in FOLDER_STEMS {
        for ext in IMAGE_EXTS {
            for path in &entries {
                let matches_stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(stem));
                let matches_ext = path
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.eq_ignore_ascii_case(ext));
                if matches_stem && matches_ext {
                    return Some(path.clone());
                }
            }
        }
    }
    None
}
```

Add `pub mod cover;` to `crates/reprise-core/src/lib.rs` (keep the module list alphabetical: after `pub mod db;` — actually place `pub mod cover;` before `pub mod db;` to stay sorted).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p reprise-core cover::tests 2>&1 | tail -5`
Expected: `3 passed`.

- [ ] **Step 5: Gates + commit**

Run: `cargo fmt --check && cargo clippy -p reprise-core --all-targets -- -D warnings && cargo test --workspace 2>&1 | grep 'test result'`
Expected: clippy clean; test totals sum to **381 passed; 1 ignored** (378 + 3).

```bash
git add crates/reprise-core/src/cover.rs crates/reprise-core/src/lib.rs
git commit -m "feat: resolve album cover source (embedded picture or folder image)"
```

---

### Task 2: `cover::thumbnail` — decode, resize, cache (pure core, adds `image`)

**Files:**
- Modify: `crates/reprise-core/src/cover.rs`
- Modify: `crates/reprise-core/Cargo.toml` (add `image`)

**Interfaces:**
- Consumes: `CoverSource` (Task 1), `dirs` (already a dep), `image` (new).
- Produces:
  - `pub enum ThumbnailSize { List, Bar, Full }` with `pub fn pixels(self) -> u32` → 48 / 96 / 512
  - `#[derive(Debug)] pub enum CoverError { Decode(String), Io(String) }` (impl `std::fmt::Display` + `std::error::Error`)
  - `pub fn thumbnail(source: &CoverSource, size: ThumbnailSize) -> Result<PathBuf, CoverError>`
  - `pub fn cache_dir() -> PathBuf` (for the test + callers)

- [ ] **Step 1: Add the dependency.** In `crates/reprise-core/Cargo.toml` under `[dependencies]`, add (note `default-features = false` to keep only the formats real music libraries embed — keeps the build surface small and the purity proof honest):

```toml
# Pure-Rust image decode/resize for cover thumbnails (GUI-A). default-features
# off: only the formats embedded covers / sidecars actually use. Cross-platform,
# pulls in no gtk/gstreamer/zbus (verify with `cargo tree -p reprise-core`).
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp", "gif", "bmp"] }
```

- [ ] **Step 2: Write the failing tests** — add to the `tests` mod in `cover.rs` (reuse `TINY_PNG`/`write` from Task 1):

```rust
    // A real, decodable 600x600 red PNG — larger than the biggest thumbnail
    // (512 px) so every size DOWNSCALES (image::thumbnail never upscales) and
    // the exact-size assertion below holds.
    fn red_png_600() -> Vec<u8> {
        let img = image::RgbImage::from_pixel(600, 600, image::Rgb([255, 0, 0]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    #[test]
    fn thumbnail_produces_png_of_requested_size_and_caches_under_cache_dir() {
        let src = CoverSource::Embedded(red_png_600());
        let path = thumbnail(&src, ThumbnailSize::List).unwrap();
        // Lands under the cache dir, NEVER in a track folder (core promise).
        assert!(path.starts_with(cache_dir()), "thumb must be in the cache dir");
        assert_eq!(path.extension().unwrap(), "png");
        // Decodes back to a PNG whose largest side is the requested pixel count.
        let decoded = image::open(&path).unwrap();
        let (w, h) = (decoded.width(), decoded.height());
        assert!(w.max(h) == ThumbnailSize::List.pixels(), "got {w}x{h}");
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn identical_bytes_hash_to_the_same_cache_path() {
        let a = thumbnail(&CoverSource::Embedded(red_png_600()), ThumbnailSize::Bar).unwrap();
        let b = thumbnail(&CoverSource::Embedded(red_png_600()), ThumbnailSize::Bar).unwrap();
        assert_eq!(a, b, "same source bytes + size -> same cache key");
        std::fs::remove_file(&a).ok();
    }

    #[test]
    fn different_sizes_get_distinct_cache_paths() {
        let bytes = red_png_600();
        let list = thumbnail(&CoverSource::Embedded(bytes.clone()), ThumbnailSize::List).unwrap();
        let full = thumbnail(&CoverSource::Embedded(bytes), ThumbnailSize::Full).unwrap();
        assert_ne!(list, full);
        std::fs::remove_file(&list).ok();
        std::fs::remove_file(&full).ok();
    }

    #[test]
    fn corrupt_image_returns_error_never_panics() {
        let src = CoverSource::Embedded(b"definitely not an image".to_vec());
        assert!(matches!(thumbnail(&src, ThumbnailSize::List), Err(CoverError::Decode(_))));
    }
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p reprise-core cover::tests 2>&1 | tail -8`
Expected: FAIL — `cannot find function thumbnail` / `ThumbnailSize` / `cache_dir`.

- [ ] **Step 4: Implement** — append to `cover.rs` (below `resolve_source`):

```rust
use std::hash::{Hash, Hasher};

/// The three cached edge lengths — one per consumer (list row / player bar /
/// Now-Playing view). Exactly three (YAGNI).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailSize {
    List,
    Bar,
    Full,
}

impl ThumbnailSize {
    pub fn pixels(self) -> u32 {
        match self {
            ThumbnailSize::List => 48,
            ThumbnailSize::Bar => 96,
            ThumbnailSize::Full => 512,
        }
    }
}

#[derive(Debug)]
pub enum CoverError {
    Decode(String),
    Io(String),
}

impl std::fmt::Display for CoverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CoverError::Decode(m) => write!(f, "cover decode failed: {m}"),
            CoverError::Io(m) => write!(f, "cover cache I/O failed: {m}"),
        }
    }
}

impl std::error::Error for CoverError {}

/// The cover thumbnail cache directory: `<XDG cache>/reprise/covers`. NEVER a
/// path inside the user's library — this is the load-bearing half of the
/// "we don't touch your files" promise.
pub fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("reprise/covers")
}

/// Returns the cache path to a thumbnail of `source` at `size`, creating it if
/// missing: hash the source bytes -> cache hit? -> else decode, resize (aspect
/// preserved, longest side = size), write PNG atomically (temp + rename).
pub fn thumbnail(source: &CoverSource, size: ThumbnailSize) -> Result<PathBuf, CoverError> {
    let bytes = source_bytes(source)?;
    let key = hash_hex(&bytes);
    let dir = cache_dir();
    let out = dir.join(format!("{key}-{}.png", size.pixels()));
    if out.exists() {
        return Ok(out);
    }
    std::fs::create_dir_all(&dir).map_err(|e| CoverError::Io(e.to_string()))?;

    let decoded = image::load_from_memory(&bytes).map_err(|e| CoverError::Decode(e.to_string()))?;
    let thumb = decoded.thumbnail(size.pixels(), size.pixels()); // aspect-preserving

    // Atomic publish: write a unique temp file in the same dir, then rename.
    let tmp = dir.join(format!(".{key}-{}.png.tmp", size.pixels()));
    thumb
        .save_with_format(&tmp, image::ImageFormat::Png)
        .map_err(|e| CoverError::Io(e.to_string()))?;
    std::fs::rename(&tmp, &out).map_err(|e| CoverError::Io(e.to_string()))?;
    Ok(out)
}

fn source_bytes(source: &CoverSource) -> Result<Vec<u8>, CoverError> {
    match source {
        CoverSource::Embedded(b) => Ok(b.clone()),
        CoverSource::FolderImage(p) => std::fs::read(p).map_err(|e| CoverError::Io(e.to_string())),
    }
}

/// Fast, non-cryptographic content hash (std DefaultHasher) over the source
/// bytes, hex-encoded. The key only needs to be deterministic on one machine
/// and collision-resistant enough for a cache — no crypto property required,
/// so no new hashing dependency.
fn hash_hex(bytes: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut h);
    format!("{:016x}", h.finish())
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p reprise-core cover::tests 2>&1 | tail -8`
Expected: `7 passed` (3 from Task 1 + 4 new).

- [ ] **Step 6: Purity proof + gates**

Run: `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus' || echo PURE`
Expected: `PURE`.
Run: `cargo clippy -p reprise-core --all-targets -- -D warnings && cargo test --workspace 2>&1 | grep 'test result'`
Expected: clean; totals sum to **385 passed; 1 ignored**.
Run: `wc -l crates/reprise-core/src/cover.rs` → < 800.

- [ ] **Step 7: Commit**

```bash
git add crates/reprise-core/src/cover.rs crates/reprise-core/Cargo.toml Cargo.lock
git commit -m "feat: thumbnail + XDG-cache album covers (image crate); hash-keyed, atomic writes"
```

---

### Task 3: `PlayerBarPosition` settings accessor (pure core)

**Files:**
- Modify: `crates/reprise-core/src/library/settings.rs`

**Interfaces:**
- Consumes: `get_setting`/`set_setting` (existing).
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum PlayerBarPosition { Top, Bottom }`
  - `pub fn get_player_bar_position(conn: &Connection) -> PlayerBarPosition`
  - `pub fn set_player_bar_position(conn: &Connection, pos: PlayerBarPosition) -> Result<(), rusqlite::Error>`

- [ ] **Step 1: Write the failing tests** — add to the `tests` mod in `settings.rs`:

```rust
    #[test]
    fn player_bar_position_defaults_to_bottom() {
        let conn = migrated_conn();
        assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Bottom);
    }

    #[test]
    fn player_bar_position_round_trips_both_values() {
        let conn = migrated_conn();
        set_player_bar_position(&conn, PlayerBarPosition::Top).unwrap();
        assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Top);
        set_player_bar_position(&conn, PlayerBarPosition::Bottom).unwrap();
        assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Bottom);
    }

    #[test]
    fn player_bar_position_falls_back_to_bottom_on_unknown_value() {
        let conn = migrated_conn();
        set_setting(&conn, PLAYER_BAR_POSITION_KEY, "sideways").unwrap();
        assert_eq!(get_player_bar_position(&conn), PlayerBarPosition::Bottom);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p reprise-core settings 2>&1 | tail -5`
Expected: FAIL — `cannot find PlayerBarPosition` / `get_player_bar_position`.

- [ ] **Step 3: Implement** — add near the other typed accessors in `settings.rs`:

```rust
pub const PLAYER_BAR_POSITION_KEY: &str = "player_bar_position";

/// Where the player bar docks. `Bottom` is the default and the fallback for any
/// unknown/hand-edited value (same tolerance posture as `get_bool`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerBarPosition {
    Top,
    Bottom,
}

pub fn get_player_bar_position(conn: &Connection) -> PlayerBarPosition {
    match get_setting(conn, PLAYER_BAR_POSITION_KEY) {
        Ok(Some(v)) if v == "top" => PlayerBarPosition::Top,
        Ok(Some(v)) if v == "bottom" => PlayerBarPosition::Bottom,
        Ok(Some(other)) => {
            tracing::warn!(value = %other, "unrecognized player_bar_position; using Bottom");
            PlayerBarPosition::Bottom
        }
        Ok(None) => PlayerBarPosition::Bottom,
        Err(error) => {
            tracing::warn!(%error, "could not read player_bar_position; using Bottom");
            PlayerBarPosition::Bottom
        }
    }
}

pub fn set_player_bar_position(
    conn: &Connection,
    pos: PlayerBarPosition,
) -> Result<(), rusqlite::Error> {
    let value = match pos {
        PlayerBarPosition::Top => "top",
        PlayerBarPosition::Bottom => "bottom",
    };
    set_setting(conn, PLAYER_BAR_POSITION_KEY, value)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p reprise-core settings 2>&1 | tail -5`
Expected: PASS (3 new).

- [ ] **Step 5: Gates + commit**

Run: `cargo clippy -p reprise-core --all-targets -- -D warnings && cargo test --workspace 2>&1 | grep 'test result'`
Expected: totals sum to **388 passed; 1 ignored**.

```bash
git add crates/reprise-core/src/library/settings.rs
git commit -m "feat: typed PlayerBarPosition settings accessor (top/bottom, default bottom)"
```

---

### Task 4: `CoverLoader` — async, off-thread cover loading (frontend substrate)

**Files:**
- Create: `crates/reprise-gnome/src/ui/cover_loader.rs`
- Modify: `crates/reprise-gnome/src/ui/mod.rs` (`pub mod cover_loader;`)

**Interfaces:**
- Consumes: `reprise_core::cover::{resolve_source, thumbnail, ThumbnailSize}`.
- Produces:
  - `pub struct CoverLoader` + `pub fn new() -> std::rc::Rc<CoverLoader>`
  - `pub fn load_into(self: &Rc<Self>, image: &gtk4::Image, track_path: &str, size: ThumbnailSize, token: u64, current: &Rc<std::cell::Cell<u64>>)` — sets the placeholder immediately, then off-thread resolves+thumbnails and, if `current` still equals `token`, sets the `gdk::Texture`. Stale results (recycled row) are dropped.
  - `pub fn set_placeholder(image: &gtk4::Image)` — the shared music-note placeholder (an `icon-name`), no decode.

This is the shared substrate for Tasks 5, 6, 8, 9. Decode + cache happen on a `gio` worker thread via `gio::spawn_blocking`; the result texture is set back on the main loop. A small bounded in-memory texture cache avoids re-reading the on-disk PNG while scrolling.

- [ ] **Step 1: Implement** `crates/reprise-gnome/src/ui/cover_loader.rs`:

```rust
//! Lazy, off-thread cover loading for GTK widgets. Decode/cache work runs on a
//! `gio` worker thread (never the main loop); the resulting `gdk::Texture` is
//! applied back on the main context, guarded by a per-widget generation token
//! so a recycled track-list row never shows a stale cover.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use reprise_core::cover::{resolve_source, thumbnail, ThumbnailSize};

/// Symbolic placeholder shown when a track has no cover (or while loading /
/// on error). No decode — just an icon name GTK already ships.
const PLACEHOLDER_ICON: &str = "audio-x-generic-symbolic";

/// Cap on the in-memory texture cache. Thumbnails are tiny; this only spares
/// re-reading the on-disk PNG during scrolling. Evicts oldest-inserted first.
const MAX_CACHED_TEXTURES: usize = 256;

pub struct CoverLoader {
    cache: RefCell<HashMap<String, gdk::Texture>>,
    order: RefCell<std::collections::VecDeque<String>>,
}

impl CoverLoader {
    pub fn new() -> Rc<Self> {
        Rc::new(Self {
            cache: RefCell::new(HashMap::new()),
            order: RefCell::new(std::collections::VecDeque::new()),
        })
    }

    pub fn set_placeholder(image: &gtk4::Image) {
        image.set_icon_name(Some(PLACEHOLDER_ICON));
    }

    fn cache_get(&self, key: &str) -> Option<gdk::Texture> {
        self.cache.borrow().get(key).cloned()
    }

    fn cache_put(&self, key: String, texture: gdk::Texture) {
        let mut cache = self.cache.borrow_mut();
        if cache.contains_key(&key) {
            return;
        }
        let mut order = self.order.borrow_mut();
        if cache.len() >= MAX_CACHED_TEXTURES {
            if let Some(old) = order.pop_front() {
                cache.remove(&old);
            }
        }
        order.push_back(key.clone());
        cache.insert(key, texture);
    }

    pub fn load_into(
        self: &Rc<Self>,
        image: &gtk4::Image,
        track_path: &str,
        size: ThumbnailSize,
        token: u64,
        current: &Rc<Cell<u64>>,
    ) {
        let key = format!("{track_path}|{}", size.pixels());
        if let Some(texture) = self.cache_get(&key) {
            image.set_paintable(Some(&texture));
            return;
        }
        Self::set_placeholder(image);

        let this = self.clone();
        let image = image.clone();
        let current = current.clone();
        let path_owned = track_path.to_string();
        glib::spawn_future_local(async move {
            // Off the main loop: resolve source + build/hit the disk cache.
            let path_for_worker = path_owned.clone();
            let cache_path = gio::spawn_blocking(move || {
                let source = resolve_source(std::path::Path::new(&path_for_worker))?;
                thumbnail(&source, size).ok()
            })
            .await
            .ok()
            .flatten();

            // Back on the main loop: bail if this cell was recycled meanwhile.
            if current.get() != token {
                return;
            }
            let Some(cache_path) = cache_path else { return };
            match gdk::Texture::from_filename(&cache_path) {
                Ok(texture) => {
                    this.cache_put(key, texture.clone());
                    image.set_paintable(Some(&texture));
                }
                Err(error) => {
                    tracing::debug!(%error, path = %path_owned, "cover texture load failed");
                }
            }
        });
    }
}
```

(Note: `gio` is reachable as `gtk4::gio`; add `use gtk4::gio;` if the compiler wants the path qualified. `resolve_source` returns `Option`, so the closure returns `Option<PathBuf>` via `?` on the `Option` — wrap the closure body to return `Option<PathBuf>`.) Add `pub mod cover_loader;` to `crates/reprise-gnome/src/ui/mod.rs`.

- [ ] **Step 2: Compile check**

Run: `cargo build -p reprise-gnome 2>&1 | tail -5`
Expected: builds clean. Fix any `gio` path / `Option`-return closure issues the compiler flags (return type of the `spawn_blocking` closure must be explicit `Option<std::path::PathBuf>`).

- [ ] **Step 3: Purity proof stays green** (the loader is frontend-only, but confirm core is untouched)

Run: `cargo tree -p reprise-core | grep -E 'gtk4|gstreamer|zbus' || echo PURE`
Expected: `PURE`.

- [ ] **Step 4: Gates + commit** (no new tests — this is GTK glue exercised by Tasks 5/6/8's smokes)

Run: `cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings && cargo test --workspace 2>&1 | grep 'test result'`
Expected: **388 passed; 1 ignored** (unchanged).

```bash
git add crates/reprise-gnome/src/ui/cover_loader.rs crates/reprise-gnome/src/ui/mod.rs
git commit -m "feat: async off-thread CoverLoader with generation-guarded texture apply"
```

---

### Task 5: Cover in the player bar

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_bar.rs` (add cover `gtk4::Image` + `set_cover`/`clear_cover`)
- Modify: `crates/reprise-gnome/src/ui/player_controller.rs` (feed the cover on track change; add `path` to `NowPlaying`)

**Interfaces:**
- Consumes: `CoverLoader` (Task 4), `ThumbnailSize::Bar`.
- Produces (player_bar): `pub fn cover_image(&self) -> &gtk4::Image` and `pub fn clear_cover(&self)`.

- [ ] **Step 1: Add the cover widget to the bar.** In `player_bar.rs`, in the `PlayerBar` struct add a field `cover: gtk4::Image`. In `new()`, build it before the title box and pack it at the start:

```rust
let cover = gtk4::Image::new();
cover.set_pixel_size(48); // bar shows a 48pt widget fed a 96px texture (HiDPI-crisp)
cover.add_css_class("player-bar-cover");
crate::ui::cover_loader::CoverLoader::set_placeholder(&cover);
bar.pack_start(&cover);
```

Store `cover` in the struct and add:

```rust
pub fn cover_image(&self) -> &gtk4::Image {
    &self.cover
}

pub fn clear_cover(&self) {
    crate::ui::cover_loader::CoverLoader::set_placeholder(&self.cover);
}
```

Call `self.clear_cover()` inside the existing `clear_track()`.

- [ ] **Step 2: Give the controller a loader + current path.** In `player_controller.rs`:
  - Add `path: String` to the `NowPlaying` struct.
  - Add a field `cover_loader: Rc<crate::ui::cover_loader::CoverLoader>` and a `bar_cover_generation: Rc<Cell<u64>>` to `PlayerController`; initialize in `new()` with `CoverLoader::new()` and `Rc::new(Cell::new(0))`.
  - In `play_track_id`, right after `self.bar.set_track(&summary.title, &summary.artist);` (~line 442), load the bar cover:

```rust
    let generation = self.bar_cover_generation.get().wrapping_add(1);
    self.bar_cover_generation.set(generation);
    self.cover_loader.load_into(
        self.bar.cover_image(),
        &summary.path,
        reprise_core::cover::ThumbnailSize::Bar,
        generation,
        &self.bar_cover_generation,
    );
```

  - Set `path: summary.path.clone()` when constructing the `NowPlaying`.

- [ ] **Step 3: Build + isolated smoke.** Ensure a smoke library with an embedded cover exists (see Task 5a below if none) or reuse the player fixture. Build, then run the standard smoke:

Run:
```bash
cargo build -p reprise-gnome 2>&1 | tail -3
dbus-run-session -- xvfb-run -a env GDK_BACKEND=x11 REPRISE_AUDIO_SINK=fakesink WAYLAND_DISPLAY= \
  REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_ACTIVATE=1 REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=4 cargo run 2>&1 | grep -iE 'cover|ERROR|panic' | head
```
Expected: exit 0, no ERROR/panic lines. (Visual cover correctness is a human check — list it for the manual pass; headless proves it does not crash and the placeholder/texture path runs.)

- [ ] **Step 4: Gates + commit**

Run: `cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings && cargo test --workspace 2>&1 | grep 'test result'`
Expected: **388 passed; 1 ignored** (unchanged).

```bash
git add crates/reprise-gnome/src/ui/player_bar.rs crates/reprise-gnome/src/ui/player_controller.rs
git commit -m "feat: show album cover in the player bar"
```

---

### Task 6: Cover column in the track list

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list_columns.rs` (add `append_cover_column`)
- Modify: `crates/reprise-gnome/src/ui/track_list.rs` (append the cover column FIRST in `new`; thread the shared `CoverLoader`)

**Interfaces:**
- Consumes: `CoverLoader` (Task 4), `ThumbnailSize::List`, `Track.path`.
- Produces: `pub(super) fn append_cover_column(column_view: &gtk4::ColumnView, shared: &Rc<Shared>, loader: &Rc<CoverLoader>)`.

- [ ] **Step 1: Implement `append_cover_column`** modeled on `append_rating_column`. The setup builds a `gtk4::Image` (48pt) per cell plus a per-cell generation `Rc<Cell<u64>>`; bind bumps the generation, sets the placeholder, and calls `loader.load_into(&image, &track.path, ThumbnailSize::List, gen, &cell_gen)`:

```rust
pub(super) fn append_cover_column(
    column_view: &gtk4::ColumnView,
    shared: &Rc<Shared>,
    loader: &Rc<CoverLoader>,
) {
    let factory = gtk4::SignalListItemFactory::new();

    factory.connect_setup(|_, item| {
        let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
        let image = gtk4::Image::new();
        image.set_pixel_size(32); // 48px texture in a 32pt cell — crisp, compact row
        CoverLoader::set_placeholder(&image);
        // Per-cell generation guards against recycled-row stale covers.
        let generation = Rc::new(Cell::new(0u64));
        unsafe {
            item.set_data("cover-generation", generation);
        }
        item.set_child(Some(&image));
    });

    let loader = loader.clone();
    let _ = shared; // reserved for future (e.g. reacting to tag edits); keep signature uniform
    factory.connect_bind(move |_, item| {
        let item = item.downcast_ref::<gtk4::ListItem>().unwrap();
        let Some(image) = item.child().and_downcast::<gtk4::Image>() else { return };
        let Some(obj) = item.item() else { return };
        let boxed = obj.downcast_ref::<glib::BoxedAnyObject>().unwrap();
        let track = boxed.borrow::<reprise_core::models::Track>();

        let generation: Rc<Cell<u64>> =
            unsafe { item.data::<Rc<Cell<u64>>>("cover-generation").unwrap().as_ref().clone() };
        let token = generation.get().wrapping_add(1);
        generation.set(token);
        loader.load_into(
            &image,
            &track.path,
            reprise_core::cover::ThumbnailSize::List,
            token,
            &generation,
        );
    });

    let column = gtk4::ColumnViewColumn::new(Some(""), Some(factory));
    column.set_resizable(false);
    column.set_fixed_width(40);
    column_view.append_column(&column);
}
```

(If `set_data`/`data` unsafe generational storage trips clippy or feels brittle, an accepted alternative is a `RefCell<HashMap<ptr, Rc<Cell<u64>>>>` keyed by the ListItem — but the `set_data` pattern is the GTK-idiomatic per-item slot; keep it if clean.)

- [ ] **Step 2: Wire it first in `TrackList::new`.** In `track_list.rs::new`, construct/receive a shared `Rc<CoverLoader>` (add a `loader` param to `TrackList::new`, or construct one and store it on `Shared`), and call `track_list_columns::append_cover_column(&column_view, &shared, &loader)` BEFORE the first `append_column(...)` so the cover is the leading column.

- [ ] **Step 3: Build + isolated smoke** (reuse the Task 5 smoke; the list now renders cover cells):

Run:
```bash
cargo build -p reprise-gnome 2>&1 | tail -3
dbus-run-session -- xvfb-run -a env GDK_BACKEND=x11 REPRISE_AUDIO_SINK=fakesink WAYLAND_DISPLAY= \
  REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=4 cargo run 2>&1 | grep -iE 'ERROR|panic' | head
```
Expected: exit 0, no ERROR/panic (cover cells bind without crashing; scrolling correctness is a manual visual check).

- [ ] **Step 4: Gates + commit**

Run: `cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings && cargo test --workspace 2>&1 | grep 'test result'`
Expected: **388 passed; 1 ignored** (unchanged). `wc -l crates/reprise-gnome/src/ui/track_list_columns.rs` → < 800.

```bash
git add crates/reprise-gnome/src/ui/track_list_columns.rs crates/reprise-gnome/src/ui/track_list.rs
git commit -m "feat: leading album-cover column in the track list"
```

---

### Task 7: Player-bar position (top/bottom), persisted + immediate

**Files:**
- Modify: `crates/reprise-gnome/src/ui/window.rs` (read the setting; attach the bar top or bottom; add a `REPRISE_SMOKE_BAR_POSITION` hook)

**Interfaces:**
- Consumes: `settings::{get_player_bar_position, set_player_bar_position, PlayerBarPosition}` (Task 3).
- Produces: an internal `fn apply_bar_position(toolbar_view, bottom_box, pos)` that detaches the bar box from its current slot and re-attaches it top or bottom.

- [ ] **Step 1: Read the setting at build + attach accordingly.** In `window.rs::build`, replace the unconditional `toolbar_view.add_bottom_bar(&bottom_box)` with:

```rust
let position = reprise_core::library::settings::get_player_bar_position(&conn.borrow());
apply_bar_position(&toolbar_view, &bottom_box, position);
```

and add the helper (idempotent re-attach — remove from both slots first, then add to the chosen one):

```rust
fn apply_bar_position(
    toolbar_view: &adw::ToolbarView,
    bottom_box: &gtk4::Box,
    position: reprise_core::library::settings::PlayerBarPosition,
) {
    use reprise_core::library::settings::PlayerBarPosition;
    // Detach from whichever slot it currently occupies (no-op if unattached).
    toolbar_view.remove(bottom_box);
    match position {
        PlayerBarPosition::Top => toolbar_view.add_top_bar(bottom_box),
        PlayerBarPosition::Bottom => toolbar_view.add_bottom_bar(bottom_box),
    }
}
```

(`adw::ToolbarView::remove` detaches a bar added via `add_top_bar`/`add_bottom_bar`; verify the exact method name against libadwaita 0.9.2 — it is `remove(&Widget)`.)

- [ ] **Step 2: Add the headless toggle hook.** Add near the other `arm_smoke_*` wiring in `window.rs`:

```rust
const SMOKE_BAR_POSITION_ENV_VAR: &str = "REPRISE_SMOKE_BAR_POSITION"; // "top" | "bottom"

fn arm_smoke_bar_position(
    conn: &Rc<RefCell<Connection>>,
    toolbar_view: &adw::ToolbarView,
    bottom_box: &gtk4::Box,
) {
    use reprise_core::library::settings::{set_player_bar_position, PlayerBarPosition};
    let Ok(value) = std::env::var(SMOKE_BAR_POSITION_ENV_VAR) else { return };
    let pos = if value == "top" { PlayerBarPosition::Top } else { PlayerBarPosition::Bottom };
    let _ = set_player_bar_position(&conn.borrow(), pos);
    apply_bar_position(toolbar_view, bottom_box, pos);
    tracing::info!(position = %value, "smoke: applied player bar position");
}
```

Call it in `build` alongside the other `arm_smoke_*` calls.

- [ ] **Step 3: Isolated smoke — both positions.**

Run:
```bash
cargo build -p reprise-gnome 2>&1 | tail -3
for POS in top bottom; do
  echo "== $POS =="
  dbus-run-session -- xvfb-run -a env GDK_BACKEND=x11 REPRISE_AUDIO_SINK=fakesink WAYLAND_DISPLAY= \
    REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_BAR_POSITION=$POS REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=3 cargo run 2>&1 | grep -iE 'applied player bar position|ERROR|panic' | head
done
```
Expected: each run logs "smoke: applied player bar position" with the right value, exit 0, no ERROR/panic.

- [ ] **Step 4: Gates + commit**

Run: `cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings && cargo test --workspace 2>&1 | grep 'test result'`
Expected: **388 passed; 1 ignored**. `wc -l crates/reprise-gnome/src/ui/window.rs` → < 800.

```bash
git add crates/reprise-gnome/src/ui/window.rs
git commit -m "feat: persisted player-bar position (top/bottom), applied at startup"
```

---

### Task 8: Now-Playing full view (Amberol-style)

**Files:**
- Create: `crates/reprise-gnome/src/ui/now_playing.rs` (the page widget + its cover/transport bindings)
- Modify: `crates/reprise-gnome/src/ui/window.rs` (wrap content in an `adw::NavigationView`; push the page on bar click; add a smoke hook)
- Modify: `crates/reprise-gnome/src/ui/player_bar.rs` (a click gesture on the bar that emits an "expand" signal)
- Modify: `crates/reprise-gnome/src/ui/player_controller.rs` (mirror now-playing + position/state into the Now-Playing page — the SAME state path)

**Interfaces:**
- Consumes: `CoverLoader` + `ThumbnailSize::Full`; the existing `PlayerController` transport actions and `PlayerBar` connect_* pattern; `NowPlaying`.
- Produces: `pub struct NowPlayingView` with `widget() -> &adw::NavigationPage`, `set_track(title, artist, album)`, `set_cover(loader, path, token, gen)`, `set_state(PlaybackState)`, `set_position(pos_ms, dur_ms)`, and transport `connect_*` mirroring the bar's.

**Design rule (critical):** the page binds to the SAME `PlayerController` and the SAME actions as the bar — no duplicated playback/seek state. The controller pushes updates to BOTH the bar and the page (extend the existing `set_track`/`set_state`/`set_position` fan-out). This is the same discipline as the MPRIS mirror.

- [ ] **Step 1: Build the page widget** in `now_playing.rs`: an `adw::NavigationPage` wrapping a centered vertical box — big `gtk4::Image` (`set_pixel_size(320)`, fed a 512px texture), a title `gtk4::Label` (CSS `.title-1`), an artist/album `gtk4::Label` (`.dim-label`), a reused seek `gtk4::Scale` with position/duration labels, and a transport row (previous / play-pause / next) plus shuffle/repeat. Expose the setters + `connect_*` listed in Interfaces. (Reuse the bar's transport wiring shape from `player_bar.rs` `connect_previous`/`connect_play_pause`/`connect_next`/`connect_seek` — same closures, same controller actions.)

- [ ] **Step 2: Put content in an `adw::NavigationView`.** In `window.rs`, wrap the current content page so it's the root page of an `adw::NavigationView` (`nav.add(&content_page)`), and give the window that nav view as the `content_page` child of the split view. Keep the Now-Playing page constructed and held (pushed on demand).

- [ ] **Step 3: Bar click opens the page.** In `player_bar.rs`, add a `gtk4::GestureClick` on a non-interactive area of the bar (e.g. the cover+labels box, NOT the transport buttons) that calls an `on_expand` callback (stored `Rc<RefCell<Option<Rc<dyn Fn()>>>>`, cloned-out-before-call per the project's RefCell discipline). Wire `on_expand` in `window.rs` to `nav.push(now_playing.widget())`. Escape / the nav back button returns to the list (built into `adw::NavigationView`).

- [ ] **Step 4: Feed the page from the controller.** In `player_controller.rs`, wherever the bar is updated (`play_track_id`, `apply_event` `StateChanged`/`Position`, `clear_track`), also call the corresponding `NowPlayingView` setter, and load its cover at `ThumbnailSize::Full` with its own generation token. Give the controller a handle to the `NowPlayingView` (constructed in `window.rs`, passed to the controller, or an `Rc` shared both ways as the bar already is).

- [ ] **Step 5: Smoke hook — open the view headlessly.** Add `REPRISE_SMOKE_NOWPLAYING=1` (model on `arm_smoke_repeat`) that, deferred via `glib::idle_add_local_once`, pushes the Now-Playing page and logs `"smoke: opened now-playing view"`, then (optionally) pops it.

- [ ] **Step 6: Build + isolated smoke.**

Run:
```bash
cargo build -p reprise-gnome 2>&1 | tail -3
dbus-run-session -- xvfb-run -a env GDK_BACKEND=x11 REPRISE_AUDIO_SINK=fakesink WAYLAND_DISPLAY= \
  REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_ACTIVATE=1 REPRISE_SMOKE_NOWPLAYING=1 REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=4 cargo run 2>&1 | grep -iE 'now-playing|ERROR|panic' | head
```
Expected: logs "smoke: opened now-playing view", exit 0, no ERROR/panic.

- [ ] **Step 7: Gates + commit**

Run: `cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings && cargo test --workspace 2>&1 | grep 'test result'`
Expected: **388 passed; 1 ignored**. All touched/new files < 800 lines (`wc -l crates/reprise-gnome/src/ui/now_playing.rs crates/reprise-gnome/src/ui/window.rs crates/reprise-gnome/src/ui/player_controller.rs`).

```bash
git add crates/reprise-gnome/src/ui/now_playing.rs crates/reprise-gnome/src/ui/window.rs crates/reprise-gnome/src/ui/player_bar.rs crates/reprise-gnome/src/ui/player_controller.rs crates/reprise-gnome/src/ui/mod.rs
git commit -m "feat: Now-Playing full view (Amberol-style), opened from the player bar"
```

---

### Task 9: Album cover in the track-change notification

**Files:**
- Modify: `crates/reprise-gnome/src/ui/player_controller.rs` (send a `gio::Notification` with the cover on track change)

**Interfaces:**
- Consumes: `cover::{resolve_source, thumbnail, ThumbnailSize::Bar}`; `gio::Application::send_notification`; `NowPlaying.path`.
- Produces: an internal `fn notify_now_playing(&self, title, artist, album, cover_path: Option<PathBuf>)`.

**Note:** No notification exists today — this is greenfield and small. Keep it behind sensible behavior: notify only on an actual track *change* (not on pause/resume), and never make it fatal.

- [ ] **Step 1: Send the notification on track change.** In `play_track_id`, after the cover is resolved for the bar, build and send a `gio::Notification`:

```rust
let notification = gio::Notification::new(&summary.title);
notification.set_body(Some(&format!("{} — {}", summary.artist, summary.album)));
// Reuse the bar-size thumbnail as the notification icon, off nothing hot:
if let Some(source) = reprise_core::cover::resolve_source(std::path::Path::new(&summary.path)) {
    if let Ok(path) = reprise_core::cover::thumbnail(&source, reprise_core::cover::ThumbnailSize::Bar) {
        notification.set_icon(&gio::FileIcon::new(&gio::File::for_path(&path)));
    }
}
if let Some(app) = self.application() {  // reach the gio::Application (via the bar's root/app)
    app.send_notification(Some("now-playing"), &notification);
}
```

(Resolve how the controller reaches the `gio::Application` — either pass an `adw::Application` handle into `PlayerController::new`, or fetch it from a widget's root window. Prefer passing it in at construction; document the choice.)

- [ ] **Step 2: Guard against notify-on-resume.** Only send when the track id actually changed (compare against the previous `now_playing.id`). Ensure a decode failure or missing app handle is a no-op, never a panic (all the `if let`s above already degrade gracefully).

- [ ] **Step 3: Isolated smoke** (a notification portal is absent under xvfb — assert it does not crash, not that a bubble appears):

Run:
```bash
cargo build -p reprise-gnome 2>&1 | tail -3
dbus-run-session -- xvfb-run -a env GDK_BACKEND=x11 REPRISE_AUDIO_SINK=fakesink WAYLAND_DISPLAY= \
  REPRISE_SCAN_DIR=$(mktemp -d) REPRISE_SMOKE_ACTIVATE=1 REPRISE_SMOKE_QUIT=1 REPRISE_SMOKE_QUIT_DELAY_SECS=4 cargo run 2>&1 | grep -iE 'ERROR|panic' | head
```
Expected: exit 0, no ERROR/panic (the send is best-effort; the portal being unavailable headless must not error the app).

- [ ] **Step 4: Gates + commit**

Run: `cargo fmt --check && cargo clippy --all-targets --workspace -- -D warnings && cargo test --workspace 2>&1 | grep 'test result'`
Expected: **388 passed; 1 ignored**.

```bash
git add crates/reprise-gnome/src/ui/player_controller.rs
git commit -m "feat: album cover in the track-change notification"
```

---

## Stage close-out (run once after Task 9)

- [ ] Full gate battery: `cargo fmt --check`; `cargo clippy --all-targets --workspace -- -D warnings`; `cargo test --workspace` → **388 passed; 1 ignored**; `cargo audit` (only the accepted `paste` advisory).
- [ ] **Core purity proof (recorded):** `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` → empty; `cargo build -p reprise-core` standalone. Confirm the ONLY new core dep is `image` and it is pure Rust.
- [ ] **Core-promise proof (recorded):** `cargo test -p reprise-core cover::tests` includes the "thumb path is under the cache dir" assertion; and a manual grep that no code writes to `track.parent()` anywhere in `cover.rs`.
- [ ] File-size gate: `wc -l` over every touched/new file → all < 800.
- [ ] Full isolated E2E battery: standard smoke; bar-position top/bottom; Now-Playing open; activate-track (bar+notification path). All exit 0, no ERROR.
- [ ] **Manual (human) visual checklist** — headless cannot verify rendering: cover actually appears in list rows, bar, and Now-Playing view; placeholder shows for cover-less tracks; covers stay correct while scrolling fast (generation-token check); bar position looks right top and bottom; Now-Playing opens on bar click and Escape returns.
- [ ] Ledger update (`.superpowers/sdd/progress.md`): GUI-A done; `image` dep added to core (purity intact); Now-Playing replaces the dropped floating bar; note the deferred in-memory-cache tuning and the lyrics/color-glow follow-ups.

## Explicitly NOT in this plan (YAGNI / later)

- Online cover fetch, `MetadataProvider` abstraction (later module).
- Album-grid / cover-wall view (later GUI stage).
- Cover editing/embedding (GUI-B tag editor).
- Cache eviction/size policy (the cache is small and safely deletable; revisit with data).
- Lyrics panel + ambient cover color-glow in the Now-Playing view (later extensions).
- Player-bar position selection UI with preview cards (later settings dialog) — GUI-A ships only the persisted state + both attachments + the smoke toggle.

## Known risks

1. **Task 8 (Now-Playing) is the largest, most integration-heavy task** — it touches window navigation, the bar, and the controller fan-out. Mitigation: it binds to the existing controller/actions (no second state path — the explicit design rule), reuses the bar's transport-wiring shape, and is gated by the open/close smoke hook plus the manual visual check.
2. **`adw::NavigationView` introduction** changes the shell's navigation root. Mitigation: the split view keeps sidebar+content; the content page merely becomes a nav-view root so a page can be pushed. Verify the back gesture/Escape works via the smoke + manual check.
3. **`set_data` generational storage on ListItems (Task 6)** is `unsafe` GTK glue. Mitigation: the documented `RefCell<HashMap>` fallback if it trips clippy or reads brittle; the generation-token behavior is the same either way and is the guard against stale covers on recycled rows.
4. **`image` crate build surface** — mitigated by `default-features = false` + an explicit format allowlist; the purity proof (`cargo tree`) is re-run at close-out.
