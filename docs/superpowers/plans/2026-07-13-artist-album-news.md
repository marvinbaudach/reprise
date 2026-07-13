# Artist & Album News Panel — Implementation Plan

**Spec:** `docs/superpowers/specs/2026-07-13-artist-album-news-design.md`

**Baseline:** 557 passing workspace tests; 13 display-only tests ignored by
the normal workspace run. The compact-player stage is the implementation base.

## Global constraints

TDD RED→GREEN for every behavior change. Code, comments, logs, errors, UI text
and commits are English; design docs stay German. Never touch real music or the
real database. Every app/display run uses private D-Bus, Xvfb, scratch
`XDG_DATA_HOME`/`XDG_CACHE_HOME`, forced X11, unset Wayland and
`REPRISE_AUDIO_SINK=fakesink`. Artist News is default off; no request occurs
before explicit opt-in. MusicBrainz calls are serial and share one process-wide
minimum one-second interval with cover lookup. Core stays free of
gtk4/libadwaita/gstreamer/zbus. Every touched file ends under 800 lines;
`window.rs` starts at 799 and must shrink through extraction before any new
composition logic is added. Before every implementation commit run fmt, strict
clippy, workspace tests, audit and core purity when core changed. Never push.

## Task 1 — Shared MusicBrainz boundary and persistent module flags

**Files:**

- new `crates/reprise-core/src/musicbrainz.rs`
- `crates/reprise-core/src/lib.rs`
- `crates/reprise-core/src/cover_download.rs`
- `crates/reprise-core/src/modules.rs`
- `crates/reprise-core/src/library/settings.rs`

**Interfaces:**

```rust
pub const CONTACT_URL: &str = "https://github.com/marvinbaudach";
pub fn user_agent() -> String;
pub fn get(url: &str) -> Result<String, FetchError>; // blocking

pub const ARTIST_NEWS_MODULE: ModuleDescriptor;
pub const INFO_PANEL_VISIBLE_KEY: &str = "ui.info_panel_visible";
pub fn get_info_panel_visible(conn: &Connection) -> bool;
pub fn set_info_panel_visible(conn: &Connection, visible: bool)
    -> Result<(), rusqlite::Error>;
```

`FetchError` distinguishes timeout/transport, HTTP status and unreadable body
without retaining response bodies. `cover_download` calls the shared GET and
uses `musicbrainz::user_agent()` for Cover Art Archive too. The old private
rate limiter and placeholder project URL are removed.

**TDD steps:**

1. Add RED tests: User-Agent contains version and reachable maintainer URL;
   request-delay helper enforces one second and recovers a poisoned mutex;
   Artist News is listed/default off/round-trips; info panel defaults visible
   and round-trips.
2. Run the targeted core tests and observe missing items.
3. Implement the smallest shared client and typed flags. Do not make a real
   request in any test.
4. Re-run targeted tests (6 new; expectation 563), full gates and core purity.
5. Review that one limiter owns all MusicBrainz calls and no placeholder URL
   remains.

**Commit:** `refactor: share MusicBrainz client and register artist news`

## Task 2 — Conservative release provider, local comparison and cache

**Files:**

- new `crates/reprise-core/src/artist_news.rs`
- new `crates/reprise-core/src/artist_news_tests.rs`
- new `crates/reprise-core/src/queries/artist_context.rs`
- `crates/reprise-core/src/queries/mod.rs`
- `crates/reprise-core/src/lib.rs`
- `crates/reprise-core/Cargo.toml` (`chrono`, already present transitively,
  direct pure-Rust date contract)

