---
slug: architecture-consolidation
worktree: —
branch: claude/project-review-refactoring-qxv9gr
phase: review
codex_session:
created: 2026-07-31
base: 577765b (origin/dev)
---
# Project review — findings before the test release

A full review after a long run of features, spec revisions and design changes.
It answers seven questions: does the core carry more than one app, is error
handling and logging good enough to debug from the field, was anything built
twice (radio, podcasts, YouTube, ordinary playlists), are playlist filters and
artist-page pills cleanly separated, where is performance actually lost, how
safe is the app, and how stable is it — and what any of that means for opening
a test round.

This document is the *findings*. `docs/plans/consolidation-plan.md` is how the
work gets done. Neither replaces an existing contract: `docs/ux-rules.md` stays
the UX source of truth, `docs/plans/multi-frontend-core.md` the architectural
foundation. What this adds is what those plans left unfinished, and what has
grown alongside them unplanned.

---

## 0. Verdict

| Area | Verdict | Next step |
| --- | --- | --- |
| Core reusability | **good** — the `Db` handle landed, purity is checked mechanically | get `rusqlite::Error` out of the public API |
| Second runtime (`reprise-runtime`) | **critical** — ~15k lines built and packaged, used by no surface | force the decision: cut over or shelve |
| Error handling | **very good** in the core, **one hard gap** at startup | replace the `expect` in `main.rs` with a reported failure |
| Logging | **fine for developing, too thin for testers** | log file plus "Copy Diagnostics" |
| Duplication across sources | **real, but bounded and already named** | redeem the promised consolidation task |
| Place vs. filter | **clean since `c565671`** | one truth for "has a sidebar row" (§5.3) |
| Performance | **one measurably expensive spot** (default sort has no index) | one index, one migration |
| Security | **above average** — no injection, no traversal, bombs covered | `--` before the yt-dlp URL, image `Limits` |
| Stability | **hard-won panic discipline — but a panic is a silent abort** | `panic::set_hook` plus a crash marker |
| Backwards compatibility | **schema yes, toolchain no** | the MSRV finding (§9.3) |

**Release recommendation.** The app is ready to be tested on its merits. The
items in wave 0 of the plan should land first; together they are small (one to
two days) and almost all of them are things a tester would otherwise report as
"it crashes" or "I have nothing to send you".

The three critical findings share one thread: **the product is well built but
cannot report on itself.** A crash is silent (§8.2), a startup failure is a
panic (§3.2), and the GTK crate's 793 `tracing` calls reach nobody (§3.3).
Those three decide whether a test round produces findings or frustration.

---

## 1. What was measured

State `577765b` (`origin/dev`, 2026-07-31). Every number measured here, not
quoted from documentation.

| Crate | Files | Lines |
| --- | ---: | ---: |
| `reprise-gnome` | 528 | 140,678 |
| `reprise-core` | 381 | 108,043 |
| `reprise-mcp` | 41 | 12,009 |
| `reprise-platform-linux` | 38 | 11,607 |
| `reprise-runtime` | 35 | 9,337 |
| `reprise-cli` | 37 | 5,181 |
| `reprise-stems` | 11 | 2,755 |
| `reprise-runtime-client` | 5 | 1,879 |
| `reprise-runtime-protocol` | 12 | 1,773 |
| **Total** | **1,088** | **293,262** |

- Roughly **24 %** of those lines sit in dedicated test files; over 4,200
  `#[test]` functions.
- `docs/ux-rules.md`: about 3,950 lines; `scripts/check-ux-traceability.sh`
  reports **298 active rules covered** by rule-named tests.
- Schema version **50**, eighteen numbered migration steps in `db.rs` plus
  extracted migration modules, each with its own migration tests.
- 30 scripts under `scripts/`, seventeen of them `check-*`.

This is a mature, heavily guarded repository. Everything below is
consolidation, not repair.

### 1.1 What the two newest commits change

`origin/main` sits at `de4138a`; `origin/dev` two commits further at `577765b`
(`#189` lyrics/cover robustness, `#193` reveal behaviour for the source lists).
Measurements were taken against `dev`, which fully contains `main`.

- **`#193` confirms the consolidation direction.** It adds
  `crates/reprise-gnome/src/ui/source_reveal.rs`: a *shared*, GTK-free decision
  about when the viewport moves, with the explicit reasoning that podcasts,
  YouTube and radio must not "drift into three answers", while *how* each
  surface reveals stays local. That is exactly the cut §4.2 and §4.3 propose
  for the filter bar and the add dialog. The road is already taken.
- **`#189` sharpens finding D3.** The lyrics path was split into a `lyrics/`
  module and gained **two more** `ureq` agents of its own (`lrclib.rs`,
  `netease.rs`). The core now constructs **16** HTTP boundaries where it built
  thirteen two commits ago. The duplication keeps growing while the shared
  boundary is missing.
- **`#189` also supplies the best building block for fixing it.**
  `lyrics/breaker.rs` is a **host-keyed** circuit breaker (three failures →
  five minutes open, a `LazyLock<Breaker>` over a host map). That is the right
  key — per host, not per module — and the natural nucleus of the
  `SourceClient` §4.4 proposes. Lift it; do not reinvent it.
- **`#189` opens a new write path into the music collection**
  (`cover_writeback.rs`, `lyrics/sidecar_write.rs`, `writeback_publish.rs`):
  Reprise now writes `cover.<ext>` and `.lrc` next to existing tracks.
  `AGENTS.md` gained the exact rule in the same commit — derived only from
  track paths, never overwriting an existing file, one precise sweep pattern
  for its own temporaries. Soundly built (§7.1); for the test round it is the
  highest "touches foreign files" risk and belongs on the manual QA list.
- **Still open:** `AGENTS.md` claims a "Three-crate Cargo workspace" (§2.5) and
  still carries the "Not released yet — no backwards compatibility" section
  (§9.1).

---

## 2. Architecture — does the core carry more than one app?

### 2.1 What already holds

Four things are right and should not be touched:

1. **A `Db` handle instead of a `Connection`** (ADR 002, landed in `#173`).
   `Db::conn()` is `pub(crate)`; no public core function takes `&Connection`
   any more. The boundary is a type rather than a convention, and the 575
   `borrow()` sites of the project's most common panic class are gone rather
   than hidden. This is the single most important precondition for a second
   app, and it is met.
2. **Mechanically checked dependency direction.**
   `scripts/check-architecture.sh` probes each crate with
   `cargo tree --target all -e normal` so no GTK/GLib/GStreamer/zbus family
   reaches `reprise-cli`, `reprise-mcp`, `reprise-stems` or `reprise-runtime`,
   and no stray workspace edge appears. The `run_dependency_probe` wrapper
   fails **closed** — a broken `cargo tree` aborts the gate instead of passing
   silently. That is better than most projects manage.
3. **No SQL outside the core**, checked separately for GTK and for the headless
   surfaces, with a multiline `rg -U` that catches statements split across
   lines.
4. **The `change_log` outbox plus the `Notifier`.** Cross-process visibility of
   foreign changes without a daemon, degrading to two-second polling when no
   filesystem watch can be armed. Exactly the right call for several processes
   on one SQLite file.

### 2.2 Finding A1 (critical) — the second runtime is built but wired to nothing

`docs/plans/multi-frontend-core.md` §9.1 draws a line: everything in SQLite
stays embedded, and everything *not* in SQLite — the audio pipeline, the
in-memory queue, a device run, a job's progress — gets exactly one owner,
`reprise-runtime`. That owner exists in full:

| Part | Lines | State |
| --- | ---: | --- |
| `reprise-runtime` (reducer, ports, fakes) | 9,337 | complete, tested |
| `reprise-runtime-protocol` (wire contract) | 1,773 | complete, versioned |
| `reprise-runtime-client` (transport, mirror) | 1,879 | complete, tested |
| `platform-linux/src/runtime_service/` (D-Bus, lease) | 1,580 | complete |
| `crates/reprise-gnome/src/ui/runtime/` (GTK session) | 809 | **`#![allow(dead_code)]`** |
| **Total** | **≈ 15,400** | |

