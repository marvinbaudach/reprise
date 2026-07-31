---
slug: podcasts-radio
worktree: /home/marvin/Projects/reprise-podcasts-radio
branch: feature/podcasts-radio
phase: planned
codex_session:
created: 2026-07-25
foundation_schema: 32
foundation_ux_section: AF
---
# Plan: Podcasts & Radio — two new sources, one UX grammar

Two new library sources under LIBRARY: **Podcasts** (RSS feeds + YouTube channels/playlists via
yt-dlp, audio only) and **Radio** (internet radio favorites via radio-browser.info). Both share one
UX grammar: a sidebar place with a live counter, a table view with filter pills, a tinted
rectangular Add button (never chip-shaped), one Add dialog with a single input field for search OR
URL, removal via context menu/hover star with an undo toast. The deepest single work item is
**playback of non-local media**: the existing `PlaybackBackend` today plays file paths exclusively,
ICY tags flow nowhere, the controller world is track-ID-based. The plan generalizes the instrumental
preview mechanism (`PlaybackMode`) into an external-media path (podcast episode / radio station)
with resume positions, ICY now-playing and live MPRIS — structurally without scrobbling and without
`listen_events`. Base `dev`, branch `feature/podcasts-radio`, its own worktree.

**Grilled decisions (2026-07-26)** — all nine branches decided and worked into the text below:
external mode confirmed (episode MPRIS fully functional except for `CanGoNext`/`CanGoPrevious=false`,
artwork as an `mpris:artUrl` pass-through, "Play next episode" as a toast + a persistent bar button,
GUID as the stable episode identity) · radio module default ON + YouTube switch default ON +
refresh app-wide, but capped and metered-gated · iTunes Search keyless with `country=` from the
system locale (Podcast Index only as an optional provider with a user-owned key, v1.1) ·
unsubscribe chain: commit-time toast "{n} downloads kept · [Delete files]" → trash, never hard;
multiple unsubscribes coalesce · radio pause = disconnect, presented as pause (reconnect "live
now", last ICY title dimmed, reconnect failure never an empty bar) · glyph tiles in v1 carry the
source distinction (remote artwork = v1.1) · CLI/MCP = v1.1 · boundary clones confirmed + a fixed
consolidation task once both features have landed · one branch, waves as planned.

## 1. Context & goal

**Goal:** Two sidebar entries in the LIBRARY section (mockup order Music → Podcasts → Radio →
Queue), counter = unplayed episodes and number of favorites respectively. One dedicated ColumnView
each:

- **Podcasts:** `Date · Episode · Show · Length · Source · Status` — date relative ("Today",
  "Yesterday", "22. Jul"), length H:MM, source pill RSS/YouTube (icon + label, outlined neutral),
  status pill New (accent) / Resume (accent outline) / Played (dim). Default sort date descending.
  Filters: Unplayed / Show / Source.
- **Radio:** `State icon(24px) · Station · Genre · Bitrate · Country · Now playing` — the playing
  station fully accented (icon, name, now-playing, row tint accent 7 %), idle "—"; bitrate
  "320k", country as an ISO code. Filters: Genre / Country.

Toolbar above every table: a **tinted rectangular Add button** (accent-bg 16 %, radius 8, plus icon
+ "Add podcast"/"Add station" — NOT a pill), next to it the "+ Add filter" chip + active pills, on
the right dim "23 episodes"/"12 stations". Add dialog: one `adw::Dialog`, one input field for search
terms or a URL — search → grouped results with Subscribe/Add buttons; URL → type detection →
preview card + options → Confirm.

**Playback:** everything through the existing GStreamer pipeline (`playbin3` plays http(s) via
souphttpsrc); radio and YouTube audio only. Radio is live: no seek, no duration, elapsed +
ICY now-playing in the player bar and MPRIS. Podcasts: seekable, resume position persisted on
pause/stop/switch/quit. **Podcasts/radio never scrobble** and produce no `listen_events` (My Stats
stays pure music statistics).

**Placement — three inherited patterns, two zones of new territory:** Inherited: (a) the fetch
infrastructure of the News/Concerts family (worker thread with its own DB connection, TTL +
deterministic jitter, fixture seam, module gate per NET-1) for the podcast refresh; (b) the view
pattern of the Concerts views (a dedicated small ColumnView instead of a TrackList rebuild,
filter/sort pure, no windowed model — Concerts decision 3 generalized); (c) tombstone undo
(`removed_at` + a 10 s high toast, the `missing_view.rs` idiom). New territory (code finding,
corrects the scout map): **subprocess wrapper** — no `std::process::Command` in production code —
and **tag/ICY plumbing** — the bus watch in `player.rs` only handles Eos/StreamStart/Element/Error;
a `MessageView::Tag` arm does NOT exist and will be built new.

**Canonical design source:** claude.ai/design project `8fb24732-431c-447f-9a74-08d3229a0c33`,
`Tourdaten Varianten.dc.html`, turn 4 (podcasts, ~l. 205–413), turn 5 (radio, ~l. 21–205). Dark
`#0c0e0f/#1b1e1f/#1f2324`, accent `#35c793` = the existing redesign, no new color tokens.

## 2. Architecture overview & crate split

Guardrail: **all decision logic as pure, testable functions in `reprise-core`** (no
gtk4/gstreamer/zbus — `cargo tree` gate); GTK thin; files < 800 lines (target 200–400), `window.rs`
< 600 (`check-architecture.sh`).

- **`crates/reprise-core`:** facades `src/podcasts.rs`+`src/podcasts/` and
  `src/radio.rs`+`src/radio/` (pattern `browser.rs`+`browser/`). Podcasts: feed parser (quick-xml —
  **already a core dependency** via the Rhythmbox import), iTunes search, yt-dlp wrapper +
  YouTube provider, URL detection, refresh (conditional GET), store/query/status, downloads. Radio:
  the radio-browser boundary (server discovery, search, click/re-resolve), M3U/PLS parser, ICY
  probe, favorites store. Plus `src/db_podcasts_radio.rs` (migration **V32**, see 3 and 13) and
  playback extensions in `src/playback.rs` + `src/media_integration.rs` (see 6). **No new
  dependencies** (ureq, quick-xml, serde_json, chrono, url, thiserror are present; yt-dlp only needs
  `std::process`).
- **`crates/reprise-platform-linux`:** `player.rs` gains (a) the trait method `play_uri` — today
  `path_to_uri` rejects everything without a leading `/`, http(s) is currently UNPLAYABLE — and (b)
  the new `MessageView::Tag` arm (ICY `title`/`organization` → `PlayerEvent::StreamTags`). `mpris/`
  learns live streams (CanSeek=false, metadata without length, non-track identity).
- **`crates/reprise-gnome`:** `src/ui/podcasts/` and `src/ui/radio/` (view, model, columns,
  presentation, filter bar, empty states, Add dialog, CSS; podcasts additionally a worker),
  `src/ui/playback/external_media.rs` (external mode, generalizes `preview.rs`),
  the player bar live state, `strings_podcasts.rs` + `strings_radio.rs`, preferences,
  sidebar/routing wiring.
- **CLI/MCP:** **no** surface in v1 (grilled decision: a named v1.1 candidate, see 12 — the
  Concerts grill tipped it the other way there; here the playback rebuild is the center of gravity
  for risk; the surface stays additively retrofittable, a package-M clone without file conflicts).

**Module gates** (`modules.rs`, `ALL_MODULES`): `PODCASTS_MODULE` (`id: "podcasts"`,
`default_enabled: false` — a scheduled feed refresh is AUTOMATIC network traffic, NET-1;
`applies_live: true`; the description names feeds AND yt-dlp) and `RADIO_MODULE` (`id: "radio"`,
**`default_enabled: true` — grilled decision**: the sharpened rule reads "modules with AUTOMATIC
network traffic start off", and radio only transmits on user action; the description discloses the
radio-browser click counter. The binding condition of the default-ON is the radio empty state with
its Add-station CTA, 7.5/SRC-1 — a visible menu entry without content must never be a dead
end). Sidebar rows only when the module is active (conversions gate idiom); toggle →
`sidebar.refresh(reason)`.

**Threading model (decision):** ONE persistent worker, only for podcast refresh + downloads
(`reprise-podcasts` thread, a clone of the `artist_news_worker` idiom: async_channel, its own
connection via `db::open_migrated`). All **user-triggered one-off operations** — search
(iTunes/radio-browser/ytsearch), URL preview/probe, yt-dlp play resolve, the click counter,
re-resolve, a manual download trigger — run as `one_shot_task::spawn` threads with a generation
guard (latest wins), so they never queue up behind a refresh. Radio has **no** worker (nothing is
periodic).

## 3. Data model & migration V32

New file **`crates/reprise-core/src/db_podcasts_radio.rs`** (pattern `db_artist_news_fetch.rs`):
`migrate_v32(conn)` — idempotent version check, `unchecked_transaction`, `execute_batch`,
`user_version` bump. `db.rs`: `SUPPORTED_SCHEMA_VERSION` → **32** + a call line after
`db_concerts::migrate_v31`. **Number caveat:** dev stands at 30 today, Concerts plans 31 — the
foundation task verifies the dev HEAD at the start and takes the next free number (rule in 13).
Migration tests in `db_podcasts_radio_migration_tests.rs` (upgrade, idempotence, downgrade
protection).

```sql
CREATE TABLE IF NOT EXISTS podcast_subscriptions (
  id              INTEGER PRIMARY KEY,
  kind            TEXT NOT NULL,              -- 'rss' | 'youtube'
  feed_url        TEXT NOT NULL UNIQUE,       -- Feed-URL bzw. kanonische Kanal-/Playlist-URL
  title           TEXT NOT NULL,
  author          TEXT,
  image_url       TEXT,                       -- v1 stores it only, does not render it
  etag            TEXT,                       -- conditional GET (RSS only)
  last_modified   TEXT,
  last_fetch_at   INTEGER,
  last_outcome    TEXT,                       -- 'ok' | 'not_modified' | 'failed'
  auto_download   INTEGER NOT NULL DEFAULT 0,
  added_at        INTEGER NOT NULL,
  removed_at      INTEGER                     -- Tombstone (Undo-Fenster)
);

CREATE TABLE IF NOT EXISTS podcast_episodes (
  id              INTEGER PRIMARY KEY,
  subscription_id INTEGER NOT NULL REFERENCES podcast_subscriptions(id) ON DELETE CASCADE,
  guid            TEXT NOT NULL,              -- Dedupe; Fallback = enclosure-/Video-URL
  title           TEXT NOT NULL,
  audio_url       TEXT NOT NULL,              -- enclosure-URL bzw. YouTube-watch-URL (NIE Stream-URL)
  page_url        TEXT,
  published_at    INTEGER,                    -- NULL erlaubt (flat-playlist ohne Datum)
  duration_secs   INTEGER,                    -- itunes:duration; NULL -> probe on first play
  downloaded_path TEXT,
  played_at       INTEGER,                    -- NULL = unplayed
  position_ms     INTEGER NOT NULL DEFAULT 0, -- Resume-Position
  first_seen_at   INTEGER NOT NULL,
  UNIQUE(subscription_id, guid)
);
CREATE INDEX IF NOT EXISTS idx_podcast_episodes_sub ON podcast_episodes(subscription_id);
CREATE INDEX IF NOT EXISTS idx_podcast_episodes_pub ON podcast_episodes(published_at);
CREATE INDEX IF NOT EXISTS idx_podcast_episodes_unplayed ON podcast_episodes(played_at) WHERE played_at IS NULL;

CREATE TABLE IF NOT EXISTS radio_stations (
  id              INTEGER PRIMARY KEY,
  uuid            TEXT UNIQUE,                -- radio-browser stationuuid; NULL for a manual URL
  name            TEXT NOT NULL,
  stream_url      TEXT NOT NULL UNIQUE,       -- resolved stream URL (after the M3U/PLS down-parse)
  homepage        TEXT,
  favicon_url     TEXT,                       -- v1 stores it only
  genre           TEXT,
  codec           TEXT,
  bitrate_kbps    INTEGER,
  country_code    TEXT,
  votes           INTEGER,
  added_at        INTEGER NOT NULL,
  removed_at      INTEGER                     -- Tombstone (Undo-Fenster)
);
```

- **Status derivation (pure, `podcasts/status.rs`):** `Played` ⇔ `played_at IS NOT NULL`; otherwise
  `Resume` ⇔ `position_ms > 0`; otherwise `New`. The "unplayed" counter = `played_at IS NULL` (New +
  Resume). The end of an episode (`TrackFinished`) sets `played_at = now`, `position_ms = 0`.
- **Tombstone semantics:** every query filters `removed_at IS NULL` (counters drop immediately).
  Undo = `removed_at = NULL`; commit (toast expiry/dismiss) = a hard delete (episodes via CASCADE).
  Re-subscribing inside the undo window: `INSERT … ON CONFLICT(feed_url) DO UPDATE SET removed_at =
  NULL` — revives the existing record instead of duplicating it (analogously for `radio_stations`
  via `stream_url`/`uuid`).
- **Episode identity = GUID (grilled decision, reversibility):** `UNIQUE(subscription_id, guid)` is
  the ONLY stable key — resume position, played state and `downloaded_path` hang off the GUID
  without exception, never off file or URL keys; YouTube GUID = video ID. This makes the
  the implemented queue citizenship possible without re-keying: the `podcast_episodes.id` upserted
  by GUID is the queue identity. Re-subscribing still finds orphaned downloads again
  deterministically (7.4).
- **Settings** (`settings` table, `library::settings`): `podcasts.import_count` (25),
  `podcasts.auto_download_default` (false), `podcasts.cleanup_policy` (`keep_all` |
  `delete_played_7d` | `keep_last_5`), `podcasts.youtube_enabled` (true), `podcasts.ytdlp_path`
  (empty = PATH), `sources.refresh_hours` (6, 1–24), `radio.search_order` (`votes` | `name` |
  `clicks`), sticky filters `podcasts.filter.*` / `radio.filter.*`. Bundled reads in
  `podcasts/config.rs` and `radio/config.rs` respectively.
- **Download location:** `{XDG_DATA_HOME}/reprise/podcasts/{fnv1a(feed_url)}/{fnv1a(guid)}.{ext}`
  (the `dirs` crate as for the DB path) — **GUID-keyed instead of row-ID-keyed** (grilled decision):
  the path is stable across unsubscribe/re-subscribe; if the target file already exists at refresh
  time, `downloaded_path` is set instead of downloading again (reclaim of orphaned files). The path
  goes in `downloaded_path`; the cleanup policy runs at the end of every refresh run (pattern
  `enforce_retention`).

## 4. Podcasts core

### 4.1 HTTP boundary (`podcasts/http.rs`, new)

Clone of the `musicbrainz.rs` idiom (deliberate duplication instead of a cross-branch refactor, see
13): its own `Mutex<Option<Instant>>` limiter (1 req/s), `HTTP_TIMEOUT = 15 s`, UA
`Reprise/{version} ( musicbrainz::CONTACT_URL )`, `PodcastError` (thiserror:
`Timeout`/`Transport`/`HttpStatus(u16)`/`Body`/`Parse`/`NotModified`/`YtDlp(String)`),
**fixture seam** `REPRISE_PODCASTS_FIXTURE_DIR` with a route enum. **Conditional GET** (new in the
repo, no precedent): the request carries `If-None-Match`/`If-Modified-Since` from the subscription;
304 → `NotModified` (only a `last_fetch_at` bump); 200 → `ETag`/`Last-Modified` back into the
subscription.

### 4.2 Feed parser (`podcasts/feed.rs`, new — pure)

quick-xml streaming across RSS 2.0 AND Atom: `parse_feed(xml, limit) -> Result<ParsedFeed,
PodcastError>` with `ParsedFeed { title, author, image_url, episodes }`, `ParsedEpisode { guid,
title, audio_url, page_url, published_at, duration_secs }`:

- `enclosure` whose `type` starts with `audio/` (fallback: the first enclosure); items without an
  enclosure are discarded (no audio = no episode).
- `guid` (Atom: `id`); missing → the enclosure URL as the dedupe key (a declared interpretation).
- `pubDate` RFC-2822 via chrono, Atom `published`/`updated` RFC-3339; unparsable → `None` (stably at
  the end, cell "—").
- `itunes:duration` tolerant ("4533", "75:33", "1:15:33" → seconds); often missing → `None`, the
  duration is filled in from the position tick on the first play (6.3).
- `image`/`itunes:image` → `image_url`; match namespace prefixes via the local name (feeds are
  dirty).

### 4.3 Search providers & URL detection

- **iTunes Search** (`podcasts/itunes.rs`): `GET
  https://itunes.apple.com/search?media=podcast&term={q}&limit=12&country={CC}` — keyless; parse
  `results[] { collectionName, artistName, feedUrl, trackCount }`, rows without a `feedUrl` are
  dropped. **`country=` from the system locale (grill guardrail):** iTunes is store-scoped — without
  a territory, German feeds for instance are missing. Pure function `locale_country(locale) -> &str`
  (territory part: `de_DE.UTF-8` → `DE`; unparsable/empty/`C` → `US`) + a test. **Decision
  (mockup deviation, grilled):** iTunes instead of Podcast Index — Podcast Index requires key
  registration (the Concerts lesson: obtaining a key is the adoption killer); the header is honest,
  "PODCASTS · APPLE PODCASTS" (the mockup will be brought in line). Podcast Index comes, if at all,
  ONLY as an optional provider with a USER-OWNED key behind the same provider interface — never as
  an embedded shared key in the OSS repo; a named v1.1 candidate (12).
- **URL detection** (`podcasts/url_detect.rs`, pure): `detect(input) -> { Search | YoutubeUrl |
  ProbableFeedUrl }` — `http(s)://` + host match
  (`youtube.com/@…|/channel/|/playlist?list=|youtu.be/`) → YouTube; other URLs → feed candidate
  (the preview verifies: content type xml / body starts with `<?xml`/`<rss`/`<feed`); otherwise a
  search.
- **Preview:** feed URL → one `http::get` + `parse_feed` (title, episode count); YouTube URL → one
  `--flat-playlist -J` for a stable channel identity. The regular channel fetch derives from it the
  official keyless long-form feed
  `videos.xml?playlist_id=UULF…` (channel ID `UC…` → `UULF…`, at most 15 entries with a publication
  date, Shorts excluded). Runs in the one_shot_task or the worker respectively.

### 4.4 yt-dlp wrapper (`podcasts/ytdlp.rs`, new — subprocess new territory)

A thin `std::process::Command` wrapper, entirely in core (std only):

- **Binary discovery:** `REPRISE_YTDLP_BIN` (env, at the same time the test seam for the fake
  script) → setting `podcasts.ytdlp_path` (developer override) → the helper shipped with Reprise in
  the `libexec` directory → `"yt-dlp"` on the PATH. `probe_version()` (`--version`, 10 s) feeds the
  preferences row and the availability gate. Release packages treat the helper as a runtime
  component, not as a prerequisite the user has to satisfy manually.
- **Explicit browser session:** the persisted setting `podcasts.youtube_browser` passes the yt-dlp
  browser identifier chosen in the Plugins window (e.g. `brave`) on to listing, search, resolve and
  download as `--cookies-from-browser`. Without that choice Reprise reads no browser cookies; the
  visible opt-out is authoritative and cannot be overridden by an environment variable.
  `REPRISE_YTDLP_COOKIES_FROM_BROWSER` remains only as a low-level seam for explicit package
  diagnostics and tests of `YtDlp::discover`, never as a hidden fallback of a product path. This is
  deliberately an opt-in: the call can be made as the YouTube account signed in to the browser;
  Reprise stores and logs neither the cookies nor the browser profile path.
- **Calls** (all `--no-warnings`, stdout=JSON, timeout with a kill via a `try_wait` loop in
  100 ms slices): `list(url)` = `--flat-playlist -J {url}` (60 s, legacy fallback);
  `list_range(url, 40)` = `--flat-playlist -I 1:40 -J {url}` only on "Load more";
  `search(terms)` =
  `--flat-playlist -J ytsearch5:{terms}` (60 s); `resolve(video_url)` = `-f bestaudio -j {url}` (45
  s) → `.url` + `.duration`; `download(video_url, out)` =
  `-f bestaudio -x --audio-format opus -o {out} {url}` (600 s, worker only).
- **Error mapping → readable messages (never a crash):** stderr classification as a pure table
  function: `Sign in to confirm`/`not a bot`/`429` → "YouTube blocked the request — update
  yt-dlp (Preferences)"; `ENOENT` → "YouTube component is unavailable — reinstall or repair
  Reprise"; otherwise a shortened stderr line. Everything as `PodcastError::YtDlp(msg)`.
- **Listing reality:** the official UULF feed delivers the dated long-form window in provider order.
  Only the extended yt-dlp window delivers `id`, `title`, `duration` (often null), but no reliable
  date; these additions sit behind the dated entries and keep their provider order within a fetch.
  `audio_url` = `https://www.youtube.com/watch?v={id}`; the **bestaudio stream URL is resolved
  exclusively at play time and NEVER persisted** (it expires after hours).
- **Feature gate:** `podcasts.youtube_enabled` (**default on — grilled decision**: the module
  opt-in is the moment of consent, the informed moment is the Add dialog's "audio only via
  yt-dlp"; the switch is the emergency stop). If the binary is missing or breaks, the
  **degradation is pure DISPLAY, never an auto-toggle**: the preferences switch shows the state
  readably (subtitle "YouTube component is unavailable — reinstall or repair Reprise", see 8), the
  setting is NEVER silently flipped; instead of the YouTube section the Add dialog shows the notice
  line; existing YouTube subs show the readable message on play.
- **Packaging (its own release task, not a current UI task):** Flatpak bundles a pinned yt-dlp
  version as a manifest module; native packages install yt-dlp via a hard package dependency or ship
  a private helper in `libexec`. The download with `-x --audio-format opus` additionally needs the
  ffmpeg program; package and Flatpak therefore provide yt-dlp + ffmpeg together and Reprise sets
  `--ffmpeg-location` where needed. No silent downloading of executable code on the first YouTube
  click. In Flatpak there is no `yt-dlp -U`: updates of the pinned component arrive with the Reprise
  package.

### 4.5 Refresh pipeline & worker

- **`podcasts/refresh.rs` (pure):** `refresh_due(last_fetch_at, now, jitter)` with the base interval
  `sources.refresh_hours` (default 6 h) + deterministic jitter (FNV-1a over the DB path — a helper
  clone from `artist_news_refresh.rs`; if Concerts lands first and makes `fnv1a_64` `pub(crate)`, it
  gets reused — see 13).
- **`podcasts/pipeline.rs`:** `refresh(conn, fetch, ytdlp, now, force) -> RefreshSummary` — serially
  over the active subscriptions: RSS → conditional GET → parse → upsert by `(subscription_id, guid)`
  (**`first_seen_at`/`played_at`/`position_ms` stay untouched by the upsert**, only metadata is
  updated); YouTube → `list()` → the same upsert mechanics. An error per subscription →
  `last_outcome = 'failed'`, the run continues (FB-3). Afterwards auto-downloads (max. 3 new
  episodes per run and subscription) + the cleanup policy.
- **Worker (`ui/podcasts/podcasts_worker.rs`):** 1:1 after `artist_news_worker.rs` (`PodcastsRuntime
  { enabled, worker, subscribers }`, requests `Refresh { generation, force }` / `Download {
  episode_id }`, replies via async_channel + `glib::spawn_future_local`). **Trigger (deviation
  from Concerts — grilled decision: app-wide, capped):** an app-start check + an hourly due check
  at **window level** (not view-bound) — the unplayed badge should be correct without a visit to the
  view. **Cap:** the timer only runs when the module is on AND ≥ 1 active subscription exists; the
  start check and the hourly timer **coalesce via the `refresh_due` TTL** (the same pure check
  decides both — never a double fetch). **Metered gate:**
  `gio::NetworkMonitor::is_network_metered()` is checked at the GTK trigger (core stays free of
  network state): metered ⇒ auto-refresh pauses, a manual "Refresh now" stays allowed. The
  gating decision itself is a pure function (inputs: enabled, sub_count, metered, due) with a test.
  Plus view-open staleness + "Refresh now" in the footer (NR-6 idiom: spinner + inline failure,
  never a rain of toasts).

## 5. Radio core

### 5.1 radio-browser boundary (`radio/http.rs` + `radio/servers.rs`, new)

Server discovery: `GET https://all.api.radio-browser.info/json/servers` → the server list; pick at
random, cache per process; on failure try the next one (max. 3). Pure selection/rotation policy;
fixture seam `REPRISE_RADIO_FIXTURE_DIR` (`servers.json`, `search-{term}.json`,
`click-{uuid}.json`). The same limiter/UA/timeout idioms as 4.1, its own `RadioError`; a meaningful
UA is mandatory for radio-browser — the existing UA string satisfies it.

### 5.2 Search, click, re-resolve

- **Search (`radio/search.rs`):** `GET
  {server}/json/stations/search?name={q}&order={votes|name|clickcount}&reverse=true&limit=50&hidebroken=true`
  → `StationCandidate { uuid, name, url_resolved, codec, bitrate, country_code, tags, votes, favicon
  }`; sub-line "Metal · 320 kbit/s · US · 4.2k votes" as a pure formatter.
- **Click + re-resolve in one (`radio/click.rs`):** every play of a uuid station sends `GET
  {server}/json/url/{uuid}` (the etiquette) — the response at the same time contains the **fresh
  stream URL**, which updates the stored value. Endpoint down → play with the stored `stream_url`
  (the click is best effort, never blocks > 5 s). **Dead stream** (GStreamer error after connect):
  re-resolve once via uuid + play again; only then a readable error toast (6.3). Stations without a
  uuid skip both.

### 5.3 M3U/PLS & ICY probe (`radio/playlist.rs` + `radio/icy.rs`, pure)

- `resolve_playlist(body, kind) -> Option<String>`: PLS (`[playlist]`, `File1=`) and M3U/M3U8 (the
  first non-`#` line). **HLS:** the body contains `#EXT-X-` → the input URL itself is the stream URL
  (the manifest belongs to GStreamer). Nesting max. depth 1.
- `parse_icy_headers(headers) -> IcyProbe { name, bitrate_kbps, genre, content_type }`: the dialog
  preview sends `Icy-MetaData: 1`, reads only response headers (`icy-name`, `icy-br`,
  `icy-genre`, `Content-Type`), closes without a body — a pure header-map function, the boundary
  call in the one_shot_task.
- Add option "Fetch logo & tags from radio-browser" (on): one `/json/stations/byurl?url=` call adds
  uuid/favicon/tags/votes where known.

## 6. Playback integration (the critical path)

### 6.1 Backend (`reprise-core/src/playback.rs` + `platform-linux/src/player.rs`)

- **`PlaybackBackend::play_uri(&self, uri: &str)`** (new trait method): accepts
  `http`/`https`/`file`; the `Player` impl shares `reset_transition` + `try_play` + the rebuild retry
  with `play` (an internal helper). `play(path)` stays strictly file-path-based.
- **`PlayerEvent::StreamTags { title: Option<String>, organization: Option<String> }`** (new):
  `attach_bus_watch` gains a `MessageView::Tag` arm — `gst::tags::Title`/`Organization` out of the
  TagList, emitted only on change (the last value in the watch state, like `spectrum_analyzer`).
- **Gapless contract:** external play parks the pre-feed (`set_next(None)` in the controller + a
  reset in `play_uri`). An episode in the manual queue also sets `set_next(None)` at its boundary;
  the outcome stays gapless-free, even though after it ends it continues via the normal queue
  advance. Radio/YouTube never get into the `about-to-finish` handoff.
- **Duration probe:** the existing position ticker supplies `duration_ms` — no special backend code.

### 6.2 MPRIS (`media_integration.rs` + `mpris/`)

`MprisState` grows by `live_stream: bool` and `external_ref: Option<String>` (identifier
`podcast/{id}` or `radio/{id}`). Pure predicates adjusted + tested: `can_pause`/`can_play`
count `external_ref` as loaded; **`can_seek` = track OR (external ∧ !live_stream)**;
`build_metadata` builds the trackid path `/org/reprise/Reprise/episode/{id}` for podcasts, still
`/org/reprise/Reprise/external/{ref}` for other external media, and
**omits `mpris:length` when `live_stream`**; `metadata_differs` sees the new fields
(ICY change → PropertiesChanged). Radio: `xesam:title` = StreamTitle (fallback the station name),
`xesam:artist` = [station name]; podcast: episode title / [show] / length, without album or rating.

Three sharpenings out of the grill (external never looks broken from the outside):

- **Narrowed by POD-21:** `can_go_next`/`can_go_previous` are false only for external **without**
  episode neighbors. A podcast/YouTube session with a frozen list context
  reports the neighbors that actually exist: on a direct start, out of the rendered episode list;
  when the origin is the "manual queue", out of its typed order.
  Radio and contextless episodes stay false. Media keys thus follow the same boundaries as the
  visible buttons; queue citizenship itself is governed by QUE-9.
- **`mpris:artUrl` = a remote URL pass-through:** podcast → the persisted `image_url`, radio →
  `favicon_url` (if present). GNOME Shell loads it itself — there is still **no
  in-app image downloader** (non-goal 8 stays untouched).
- **Radio pause is also the MPRIS truth:** the presented pause state (6.3/6.4) reports
  `PlaybackStatus = Paused` and `CanPause = true`, even though the pipeline is disconnected.

### 6.3 Controller: external media mode (`ui/playback/external_media.rs`, new)

Generalization of the preview pattern (`preview.rs` stays functional, its enum is extended):

```rust
pub(in crate::ui) enum PlaybackMode { Queue, QueuedEpisode, Preview, Podcast, Radio }
// advances_queue_on_finish: Queue | QueuedEpisode; credits_listening: Queue only
pub(in crate::ui) enum ExternalMedia {
    Podcast { episode_id: i64, title: String, show: String,
              source: EpisodeSource /* Url(String) | File(String) */,
              resume_ms: i64, duration_ms: Option<i64> },
    Radio   { station_id: i64, name: String, stream_url: String, uuid: Option<String> },
}
```

- **`play_external(media)`:** like `play_preview` — `evaluate_play_tracking` (closes the previous
  session), `current_track = None` (**structurally no play credit, no scrobble, no
  listen_event** — `begin_scrobble` is never reached), `sync_lyrics_track(None)`, `set_next(None)`,
  a marked `NowPlaying`, then `play_uri` (or `play(path)` for a downloaded episode).
  Podcast: after `Ok`, one `seek_to(resume_ms)`; if the early seek fails, the first position event
  with `duration_ms > 0` makes it up once (pure resume policy, tested).
- **YouTube play:** activation → immediate reaction (P-2): the bar shows the episode + a "Resolving
  audio…" state; a one_shot_task calls `ytdlp::resolve`; the generation guard discards stale
  resolutions; error → readable toast (FB-1), the bar falls back to Stopped.
- **Events (podcast):** `TrackFinished` → `mark_played` + `end_external()` (stop, no
  auto-advance — a named v1.1 candidate, 12) + a sidebar/view refresh, **plus the "Play
  next" hook-up (grilled decision):** the pure query
  `podcasts::query::next_unplayed_of_show(subscription_id, after_published_at)` returns the
  next UNPLAYED episode of THE SAME show by date — never "the next table row". It
  feeds **two offers of the same action**: (a) a toast ~10 s "Play next: “{title}”" with an
  action button directly after the episode ends, (b) a **persistent "Play next
  episode" button in the empty/stopped player bar** (6.4) that does not disappear with the toast.
  Playback never starts automatically.
- **Events (radio, pause=disconnect — grilled decision):** pause (bar button/MPRIS) → the pipeline
  stops (disconnect), the controller holds the station as **presented-paused** (bar/MPRIS:
  Paused); play → reconnect "live now" (a fresh `play_uri`, elapsed starts over).
  Stream drop-out/`PlayerEvent::Error` → one re-resolve via uuid (5.2) + reconnect; if that fails
  too, the station stays **presented-paused with a readable inline error + retry in the
  bar — never back to an empty bar**; the table shows "—" for the paused station (RAD-1).
  The queue skip path (`playback_faults.rs`, FB-6) stays queue-only.
- **Position persistence (podcast):** throttled every 5 s out of the position tick + on
  pause/stop/switch/app quit (`podcasts::store::save_position`); the first duration > 0 when
  `duration_secs IS NULL` → filled in. The quit hook sits at the same place as the session
  persistence.
- **StreamTags:** the controller holds `on_stream_tags` callbacks; the radio view (now-playing
  cell), the player bar and the MPRIS mirror are fed from **one** event. Session restore does not
  restore external playback (a non-goal; the episode stays reachable via "Resume").

### 6.4 Player bar & mini player

`player_bar_state.rs` gains a display mode (a pure derivation): **radio/live:** the waveform is
hidden, a geometrically identical placeholder (P-4/PLAY-7b: nothing shifts), time = elapsed only
(wall clock since play start — live position values are unreliable depending on the source; pure
formatters), seek/drag disabled, title = the ICY StreamTitle, sub-line = the station name. **Radio
paused (grilled decision):** the bar keeps the station (play symbol, MPRIS Paused), the **last
ICY title stays visible DIMMED** (the past, not live info); reconnect failures appear
as an inline line with a retry, never as an empty bar. The split is deliberate: **the table is
live truth (a paused station = not connected = "—", RAD-1), the bar is
session memory** (the dimmed last title). **Podcast:** the waveform in its fallback shape (flat —
the draw path can do that, there are simply no peaks), seek active, "Elapsed / Total" as soon as the
duration is known; after an episode ends the stopped/empty bar shows the **persistent "Play next
episode" button** (6.3) as long as an unplayed episode of the same show exists.
**Mini player audit (MINI-1..4):** the same state feeds the 46-bar compact waveform — in
live mode likewise a placeholder, including the pause presentation; a checklist item in E3.

## 7. UI

### 7.1 ViewSource, sidebar, routing

- `view_source.rs`: `ViewSource::Podcasts` + `ViewSource::Radio` (+ label tests); `browser.rs`
  BrowserPlace pairs analogous to `MyStats`; `browser/navigation.rs` SidebarTargets; `ui/nav_history.rs`
  intent arms. Session deserialization is lenient (downgrade → library root, a Concerts finding).
- **Sidebar** (`sidebar_rebuild.rs`): two rows in the LIBRARY section **between Music and Queue**,
  module-gated; counts via the existing count block: `podcasts::count_unplayed(conn)` /
  `radio::count_stations(conn)` via `nonzero_count`. `sidebar_presentation.rs`: `NavIcon::Podcasts`
  = `audio-input-microphone-symbolic` (Adwaita/devices, verified), `NavIcon::Radio` =
  `network-wireless-symbolic` (airwaves metaphor; **`radio-symbolic` is the radio BUTTON glyph —
  do not use**), runtime fallback via `IconTheme::has_icon` → `network-cellular-symbolic`.
  Optics = a manual pass.
- **Routing:** `window.rs` `content_stack.add_named(…, Some("podcasts"))`/`Some("radio")` next to
  `"stats"` (~line 330; construction encapsulated in `ui/podcasts/mod.rs::install` /
  `ui/radio/mod.rs::install` — the window.rs budget), `library_shell.rs::wire_source_routing`
  (~line 140) both branches, `track_list_smoke::parse_smoke_source` extended by
  `"podcasts"`/`"radio"`.

### 7.2 Table views (`ui/podcasts/`, `ui/radio/`, new)

The pattern = the Concerts view: a dedicated small ColumnView, `gio::ListStore` + `SingleSelection`,
filter/sort pure over `Vec<Row>`, no windowed model (dozens of stations, hundreds to a few thousand
episodes; the threshold "> 5000 → retrofit windowing" is noted as a risk). Files per feature
(150–350 lines): `mod.rs` (+ `install`), `*_view.rs` (filter row + `GtkStack` list/status + footer),
`*_model.rs`, `*_columns.rs` (SignalListItemFactory, label recycling), `*_presentation.rs` (pure
formatters: relative dates, H:MM, "320k", pill mappings, sorting, count lines, elapsed),
`*_filter_bar.rs`, `*_empty_state.rs`, `add_dialog.rs`, `css.rs`; podcasts additionally
`podcasts_worker.rs`.

- **Podcasts columns:** Date (relative; the only sortable column, default descending) · Episode
  (1.65fr) · Show (0.95fr) · Length · Source (pill `application-rss+xml-symbolic`+"RSS" or
  `video-x-generic-symbolic`+"YouTube", outlined neutral) · Status (pill New/Resume/Played).
  Activation (double-click/Enter) = **Play** (in the spirit of NAV-4; resume from the stored
  position); the playing episode carries a row tint accent 7 %.
- **Radio columns:** state icon (playing `audio-volume-high-symbolic` accent, idle
  `network-wireless-symbolic` dim) · Station · Genre · Bitrate · Country · Now playing (ICY only for
  the playing station, otherwise "—"). Activation = Play; activating the playing station again =
  **Stop** (grill-confirmed, uncontested); the pause button has its own model
  (disconnect-presented-as-pause, 6.3/6.4) — the **paused** station counts as not connected
  and shows "—" in the table. Sort: Station A–Z.
- **Toolbar:** the Add button on the left (`buttons.rs` gains `ADD_ACTION_CLASS` "reprise-btn-add" —
  accent-bg 16 %, radius 8, BTN-1..4 states centrally; deliberately NOT `.reprise-filter-chip`),
  then the "+ Add filter" MenuButton + chips (`CHIP_CSS_CLASS` becomes `pub(in crate::ui)` — the
  identical foundation line as in the Concerts plan, see 13), on the right the dim total; the
  `FILTER_BAR_MIN_HEIGHT` idiom against layout shift (in the spirit of FIL-2).
- **Filters:** podcasts `Unplayed` (bool) / `Show` (facet: subscription title) / `Source`
  (RSS|YouTube); radio `Genre` / `Country` (DISTINCT facets). Sticky, "Clear all ×", "X of Y
  episodes/stations"; 0 hits with filters active → a StatusPage with exactly one "Show all N …" step
  (in the spirit of FIL-6).

### 7.3 Add dialogs (`add_dialog.rs` per feature)

One `adw::Dialog` (precedent: the tag editor form; SET-3: level 1), title centered, ✕ on the right,
one input field with the hint "or paste RSS / YouTube URL" or "or paste a stream / M3U / PLS URL"
respectively. State machine pure: `Idle → Searching → Results | UrlDetected → Previewing → Preview |
Error`.

- **Search** fires on Enter/submit (never per keystroke); for podcasts it fans out into two
  one_shot_tasks: iTunes (fast) + `ytsearch5:` (slow, the section fills in afterwards with a row
  spinner). Section headers in small caps "PODCASTS · APPLE PODCASTS" / "YOUTUBE · audio only" /
  "RADIO-BROWSER.INFO · {n} matches · by votes". Result rows: a 40px glyph tile — no remote artwork
  in v1 (grilled decision; a remote-artwork module = a named v1.1 candidate, see 12 no. 8). **The
  glyph carries the source distinction** (grill guardrail): RSS podcast = microphone
  (`audio-input-microphone-symbolic`), YouTube = video glyph (`video-x-generic-symbolic` — the app
  bundles no brand logos), radio = antenna glyph (`network-wireless-symbolic`); applies to the
  result AND the preview tile and is consistent with the source pills of the table (7.2). Next to it
  title, sub-line, an outlined accent button "Subscribe"/"Add" — the click takes effect immediately
  (button → spinner → ✓, the dialog stays open for multiple adds), errors inline on the row.
- **URL mode:** a detection card ("YouTube channel detected — videos become episodes · audio only
  via yt-dlp" / "Playlist file detected (PLS) — resolved to {host}" / "RSS feed detected"), a
  preview line (title + "487 videos · updated today" or "MP3 · 128 kbit/s · name from ICY
  header"), option rows: podcasts `Import the latest {N} episodes` (switch on; off = start empty,
  only future items — a decision) + `Download new episodes automatically` (off); radio `Fetch logo &
  tags from radio-browser` (on). Footer Cancel / Subscribe or Add station (confirm disabled until
  the preview is ok — the `dialogs.rs` idiom); footnotes "YouTube subscriptions are played
  audio-only via yt-dlp." / "Community database — a play sends the etiquette click count to
  radio-browser."

### 7.4 Removal: context menu, hover star, undo

- **Context menus** (gio::Menu + SimpleActionGroup at the click point — the
  `track_list_context_menu` idiom, small dedicated builders): episode: `Play`/`Resume` · `Copy
  episode URL` · `Mark as played`/`Mark as unplayed` · `Download episode`/`Delete download` · ── ·
  `Unsubscribe from “{show}”` (destructive). Station: `Play`/`Stop` · `Copy stream URL` · `Edit
  station…` (a small adw::Dialog: name/genre/URL) · ── · `Remove favorite` (destructive). In the
  spirit of CTX-5a: destructive at the bottom, context-named.
  **Queue entries were deliberately missing in v1 (replaced grilled decision):** radio keeps the
  asymmetry and still never shows "Play Next"/"Add to Queue". Episodes are now typed
  citizens of the manual queue: "Play Next"/"Add to Queue" act on the current selection, and
  the typed drag payload distinguishes track and episode IDs even when the numeric value is the same.
- **Hover star:** the cell follows the `rating.rs` recipe — **real `gtk::Button`s, no GestureClick
  in ColumnView cells** (a documented finding), MotionController reveal, re-bind on cell recycling.
  Radio: a filled accent star = favorite, a click removes it. Podcast episode: the star acts on the
  **show** (tooltip "Unsubscribe from {show}", TIP-1d).
- **Undo flow** (a clone of `missing_view::tombstone_with_undo`): `removed_at = now` →
  views/counters immediately → an `adw::Toast` with `set_button_label("Undo")`, `set_timeout(10)`,
  `ToastPriority::High` (FB-1) → undo = `removed_at = NULL` + refresh; commit (dismiss/timeout, the
  pending counter) = a hard delete. If the removed station / an episode of the removed show is
  currently playing → playback stops with it (no orphaned external state).
- **Downloads on unsubscribe (grilled decision: the toast chain):** unsubscribing never deletes
  files silently. If downloads exist, a second toast follows at **commit time**:
  "Unsubscribed from “{show}” — {n} downloads kept · [Delete files]"; ignoring it = the files stay
  (the cleanup policy clears them in the long run). **[Delete files] = trash** via
  `gio::File::trash()`, NEVER a hard delete — otherwise it would be the only irreversible one-click
  action in the app. A deliberate consensus distinction: the CONFIGURED cleanup policy (3/8) still
  deletes hard — policy consent in the preferences ≠ a one-click toast. **Multiple unsubscribes
  coalesce:** if several commits with downloads pile up, ONE toast aggregates them ("3 shows — 12
  downloads kept · [Delete files]") — a pure aggregation function + test, the accumulator in the
  view controller. Re-subscribing finds orphaned files again deterministically via the GUID-keyed
  download path (3).

### 7.5 Empty/status states

Pure `*_empty_state_for(...)` + a shared `adw::StatusPage` (the `track_list_empty_state` idiom):

| View | State | Condition | StatusPage |
|---|---|---|---|
| Podcasts | `Empty` | no subscriptions | "No podcasts yet" + exactly one button "Add podcast" (opens the dialog; FB-5a tone) |
| Podcasts | `NoEpisodes` | subs present, 0 episodes | "No episodes yet" + "Refresh now" |
| Podcasts | `NoResults` | 0 rows, filter active | one button "Show all N episodes" (FIL-6) |
| Radio | `Empty` | no favorites | "No stations yet" + "Add station" |
| Radio | `NoResults` | 0 rows, filter active | one button "Show all N stations" |

The radio `Empty` state with its Add-station CTA is a **binding condition of the
module default-ON** (grilled decision, anchored in SRC-1): radio is born visible to everyone —
the first look must lead to the Add dialog in one click, never into a dead end.

Offline is not an empty state: the tables render from the DB; the podcasts footer shows "Updated X
ago" + an inline failure (NR-6 idiom). Fetch results come in **hard** (MOT-2).

## 8. Preferences

No new `PageId` (SET-1: a section instead of a page): both modules via `ALL_MODULES` on the
**Plugins page** (the SET-6a group of the source intent, like New Releases/Concerts), extra rows
following the `scope_row` helper idiom (`preference_plugins.rs` ~line 154: `descriptor.id ==
"podcasts"`/`"radio"` branches + display-name/description arms):

- **`preference_podcasts.rs` (new):** SpinRow "Import latest N episodes" (5–100, default 25) ·
  SwitchRow "Download new episodes automatically (default for new subscriptions)" · ComboRow
  "Downloads cleanup" (Keep all / Delete played after 7 days / Keep last 5 per show; deletes hard —
  the policy consensus, see 7.4) · ActionRow "yt-dlp" (the version as the subtitle via a one_shot
  probe, a context-dependent update action: `yt-dlp -U` for an external developer override, "Update
  Reprise" for a bundled Flatpak component) + SwitchRow "YouTube sources" (default on; if the
  component is missing, the subtitle readably shows "YouTube component is unavailable — reinstall or
  repair Reprise" — **a pure display state as a pure decision function, the setting is never
  flipped automatically**, a grill guardrail) · SpinRow "Refresh every N hours" (1–24, default 6 —
  `sources.refresh_hours`).
- **`preference_radio.rs` (new):** ComboRow "Search order" (Votes / Name / Clicks).
- SET-4: everything takes effect immediately (`connect_*` → `set_setting`); module toggles notify
  the runtimes (the enabled subscription) → the sidebar row appears/disappears.

## 9. UX rulebook (docs/ux-rules.md)

A new section **"AF. Podcasts & Radio"** — AD is the last section today, Concerts reserves AE;
verify against the dev state when inserting (rule in 13). Rules as `[planned]` in the foundation,
flipped to `[active]` in the respective implementation commit with rule-named tests
(`check-ux-traceability.sh`):

- **SRC-1** [gtk] — Sidebar places in the LIBRARY section (Music → Podcasts → Radio → Queue), only
  when the respective module is active; counters: podcasts = unplayed episodes, radio = favorites;
  0 → no counter. Radio is active by default (only modules with AUTOMATIC network traffic start
  off); the binding condition of this default is the empty state with the Add-station CTA (7.5).
- **SRC-2** [gtk] — Add actions are tinted rectangular buttons (accent surface, radius 8, plus +
  label), never chip-shaped; filter chips stay outlined pills. Both views share one toolbar
  grammar: Add button · "+ Add filter" · active chips with an × target ≥ 20 px · the count on the
  right (in the spirit of FIL-1a/FIL-2).
- **SRC-3** [gtk] — One Add dialog per source with exactly one input field for search terms or a
  URL: a search yields grouped results with row buttons; a URL leads via type detection to a
  preview card + options + one confirm. Network fetches fire only on submit and never run on the
  main loop.
- **SRC-4** [gtk] — Removal is immediate + an undo toast (10 s, non-displaceable, FB-1): a row
  context menu with a destructive Unsubscribe/Remove at the bottom plus a hover star; until the
  toast commit the entry is only tombstoned. Context menus of episodes/stations never show "Play
  Next"/"Add to Queue" (omitted, not grayed out). Podcasts: unsubscribing never deletes files
  silently — if downloads exist, a commit-time toast "{n} downloads kept · [Delete files]" offers
  the trash (`gio::File::trash`, never a hard delete; multiple unsubscribes aggregate into one
  toast).
- **POD-1** [core] — Episode status is a pure derivation: Played ⇔ `played_at` set, otherwise
  Resume ⇔ `position_ms > 0`, otherwise New; the end of an episode sets Played and clears the
  position. Table `Date · Episode · Show · Length · Source · Status`, default sort date descending.
- **POD-2** [core] — RSS is the API: enclosure/guid/pubDate/itunes:duration; the GUID (fallback the
  enclosure URL; YouTube = the video ID) is the ONLY episode identity — dedupe, resume, played
  and download hang off it; refresh via conditional GET (ETag/Last-Modified) on a worker thread with
  an interval + jitter; upserts never overwrite seen/position state. Auto-refresh runs only when the
  module is active with ≥ 1 subscription and pauses on metered connections (manual refresh stays).
- **POD-3** [core] — YouTube exists only behind the provider boundary via yt-dlp: listing
  flat-playlist, audio URL resolution exclusively at play time (never persisted), errors are
  classified into readable messages and never crash; without the binary the provider degrades
  visibly. A switch in the preferences (default on); the degradation is display on the switch — the
  setting is never flipped automatically.
- **POD-4** [gtk] — Episode playback resumes at the stored position; the position is persisted on
  pause/stop/switch/quit and throttled during playback. After an episode ends the app offers "Play
  next" of the same show (toast + persistent bar button, query by date), but never plays on
  automatically. Podcasts never produce scrobbles, listen_events or play counts.
- **POD-5** [gtk] — Downloads are opt-in (per subscription), live under the app's XDG data path and
  follow the cleanup policy; downloaded episodes play locally (the offline path).
- **RAD-1** [gtk] — The playing station is the only accented table state (icon, name, now-playing,
  row tint); idle stations show "—". Now-playing text exists only during a connection (ICY), never
  from a cache — a paused station counts as not connected and shows "—"; only the player bar may
  remember the last title dimmed (session memory), the table never.
- **RAD-2** [gtk] — Live playback has no seek and no duration: the player bar shows elapsed +
  ICY now-playing, the waveform gives way to a geometrically identical placeholder (P-4), MPRIS
  reports CanSeek=false and metadata without length. Radio never scrobbles. Activating the playing
  row again stops it. Pause is disconnect, presented as pause: the bar keeps the station
  (play symbol, the last ICY title dimmed), MPRIS reports Paused/CanPause=true, play reconnects
  "live now" (elapsed starts over); a failed reconnect leaves the station standing paused
  (a readable inline error + retry), never an empty bar.
- **RAD-3** [core] — radio-browser etiquette: server choice via `all.api.radio-browser.info` with
  fallback rotation; every play of a uuid station sends the click counter; a dead stream is
  re-resolved via the uuid exactly once before any error is shown.
- **RAD-4** [core] — A pasted URL is parsed down to the stream URL (PLS/M3U one level; HLS
  manifests remain the stream URL); the preview checks via an ICY header probe (name, bitrate)
  without streaming the body.

## 10. i18n

New catalogs `ui/strings_podcasts.rs` and `ui/strings_radio.rs` (N_! constants + formatters: column
titles, pills, dialog texts, error classes including "YouTube blocked the request — update
yt-dlp", count lines, empty states, undo texts), re-exported via `strings.rs`; **both in
`po/POTFILES.in`**. All strings in English; no literal strings at widget call sites.

## 11. Test strategy (TDD)

Every task red first; the gate battery per commit (see 15). No test contacts the network; no test
starts the real yt-dlp.

- **Pure core units:** `parse_feed` (RSS 2.0, Atom, the itunes namespace, guid fallback, enclosure
  drop, broken XML, `limit`), the `parse_duration` table, pubDate formats, `detect` URL detection,
  the iTunes/radio-browser/servers parsers (fixtures), `resolve_playlist`
  (PLS/M3U/HLS passthrough/depth 1), `parse_icy_headers`, the yt-dlp stderr classification table,
  `refresh_due` + jitter determinism, the POD-1 status matrix, cleanup policy cases, the resume
  policy (early seek failed → a single catch-up), the elapsed formatter. New out of the grill:
  `next_unplayed_of_show` ordering (same show, date after the reference episode, skips played,
  None at the end of the show), `locale_country` mapping (`de_DE.UTF-8`→DE, `C`/empty/broken→US),
  unsubscribe aggregation (1 show / n shows / 0 downloads → no toast),
  the auto-refresh gating decision (enabled × sub_count × metered × due — only one combination
  fires; the start check and the timer coalesce via the same TTL).
- **Store/pipeline (in-memory V32):** the upsert preserves `played_at`/`position_ms`/`first_seen_at`;
  the conditional-GET cycle (200 → ETag stored; 304 → bump only); the tombstone cycle (remove →
  counter 0 → undo → back; commit → hard gone, CASCADE); re-subscribe revives a tombstone;
  the `count_unplayed` invariant; radio click/re-resolve against fixtures (server rotation, url
  update, fallback).
- **Subprocess:** a fake yt-dlp as a shell script, written into a tempdir by the test
  (`REPRISE_YTDLP_BIN`): flat-playlist JSON, resolve JSON, exit-1-with-bot-stderr, a hang →
  timeout kill, ENOENT, the version probe.
- **Playback core:** `play_uri` scheme validation; the MPRIS predicate matrix
  (`can_seek`/`can_pause`/`build_metadata` with `live_stream`/`external_ref`;
  `can_go_next`/`can_go_previous` = episode neighbors for context-bound external, otherwise false;
  length omitted; `artUrl` from
  `image_url`/`favicon_url`; `metadata_differs` on an ICY change); the `PlaybackMode` matrix (only
  Queue advances; podcast finish → played + the play-next offer; radio finish → the reconnect
  policy); the **radio pause state matrix** (paused→play = reconnect with an elapsed reset;
  reconnect failure → paused + inline error, never an empty state; activation = stop; the bar dims
  the last title, the table "—"); **scrobble exclusion** as a rule-named test (a simulated external
  session produces neither `listen_events` nor scrobble queue rows).
- **On the GTK side:** UI logic exclusively pure in
  `*_presentation.rs`/`*_empty_state.rs`/the dialog state machine headless; display tests `#[ignore =
  "requires a display; run via xvfb-run"]`, individually via `dbus-run-session -- xvfb-run -a cargo
  test -p reprise-gnome <name> -- --ignored --test-threads=1` (MainContext races: never judge
  display tests in a pack).
- **Rule-named tests** per flip: `src_2_add_action_is_tinted_button_not_chip`,
  `src_4a_remove_is_tombstone_until_toast_commit`,
  `src_4b_unsubscribe_commit_toast_trashes_never_hard_deletes`,
  `src_4b_podcast_context_menu_exposes_queue_membership_actions`, `pod_1_status_matrix`,
  `pod_3_ytdlp_errors_are_readable_never_panic`, `pod_4_external_session_never_scrobbles`,
  `pod_4_finish_offers_next_unplayed_of_show`,
  `rad_2_live_state_disables_seek_and_reports_no_length`,
  `rad_2_pause_is_disconnect_presented_as_paused`, `rad_3_dead_stream_reresolves_once`, … —
  `check-ux-traceability.sh`.
- **Fixtures:** inline strings for the parsers; files under
  `REPRISE_PODCASTS_FIXTURE_DIR`/`REPRISE_RADIO_FIXTURE_DIR` for the pipeline end-to-end (feed XML
  across 2 runs: a new episode, a changed title, 304; radio-browser servers/search/click JSON).

## 12. Risks & scope boundary

**Non-goals for v1 (each justified):**

1. **OPML import/export** — its own adoption slice, retrofittable without a schema change.
2. **Download manager UI** — the policy + context actions cover v1; a progress UI would be FB-2b
   terrain.
3. **Chapters & transcripts** — a heterogeneous data situation, its own part of the player UI.
4. **Playback speed** — needs rate plumbing in the backend + bar UI; orthogonal. **Named
  v1.1 candidate no. 1** (podcast listeners want it; grilled decision).
5. **Auto-advance to the next episode of the same show** stays excluded: direct
  episodes still end with played + stop + the manual "Play next" offer (6.3). The
  queue citizenship implemented in the meantime is separate from this: only an episode explicitly
  queued manually continues with the next queue entry.
6. **Desktop notifications** for new episodes (parity with the Concerts decision).
7. **CLI/MCP surface** — a **named v1.1 candidate** (grilled decision): read-only `reprise-cli
  podcasts list` / `reprise://podcasts` + `reprise://radio` (pure cache reads, a package-M clone
  without file conflicts), attachable additively at any time after wave 1.
8. **Remote artwork** (podcast covers, station logos) — the URLs are persisted; rendering would need
  a generic image downloader outside the cover module; v1 uses glyph tiles with source glyphs
  (7.3, a declared mockup deviation; the mockup will be brought in line). **A named
  v1.1 candidate module** — purely additive, all the data is already in the DB.
9. **Session restore of external playback** — the episode stays reachable via "Resume"; radio
  streams are ephemeral.
10. **Video path** — never (spec: audio only).
11. **Podcast Index as a search provider** — only as an optional provider with a user-owned key
  behind the same provider interface (4.3); never an embedded shared key. **A named
  v1.1 candidate.**

**Risks:**

- **yt-dlp breakage/bot checks** (the core risk): bounded by error classification (readable, never a
  crash), the feature switch, a managed package version and fake-binary tests. Developer builds may
  fall back to a host binary override; release installations bundle or declare yt-dlp
  and ffmpeg as runtime components.
- **googlevideo 403/URL expiry mid-play:** one re-resolve attempt (generation guard), then a
  readable toast.
- **The extended yt-dlp window without dates:** entries without `published_at` sort behind the
  dated official UULF window and keep their provider order; cell "—".
- **radio-browser churn:** server rotation; the click is best effort; a total outage only affects
  search/logo/click — favorites keep playing (the URLs are local).
- **ICY character set:** legacy streams send Latin-1; replace broken sequences lossily (never
  panic); streams without ICY show "—" permanently — both are tested paths.
- **HLS radio:** depends on `hlsdemux` (gst-plugins-bad); the error path = a normal playback error
  with a toast; no HLS code of our own.
- **HTTP seek (podcast):** needs range support from the server; a failure is logged, the position
  keeps running.
- **MPRIS live edge cases:** clients that expect `mpris:length` — omitting it is spec-conformant;
  a manual pass with the GNOME Shell widget in Z1.
- **Cardinality:** the import cap (N) + the threshold 5000 for retrofitting windowing.
- **Undo window vs. running playback:** removing the playing source stops playback immediately
  (7.4) — otherwise the external state would hold on to a tombstoned row.
- **Schema/section collision with Concerts:** section 13.

## 13. Coordination with the parallel Concerts feature

Concerts (docs/plans/concerts.md, branch feature/concerts, phase planned) touches the same
seams:

| Seam | Concerts | This feature | Strategy |
|---|---|---|---|
| `db.rs` SUPPORTED_SCHEMA_VERSION | 31 | 32 | **Rule: the migration number belongs to the branch that merges to dev first.** Every foundation task verifies the dev HEAD and takes the next free number (the plan numbers are placeholders). If Concerts does not land first, this becomes 31. |
| `docs/ux-rules.md` section letter | AE | AF | The same rule: the next free letter at the dev HEAD at the F1 commit. Append-only ⇒ the merge conflict is trivial. |
| `modules.rs` ALL_MODULES | +CONCERTS | +PODCASTS, +RADIO | Additive insertions, adjacent lines, semantically independent. |
| `view_source.rs`, `browser.rs`, `browser/navigation.rs`, `nav_history.rs` | +Concerts/+Releases | +Podcasts/+Radio | Additive enum arms, mechanically resolvable. |
| `sidebar_rebuild.rs` / `sidebar_presentation.rs` | SMART section | LIBRARY section | Different insertion points; one compact block each. |
| `window.rs` / `library_shell.rs` / `track_list_smoke.rs` | +2 pages/branches | +2 pages/branches | Both encapsulate in `install(…)` (3–4 lines); a conflict = adjacent lines. |
| `browse_bar.rs` `CHIP_CSS_CLASS` → `pub(in crate::ui)` | yes | yes | **An identical one-line change** — whoever merges second drops their version. |
| `artist_news_refresh.rs::fnv1a_64` `pub(crate)` | yes | a clone until Concerts has landed | After Concerts lands, switch the clone over to the shared helper (a cleanup line). |
| `strings.rs` mod lines, `po/POTFILES.in`, `style/mod.rs` app_css | +2 catalogs | +2 catalogs | Append-only lists, trivial. |
| `preference_plugins.rs` | +concerts branch | +podcasts/+radio branches | Additive match arms; deliberately one compact arm per module. |

**No shared refactor pulled forward onto dev** (the HTTP boundary, a generic filter bar): both
features need only tiny additive lines in shared files; a refactor now would block both
branches. Duplicate instead — the boundary clones no. 3 and 4 (`podcasts/http.rs`, `radio/http.rs`)
are **grill-confirmed**. **The consolidation is a DECIDED follow-up task** (not merely an
option): once BOTH features have landed, a fixed dev task — a `sources_http` helper (limiter, UA,
timeout, fixture seam once), possibly a generic source filter bar, plus switching the
`fnv1a_64` clone over to the shared helper (the cleanup line from the table above). The task is
carried along as a memory note so that it does not evaporate. Rebase discipline: this branch rebases
onto dev as soon as Concerts is merged; the F tasks re-check both "next free number" rules.

## 14. Acceptance criteria (made concrete for the repo)

| # | Criterion | Verification |
|---|---|---|
| 1 | Both sidebar entries (LIBRARY, Music→Podcasts→Radio→Queue) with live counters (unplayed / favorites), module-gated, icons from the system set with a runtime fallback; radio default ON, podcasts default OFF; the radio empty state leads to the Add dialog in one click | `src_1_*`, sidebar rebuild test, module default tests, display smoke |
| 2 | The podcasts table renders/sorts/filters as specified (relative dates, H:MM, source/status pills; filters Unplayed/Show/Source sticky) | presentation units, `pod_1_*`, filter units, display test |
| 3 | Radio table with a state icon, an accented playing row, now-playing only live, filters Genre/Country | presentation units, `rad_1_*`, display test |
| 4 | Podcasts Add dialog: search (iTunes with `country=` from the locale + ytsearch, grouped, "audio only" label, source glyph tiles) AND URL paste (RSS + YouTube → preview + options) | dialog state units, URL detect units, `locale_country` test, fixture E2E, display test |
| 5 | Radio Add dialog: search by votes with Add buttons; URL paste for direct streams AND M3U/PLS (down-parse) with an ICY preview | `rad_4_*`, playlist/ICY units, display test |
| 6 | Add buttons are tinted rectangular buttons, clearly distinguished from chips (their own CSS class, never `.reprise-filter-chip`) | `src_2_*`, CSS class test, manual optics pass |
| 7 | Context-menu removal + hover star with an undo toast (10 s, tombstone-based); unsubscribing keeps downloads, the commit-time toast offers [Delete files] → trash (never hard), multiple unsubscribes aggregate; radio never shows Play Next/Add to Queue, podcast and YouTube episodes offer both actions for the current selection including the keyboard and typed drag route | `src_4a_*`/`src_4b_*` including the trash test, aggregation units, tombstone cycle units, `acc_8_episode_menu_queue_actions_are_the_keyboard_partner_for_drag`, display test |
| 8 | YouTube: the official UULF feed delivers the first long-form window; "Load more" extends it once via yt-dlp up to entry 40; audio-only/opus, bestaudio resolve per play (never persisted), yt-dlp errors readable, without the managed component it degrades as display (switch subtitle, the setting never auto-flipped) | `pod_3_*`, `pod_10_*`, fake binary tests, resolve generation test, prefs decision units |
| 9 | Podcast resume: the position is persisted (pause/stop/switch/quit + throttle), playback continues; end → played; duration probe on the first play | `pod_4_*` resume units, store roundtrips |
| 10 | Radio live: ICY now-playing in the table + player bar + MPRIS from one event; no seek, no duration, elapsed only; pause = disconnect-presented-as-pause (the bar dims the last title, the table "—", reconnect "live now", failure → paused + inline retry, never an empty bar); dead favorites re-resolve via uuid | `rad_2_*`/`rad_3_*`, pause state matrix, MPRIS matrix, StreamTags plumbing test |
| 11 | Podcasts/radio never produce scrobbles/listen_events/play counts | `pod_4_external_session_never_scrobbles` + the radio counterpart |
| 12 | All network + yt-dlp off the main loop (worker/one_shot); auto-refresh only when the module is on ∧ ≥ 1 subscription, TTL-coalesced, metered-gated (manual stays); strings translatable (both catalogs + POTFILES); gates green | code audit "no http/Command outside the boundaries", gating decision units, gate battery, `check-ux-traceability.sh` |
| 13 | The end of an episode offers "Play next" of the same show by date (toast + persistent bar button), never plays automatically; MPRIS external fully functional except for CanGoNext/CanGoPrevious=false, artUrl = a remote pass-through | `pod_4_finish_offers_next_unplayed_of_show`, ordering units, MPRIS predicate matrix, manual GNOME Shell pass |

## 15. Work packages as waves (file ownership)

Scope grill-confirmed: ONE branch `feature/podcasts-radio`, the wave plan unchanged.
Order: 0) foundation → 1) core data layer + playback → 2) views + wiring → 3)
preferences + wrap-up. Rules: **no package shares files with a package running in parallel**;
all conflict points (db.rs, view_source.rs, browser*, modules.rs, catalogs, POTFILES, ux-rules.md,
style/buttons.rs, mod stubs) lie in the foundation. At the start of a wave the coordinator writes
the **file ownership table of the running wave into AGENTS.md** (main moves under parallel
agents — a lesson learned). Every task: TDD (red first), the full gate battery, one commit.

### Wave 0 — Foundation (one owner, sequential)

- **F1 · Rules + strings + modules.** Files: `docs/ux-rules.md` (section AF — verify the letter
  against the dev HEAD — with SRC-1..4, POD-1..5, RAD-1..4 `[planned]`), `ui/strings_podcasts.rs` +
  `ui/strings_radio.rs` (new, complete catalogs + formatters), `ui/strings.rs` (mod lines),
  `po/POTFILES.in`, `modules.rs` (both descriptors + ALL_MODULES). TDD: formatter units,
  module default tests.
- **F2 · Migration V32.** Files: `db_podcasts_radio.rs` (new) +
  `db_podcasts_radio_migration_tests.rs` (new), `db.rs` (the SUPPORTED number per the rule in 13 +
  the call line). TDD: migration tests first.
- **F3 · Enums/facades/stubs.** Files: `view_source.rs`, `browser.rs`, `browser/navigation.rs`,
  `ui/nav_history.rs` (both arms each), `lib.rs` exports, `podcasts.rs` + `radio.rs` (facades with
  the public types `EpisodeRow`, `SubscriptionRow`, `StationRow`, `EpisodeStatus`, error enums —
  a re-export scaffold so that A/B/C/E never need the same files), `ui/podcasts/mod.rs` +
  `ui/radio/mod.rs` (compiling minimal stubs), `ui/podcasts/css.rs` + `ui/radio/css.rs`
  (section stubs), `ui/style/mod.rs` (app_css registration + section test), `ui/style/buttons.rs`
  (`ADD_ACTION_CLASS` including the BTN states), `ui/browse/browse_bar.rs` (only the
  `CHIP_CSS_CLASS` pub line; dropped if Concerts already brought it). TDD:
  label/roundtrip tests. After that `cargo build` is green.

### Wave 1 — Core data layer + playback (four owners in parallel)

- **Package A · Podcasts core (owner A).** Files (all new): `podcasts/http.rs`, `feed.rs`,
  `itunes.rs` (including `locale_country` + the `country=` param, test), `url_detect.rs`, `store.rs`,
  `status.rs`, `query.rs` (including `next_unplayed_of_show` + an ordering test), `config.rs`,
  `refresh.rs`, `downloads.rs` (GUID-keyed location + reclaim), `pipeline.rs` +
  the `*_tests.rs` neighbors. TDD: parser/store/pipeline (see 11).
- **Package B · yt-dlp & YouTube (owner B, after F3, in parallel with A).** Files (new):
  `podcasts/ytdlp.rs`, `podcasts/youtube.rs` + tests + a fake-binary helper. Touches NO A files
  (provider types from the F3 facade). TDD: the subprocess matrix.
- **Package C · Radio core (owner C).** Files (all new): `radio/http.rs`, `servers.rs`,
  `search.rs`, `station.rs` (store/query/tombstone), `playlist.rs`, `icy.rs`, `click.rs`,
  `config.rs` + tests. TDD: see 11.
- **Package E · Playback integration (owner E; E1→E2→E3 sequential).**
  - **E1 · Backend + MPRIS core.** Files: `reprise-core/src/playback.rs` (StreamTags,
    the play_uri trait), `media_integration.rs` (MprisState fields + predicates including
    `can_go_next`/`can_go_previous` = false for external, later narrowed by POD-20 to external
    without episode neighbors; the `artUrl` pass-through in
    `build_metadata`), `platform-linux/src/player.rs` (play_uri impl, tag arm), `mpris/state.rs` +
    `mpris/mod.rs`. TDD: the predicate matrix, schema validation.
  - **E2 · Controller external mode.** Files: `ui/playback/external_media.rs` (new), `preview.rs`
    (the PlaybackMode extension), `player_event_handling.rs` (finish/error arms),
    `player_controller.rs` (play_external, the StreamTags fan-out, the position throttle, the quit
    hook; podcast finish → the `next_unplayed_of_show` call + the play-next toast; the radio pause
    state machine disconnect/reconnect/error-stays-paused). Depends on E1 + the A/C stores
    (save_position, mark_played, next_unplayed_of_show, click/re-resolve). TDD: the mode matrix, the
    resume policy, the pause state matrix, scrobble exclusion,
    `pod_4_finish_offers_next_unplayed_of_show`.
  - **E3 · Player bar/mini live.** Files: `player_bar_state.rs`, `player_bar.rs`,
    `waveform_seek.rs` gating, `player_bar_seek.rs` (drag guard), the compact audit (`ui/compact/*`
    reviewed read-only, changes minimal). Bar states out of the grill: radio paused (play symbol,
    the dimmed last ICY title, an inline reconnect error + retry, never an empty bar) + the
    persistent "Play next episode" button in the stopped/empty bar. Flips: the RAD-2 bar share. TDD:
    pure state/formatter units (including the dim/paused derivation), display tests `#[ignore]`.

### Wave 2 — Views + wiring

- **Package P · Podcasts view (owner A; after A/B/E2).** Files: `ui/podcasts/*` (fill the F3 stubs).
  Sequence: **P1** presentation+model (pure) → **P2** view+columns+empty (flip POD-1) → **P3**
  filter bar + toolbar Add button (the SRC-2 share) → **P4** Add dialog (the SRC-3 share; iTunes
  with the country param + ytsearch + URL preview, source glyph tiles) → **P5** context menu (without
  Play Next/Add to Queue) + star + undo + the **commit-time toast chain ([Delete files] →
  `gio::File::trash`, multi-aggregation)** + download actions + worker wiring (flips
  SRC-4, POD-2, POD-3, POD-5).
- **Package R · Radio view (owner C; after C/E2, in parallel with P — no shared files).** Files:
  `ui/radio/*`. Sequence: **R1** presentation+model → **R2** view+columns+now-playing cell
  (the StreamTags hook-up; a paused station = "—"; the empty state with the Add-station CTA — the
  condition of the default-ON; flip RAD-1) → **R3** filter bar + toolbar → **R4** Add dialog (source
  glyph tiles; flip RAD-4) → **R5** context menu (without Play Next/Add to Queue) + star + undo +
  edit dialog + hooking click/re-resolve and pause/reconnect up to E2 (flips the RAD-2 remainder,
  RAD-3, the SRC-4 share).
- **Task V · Wiring (one owner, after P2 and R2).** Files: `sidebar_presentation.rs` (both
  NavIcons + fallbacks), `sidebar_rebuild.rs` (both rows + counts + gates), `ui/window/window.rs`
  (both stack pages + installs; the app-start/timer trigger of the podcasts worker **with the
  grill cap: only module on ∧ ≥ 1 subscription, TTL coalescence of the start check and the hourly
  timer, the metered gate via `gio::NetworkMonitor` at the trigger**), `ui/window/library_shell.rs`
  (routing branches), `ui/track_list/track_list_smoke.rs` (smoke sources). Flips: SRC-1 (including
  radio default-ON + the empty-CTA condition — the empty state is delivered by R2). TDD:
  rebuild/routing tests, gating decision units, display smoke.

### Wave 3 — Preferences + wrap-up (one owner)

- **Package S · Preferences.** Files: `ui/preferences/preference_podcasts.rs` +
  `preference_radio.rs` (new), `preference_plugins.rs` (both branches), `preferences/mod.rs`. Flips:
  the POD-3 prefs share. TDD: settings roundtrips; yt-dlp row/switch states
  (present/missing/update error → subtitle display, **never an auto-toggle of the setting**) as pure
  decision functions + a display test.
- **Z1 · Traceability + headless smoke + ledger.** `check-ux-traceability.sh` green (SRC-1..4,
  POD-1..5, RAD-1..4); an end-to-end smoke with full isolation (`dbus-run-session -- xvfb-run -a env
  XDG_DATA_HOME=$(mktemp -d) REPRISE_AUDIO_SINK=fakesink REPRISE_PODCASTS_FIXTURE_DIR=…
  REPRISE_RADIO_FIXTURE_DIR=… REPRISE_YTDLP_BIN=… cargo run`): modules on, subscribe (RSS + YouTube +
  a station via fixture), tables + filters, the play/resume/played cycle + the play-next offer,
  the ICY fan-out, remove/undo + the commit toast chain, prefs. Manual pass: a real stream including
  pause/reconnect, real yt-dlp, MPRIS in the GNOME Shell widget (artUrl, CanGoNext/Prev off),
  icon optics. A ledger line in `.superpowers/sdd/progress.md`.

### Verification (every commit)

`cargo fmt --check` · `cargo clippy --all-targets --workspace -- -D warnings` · `cargo test
--workspace` · `cargo audit` (accepted advisory RUSTSEC-2024-0436) · after core changes `cargo
tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` empty · the script gates
`check-architecture.sh`, `check-motion-tokens.sh`, `check-input-parity.sh`,
`check-accessibility-semantics.sh`, `check-display-tests.sh`, `check-ux-traceability.sh`. Not
verifiable headless (a manual pass): real streams/feeds, yt-dlp against YouTube, ICY from a real
station, MPRIS live behavior, icon optics per theme.
