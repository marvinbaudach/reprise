# Synchronized Lyrics — Implementation Plan

**Spec:** `docs/superpowers/specs/2026-07-13-synchronized-lyrics-design.md`

**Baseline:** `e7c5f5f` in the isolated `reprise-agentB` worktree while the
Preferences task owns `main`; record the exact workspace/display baseline
before Task 1. Synchronize the completed Preferences commits before local
integration, never by modifying its in-flight worktree.

## Global constraints

TDD RED→GREEN for every behavior change. Code, comments, logs, errors, UI
text and commits are English; design docs stay German. Never touch real music
or the real database. Never log or fixture real lyrics. Every app/display run
uses private D-Bus, Xvfb, scratch `XDG_DATA_HOME`/`XDG_CACHE_HOME`, forced X11,
unset Wayland and `REPRISE_AUDIO_SINK=fakesink`. LRCLIB receives only played
track title, artist, album and duration; there is no whole-library fetch and
no disable setting. No test reaches a real network service. The existing
500-ms `PlayerEvent::Position` cadence is the only lyrics clock. Core stays
free of gtk4/libadwaita/gstreamer/zbus. Every touched file ends under 800
lines; `player_controller.rs`, `window.rs`, `strings.rs` and `info_panel.rs`
must grow only through cohesive sibling extraction. Before every
implementation commit run fmt, strict clippy, workspace tests, audit and core
purity when core changed. Never push.

## Task 1 — Pure lyrics provider, LRC parser and cache

**Files:**

- new `crates/reprise-core/src/lyrics.rs`
- new `crates/reprise-core/src/lyrics_tests.rs`
- `crates/reprise-core/src/lib.rs`