Shipped alongside: `data/org.reprise.Reprise1.service.in`,
`data/reprise-runtime.service.in`, its own Meson target, and
`scripts/check-runtime-service-install.sh`, which verifies both artefacts land
correctly under two prefixes.

None of it is wired. `crates/reprise-gnome/src/ui/runtime/mod.rs` says so
itself: *"This module is not that migration — nothing here is wired into
`PlayerController` or the window yet, on purpose."* `reprise_runtime_client` is
referenced only by that dead module and by tests in `reprise-platform-linux`.
`reprise-mcp` and `reprise-cli` still drive playback over MPRIS
(`org.mpris.MediaPlayer2.reprise` plus `org.reprise.Player1`), not over
`org.reprise.Reprise1`.

Meanwhile the productive state still lives in
`crates/reprise-gnome/src/ui/playback/` (6,916 lines; `player_controller.rs`
792, `queue_transport.rs` 753, `up_next_transport.rs` 639).

**Why this is an architectural finding and not merely unfinished work:**

- There are **two command surfaces for one domain**. Both wrap the same core
  types (`reprise_core::queue::Queue`, `up_next::UpNextQueue`) — that part is
  good — but the *binding* between them is written twice.
  `crates/reprise-runtime/src/transport_parity_tests.rs` says it outright:
  *"what lived in the controller was the binding between the two, and that is
  what these tests pin."* Every future queue rule has to be implemented twice
  and held together by parity tests.
- There are **two control planes shipped**. An agent can bus-activate
  `reprise-runtime` while the GTK app runs, and then two processes own a
  playback. The single-owner lease (`runtime_service/lease.rs`) protects the
  *runtime* from itself, not from the GTK app, which never takes the lease.
- For the test release it means a service with a systemd unit and a D-Bus name
  ships that no product path reaches — attack surface and support load with no
  return.

**Recommendation — one of two, not both:**

- **(A) Cut over.** `PlayerController` becomes a client of `RuntimeSession`;
  `queue_transport`/`up_next_transport` become commands plus snapshot
  rendering. One domain, one owner, and MCP/CLI lose the MPRIS crutch. Large
  effort (three to five packages on the scale of "episodes as queue citizens"),
  high risk, large and lasting payoff.
- **(B) Shelve and unship.** Delete `ui/runtime/`, park the runtime crates on a
  branch, take the two `.service` files and the Meson target out of the
  install, and take `check-runtime-service-install.sh` with them. Small effort,
  small risk, recovers roughly 15,000 lines of maintenance load.

**Not recommended: carrying the limbo through the test release.** It is the one
option that pays the costs of both. For a near-term test round, **(B) with a
documented resumption trigger** is the honest choice; **(A)** is the right
answer to "several apps on one core", but not in the same weeks as a first test
round.

### 2.3 Finding A2 — `rusqlite::Error` is the core's public error type

858 public core signatures return `Result<_, rusqlite::Error>`. Every surface
therefore has to depend on `rusqlite` just to name a core error. The manifests
admit it — `reprise-mcp`: *"`rusqlite` is a direct dependency only because the
core facades surface `rusqlite::Error` in their signatures."*

This is the last persistence leak after ADR 002. For a second app (KDE/Qt,
Android, iOS) it means compiling SQLite while never seeing SQL, and matching on
a foreign error type whose variants say nothing about the domain.

**Recommendation.** A `reprise_core::CoreError` (thiserror) with the few
classes callers actually distinguish: `NotFound`, `Conflict`, `Busy`
(SQLITE_BUSY, for retry decisions), `Invalid`, `Backend(String)`.
`rusqlite::Error` folds in via `#[from]` and is never handed out. The migration
is mechanical and incremental — module by module, with the `From` impl carrying
the intermediate states. Only afterwards can `rusqlite` leave
`reprise-cli`/`reprise-mcp`, which the architecture gate can then enforce.

### 2.4 Finding A3 — the composition root knows every view

`crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs` defines
`RuntimeWiring` with **over 40 fields**: every view, every runtime, every
window-decoration widget. `window.rs` itself stays disciplined (597 lines
against a 600 gate), but the wiring is displaced rather than dissolved.

For "several apps" this is the practical brake: no view can be raised in a
different shell on its own, because its wiring exists only as one package. For
*today's* app it is bearable, which is why this sits in wave 3 rather than wave
0.

**Recommendation.** Do not split the struct — that just displaces it again.
Introduce a narrow `…Ports` struct per view naming exactly the collaborators it
needs. `RuntimeWiring` builds those ports and hands them over; the view stops
knowing `RuntimeWiring` at all. Incremental per view, every stage compiles.

### 2.5 Finding A4 — `AGENTS.md` describes the project from three crates ago

`AGENTS.md` says "Three-crate Cargo workspace" and lists three. There are nine.
Its roadmap ends at "GUI-A2 (cover download)"; since then podcasts, YouTube,
radio, concerts, new releases, device sync, library doctor, my stats, the tag
editor, stems and the runtime have landed. The "Not released yet — no backwards
compatibility" section is the rule that **flips** with the test release (§9.1).
And it claims `docs/ux-rules.md` is written in German — that document is
English throughout.

This is not cosmetic: `AGENTS.md` is by its own account the first thing an
agent reads. A wrong model there produces exactly the class of mistake this
review is looking for.

---

## 3. Error handling and logging

### 3.1 What holds

- **Panic freedom is effectively achieved.** In production code (excluding test
  files and `#[cfg(test)]` blocks): `reprise-core` **1** `unwrap`,
  `reprise-gnome` about twenty at narrowly argued single sites,
  `reprise-runtime`/`-client`/`-protocol` **zero**. For 293k lines that is
  exceptional.
- **`reprise_core::source_error` is a model error projection.** `Display`
  carries only safe sentences, technical payload is reachable only through
  `details()`, and tests prove that neither `Display` nor `Debug` leaks a host,
  a token or a status code. On top of it sits a shared presentation decision
  (banner versus full area, per-source actions, one collected notice from three
  failures on).
- 54 `thiserror` enums with speaking messages.

### 3.2 Finding E1 (release blocker) — the database opens or the app panics

`crates/reprise-gnome/src/main.rs`:

```rust
let conn = db::Db::open_migrated(Some(&path)).expect("failed to open or migrate database");
```

That is the only way into the app. `DbError` has four cases and three of them
are realistic for a tester:

- `SchemaTooNew` — the tester tried a newer build and went back. **This is the
  downgrade case `db.rs` deliberately detects** — and the GUI throws the
  detection away.
- `Io` — disk full, `~/.local/share` not writable, home on a network mount.
- `Sqlite` — a file damaged by a hard power-off.

In all three the process aborts with a panic message on stderr: no window, no
message. The tester reports "it does not start".

**Recommendation.** Handle `open_migrated` and present the failure — see the
plan for the shape. Small, but it decides whether a test round produces usable
reports.

### 3.3 Finding E2 (release blocker) — logging never reaches the tester

`init_logging()` writes to **stderr** only, filtered by `REPRISE_LOG`,
defaulting to `info,lofty=error`. There is no log file, no rotation, no in-app
export, and no line in `README.md` telling a tester how to get logs. Under
Flatpak stderr goes to the journal; someone launching from the overview has no
visible path there. 793 `tracing` calls in the GTK crate exist and are
effectively unreachable.

Two smaller points of the same family:

- **No correlation.** Zero `tracing::trace!`, no spans. A podcast refresh
  crossing worker thread, HTTP boundary, store and view leaves lines with no
  common thread. `#[tracing::instrument]` on the few entry points (scan,
  refresh per source, device run, job) would fix that without touching call
  sites.
- **Skewed levels.** 559 `warn!` against 138 `info!` and 105 `debug!`. When
  `warn` is the default shelf for "unexpected but harmless" it stops being a
  signal. Worth one reclassification pass while the log export is built.

### 3.4 Finding E3 — 54 error types with no common axis