**Interfaces:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NewsKind { Upcoming, New }

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlbumNews {
    pub release_group_mbid: String,
    pub title: String,
    pub first_release_date: String,
    pub primary_type: String,
    pub kind: NewsKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtistNews {
    pub artist: String,
    pub artist_mbid: String,
    pub fetched_at: i64,
    pub items: Vec<AlbumNews>,
    pub stale: bool,
}

pub fn artist_search_url(artist: &str) -> String;
pub fn release_groups_url(mbid: &str) -> String;
pub fn parse_artist_mbid(json: &str, artist: &str) -> ArtistMatch;
pub fn parse_release_groups(
    json: &str,
    local_albums: &[String],
    today: chrono::NaiveDate,
) -> Vec<AlbumNews>;
pub fn load_or_refresh(
    artist: &str,
    local_albums: &[String],
    today: chrono::NaiveDate,
    force: bool,
) -> Result<ArtistNews, NewsError>; // blocking

pub fn query_artist_albums(
    conn: &Connection,
    artist: &str,
) -> Result<Vec<String>, rusqlite::Error>;
```

The cache is versioned JSON under `$XDG_CACHE_HOME/reprise/artist-news` and is
published atomically. Fresh TTL is seven days; negative match TTL is one day;
network failure returns a stale positive cache when one exists.

**TDD steps:**

1. Add RED fixture tests for percent encoding; one exact high-score artist;
   weak/no/ambiguous matches; Album/EP parsing; excluded secondary types;
   local album suppression; ±365-day boundaries; ordering/Five-item cap.
2. Add RED cache tests using a temporary XDG cache and an injected fixture
   fetch closure: fresh cache makes zero fetches; forced refresh fetches;
   failure returns stale; corrupt cache degrades safely. Add query tests for
   artist/album-artist, missing exclusion and deduplication.
3. Implement pure parsing/filtering first, then cache orchestration around a
   private injectable fetcher; the public function uses `musicbrainz::get`.
4. Run targeted tests (12 new; expectation 575), full gates, core purity and
   file-size checks.
5. Review ambiguity, partial dates, hostile strings, cache atomicity and that
   no library path enters a request or cache key.

**Commit:** `feat: resolve and cache artist album news`

## Task 3 — One live runtime and generation-guarded context state

**Files:**

- new `crates/reprise-gnome/src/ui/artist_news_worker.rs`
- new `crates/reprise-gnome/src/ui/info_panel_state.rs`
- `crates/reprise-gnome/src/ui/mod.rs`
- `crates/reprise-gnome/src/ui/track_list.rs`
- new `crates/reprise-gnome/src/ui/track_list_selection.rs`

**Interfaces:**

```rust
pub(super) struct ArtistNewsRequest {
    pub generation: u64,
    pub artist: String,
    pub local_albums: Vec<String>,
    pub force: bool,
    pub response: async_channel::Sender<ArtistNewsResponse>,
}

#[derive(Clone)]
pub(super) struct ArtistNewsRuntime {
    pub enabled: Rc<Cell<bool>>,
    worker: async_channel::Sender<ArtistNewsRequest>,
    // weak enabled subscribers
}

impl ArtistNewsRuntime {
    pub(super) fn setup(conn: &Connection) -> Rc<Self>;
    pub(super) fn set_enabled(&self, conn: &Connection, enabled: bool)
        -> Result<(), rusqlite::Error>;
    pub(super) fn subscribe_enabled(...);
    pub(super) fn request(&self, request: ArtistNewsRequest);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PanelContext {
    Empty,
    Multiple(usize),
    Track(Track),
}

impl TrackList {
    pub(super) fn set_on_selection_changed(
        &self,
        callback: impl Fn(PanelContext) + 'static,
    );
    pub(super) fn shared_cover_loader(&self) -> Rc<CoverLoader>;
}
```

Selection callbacks clone values before invoking and never hold `Bitset`,
model, DB or `RefCell` borrows across the callback. The worker is one serial OS
thread; GTK types never cross it.

**TDD steps:**

1. Add RED pure tests: zero/one/multiple selection context; blank artist does
   not request; generation increments on context/disable/refresh; only current
   generation applies; runtime defaults off and subscriber removal is safe.
2. Observe RED, implement state helpers, then worker/runtime and the small
   TrackList callback sibling extraction (track_list.rs must end under 800).
3. Run targeted tests (7 new; expectation 582), full gates and adversarial
   RefCell/Send/stale-response review.

**Commit:** `feat: add artist news runtime and selection state`

## Task 4 — Adaptive information panel, Preferences and translations

**Files:**

- new `crates/reprise-gnome/src/ui/info_panel.rs`
- new `crates/reprise-gnome/src/ui/library_shell.rs`
- `crates/reprise-gnome/src/ui/window.rs`
- `crates/reprise-gnome/src/ui/preferences.rs`
- `crates/reprise-gnome/src/ui/primary_menu.rs` only if a shared action is
  needed
- `crates/reprise-gnome/src/ui/strings.rs`
- `crates/reprise-gnome/src/ui/mod.rs`
- `po/reprise.pot`
- `po/de.po`

**Interfaces:**

```rust
pub(super) struct InfoPanel { /* OverlaySplitView + widgets + generation */ }

impl InfoPanel {
    pub(super) fn new(
        content: &impl IsA<gtk4::Widget>,
        window: &adw::ApplicationWindow,
        conn: Rc<RefCell<Connection>>,
        runtime: Rc<ArtistNewsRuntime>,
        cover_loader: Rc<CoverLoader>,
    ) -> Rc<Self>;
    pub(super) fn widget(&self) -> &adw::OverlaySplitView;
    pub(super) fn toggle_button(&self) -> gtk4::ToggleButton;
    pub(super) fn set_context(&self, context: PanelContext);
}

pub(super) struct LibraryShell {
    pub split_view: adw::NavigationSplitView,
    pub content_nav: adw::NavigationView,
    pub info_panel: Rc<InfoPanel>,
}
```

`library_shell::build` extracts the existing sidebar/content/Now-Playing
composition from `window.rs`, inserts the end-positioned OverlaySplitView and
returns the same handles later wiring needs. Preferences receives the shared
runtime; the generated Artist News `SwitchRow` calls `runtime.set_enabled` and
rolls back on persistence failure. The panel's switch uses the same method.

**TDD steps:**

1. Add RED pure/widget tests for wide pinned versus narrow overlay metrics;
   persisted show/hide; disabled privacy card; Loading/Error/Cached/News
   rendering model; exact Upcoming/New order and accessible card names.
2. Add one ignored display test with fixture responses: open panel, enable,
   select artist A, switch to B before A completes, and assert only B cards;
   resize narrow and assert the panel remains reachable without clipping.
3. Implement `library_shell` extraction first so `window.rs` shrinks, then
   panel widgets/wiring and live Preferences toggle. Use the TrackList's
   existing CoverLoader and generation token.
4. Add complete English source strings and German translations; run gettext
   coverage.
5. Run targeted tests (7 new + 1 ignored; expectation 589 passing, 14 ignored),
   the display test in fully isolated Xvfb, full gates and line counts.
6. Adversarially review action rollback, disabled-network guarantee, weak
   ownership, URI launch, narrow geometry and compact-mode transitions.

**Commit:** `feat: show artist album news in information panel`

## Task 5 — End-to-end QA and stage close-out

**Files:**

- `scripts/ptr-e2e/run.sh`
- `docs/agent-workflow/MANUAL-QA.md`
- `RELEASING.md`
- `.superpowers/sdd/progress.md` when available in the owning worktree
- `docs/agent-workflow/STATUS.md` after integration lock ownership

**Steps:**

1. Extend PTR with a scratch library and fixture provider: select a track,
   open Information, explicitly enable News, capture Upcoming/New cards,
   switch selection while a delayed result is pending, close/reopen, disable.
2. Assert the fixture request log contains only artist names, uses one shared
   ≥1-second MusicBrainz schedule and makes zero calls while disabled.
3. Run all display-only tests individually, release checker, standalone core
   build/purity, audit, gettext and touched-file line counts.
4. Whole-stage review against both specs; fix Important/Critical findings with
   RED regressions and separate commits.
5. Record manual GNOME checks: real proportions, browser launch, offline/cache
   copy and opt-in clarity. Update the former MusicBrainz placeholder blocker:
   maintainer profile is now the contact URL; public release/source ownership
   remains separately blocked.

**Commit:** `docs: record artist news panel QA`