**Interfaces:**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricsQuery {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimedLine {
    pub start_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LyricsBody {
    Synced(Vec<TimedLine>),
    Plain(String),
    Instrumental,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LyricsError { MissingMetadata, NotFound, Temporary, InvalidResponse }

pub fn request_url(query: &LyricsQuery) -> Result<String, LyricsError>;
pub fn parse_lrc(input: &str) -> Vec<TimedLine>;
pub fn active_line_index(lines: &[TimedLine], position_ms: i64) -> Option<usize>;
pub fn load_or_fetch(query: &LyricsQuery) -> Result<LyricsBody, LyricsError>;
```

Private `load_or_fetch_at(cache_dir, now, query, fetch)` injects time, cache
and transport for deterministic tests. `fetch` returns a typed HTTP outcome
so 404 alone becomes `NotFound`; transport/429/5xx remain `Temporary`. The
public path uses a 15-second blocking `ureq` client with a Reprise User-Agent.

**TDD steps:**

1. Add RED tests asserting an exact LRCLIB URL decodes to the four required
   query pairs and rounds 180_499 ms to 180 seconds; blank title/artist is
   rejected before calling the injected fetcher.
2. Add RED JSON tests proving `syncedLyrics` wins, empty synced content falls
   back to `plainLyrics`, `instrumental=true` is distinct, and an empty
   non-instrumental response is invalid.
3. Add RED LRC tests with synthetic lines for `[01:02]`, decimals of one to
   three digits, multiple timestamps, ignored metadata/malformed lines,
   stable equal-time ordering and the before/at/between/after active-index
   boundaries.
4. Add RED cache tests: positive roundtrip skips fetch; matching identity is
   mandatory; fresh negative skips fetch; an eight-day negative retries;
   corrupt cache retries; 404 writes a negative; a temporary failure does
   not; stale positive remains available on temporary failure. Assert cache
   files stay below the supplied temp cache and publishing leaves no temp
   sibling.
5. Run only `reprise-core` lyrics tests and observe compile/assertion failure.
6. Implement the smallest types, parser, lookup and versioned atomic cache.
   Reuse `url::Url`, `cover::hash_hex`, `dirs`, `serde_json`, `fastrand` and
   `ureq`; add no dependency.
7. Re-run targeted tests, full gates, Rustdoc, core purity and file-size
   checks. Adversarially review duration identity, JSON-body logging,
   traversal, Unicode, negative TTL and transient classification.

**Commit:** `feat: fetch and cache synchronized lyrics`

## Task 2 — Serial runtime and lyrics view state

**Files:**

- new `crates/reprise-gnome/src/ui/lyrics_worker.rs`
- new `crates/reprise-gnome/src/ui/lyrics_state.rs`
- `crates/reprise-gnome/src/ui/mod.rs`

**Interfaces:**

```rust
pub(super) struct LyricsRequest {
    pub generation: u64,
    pub query: LyricsQuery,
    pub response: async_channel::Sender<LyricsResponse>,
}

pub(super) struct LyricsResponse {
    pub generation: u64,
    pub result: Result<LyricsBody, LyricsError>,
}

pub(super) struct LyricsRuntime { sender: async_channel::Sender<LyricsRequest> }

impl LyricsRuntime {
    pub(super) fn setup() -> Rc<Self>;
    pub(super) fn request(&self, request: LyricsRequest);
}

pub(super) struct LyricsState { /* generation, query, body, active */ }

impl LyricsState {
    pub(super) fn set_track(&mut self, query: Option<LyricsQuery>) -> RequestIntent;
    pub(super) fn retry(&mut self) -> Option<RequestIntent>;
    pub(super) fn accepts(&self, generation: u64) -> bool;
    pub(super) fn set_body(&mut self, body: LyricsBody);
    pub(super) fn update_position(&mut self, position_ms: i64) -> Option<usize>;
}
```

**TDD steps:**

1. Add RED pure tests: new track increments generation and requests once;
   same identity is idempotent; clear invalidates; retry keeps identity but
   increments generation; old responses are rejected.
2. Add RED position tests: Synced updates only when line index changes;
   before-first/seek-back work; Plain and Instrumental never produce a line.
3. Add a test-only injected worker lookup and prove requests execute serially
   and return their generation without GTK data crossing the thread.
4. Observe RED, implement the state and one dedicated worker thread.
5. Run targeted tests, full gates, Rustdoc and file sizes. Review channel
   teardown, `Send`, stale responses and absence of UI-thread network work.

**Commit:** `feat: add generation-safe lyrics runtime`

## Task 3 — Native lyrics page with synchronized highlighting

**Files:**

- new `crates/reprise-gnome/src/ui/lyrics_view.rs`
- new `crates/reprise-gnome/src/ui/lyrics_strings.rs`
- new `crates/reprise-gnome/src/ui/lyrics_view_tests.rs`
- `crates/reprise-gnome/src/ui/mod.rs`
- `po/POTFILES.in`
- `po/reprise.pot`
- `po/de.po`

**Interfaces:**

```rust
pub(super) struct LyricsView { /* scroller + state widgets + line labels */ }

impl LyricsView {
    pub(super) fn new() -> Rc<Self>;
    pub(super) fn widget(&self) -> &gtk4::Widget;
    pub(super) fn show_empty(&self);
    pub(super) fn show_loading(&self, title: &str, artist: &str);
    pub(super) fn show_result(&self, body: &LyricsBody);
    pub(super) fn show_error(&self, error: &LyricsError);
    pub(super) fn set_active_line(&self, index: Option<usize>);
    pub(super) fn set_on_retry(&self, callback: impl Fn() + 'static);
}

fn centered_scroll_value(
    row_y: f64,
    row_height: f64,
    page_size: f64,
    upper: f64,
) -> f64;
```

The view installs one scoped CSS provider for `.lyrics-line-active` and
removes/adds that class only when the index changes. Labels are selectable,
wrapped and left aligned. Scroll value is row center minus half the viewport,
clamped to `0..upper-page_size`.

**TDD steps:**

1. Add RED pure tests for middle, start and end clamping.
2. Add an ignored isolated GTK test that renders synthetic Synced, Plain and
   Instrumental bodies; assert line count, selectable/wrapped text, exactly
   one active class after a position change, and no duplicate class after an
   idempotent update.
3. Add a mapped 340-pixel-wide regression proving a middle active row is
   scrolled close to viewport center and first/last rows clamp safely.
4. Observe RED, implement view and scoped CSS. Keep provider/body logic out.
5. Add all strings to the extracted catalogue, regenerate POT, translate
   German completely and add the file to `POTFILES.in`.
6. Run targeted tests one GTK process at a time, full gates, gettext checks,
   Rustdoc and file sizes. Review keyboard/a11y, selectable text, high
   contrast and repeated-tick jitter.

**Commit:** `feat: render position-synchronized lyrics`

## Task 4 — Information-panel tabs and single playback fan-out

**Files:**

- new `crates/reprise-gnome/src/ui/player_lyrics.rs`
- `crates/reprise-gnome/src/ui/player_controller.rs`
- `crates/reprise-gnome/src/ui/now_playing_wiring.rs`
- `crates/reprise-gnome/src/ui/info_panel.rs`
- `crates/reprise-gnome/src/ui/library_shell.rs`
- `crates/reprise-gnome/src/ui/window.rs`
- `crates/reprise-gnome/src/ui/mod.rs`

**Interfaces:**

```rust
impl PlayerController {
    pub(super) fn set_lyrics_view(&self, view: &Rc<LyricsView>);
    pub(super) fn sync_lyrics_track(&self, query: Option<LyricsQuery>);
    pub(super) fn sync_lyrics_position(&self, position_ms: i64);
}

impl InfoPanel {
    pub(super) fn lyrics_view(&self) -> Rc<LyricsView>;
}
```

`PlayerController` stores only a weak lyrics target plus the runtime/state in
the cohesive sibling module. `play_track_id` feeds title/artist/album/duration
after successful backend start; `sync_position` fans the existing position
to lyrics; stopped/failed reset clears context. No second ticker exists.

`InfoPanel` moves the existing body/scroller into the `Information` stack
page, inserts the LyricsView as the second page, and uses a top
`GtkStackSwitcher`. Refresh acts on Artist News in Information and on retry
in Lyrics. Progress reflects the visible page's current request without
changing the other page's state.

**TDD steps:**

1. Add RED panel display tests proving the two top tabs exist in order,
   existing Information selection behavior is unchanged, Lyrics follows a
   separate playback context, and removing/closing the panel leaves its state
   intact.
2. Add RED controller tests around a fake playback backend: successful play
   issues one lyrics query with exact summary metadata; a failed start never
   fetches; a later track invalidates the prior generation; Position uses the
   existing event and Stop clears while Pause preserves.
3. Observe RED, extract `player_lyrics.rs`, wire one shared runtime/view from
   `library_shell`/`window`, and make the smallest panel stack change.
4. Run targeted tests, every display test separately, full gates, Rustdoc and
   file sizes. Adversarially review `RefCell` lifetimes, weak ownership,
   successful-play ordering, stale responses and all stop/failure paths.

**Commit:** `feat: integrate lyrics with playback and information panel`

## Task 5 — Fixture smoke, privacy docs and close-out

**Files:**

- `crates/reprise-core/src/lyrics.rs` (fixture-only transport hook)
- `crates/reprise-gnome/src/ui/lyrics_view.rs` or new
  `crates/reprise-gnome/src/ui/lyrics_smoke.rs`
- `crates/reprise-gnome/src/ui/window_smoke.rs`
- `README.md`
- `data/org.reprise.Reprise.metainfo.xml.in`
- `RELEASING.md`
- `docs/agent-workflow/MANUAL-QA.md`
- `docs/agent-workflow/STATUS.md` only after main integration/lock ownership

**TDD steps:**

1. Add a local `REPRISE_LRCLIB_FIXTURE_DIR` contract that maps exact
   synthetic metadata to JSON and optionally logs only query fields; it is
   checked before real HTTP. Unit-test malformed paths and exact lookup.
2. Add `REPRISE_SMOKE_LYRICS=1`: start a synthetic track, open Lyrics, wait
   for the fixture, seek across two synthetic timed lines, switch quickly to
   a delayed first/fast second fixture, and log only state/index/generation.
3. Run the complete command with `dbus-run-session`, Xvfb, scratch data/cache,
   forced X11, unset Wayland and fakesink. Assert no real network endpoint,
   stale text, GTK/GLib critical, panic or `RefCell` failure appears.
4. Document automatic played-track metadata lookup, LRCLIB, local cache,
   plain fallback and manual native-GNOME checks in README/AppStream/
   release QA. Do not claim a license for provider text.
5. Run every ignored display test separately, full release checker, full
   gates, Rustdoc, core purity, gettext/metadata/Flatpak-source checks,
   optimized Meson DESTDIR install and file-size scan.
6. Whole-branch adversarial review against the design. Fix all Important or
   Critical findings and rerun affected evidence.
7. Synchronize current `main`, resolve only scoped conflicts, rerun the full
   combined gate battery, then integrate locally only after the main lock is
   free and claimed according to `STATUS.md`. Update the authoritative ledger
   and release the lock. Never push.

**Commit:** `docs: verify and document synchronized lyrics`