The enums are individually clean but share no axis. Two questions every surface
asks are answered per-enum today: *is this user-visible or diagnostic?* and *is
retrying meaningful?*

`source_error` answers both — but only for network sources. `PodcastError`,
`RadioError`, `ConcertError`, `ProviderError`, `NewsError`, `LyricsError`,
`FetchError`, `PortraitError` and `RemoteStatsError` each model the same HTTP
states (timeout, transport, status, rate-limited, parse) and are then folded
into `SourceErrorKind`.

**Recommendation.** Introduce a `SourceTransportError` as the shared return of
the HTTP boundary (§4.4) and let the domain enums carry only *domain* cases.
That closes D3 and E3 in one move.

### 3.5 Finding E4 — swallowed errors

About 330 `let _ = …` / `.ok();` in production code across the workspace.
Samples are mostly legitimate (best-effort cleanup, GTK return values). Not a
gate matter, but a good candidate for one pass over the core and worker paths
asking: "would I want this error in a bug report?" — and if so, `tracing::debug!`
instead of silence.

---

## 4. Duplication: radio, podcasts, YouTube, playlists

Short answer: **yes, but less and more specifically than the question fears.**
The large axes are shared correctly; what is duplicated is the shell around
them.

### 4.1 Already shared correctly — leave alone

- **YouTube is not a second podcast system.** One `PodcastKind { Rss, Youtube }`
  over one store, one pipeline, one data model (`SubscriptionRow`,
  `EpisodeRow`); YouTube differs only in its fetcher (`YoutubeFetcher` against
  `FeedFetcher`) and its projections (`podcasts/youtube.rs`, 245 lines). The
  GTK side even shares the type: `RuntimeWiring` holds `podcasts_view` **and**
  `youtube_view` as `Rc<PodcastsView>`. Exemplary.
- **One track-list model for every local source.** `ViewSource` has 17
  variants; Library, RecentlyAdded, Playlist, Smart, Queue, Missing, Album,
  Artist and Genre all run through one `TrackListModel` and one `ColumnView`.
  There is no second playlist widget.
- **Shared source surfaces:** `source_error.rs` (failure presentation),
  `source_empty_state.rs` (empty state for podcasts/YouTube/radio),
  `source_error_banner.rs`, `source_context_surface.rs` (the full-cell hit area
  for context menus), `source_add_action.rs`, `one_shot_task.rs`, and since
  `#193` `source_reveal.rs`.
- **One queue engine.** The GTK controller and the runtime wrap the same
  `Queue`/`UpNextQueue` from the core. (The *binding* is doubled — §2.2 — which
  is a runtime finding, not a source finding.)

### 4.2 Finding D1 — five filter bars

| File | Lines |
| --- | ---: |
| `ui/browse/browse_bar.rs` (+ `_chips`, `_chooser`, `_count`, `_strings`) | 692 (+638) |
| `ui/concerts/concerts_filter_bar.rs` | 574 |
| `ui/releases/releases_filter_bar.rs` | 426 |
| `ui/radio/radio_filter_bar.rs` | 416 |
| `ui/podcasts/podcasts_filter_bar.rs` | 313 |

Exactly **one CSS class** is shared (`browse_bar::CHIP_CSS_CLASS`). Everything
else is written five times: its own facet enum, its own `remove_filter`, its
own popover with facet and value pages, its own result line, its own
persistence keys. The copying shows down to the constants:

- `const FILTER_BAR_MIN_HEIGHT: i32 = 34;` — **five times**, identical.
- `const FACET_PAGE: &str = "facets"; const VALUE_PAGE: &str = "values";` —
  **three times**.

The consequence is that the filter rules in `docs/ux-rules.md` section K apply
in practice only to `browse_bar`. The freshly decided place/filter separation
(§5) is absent from the other four bars because it never reached their code.

**Recommendation.** A generic `FilterBar<F: FilterModel>` in `ui/browse/`
owning geometry, chip construction, popover navigation, "Clear all" and the
counting line. Each source keeps a small `FilterModel` impl (facets, labels,
values, persistence key) — realistically 60 to 120 lines instead of 300 to 570.
Expected net reduction around 1,200 lines, and section K becomes true for every
source for the first time.

### 4.3 Finding D2 — two "search or URL" dialogs

`ui/podcasts/add_dialog.rs` (754) plus `add_dialog_input.rs` (430) plus
`add_dialog_results.rs` (95), against `ui/radio/add_dialog.rs` (788) plus
`radio_add_input.rs` (18) plus `station_preview.rs` (79). Both have:

- the same phase machine — `Idle → Searching → Results → Previewing → Preview
  → Error`,
- the same `classify_input` → `AddInput` split (search term versus URL),
- the same generation counter guarding against stale results,
- the same `one_shot_task` plus `source_add_action` wiring,
- the same connectivity check before submitting.

`docs/plans/podcasts-radio.md` §7.3 deliberately planned "an `add_dialog.rs`
per feature". After landing, the evidence says the commonality is larger than
assumed.

**Recommendation.** A `ui/source_add_dialog.rs` holding the phase machine and
the result list; a trait per source with `classify_input`, `search`, `preview`,
`commit` and the copy identities. Medium size, clear payoff, low risk — both
dialogs have their own tests to serve as the net.

### 4.4 Finding D3 — 16 HTTP boundaries, and the consolidation task already promised

`ureq::Agent::config_builder()` is constructed at **16** places in the core:
`artist_portrait/deezer.rs`, `concerts/http.rs`, `cover_download.rs`,
`library/lastfm_stats.rs`, `library/library_doctor/remote/network.rs`,
`library/listenbrainz.rs`, `lyrics/lrclib.rs`, `lyrics/netease.rs`,
`musicbrainz.rs`, `podcasts/http.rs` (×2), `podcasts/source_artwork.rs`,
`radio/http.rs` (×2), `scrobbling.rs`, `scrobbling/lastfm.rs`. It was thirteen
two commits ago — the number grows while the shared boundary is missing.

`podcasts/http.rs` and `radio/http.rs` are near-identical line for line: the
same `static LAST_REQUEST: Mutex<Option<Instant>>`, the same
`MIN_REQUEST_INTERVAL`, the same `FIXTURE_DIR_ENV` mechanism with a
`thread_local` override, the same `classify_transport` fold into
`SourceErrorKind`.

Rate limiting is implemented **five times** separately (`radio`, `podcasts`,
`concerts`, `musicbrainz`, `artist_portrait/deezer`), each with its own
process-wide mutex. There is therefore *no* shared request budget: five sources
may each fire once per second in parallel. For "network off", "metered
connection" and backoff, that is five places a policy has to be honoured.

Five separate fixture-directory variables (`REPRISE_RADIO_FIXTURE_DIR`,
`REPRISE_PODCASTS_FIXTURE_DIR`, `REPRISE_CONCERTS_FIXTURE_DIR`,
`REPRISE_MUSICBRAINZ_FIXTURE_DIR`, `REPRISE_LRCLIB_FIXTURE_DIR`) are the test
side of the same pattern.

**This is already agreed work.** `docs/plans/podcasts-radio.md`, in its grilled
decisions: *"boundary clones confirmed + a fixed consolidation task once both
features have landed."* Both landed. The task is due.

**Recommendation.** A `reprise_core::net` with a
`SourceClient { agent, user_agent, timeout }`, **one** rate limiter budgeted per
host rather than per module, the host-keyed circuit breaker lifted out of
`lyrics/breaker.rs`, a `SourceTransportError` fold (which also closes E3), and
**one** fixture variable with a subdirectory per provider. Core-only, GUI-free,
testable without a network — precisely the kind of work that serves the
multi-app goal, because a second app would otherwise rebuild these policies.

### 4.5 Finding D4 — five parallel table pages

`track_list`, `podcasts`, `radio`, `releases` and `concerts` each have
`*_view.rs`, `*_columns.rs`, `*_model.rs`, `*_presentation.rs`,
`*_empty_state.rs`, `*_failure_ui.rs`, `*_filter_bar.rs` and `css.rs` — the
same file grammar written five times, with four separate `ColumnView`
constructions and four separate `SignalListItemFactory` sets.

