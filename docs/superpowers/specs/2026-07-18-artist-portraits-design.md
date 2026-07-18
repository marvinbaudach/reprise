# Artist Portraits from Deezer — Design

**Date:** 2026-07-18
**Status:** Approved (brainstorming), ready for implementation plan
**Author:** Marvin + Claude

> **Privacy note (implemented behavior):** Artist portraits are always fetched
> from Deezer, so every viewed artist name is sent to that third party with no
> opt-out. This is intentional and mirrors the always-on `cover_download`
> requests to MusicBrainz and the Cover Art Archive; it supersedes the earlier
> opt-in/toggle passages retained below in this historical design.

## Problem

Artists render only as a deterministic initials-in-a-gradient-circle placeholder
(`artist_avatar.rs`). No real artist portrait is ever loaded — there is no data-model
field, cache, or fetch path. The user wants real artist photos shown in the UI.

This is net-new work, not a bug fix. The initials-gradient is the existing designed
fallback and stays as the base layer.

## Decisions (locked)

- **Source:** Deezer public API (`https://api.deezer.com/search/artist`), no API key.
  Chosen for best coverage of the user's underground-metalcore library.
- **Fetch strategy:** Lazy on artist-detail/info open **plus** throttled prefetch of the
  currently-visible master-list rows, so the list fills in quickly at startup/scroll.
- **Privacy:** New network opt-in module, **default OFF** (consistent with `artist_news`,
  ListenBrainz, Last.fm). Nothing hits the network until the user enables it.
- **Fallback:** The initials-gradient circle is always the base layer. A portrait is an
  overlay shown only when available. Disabled / not-found / offline → gradient stays.
- **Display surfaces (3):** master list rows, artist detail hero, and the info panel's
  artist section.

## Core (`reprise-core`) — new module `artist_portrait/`

Mirrors the `artist_news.rs` pattern: blocking, worker-thread only, injectable fetch fn
for offline-deterministic tests, on-disk cache with TTL + negative markers.

### `deezer.rs` — client + parse

- `GET https://api.deezer.com/search/artist?q=<urlencoded name>&limit=5`, no key.
- **Own** lightweight HTTP getter (`ureq` v3, UA `Reprise/<version>`, ~15s timeout) with an
  **own** process-wide rate throttle (~300 ms min interval via `static LAST_REQUEST:
  Mutex<Option<Instant>>`, same shape as `musicbrainz.rs`). Do **NOT** route through
  `musicbrainz::get` — that would apply MusicBrainz's user-agent and 1 req/s limiter to
  Deezer, which is wrong. Deezer allows ~50 req/5s; 300 ms keeps us far under.
- Parse the `data` array with `serde_json` (fields: `name`, `picture_xl`, `picture_big`).
- **Matching:** accept a candidate only on **exact normalized-name equality** (lowercased,
  whitespace-collapsed) against the query. No exact match → `NotFound`. This prevents
  wrong-portrait mismatches (e.g. an album/track name colliding with an artist).
- **Deezer placeholder guard:** artists with no photo return Deezer's default silhouette
  URL. Detect the known default-image URL/path and treat it as `NotFound` (do not download
  the silhouette as a portrait).
- Image download: pick `picture_xl` (1000²). Reuse `cover_download::http_get_bytes` +
  `validated_image_extension` (20 MB cap, image-format validation).

### `cache.rs` — on-disk cache

- Directory: `cover::cache_dir()/artist-portraits` (i.e. `dirs::cache_dir()/reprise/…`).
- Key: `cover::hash_hex(normalize(name))` (reuse the existing `pub(crate)` helper, same as
  `artist_news` keys its cache).
- Positive entry: image file `<key>.<ext>`. Negative entry: `<key>.notfound` marker.
- TTL via file mtime: **positive 30 days, negative 7 days**. Atomic temp-file + rename.

### Public API (`mod.rs`)

- `portrait_path(name: &str) -> Option<PathBuf>` — pure cache read, **no network**. Returns
  a fresh cached image path or `None`. Used by list rows for instant paint of what's cached.
- `load_or_fetch(name: &str, force: bool) -> Result<PortraitOutcome, PortraitError>` —
  blocking. Fresh positive → return path; fresh negative → `NotFound`; stale/missing →
  fetch from Deezer, store, return. `PortraitOutcome::{ Found(PathBuf), NotFound }`.
- Internally delegate to a `load_or_fetch_with(..., fetch: &mut F)` taking the fetch fn as a
  parameter (default = real getter), plus a fixture-dir env hook, so tests run offline —
  exactly like `artist_news::load_or_refresh_with`.

## GNOME (`reprise-gnome`)

### Module registration