This grew deliberately and it works. I recommend **no** large unification here:
the row shapes genuinely differ (a track row with rating and cover; an episode
row with download state; a station row with a favourite star; a release row
with cover and affiliate link). A shared `SourceTablePage` abstraction would
mostly produce configuration instead of code.

What is worth sharing is the narrow part: the filter bar (D1), the empty state
(already shared), the error banner (already shared), the context surfaces
(already shared) — that is, everything that is not the row shape.

### 4.6 Finding D5 — two queue command surfaces

See §2.2. Resolved by the runtime decision; not a task of its own.

---

## 5. Playlist filters versus artist-page pills

### 5.1 The answer

**Yes, the separation is clean — since `c565671` (2026-07-31).** Before that it
was not, and the question lands exactly where it was stuck.

The model now lives in `crates/reprise-gnome/src/ui/browse/filter_restriction.rs`
as a pure, GTK-free decision layer:

| | **Place** | **Filter** |
| --- | --- | --- |
| Meaning | where you are | what is withheld inside it |
| Entered/left by | navigation, history push | state change at the same place |
| Shown by | sidebar row — or, where none exists, the **place pill** | chips plus "Clear all" |
| Applies to | Artist, Album, Genre | search, facets, "Hide AI music" |

Three functions carry it:

- `has_place_pill(source)` — true **only** for `Artist`/`Album`/`Genre`, the
  places entered from inside the track list that have no sidebar row.
- `is_restricted(search, browse, exclude_ai)` — a place is **never** a
  restriction. Only search, facets and the AI exclusion withhold rows.
- `row_visible(is_track_source, restricted, has_place_pill, preference_visible)`
  — the row appears when restricted **or** a place pill is due **or** the
  preference asks for it.

Dismissal behaviour is therefore structurally different, not merely visually:

- **Playlist with a filter** → the chip carries a `×`. Clicking removes the
  filter; the place stays the playlist, and the count stays "X of Y" relative
  to it.
- **Artist page** → a place pill, **without** a `×`, prefixed with `‹`, the
  whole pill a click target, tooltip and accessible name naming a destination
  ("Leave the artist page") rather than a removal. The click is a NAV-2
  navigation with a history push. Playback is untouched (PLAY-8).
- **Artist page with a filter** → both zones side by side with a separator, and
  the count reads "2 of 3 tracks" relative to the **place**, never to the
  library.

It is well built: the decision sits in pure functions with rule-named tests
(`fil_1c_places_carry_a_pill_without_restricting`,
`fil_1c_sidebar_places_carry_no_pill`,
`fil_8_recently_added_is_a_sidebar_place_without_a_pill`,
`fil_2_row_shows_for_a_place_pill_without_any_filter`), not in widget code.

### 5.2 How it was wrong before — for the record

`docs/superpowers/specs/2026-07-31-place-pill-vs-filter-pill-design.md` records
the measurement: an artist page showed `FILTER`, a pill `Alpha Artist ×` and a
count `3 of 9 tracks` — visually indistinguishable from a facet chip, yet the
`×` left the place instead of removing a filter. Same shape, same heading, same
counting vocabulary; different meaning, different gesture, different
consequence. Exactly the suspicion behind the question.

### 5.3 What remains

1. **The other four filter bars do not know the distinction** (§4.2). Podcasts,
   radio, releases and concerts build their chips themselves. They have no
   places in the Artist/Album/Genre sense today, so it is not an active bug —
   but `youtube_channel_detail.rs` (629 lines) *is* a place inside a source.
   Whether its way back follows the same grammar should be checked against
   FIL-1c.
2. **The neighbouring bug is fixed — verified, not assumed.** The design
   document names it: every queue mutation triggered a sidebar refresh, and
   `resolve_select_source` fell back to Library because Artist/Album/Genre have
   no sidebar row, so the artist page jumped away on a double-click. The guard
   is present on `dev` **and** `main`: `sidebar/sidebar.rs:466` defines
   `has_sidebar_row`, `sidebar_rebuild.rs:370` uses it, `sidebar_tests.rs:663`
   and `:674` cover both sides. Nothing open; kept here as evidence.
3. **Two truths for "has a sidebar row" — and this one is open.**
   `has_place_pill()` (`browse/filter_restriction.rs`) and `has_sidebar_row()`
   (`sidebar/sidebar.rs`) draw the same distinction in two separate `matches!`
   expressions in two modules. They agree today; nothing keeps them agreeing.
   The next place added will be added to exactly one of them. **One function,
   two callers** — the cheapest fix in this document.

---

## 6. Performance

### 6.1 Measurement — the default sort runs without an index

Reproduced against a replica of the real table and index set from `db.rs`,
using the real query strings from `queries/clauses.rs`, 100,000 rows, `ANALYZE`
run, SQLite 3.45.1. `EXPLAIN QUERY PLAN` per sort field:

| Sort | Plan |
| --- | --- |
| `artist` (**default view**) | `SCAN tracks` + `USE TEMP B-TREE FOR ORDER BY` |
| `title` | `SCAN tracks USING INDEX idx_tracks_present_title_nocase` |
| `album` | `SCAN tracks USING INDEX idx_tracks_present_album_order` |
| `genre`, `year`, `added_at`, `rating`, `play_count`, `duration_ms` | `SCAN` + temp B-tree |

Only `title` and `album` have matching partial NOCASE indexes. The default sort
`artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no` has none —
`idx_tracks_artist ON tracks(artist)` is neither NOCASE nor partial and cannot
deliver the ordering.

Timings for the same 200-row window, median of nine runs:

| Case | offset 0 | offset 50,000 | offset 99,800 |
| --- | ---: | ---: | ---: |
| `artist`, no index (**today**) | 14.9 ms | **312 ms** | **380 ms** |
| `artist`, with the candidate index | 0.44 ms | 1.95 ms | 3.37 ms |

The candidate:

```sql
CREATE INDEX idx_tracks_present_artist_order
ON tracks(artist COLLATE NOCASE, year, album COLLATE NOCASE, track_no)
WHERE missing_since IS NULL AND removed_at IS NULL;
```

By a wide margin the largest single effect in this review: **one migration, one
index line, a factor of 30 to 100 on the app's most-used path.**

Stated limitation: an in-memory database, synthetic rows, Python's SQLite.
Absolute numbers on real hardware against a real file will differ; the plan
shapes and the ratio will not.

### 6.2 Why it shows up as stutter

`TrackListModel::item()` runs the window query **synchronously on the GTK
thread** on a cache miss — documented and deliberately so. At 0.4 ms that is
right. At 312 ms it is a visible frame drop while scrolling. The cache (eight
windows of 200 rows) only helps moving back through the same region. After the
index, the original justification holds again.

### 6.3 Further findings, by payoff

1. **Search without a full-text index.** `filter_clause` builds
   `title LIKE '%x%' OR artist LIKE … OR album LIKE … OR genre LIKE …` — no
   index can serve it. Measured: 28 ms for the window plus 29 ms for
   `COUNT(*)` per keystroke at 100k tracks, roughly 57 ms on the UI thread.
   A 200 ms debounce (`window.rs`) saves typing, but every character still
   costs two full scans.
   **Staged recommendation:** first, stop counting exactly where the total only
   feeds "X of Y" — a `LIMIT`-based "more than N" suffices above a threshold.
   Then an FTS5 contentless table over `(title, artist, album, genre)`,
   maintained by triggers. FTS5 ships inside `rusqlite`'s `bundled`, so no new
   dependency.
2. **`OFFSET` paging.** Even with the index, `OFFSET 99,800` is linear (3.4 ms
   against 0.4 ms). Fine for 100k, not for "a library of any size". Keyset
   paging is the clean answer but a larger change: the sort whitelist would
   have to guarantee stable tiebreakers. **Re-evaluate after the index, not
   before.**
3. **Missing indexes for the remaining sort fields.** `genre`, `year`,
   `added_at`, `rating` and `play_count` all go through a temp B-tree.
   `added_at DESC` backs "Recently added" and is probably worth a second index;
   the rest only after measuring which columns users actually sort by — every
   index costs write load during a scan.
4. **`ANALYZE` never runs.** Neither on open nor after a scan, so SQLite plans
   heuristically. One `PRAGMA optimize` after a large scan (cheap, a no-op when
   there is nothing to do) is the conventional answer.
5. **Five independent rate limiters** (§4.4) are a performance matter too: at
   startup several sources can claim network and CPU at once with no total
   budget anywhere.
6. **The writeback leftover sweep is O(n²) over a directory.**
   `writeback_publish::publish` calls `sweep_leftovers(target.parent())` on
   **every** publication, and that sweep is a full `read_dir` of the directory.
   For album folders this is free. For a flat music directory — one folder,
   every track, which plenty of people have — a library-wide lyrics batch reads
   the whole directory once per file. Measured on a warm tmpfs, replicating the
   sweep's own logic (`read_dir`, name match, `metadata()` only on matches):

   | Files in the directory | One sweep | A batch over all of them |
   | ---: | ---: | ---: |
   | 200 | 0.17 ms | negligible |
   | 2,000 | 1.20 ms | 2.4 s |
   | 10,000 | 7.43 ms | **74 s** |

   Seventy-four seconds of pure directory scanning, on the fastest possible
   filesystem; on a spinning disk or an NFS mount, far worse. The sweep itself
   is right to exist — an abandoned temporary in someone's album folder is
   exactly the kind of litter nobody else was looking for. It just does not
   need to run per file. Once per directory per batch, or once at startup, has
   the same effect for a fraction of the cost.

### 6.4 What is already good

Worth not overlooking: widget **and** data virtualization, a bounded window
cache, generation tokens against stale async results,
`REPRISE_PERF_RUNTIME_REPORT` as a built-in widget/cache measurement, and
`scripts/performance-baseline.sh` with 10k and 100k profiles. The
infrastructure to prove these improvements is already there and should be used
for the index fix.

---

## 7. Security

Reprise is a local desktop player, but it has three real trust boundaries:
**foreign feeds** (RSS/YouTube/radio-browser deliver third-party text, URLs and
images), **a subprocess** (`yt-dlp`), and **the agent surface** (MCP/CLI write
into the same database). Each was reviewed separately.

### 7.1 What is already right

Notably careful, and should stay:

- **No SQL injection through sort parameters.** `SORT_WHITELIST` in
  `queries/clauses.rs` is a lookup table; `sort_field` is only ever a key,
  never interpolated, and unknown values fall back to `title` silently. All
  user input is bound.
- **No path traversal from foreign data.** Podcast downloads land under
  `fnv1a_64(feed_url)/fnv1a_64(guid).ext` (`podcasts/downloads.rs`), so a feed
  cannot choose its filename at all. A `../..` in a GUID becomes a hex word.
- **The new write path into the music collection is tightly scoped.** Since
  `#189`, Reprise writes `cover.<ext>` and `.lrc` beside existing tracks
  (`cover_writeback.rs`, `lyrics/sidecar_write.rs`, `writeback_publish.rs`).
  The target is derived from the *track path* only, never from provider data;
  `write_album_cover` additionally checks that the bytes really are the claimed
  image format (`validated_image_extension`), that the extension is in
  `cover::IMAGE_EXTS`, and bails out when the album already has artwork. An
  existing file is never overwritten. The right construction for the riskiest
  operation the app performs.
- **Response sizes are capped.** `http_body::read_bounded_string` at 2 MB for
  feeds and JSON, `cover_download::MAX_IMAGE_BYTES` at 20 MB,
  `source_artwork::MAX_IMAGE_BYTES` at 4 MB — each with `take(N+1)` and a
  check, so a hostile server cannot exhaust memory.
- **XML bombs are excluded.** `podcasts/feed.rs` uses `quick-xml` with
  `check_end_names = true` and **expands no entities**: an undeclared entity is
  kept verbatim and logged, never resolved. Billion Laughs is structurally
  impossible rather than merely unlikely.
- **Errors leak nothing.** `SourceError` separates safe display from technical
  payload, with three tests proving neither `Display` nor `Debug` emits a host,
  token, status code or path.
- **`unsafe` is confined to one site** with a reasoned SAFETY comment
  (`kill(-pgid, SIGKILL)` in `podcasts/ytdlp.rs`); in the GTK crate a gate
  limits `unsafe` to exactly one allow-listed file.
- **Image IDs are validated, not trusted.** `youtube.rs` admits only the
  YouTube ID alphabet into the thumbnail URL, with reasoning in the code that
  is exactly right ("turns an implicit assumption into a checked one").
- **The Flatpak sandbox is narrow and mechanically guarded.** No
  `--filesystem=home`, no `--device=all`, no session bus;
  `check-flatpak-device-permissions.sh` admits no `--filesystem=` line beyond
  `xdg-run/gvfsd` and aborts otherwise.
- **Credentials live in the keyring** (`oo7`), not the database, and the
  bundled Ticketmaster key is explicitly marked in `RELEASING.md` as
  extractable from a published binary rather than treated as a secret.

### 7.2 Finding S1 — no `--` before the URL in yt-dlp calls

`ytdlp.rs::list/resolve` and `ytdlp_download.rs` append the URL as the last
positional argument **without** a preceding `--`. yt-dlp parses options at any
position, so the string's content alone decides whether it is read as a URL or
as an option.

It is **not exploitable today**, for three independent reasons — all of which
are accidental rather than guaranteed:

1. User and agent input goes through `url_detect::detect`, which admits only
   `http`/`https`; a string starting with `-` does not parse as a URL and
   becomes a search.
2. Episode URLs are built as `format!("https://www.youtube.com/watch?v={id}")`
   — the prefix is a literal.
3. Search terms become `ytsearch5:{terms}` and therefore never start with `-`.

Nothing *holds* that invariant. A future caller passing a stored `feed_url`
straight through — after an import, say, or a migration — breaks it, and the
compiler will not say a word.

**Recommendation (small, defensive).** Insert a `--` immediately before the
first positional argument in `run()` and the download path, plus a debug
assertion that the URL starts with `http://` or `https://`. Two lines, and the
invariant moves from three separate accidents into the code.

### 7.3 Finding S2 — `--cookies-from-browser` hands cookies to a foreign process

With a browser session configured, every yt-dlp call appends
`--cookies-from-browser <browser>`; yt-dlp then reads the browser's cookie
database and sends those cookies to YouTube. That is the feature working as
intended (POD-22, "YouTube needs a signed-in browser"), and
`resolve_browser_session` restricts the value to supported browsers (covered by
`pod_22_browser_session_round_trips_only_supported_browsers`).

It is nonetheless the broadest permission the app ever exercises: access to
another program's credentials. Two observations:

- **Under Flatpak it cannot work anyway** — the sandbox cannot see the browser
  profile. Good, but it should be *explained*, or it gets reported as a bug.
- **`REPRISE_YTDLP_COOKIES_FROM_BROWSER`** (`ytdlp_discovery.rs`) overrides the
  setting from the environment. Sensible for development; in a release build
  the variable should be ignored so the visible setting is the only source of
  that decision.

### 7.4 Finding S3 — redirects without target checks (SSRF, low severity)

`ureq` follows redirects (default up to ten) with no check on the target, so a
hostile feed can point at `http://127.0.0.1:…` or an address on the local
network and Reprise will fetch it.

Severity is low: the response is parsed as a feed, JSON or image, the result
never reaches the attacker, and a desktop client is not an interesting SSRF
pivot. Worth noting anyway, because a user subscribes to a URL and does not
expect their machine to probe their own network as a result.

**Recommendation.** No custom resolver. A small check inside the shared
`SourceClient` (§4.4) suffices: reject redirects to loopback, link-local and
private ranges and report `SourceErrorKind::Unreachable`. The shared HTTP
boundary is exactly the right place — today it would have to be built five
times.

### 7.5 Finding S4 — the agent surface writes into the same database