- Add `ARTIST_PORTRAIT_MODULE` to `modules.rs` (`default_enabled: false`, description noting
  "(network; off by default)"). Append to `ALL_MODULES`. Add its id to `plugin_applies_live`
  (`preference_plugins.rs`). This auto-renders a `SwitchRow` on the Plugins preferences page,
  persists via the `settings` table, and applies live — no bespoke UI needed.

### Worker `artist_portrait_worker.rs`

Copy of `artist_news_worker.rs`:

- `ArtistPortraitRuntime` owning an `Rc<Cell<bool>>` `enabled` flag, an
  `async_channel::Sender<Request>` to a single dedicated blocking OS thread
  (`reprise-artist-portrait`) that loops on `recv_blocking()` and calls
  `artist_portrait::load_or_fetch`, plus `EnabledSubscribers` fan-out and `subscribe_enabled`.
- Request kinds: `Hero`, `ListRow`, `InfoPanel`. Each request carries a monotonic
  `generation`. Response carries the same generation + outcome; consumers discard responses
  whose generation they no longer accept (stale artist-switch guard).
- Worker dedupes in-flight requests per artist; cache hits short-circuit without network.
- Enabled gate: `request()` early-returns when disabled or artist blank.

### Display: base gradient + portrait overlay

The gradient avatar stays as the base layer everywhere; the portrait is a `gtk::Picture`
(`content_fit: Cover`, circular via `overflow: Hidden` + CSS `border-radius: 50%`) shown on
top only when loaded. Use a `gtk::Overlay` (gradient `Box` as child, `Picture` as overlay,
hidden until a texture is loaded). Textures load off-thread via
`gdk::Texture::from_filename` in `gio::spawn_blocking`, guarded by a per-widget generation
token (CoverLoader pattern).

- **Detail hero** (`artist_detail_hero.rs`): on `update()` set initials+gradient immediately;
  if the module is enabled, send a `Hero` request; on `Found` for the current generation load
  and reveal the Picture; on `NotFound`/disabled/switch keep the gradient.
- **Master list rows** (`artist_master_row.rs`): in `bind_row` set initials+gradient
  immediately, then (module enabled) send a `ListRow` prefetch request. ColumnView binds only
  visible rows + a small buffer, so this is effectively "visible-row prefetch". The response
  must be validated against the row's **currently bound** artist identity before revealing the
  texture (per gtk4 recycling rule: rebuild the callback per bind, re-check identity). The
  Deezer throttle serialises fetches (~3/s); cache hits are instant.
- **Info panel** (`info_panel/…`): the artist info section already triggers lazily on
  selection change (same place `artist_news` is requested). Add an `InfoPanel` portrait
  request on that same trigger and show the portrait at the top of the artist info section,
  gradient fallback as elsewhere.

### Privacy behaviour on toggle

- Toggle OFF → hide all portraits (Picture hidden), revert to gradient.
- Toggle ON → re-request the currently-open hero, info panel, and visible list rows via
  `subscribe_enabled` (mirror `info_panel.rs`'s existing live-toggle handling).

## Testing

- **Core unit tests** (offline via injected fetch fn + fixture dir):
  - Deezer JSON parsing from a fixture response.
  - Name matching: exact normalized match accepted; near-but-not-exact → `NotFound`.
  - Deezer default-silhouette URL → `NotFound` (no download).
  - Cache freshness/TTL (positive 30d, negative 7d) and `.notfound` negative marker.
- **Headless smoke:** module toggle persists; worker returns `Found` for a fixture image;
  no panic on rapid artist switching (generation discard path).
- **Human verification (headless cannot confirm):** real Deezer fetch end-to-end, circular
  clip rendering, HiDPI sharpness.

## Files

**New:**
- `crates/reprise-core/src/artist_portrait/{mod,deezer,cache}.rs`
- `crates/reprise-gnome/src/ui/library_views/artist_portrait_worker.rs`

**Modified:**
- `crates/reprise-core/src/modules.rs` (new module descriptor)
- `crates/reprise-gnome/src/ui/preferences/preference_plugins.rs` (`plugin_applies_live`)
- `crates/reprise-gnome/src/ui/library_views/artist_detail_hero.rs`
- `crates/reprise-gnome/src/ui/library_views/artist_master_row.rs`
- `crates/reprise-gnome/src/ui/library_views/artist_view_css.rs` (circular Picture class)
- `crates/reprise-gnome/src/ui/info_panel/…` (artist info section: portrait display + request)

No DB migration required — module state lives in the existing `settings` table.

## Out of scope (YAGNI)

- Persisting a per-artist MusicBrainz ID.
- Reading local/embedded artist art (`artist.jpg`, tags).
- Full-library background sync (rejected in favour of lazy + visible-row prefetch).
- Multiple/fallback image sources.