`reprise-mcp` is designed as "read-only resources plus capability-gated create
tools" and holds that line: `PlayTrackIds`, `QueueAddNext` and `QueueAddLast`
stay track-only and validate against existing IDs, and there are no delete, tag
or playback tools beyond the intended scope. `capability.rs` is the gate.

Two things should be clear before a release with MCP enabled:

- **Capability grants are invisible in the app.** Someone who sets up the MCP
  server cannot see inside Reprise which classes of operation an agent
  currently holds.
- **`source_actions.rs` accepts URLs from agents** and creates subscriptions.
  The path goes through `url_detect`, so it is as narrow as the GUI path — but
  an agent can trigger unattended network connections to hosts of its own
  choosing. That is intended (hence capability-gated), but it belongs in the
  test-round documentation rather than in a tester's discovery.

**Recommendation for the test round.** MCP off by default, and when on, a
visible line in preferences naming the granted capabilities.

### 7.6 Still to check, not conclusively assessable here

- `cargo audit` runs in the gate with exactly one accepted advisory
  (RUSTSEC-2024-0436, `paste` via `lofty`). The "a new advisory means STOP"
  rule is right; for a release, `cargo deny` (licences plus duplicates) should
  also run once, because `LICENSING.md` makes claims nothing checks today.
- Image decoding uses `image::load_from_memory` without explicit `Limits`. The
  byte cap in front covers the simple case; a decompression bomb (a small PNG
  with an enormous pixel area) is **not** covered. Setting `image::Limits` with
  `max_alloc` and a maximum edge length is one line per decode site.

---

## 8. Stability

"Does the app crash in the field?" has an unusually good and an unusually bad
answer here — both measurable.

### 8.1 The good half: panic discipline is earned, not claimed

- Production code holds effectively no `unwrap`/`expect` (§3.1): core **1**,
  runtime crates **0**, GTK about twenty at narrowly argued sites.
- `TrackListModel` degrades to `None`/`0` and logs on any database error rather
  than panicking — stated explicitly: *"a broken DB connection must never crash
  the UI thread."*
- Generation tokens stop late covers, metadata, lyrics or progress values from
  writing into a recycled row.
- `one_shot_task` names every worker thread (findable in a backtrace) and is
  cancellation-safe: a dropped receiver simply discards the result.
- The `Db` handle migration removed **575** `RefCell` borrows on the database
  path outright — that panic class is structurally excluded there.
- The ledger shows the class being actively hunted (task 0.4: a re-entrant
  subscriber borrow in the podcast runtime, closed with a red-green
  regression).

### 8.2 Finding T1 (critical) — a panic is a silent abort

The GTK crate nonetheless still holds **1,633** `borrow()`/`borrow_mut()` calls
across roughly 160 `Rc<RefCell<…>>` cells. `AGENTS.md` calls exactly that "the
#1 recurring panic class". What happens when one fires:

1. A `BorrowMutError` panics inside a GTK callback.
2. The callback was invoked across the C boundary; unwinding through
   `extern "C"` is an **abort** in current Rust, not an error path. The process
   is gone.
3. The panic message goes to **stderr** — where the tester never sees it
   (§3.3).
4. There is **no `panic::set_hook`** anywhere in the workspace.

So the worst failure mode is also the least diagnosable: the window vanishes,
with no message, no artefact, and no bug report saying more than "it suddenly
disappeared".

**Recommendation (wave 0, together with the log file).** A
`std::panic::set_hook` that writes message, location and backtrace to the log
file before the process ends, plus a marker file that lets the next start offer
"Reprise closed unexpectedly last time — copy diagnostics?" exactly once. Little
code, and it turns the worst failure class from invisible into reportable. The
hook should force its own backtrace capture, because a tester never sets
`RUST_BACKTRACE`.

### 8.3 Finding T2 — a dead worker is not noticed everywhere

`one_shot_task` delivers its result over a channel. If the task panics the
sender drops and the receiver gets `Err(RecvError)`. Callers handle that
inconsistently:

- **Exemplary:** `tag_edit/tag_edit_flow.rs` has its own `Err` arm, logs
  "worker channel closed unexpectedly", re-enables the dialog buttons and shows
  a message.
- **Gap:** `delete_tracks.rs` does
  `let Ok(result) = receiver.recv().await else { return; };` — the dialog stays
  as it was, with no message and no log line.

**Recommendation.** Anchor the convention in `one_shot_task` rather than
repeating it per caller — a `recv_or_fault(&receiver, "delete tracks")` that
logs and returns a typed reason. After that, "the worker died" is a state with
a name instead of a `return`.

### 8.4 Finding T3 — the startup path has no fallback

Summarised from §3.2 under the stability lens: the app has **exactly one** way
to start, and it ends in a panic on trouble. No error dialog, no read-only
mode, no "open a library elsewhere". For a test round on other people's
machines — different filesystems, full disks, home on NFS, an older file after
a downgrade — that is the most likely crash source of all, and simultaneously
the cheapest to fix.

### 8.5 Finding T4 — 72 borrows that outlive their block, and the idiom that hides it

`AGENTS.md` names the rule: *never hold a `Ref`/`RefMut` across a call that can
re-enter GTK/callbacks — clone/copy the value out in its own statement first.*
The GTK crate has 1,633 borrows over 426 cells, and almost all of them are
fine: a borrow in its own statement drops at the semicolon.

The dangerous shape is a borrow used as a **scrutinee**, because Rust keeps
scrutinee temporaries alive until the end of the whole statement — body
included. Measured with `rustc 1.94.1`, using `try_borrow_mut().is_err()`
inside each body to ask directly whether the borrow is still held:

| Shape | edition 2021 | edition 2024 |
| --- | --- | --- |
| `if let Some(v) = c.borrow().clone() { … }` — then body | **held** | **held** |
| the same, `else` body | **held** | released |
| `match c.borrow_mut().take() { … }` — arms | **held** | **held** |
| `for x in c.borrow().iter() { … }` — loop body | **held** | **held** |
| `let v = c.borrow().clone();` then `if let` | released | released |

Two things that look like protection are not:

- **`.clone()` and `.take()` in the scrutinee do not release the borrow.**
  This is the trap. The value is owned, so the code reads as if the cell were
  released, and the `Ref` is still alive through the body. Every one of the 72
  sites below is written this way.
- **Migrating to edition 2024 would not fix it.** That change shortened `if
  let` temporaries only for the `else` branch — measured above. The then-body,
  match arms and loop bodies are unchanged.

Only the project's own prescription works: hoist the borrow into its own
statement.

**The 72 sites, by what the body does while the borrow is alive:**

| Class | Count | Why it matters |
| --- | ---: | --- |
| **Invokes a user-supplied callback** | 19 | The cell holds a callback, the body calls it while borrowing the cell that holds it. Code the owner does not control runs inside the borrow. |
| Calls into GTK (`dismiss`, `set_*`, `remove`, `present`) | 22 | GTK emits signals synchronously; any handler is a re-entry candidate. |
| Plain data manipulation | 31 | Safe unless the body grows a call later. |

The 19 are the interesting ones, and the project has already been bitten by
exactly this shape: the ledger records *"released the podcast runtime's
subscriber `RefCell` borrow before invoking callbacks so a callback can safely
register another subscriber during notification without a reentrant panic"*.
That was fixed in one place; the same construction stands in nineteen others,
including `view_session.rs:138`, whose callback calls
`GtkSearchEntry::set_text` — GTK signal emission — while the borrow is live.

**Honest severity: latent, not demonstrably live.** Every callback slot has
exactly one writer, a `set_on_*` method, and every call to those setters is in
window construction or view wiring. So no current path re-enters, and I could
not produce a reachable panic by reading. What makes it worth fixing anyway is
the combination: the guard is convention rather than compiler-enforced, the
idiom that violates it looks correct, a panic in a GTK callback is a process
abort, and that abort currently leaves nothing behind (§8.2). One new callback
registration inside a handler turns a latent site into a silent crash for a
tester.

**Remedy.** `clippy::significant_drop_in_scrutinee` targets precisely this
class and would make it mechanical instead of a review habit. It is a nursery
lint, so it needs enabling deliberately — and it cannot simply be switched on
today, because it would fire on all 72 at once. The order is: fix the 19
callback sites, then the 22 GTK ones, then turn the lint on so the class
cannot come back. How many sites the lint actually flags is unverified here —
this checkout cannot build the workspace (§9.3), so the 72 come from a
structural scan, not from clippy.

That scan is `scripts/tests/scan-scrutinee-borrows.py`, kept so the number can
be re-derived rather than trusted: run it from the repository root, and it
prints every site with its construct and its line. It is a text scan, not a
borrow checker — it reports the shapes above, and deciding whether a given body
can re-enter is still a reading job. It also lists the 153 explicitly bound
guards (`let g = cell.borrow_mut();`) by file, which are the other way a borrow
reaches a call and are not classified here.

### 8.6 Finding T5 — the watcher's ignore registry now grows without bound

`library/watcher.rs` keeps a process-lifetime
`static IGNORE_LIST: OnceLock<Mutex<HashMap<PathBuf, Instant>>>`. Its only
pruning is inside `is_ignored`, and only for the exact path being asked about,
and only once that path's deadline has already passed. There is no sweep. The
comment justifying that design says so plainly:

> the registry only ever holds a handful of recently-written paths at a time,
> so a stale entry sitting unpruned until its own path is next checked is not a
> meaningful leak

That was true when the only caller was the tag editor, writing a handful of
files the user had just selected. `#189` made it false.
`writeback_publish::publish` arms an ignore window for **two** paths per
publication — the target and a temporary named `.reprise-<16 hex>.tmp` — and
the temporary's name is unique per publication by construction (64 random
bits).

The target's entry is eventually prunable: that path recurs, so a later
`is_ignored` on it clears the stale entry. **The temporary's never is.** The
file is unlinked within the same publication, so no inotify event ever carries
that path again, so `is_ignored` is never called for it again, so its entry
stays for the life of the process.

One permanently unprunable entry per published file. A library-wide lyrics
batch writes one sidecar per track; cover writeback adds one per album. On the
maintainer's 1,686-track library that is a few hundred kilobytes — nothing. On
a 100k library it is on the order of 15–20 MB retained in a long-running
desktop process, and it grows again every time a batch runs.

Severity is low and the fix is small: give the registry a bounded sweep, or
have `publish` drop the temporary's entry once the file is gone. What matters
more than the bytes is that the **rationale in the code is now wrong**, and the
next person to reason about that registry will read it and believe it.

### 8.7 What does *not* threaten stability today

For an honest list, the verified non-findings:

- **Threading:** few, named threads; GStreamer events cross the thread boundary
  only as `Send` data over an `async-channel` drained by a single long-lived
  loop on the main context. The drain holds only a `Weak`, so it cannot keep
  the controller alive.
- **Database concurrency:** WAL, a named 5 s `busy_timeout`, workers opening
  their own handles instead of sharing a connection, migrations running
  transactionally together with the `user_version` bump.
- **Subprocess cleanup:** yt-dlp runs in its own process group with a deadline;
  timeout and failure paths kill the whole group, leaving no orphaned
  downloader.
- **Known upstream bugs are documented** rather than worked around
  (`docs/upstream/`), including reproduction scripts.

---

## 9. Backwards compatibility and the test release

### 9.1 The rule flips with the release

`AGENTS.md` says today:

> **Not released yet — no backwards compatibility.** Reprise has **not** shipped
> and there are **no existing installations**.

From the day the first tester installs, that sentence is false, and the
permission built on it ("where a clean and a backwards-compatible data model
collide, take the clean one and delete the old shape outright") becomes a
data-loss risk in other people's libraries.

**Recommendation, in the same commit as the release:** replace the section with
a cut-off rule — *from schema 50 / version 0.1.1 onward installations exist;
migrations are forward-only and lossless; a field may disappear once a
migration has carried its content over; settings keys are migrated, not
discarded.* Without that change, the next rule-abiding "clean rewrite" will
delete tester data.

### 9.2 What is already compatible — good

- **Schema 50 with forward migrations**, each step in a transaction together
  with its `user_version` bump (the comment in `db.rs` explains the crash case
  that shaped it).
- **`SchemaTooNew` is detected** — a downgrade never silently runs against a
  newer file. (The GUI throws the information away — §3.2.)
- **`db_grandfather.rs` is already a real compatibility mechanism:** existing
  databases keep the network features they had before the module gate existed,
  decided from evidence in the data (subscriptions, radio favourites,
  downloads, cover cache) rather than a blanket assumption.
- **A protocol compatibility test** in the runtime protocol: an older
  dictionary without typed fields still decodes.
- **Dedicated migration test files** per area (`db_recent_migration_tests.rs`,
  `db_podcasts_radio_migration_tests.rs`, `db_network_migration_tests.rs`, …).

### 9.3 Finding K1 (release blocker) — the declared MSRV is unreachable

Reproduced here with `cargo build -p reprise-core --locked`:

```
Compiling libsqlite3-sys v0.38.1
error[E0658]: use of unstable library feature `cfg_select`
  --> libsqlite3-sys-0.38.1/build.rs:110:9
```

The **pinned** dependency graph does not build with `rustc 1.94.1`, while every
workspace manifest declares `rust-version = "1.92"`. `scripts/tests/msrv.sh`
cannot catch this: it reads `cargo metadata` and checks that the *field* says
`1.92` everywhere. It never builds.

This matters for the release because `org.reprise.Reprise.yml` builds with
`org.freedesktop.Sdk.Extension.rust-stable` under GNOME runtime 50 and
`CARGO_NET_OFFLINE=true` against `flatpak/cargo-sources.json` — that is,
against exactly these pinned versions. Whether that SDK extension's rustc is
new enough decides whether the Flatpak builds at all. CI builds on rolling Arch
and does not answer the question.

**Recommendation.** Determine the version actually required, set `rust-version`
to it (or take `rusqlite`/`libsqlite3-sys` back to a version that holds 1.92),
give `msrv.sh` a real build with the declared toolchain, and consider a
`rust-toolchain.toml` so developers and CI see the same one.

### 9.4 Finding K0 (release blocker) — `dev` is red on its own merge gate

Measured 2026-07-31 against an untouched `origin/dev` checkout — **two** gates,
not one:

```
frontend thinness: filesystem grew from 17 to 19
frontend thinness: threads   grew from 14 to 15

window.rs has 600 lines; the composition root must stay below 600
```

The second is the harder stop: it is the *first* check in
`check-architecture.sh`, so that script exits before reaching any other
architectural rule. Until it passes, no other violation in the repository can
even be observed.

`scripts/check-frontend-thinness.sh` treats each budget as a ceiling **and** a
floor: a commit that adds a use raises the number in the same change, with a
reason. `65f0b14` (`#189`, lyrics and covers) added
`crates/reprise-gnome/src/ui/lyrics/lyrics_batch.rs` — a batch worker with its
own thread and filesystem access — and did not.

That script runs inside `scripts/check-merge-readiness.sh`, so the merge gate
is red on the integration branch right now and every following PR inherits it.
The mechanism is not broken; it is doing exactly its job and saying so.

Two ways out, and the choice is really about where `lyrics_batch.rs` belongs.
Raising the budgets to 19 and 15 with a written reason is legitimate if the
batch worker belongs in the frontend. It probably does not:
`reprise_core::lyrics` already owns the providers, the cache and the circuit
breaker, and the batch runs a provider chain and writes `.lrc` sidecars — core
work by every line this project draws elsewhere. The pragmatic answer for a
near-term test round is to raise the budgets with the reason recorded and log
the move as a follow-up; the dangerous answer is to raise them quietly, which
is how a budget stops meaning anything.

### 9.5 Finding K2 — `check-stem-runtime-packaging` is red on the base

The ledger records it: *"the extra release-only
`scripts/check-stem-runtime-packaging.sh` probe remains red on the unchanged
base because `build-aux/meson-cargo-build.sh` lacks the two ONNX runtime
environment markers the check requires."* It belongs to
`scripts/check-release.sh`, not to `check-merge-readiness.sh`, so it correctly
never blocked a merge — but it will stop every release check.

**Recommendation.** Before the release, either fix it or turn the stem feature
off for the test round (`-Dstem_backend=false`) and gate the check
accordingly. For a first round the latter is the smaller bet — an experimental
ML feature creates support load that distracts from the actual test goal.

### 9.6 Finding K3 — the Flatpak sandbox is strict, and it is the first hurdle

`finish-args` contains **no** `--filesystem=home` and no
`--filesystem=xdg-music`; `check-flatpak-device-permissions.sh` forbids every
`--filesystem=` line except `xdg-run/gvfsd`. Library access therefore runs
exclusively through the portal folder chooser. A good, deliberate decision —
and simultaneously the first thing every tester touches.

**Recommendation.** Verify exactly that path before the release, in a real
Flatpak rather than a dev build: choose a folder → scan → **quit the app** →
restart → are the tracks still playable? If the portal permission is not
granted persistently, the app is empty after the first restart, and that is the
bug report that would dominate a whole test round. `RELEASING.md`'s "Manual
GNOME QA" already lists the step — it is the most important one on the list.

### 9.7 Further release observations

- **`reprise-runtime` ships** (Meson target, two `.service` files) and is used
  by nothing (§2.2). For a test round: do not ship it.
- **`AGENTS.md` is out of date** (§2.5).
- **`docs/ux-rules.md` has two sections lettered `T`** (line 1921
  "T. Accessibility & Keyboard", line 1995 "T. Network features opt-in") and no
  section `AC`. With 313 active rules mapped to tests by
  `check-ux-traceability.sh`, a duplicate section letter is a trap for the next
  rule number.

---

## 10. What is explicitly *not* recommended

So that later sessions do not walk into the same temptations:

- **No universal `SourceTablePage` abstraction** across the track list,
  podcasts, radio, releases and concerts (§4.5). The row shapes genuinely
  differ; a shared table would replace code with configuration and turn special
  cases into flags. Only the frame (filter bar, empty state, error banner)
  belongs shared — and three of those four already are.
- **No merging of radio and podcasts in the core.** Stations are not episodes:
  no feed, no GUID, no resume position, no download. The commonality is the
  HTTP boundary and the UI shell, not the data model.
- **No splitting `reprise-core` into several crates.** 108k lines is a lot, but
  the module boundaries are clean, purity is checked mechanically, and several
  crates would mostly generate feature-flag combinatorics.
- **No move to async/tokio.** Blocking HTTP on worker threads plus
  `async-channel` at the GTK boundary is the simpler and already proven
  solution for this program. `tokio` correctly lives only in `reprise-mcp`,
  where the SDK forces it.
- **No lowering of gates to land waves faster.** The 800-line limit, the
  orchestrator caps and the frontend thinness budgets are why this repository
  is still navigable at 293k lines.

---

## 11. Proposed gate additions

Each line makes one finding unrepeatable. They land with the task that closes
the finding, never as a sweep at the end.

**Two of these already landed**, because they were green the day they were
written — a gate that fails on arrival teaches everyone to run the suite with
one known red, which is how a gate dies:

- ✅ **The engine HTTP-boundary budget** (gate 4 below), capped at today's 16
  and lowerable only. Proven in both directions: a 17th
  `ureq::Agent::config_builder()` fails it, and removing one without lowering
  the budget fails it too.
- ✅ **Documentation paths cited from code resolve** — deliberately narrow.
  Markdown-to-markdown links are *not* checked, because two legitimate cases
  would need carve-outs: the append-only ledger names plans that have since
  been deleted, and a plan may forward-declare the file it creates. A gate
  whose exception list is as interesting as its rule does not survive contact.
  Code has neither excuse, so code is what it checks.

The rest stay proposals, because each would be red until its own fix lands:

1. **`msrv.sh` actually builds** with the declared toolchain (§9.3).
2. **No `expect`/`unwrap` in `main.rs`** — an `rg` ban in
   `check-architecture.sh`, three lines (§3.2).
3. **Duplicated UI constants** — `FILTER_BAR_MIN_HEIGHT`, `FACET_PAGE`,
   `VALUE_PAGE` may be defined exactly once (§4.2).
4. **Unique section letters in `ux-rules.md`** — one line in
   `check-ux-traceability.sh`. Needs its fix first: there are two sections
   lettered `T` and no `AC` (§9.7). Renaming is safe — nothing reads a section
   letter programmatically, and rule IDs do not derive from it — but it moves
   a heading in the binding contract, so it is the maintainer's call, not a
   drive-by.
5. **If the runtime ships, someone uses it**: a check that
   `reprise_runtime_client` is referenced outside tests as soon as
   `data/*.service.in` are installed (§2.2).
6. **Every yt-dlp positional argument sits behind `--`** (§7.2).
7. **`cargo deny` in the release gate** for licences and duplicates, because
   `LICENSING.md` makes claims nothing checks today (§7.6).

---

## 12. The questions, answered compactly

**"Clean architecture so several apps can build on one core?"**
The foundation holds: the `Db` handle, checked purity, no SQL outside the core.
Three things are missing: `rusqlite::Error` out of the public API (§2.3), a
decision about the second runtime (§2.2), and per-view ports instead of a
40-field composition root (§2.4).

**"Clean error handling and logging for debugging?"**
Error handling: very good, with **one** hard gap — the app panics when opening
the database (§3.2). Logging: usable for developing, unusable for testers,
because it only reaches stderr (§3.3).

**"Did we build things twice — radio, podcasts, YouTube, playlists?"**
YouTube and podcasts: no, exemplarily shared. Playlists: no, one model for all
local sources. What is doubled is the *shells*: five filter bars (§4.2), two add
dialogs (§4.3), sixteen HTTP boundaries with five rate limiters (§4.4) — and
for the last of those a consolidation task was already promised and is now due.
The largest duplication is elsewhere: two command surfaces for playback and the
queue (§2.2).

**"Are playlist filters and artist-page pills cleanly distinguishable?"**
Since `c565671`, yes — place and filter differ in shape, position, gesture and
counting vocabulary, and the decision lives in pure, rule-named functions. Three
residual points in §5.3, one of them worth doing today: `has_place_pill` and
`has_sidebar_row` should be one function.

**"Performance optimisations?"**
One stands out: the library's default sort has no index and costs up to 380 ms
per window at 100k tracks — on the UI thread. An index brings that to 3.4 ms
(§6.1). After that: search without FTS and `OFFSET` paging, both to be
re-evaluated once that measurement exists.

**"How is security?"**
Above average. No SQL injection (whitelisted keys), no path traversal (hashed
download paths), bounded bodies, no entity expansion, a redaction-tested error
type, one justified `unsafe`, a narrow Flatpak sandbox. Three defensive gaps:
no `--` before the yt-dlp URL (§7.2, unexploitable today only by accident),
redirects without target checks (§7.4, low severity), and image decoding
without limits (§7.6). All three are small and belong in the wave that builds
the shared HTTP boundary.

**"How stable is the app?"**
Panic discipline is earned, not claimed: effectively no `unwrap` in production,
generation tokens against stale async results, named worker threads, and 575
borrows of the most common panic class removed by the `Db` migration. The
problem is not how often it crashes but how **invisibly**: 1,633 remaining
`RefCell` borrows, a panic in a GTK callback is a process abort, there is no
`panic::set_hook`, and the message goes to stderr where no tester sees it
(§8.2). A crash leaves nothing behind today. That is the most valuable small
fix in this document.

**"Is it backwards compatible?"**
Data model: yes — forward-only transactional migrations up to schema 50, a
detected downgrade, real grandfathering. But the project rule still says
explicitly "no backwards compatibility needed", and that has to flip with the
release (§9.1), or the next rule-abiding "clean rewrite" deletes tester data.
What is *not* compatible right now is the **build** side: the declared MSRV of
1.92 is unreachable with the pinned dependency graph (§9.3).
