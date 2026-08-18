---
slug: multi-frontend-core
worktree: ~/Projects/reprise-multi-frontend-core
branch: feature/multi-frontend-core
phase: shipped
codex_session:
created: 2026-07-21
---
# Multi-frontend core — architecture plan

This shipped plan is retained because tracked files still reference it as durable documentation.

Grilled and decided 2026-07-21 (section 7); base `origin/dev`
`797afa2dfa`. This document is the execution document. Deliberately open
are only: (a) the ML runtime choice — the spike in package E decides it
on the facts; (b) the file lists of packages C and F — to be verified at
package start against whatever the state is then and pinned down exclusively.

Goal: all surfaces — the existing GTK/GNOME app, a standalone CLI, an MCP
server, later KDE/Qt, Windows, Android, iOS — build on the same Rust core
(`crates/reprise-core`). CLI and MCP run as their own processes, the MCP
server also **while** the app is running. Changes from MCP/CLI appear
**live** in the running app, without a restart.

Cross-surface features of this plan:

1. **Create playlists** (CLI + MCP, visible live in the app); the CLI
   additionally with `rename`/`delete` (decision 3).
2. **Instrumental versions (vocal removal, experimental)**: explicitly
   selected songs are processed by ML stem separation (Demucs-class quality —
   quality is the inclusion condition), initially land as immediately
   playable **staging renders** in the conversion playlist
   and only become real, permanent instrumental tracks in a dedicated folder
   through an explicit **save decision** —
   as regular library titles clearly marked as AI-manipulated.
   A library filter can hide AI music (opt-in, decision 17).
   Triggered via context menu, the conversion playlist, CLI and MCP;
   progress live everywhere. The feature is switched to **experimental**
   and planned as an isolated, removable package — it is not on
   the critical path of the architecture work. What is output is
   exclusively the instrumental track (decision 19).

Two models considered early on were rejected: a global
"remove vocals" switch with transparent substitution during playback
as well as a rolling render window with a transient cache. Decision
(2026-07-21): explicit, permanent instrumental tracks instead of a toggle and
cache — simpler, predictable, no eviction machinery. Genre remixes
remain dropped (quality), likewise the cheap DSP center-cancel mode.

Relationship to existing documents:

- `docs/ux-rules.md` remains binding and ranks above this plan.
- `docs/superpowers/specs/2026-07-19-audio-character-mcp-design.md` and
  `docs/plans/audio-character-mcp.md` (phase: ready-for-review) remain valid
  for sound profile / mix planning. This plan **pulls their stage 2 task M1
  forward** (founding of `crates/reprise-mcp`) and **extends the tool domain**
  (decisions 2, 10). Marking the M1 paragraph there as
  "superseded by multi-frontend-core" and the addendum note on D17
  in the spec document are named tasks in package I; M2–M5 and stage 1B
  remain untouched. Sound profile analysis (stage 1A) is merged onto `dev`
  and is neither touched nor assumed.
- `docs/plans/android-sync.md` / `android-sync-handoff.md` concern
  device sync (MTP), **not** an Android frontend; untouched. The
  V2 "companion app" there is a later seed of an Android surface.
  Instrumental versions are normal tracks and therefore automatically
  sync-capable — no special logic needed. The unified bottom slot noted
  there as V2 is not touched by this plan
  (decision 18).
- `motion-player` (planned), `mystats-optimization` (shipped),
  `accessibility-keyboard`, `ux-rules-*`: no overlap other than the
  obligation to anchor new visible behavior as `[planned]` rules first.

## 1. Current state — an honest inventory

### 1.1 What the separation already achieves today

The workspace really is split in three, the direction is enforced mechanically
(`scripts/check-architecture.sh`, core purity via `cargo tree`):

- **`reprise-core`** (MIT, dependency-pure: no gtk/glib/gstreamer/zbus)
  owns almost the entire domain logic: `db` (open/migrate, schema v18, WAL +
  `busy_timeout=5000` + `foreign_keys=ON` already set), `library`
  (scanner with atomic mark-vanished, `notify` watcher, playlists,
  smart playlists, M3U, settings, session, stats, tag edit incl.
  tag **write** path via lofty, trash facade), `queries`/`view_source`
  (windowed 200-row windows), `queue` (+ snapshot),
  `audio_analysis`/`sound_profile` (stage 1A merged), `device_sync`,
  `modules` (persisted feature flags), cover/lyrics/MusicBrainz/
  scrobbling paths as well as the platform **contracts** `playback`,
  `media_integration`, `waveform`, `fingerprint`, `audio_analysis` backend.
- **`reprise-platform-linux`** (MIT): GStreamer player (playbin3, effects,
  gapless/crossfade), MPRIS via zbus, MTP incl. Opus transcode pipeline,
  trash, GStreamer analysis adapter (streamed PCM chunks via AppSink —
  an existing, reusable decode path).
- **`reprise-gnome`** (GPL-3.0-or-later, only binary `reprise`):
  presentation and interaction; SQL in the frontend is forbidden by gate.

Found favorable for multi-process operation:

- Several connections over the same DB path are already everyday practice
  **inside** the app today: the UI holds `Rc<Db>`; scan worker, watcher thread and
  analysis worker each open their **own** `Db` handle (documented in
  `library/watcher.rs`). The step to several processes is the same mechanism
  with SQLite/WAL.
- `notify` is already a core dependency (deliberately as a cross-platform
  abstraction over inotify/FSEvents/ReadDirectoryChangesW).
- GApplication is single-instance (`org.reprise.Reprise`); a second
  app start does not touch the DB at all. MPRIS exists as a
  cross-process playback interface.
- LICENSING.md plans for foreign/proprietary frontends over the MIT engine path
  and already contains a license gate for audio analysis models that
  can be extended verbatim to separation models.

Since PR #23/#29, Similar Mix + Artist Discovery as well as the
song visuals are additionally on `dev`; PR #29 centralized browser navigation
(`ui/window/window_navigation.rs`, `ui/window/library_shell.rs`,
`ui/browse/**`) and replaced the old library modes with a scoped track list.
That shapes the file lists of packages C and F (section 4) and the
idea parking lot (section 8).

Test baseline: binding is the **current ledger state at
package start** (`.superpowers/sdd/progress.md`) — the most recently merged
parallel states (Similar Mix, song visuals, single-track browser) carry
slightly different counts; the number measured at the P0 start is
the reference baseline against which every package stays green.

### 1.2 Where the seams leak

1. **No change signal between processes.** No event/change concept in
   core. The app refreshes itself after its own actions
   (`sidebar.refresh(reason)`, watcher channel for file system events); a
   foreign DB writer would remain invisible. SQLite update hooks do not solve
   this: they only fire for the **own** connection, never for foreign
   processes.
2. **No schema future protection.** `db::migrate` runs up 1..=18, but never
   checks `user_version >` target version. As soon as app, CLI and MCP are
   updated separately, an older binary works silently on a newer
   schema. Becomes fail-closed (decision 8, P0).
3. **Orchestration is stuck in `reprise-gnome`.** Scan worker, analysis
   scheduler, cover batch, scrobble runtime are conceivable GTK-free, but are
   wired into `ui/*` runtimes. Uncritical for CLI/MCP v1 (they call
   facades); the biggest open debt for later native frontends. This
   plan deliberately moves none of that (2.3).
4. **Long write transaction during the scan.** `scan_folder` is one
   transaction (walk + mark-vanished, deliberately atomic). During large
   rescans an external writer can blow through the 5 s `busy_timeout` —
   an operating window, to be cushioned by retry.
5. **No CLI, no MCP, no provenance.** The only binary is `reprise`.
   No `mcp`/`rmcp` code in the workspace (`crates/reprise-mcp` exists only
   as a plan). The schema knows neither derived tracks/provenance nor
   jobs (tables: tracks, playlists(+tracks), smart_playlists, settings,
   listen_events, device_*, import_errors, new_releases,
   track_audio_analysis, scrobble queues).
6. **Playback state is the app's process property.** Queue and pipeline live
   in the app process; the DB only holds the session projection. Externally
   controllable, playback is today only via MPRIS (Linux) — exactly what the CLI
   builds on (decision 3).
7. **Exactly one scan root.** `settings.get_library_root` delivers a
   single path; the watcher arms on it. A "dedicated folder" for
   instrumentals is automatically scannable only **inside** this root
   without a multi-root rebuild (shapes decision 13).
8. **Smart playlists are rule queries, not drop targets.**
   `smart_playlists` stores a validated field whitelist, joined by `AND`
   (`queries/smart.rs`); membership is a query result.
   "Dragging songs in" does not exist there conceptually — the desired
   conversion playlist therefore needs its own playlist type (3.2).

## 2. Target architecture

### 2.1 Process/concurrency model — the central decision

**Decision 1: (i) Every surface embeds `reprise-core` as a library
and works on the same SQLite file (WAL), plus a thin
notification layer (2.2). No daemon. MPRIS remains the
playback IPC.**

```text
reprise (GTK, GPL)      reprise-cli (MIT)      reprise-mcp (stdio, MIT)
   | own conn(s)            | own conn              | own conn
   +------------+-----------+-----------+-----------+
                |                       |
        reprise-core facades    (commands/queries)
                |                       |
          SQLite (WAL, busy_timeout, foreign_keys)
            +-- change_log      (outbox, same transaction)
            +-- audio_jobs / track_provenance   (track 2, 2.4)
                |
        core::events::Notifier (notify on DB/WAL + data_version fallback)
                |
   GTK app: external_changes runtime -> coalesced refresh
   GTK app: AI job worker (experimental) -> batch progress
   optional: reprise-cli jobs work (feature `worker`) -> second worker host
```

Why (i):

- **The forcing functions do not require a daemon.** "MCP runs while the
  app runs" is trivial with WAL (n readers + 1 writer at a time,
  `busy_timeout` serializes short writers). "Visible live" only needs
  a wake-up call + re-read, no shared process state.
- It is **the existing model** — the app is already a
  multi-connection system over exactly this path today; the GTK wiring stays
  untouched (no migration risk).
- **Portable and license-compliant:** no D-Bus/socket in the core; Android/iOS/
  Windows use the same embedded path. The decided MCP spec
  (D16/D20: "opens the local database over the normal core path")
  already presupposes this model.
- CLI works **without** a running app (headless maintenance), which would make a
  daemon model a special case.

Rejected alternatives (decision 1, in short): (ii) core daemon with
D-Bus/socket IPC — the largest conceivable rebuild, IPC on the hotly
optimized window query path, not portable, contradicts the MCP spec; no
feature of this plan needs shared process state. (iii) App-hosted
services + standalone fallback — superfluous for data (there (i) applies);
the only real app ownership (playback/queue) already has a standardized
app-hosted interface with MPRIS; a later
`org.reprise.Reprise1` service remains a noted extension point
(section 8), is not built.

Consequences of (i), honestly: busy windows during scans (retry + clear
errors); no external access to the in-memory queue/position other than MPRIS
(accepted; MCP exposes no transport tools anyway); the schema guard
is mandatory (2.3, P0).

> **Addendum 2026-07-28 — the second of these consequences has meanwhile
> been lifted.** Decision 1 holds unchanged for *data*: no daemon, no
> IPC on the query path. For state that is never in the database —
> player pipeline, in-memory queue, running device runs and jobs — there is
> from stage 1 of the thin-core plan a runtime service with a single
> owner. Section 9 is the binding seam for that; it pulls the
> `org.reprise.Reprise1` parked in section 8 off the parking lot and limits it
> expressly to this slice.

### 2.2 Change propagation — mechanism, ordering, races

**Decision 5: transactional outbox (`change_log`) as the truth about the
*what*; wake-up call via `notify` watch on the DB/WAL file with 250 ms debounce +
`PRAGMA data_version` check; degradation to 2 s polling. All numbers
are named constants (`WAKE_DEBOUNCE_MS`, `POLL_FALLBACK_SECS`,
prune limits), no scattered literals.**

1. **Writing:** every mutating core facade (playlist create/rename/
   delete/add/remove/move, smart playlist create, settings/module change,
   scan completion as one collective event, job/track lifecycle from 2.4)
   appends **in the same transaction** a row to `change_log`:
   `(id AUTOINCREMENT, entity, entity_id, op, writer, at)`. `writer` is
   a per-process random 64-bit token (fastrand, present). Atomic ⇒
   no event without a change and vice versa; total ordering over `id`.
2. **Waking:** `core::events::Notifier` (own thread, own connection —
   exactly the watcher pattern) observes DB + `-wal` via `notify`, checks
   after 250 ms of quiet `PRAGMA data_version` (changes only on commits
   from *other* connections, microsecond-cheap). If `notify` cannot be
   armed (network FS, inotify limit), it degrades to pure
   polling (2 s). Visibility budget: ≤ 1 s normal, ≤ 3 s fallback.
3. **Consuming (GTK):** a new `ui/external_changes` runtime holds
   `last_seen_id`, reads newer rows on the wake-up call, filters out its own
   writer token (self-refresh exists), **coalesces per entity** and
   sends coarse refresh commands via `async_channel` into the main loop —
   the same pattern as `ui/scan/scan_watcher.rs`. Sidebar via the existing
   `refresh(reason)`; views via the existing reload paths. UX of the
   external changes (decision 6): update **silently** — no
   toast, no indicator; selection/scroll are preserved; no
   focus theft (as `[planned]` rules in package C).
4. **Reverse direction (GTK → MCP/CLI): free.** MCP/CLI are per-call
   stateless readers; every query sees the last commit (WAL snapshot per
   statement/transaction). No subscription needed.

Progress numbers flow over the same bus idea, but throttled:
job progress is updated in place in the job row (≤ 2 writes/s,
constant); `change_log` only receives lifecycle transitions. Every surface
reads **the same numbers** from the same rows — GTK bar, CLI output
and MCP status show identical progress.

Ordering and races: consumers do not replay operations, they
refresh state — at-least-once + coalescing is idempotent.
Rename/delete under an open view ⇒ refresh reads the actual state,
a vanished entity takes the existing empty path (its own
acceptance criterion). Running playback is never affected: the queue is
a snapshot (`queue::snapshot`); external changes only change views
(test, no new rule needed). Growth: prune at `open_migrated`
(keep 10 000 rows or 7 days, named constants); AUTOINCREMENT
prevents rowid reuse.

Rejected (in short): SQLite update/preupdate hooks (own
connection only); pure file watching without an outbox (no "what", storm on
self-writes); D-Bus signal from the CLI/MCP to the app (Linux-only, not
representable in the zbus-free core, non-transactional) — the latter
at most later as an additional latency optimizer in the platform layer
(section 8).

### 2.3 The API seam of `reprise-core`

**Position: the seam remains "facade functions over `&Db`" — commands,
queries (windowed), new: events. No command bus, no service object, no
async rebuild.** ADR 002 replaces the earlier formulation with
`&Connection` here: public facades take `&Db`; their private query helpers
may continue to use `&Connection`. The facades are already the boundary
enforced by gate, synchronous, headless testable and FFI-tolerant (values only,
no GTK/runtime types). A command bus would be speculation and would force the
GTK app to come along.

New in core (track 1, architecture):

- `events`: `record` (only from facades), `read_since`, `prune`,
  `writer_token`, `Notifier::start(db_path, on_wake) -> Option<Handle>`
  (failure ⇒ app stays usable, only without live updates —
  the watcher's degradation pattern).
- Schema guard (decision 8, fail-closed): `open_migrated` rejects
  `user_version >` target with a typed
  `DbError::SchemaTooNew { found, supported }`. No
  read-only degradation, no silent continuation.

New in core (track 2, feature — isolated and removable):

- `ai_jobs`: generic job table + state machine for AI audio jobs
  (2.4). "Instrumental" is the first job kind (`kind` field), not the
  name of the system.
- `provenance`: origin registry for AI-generated/-manipulated tracks;
  `source_track_id` is **optional**, so that generated titles
  (without a source track, with prompt/parameters as provenance) fit in later too.
- Platform contract `stem_separation` (`StemSeparationBackend` trait +
  fake for tests) following the pattern of the existing backends.

Does **not** move (deliberately): scan/analysis/cover orchestration stays in
`reprise-gnome`; CLI/MCP call the synchronous facades directly. The
extraction of the runtimes into a GTK-free `core::runtime` layer is the
right next portability step **after** this plan, when a
second native frontend needs it.

### 2.4 Instrumental versions — architecture of the feature slice

Semantics (user decision): **explicit, permanent, marked.**

1. **Triggering:** (a) context menu "Create instrumental" on one or
   several selected tracks; (b) a special
   **conversion playlist**: dragging songs in = queue for conversion;
   (c) CLI/MCP (3.2). Important: smart playlists are rule queries without
   drop semantics in this codebase (1.2/8) — the
   conversion playlist is therefore modeled as a **system playlist with a role**
   (new `role` column or system marker on `playlists`),
   which accepts drag-and-drop and whose insertions create jobs. The
   user term "smart playlist" is served on the UX side, technically it is
   deliberately not a rule playlist.
2. **Jobs:** `ai_jobs(id, kind='instrumental', batch_id, source_track_id,
   params_json, params_fingerprint (model+version+parameters),
   status queued|running|done|failed|cancelled, progress_permille,
   claimed_by, lease_expires_at, cancel_requested, error_kind,
   created/started/finished_at, result_track_id)`. An app-hosted
   worker (1 job at a time, own connection, pattern of the
   analysis scheduler) works through the queue; optionally additionally the
   CLI worker `reprise-cli jobs work` (decision 3, package H1). Lease +
   heartbeat make crashed workers reclaimable and coordinate
   several worker hosts (exactly one claimer per job; reclaim only after
   lease expiry); cancel takes effect between chunks. Multi-select creates
   a batch (`batch_id`) for aggregate progress.
   **Duplicate/delete semantics (decision 16):** dedup via
   `UNIQUE(kind, source_track_id, params_fingerprint)` over open and
   successful jobs — triggering again is a **skip with a reference to
   the existing one**, not a silent double render (a later `--force`
   is conceivable, not v1). If the **original** is deleted, the
   version remains as a standalone track; the source reference becomes
   pure provenance text. If the **instrumental** is deleted, that is
   a normal track delete — recreatable at any time.
3. **Staging before saving (decision 15).** Job completion first creates
   a **temporary staging render** (FLAC, once in final quality)
   under `~/.local/share/reprise/staging/` — app-managed storage,
   **not** in the dedicated folder, not in the library, not under
   scan roots. It is immediately playable in the conversion playlist.
   Undecided renders **are preserved, across restarts too** —
   hours of compute do not evaporate; the disk cost is visible in the
   conversion playlist, there is no silent reaper.
   Only the **save decision** (per entry; plus "Save all")
   **promotes** the render: move into the dedicated folder,
   final tags incl. AI provenance, registration in **one**
   transaction (track row via the existing
   scanner metadata path, `provenance` row, `change_log` events) —
   **no re-render**. Discarding (removing the entry or an explicit
   discard action) deletes the staging render; undecided items never
   appear in the library. This staging model deliberately unites the
   original "streaming" instinct (listen transiently) with the
   decided persistence model (deliberately keep). A later
   full rescan is idempotent (path identity); on a **fresh**
   DB the scanner reconstructs the marking best-effort from the
   embedded tags (source link then textual, decisions
   13/14).
4. **Storage location of promoted versions (decision 13):**
   `<library_root>/Reprise Instrumentals/<Artist>/<Title> (Instrumental).flac`,
   **configurable**. Inside the library root, because there is exactly
   one scan root today (1.2/7) — this way watcher, Android sync and
   all views take effect without a multi-root rebuild. This is a write into a
   clearly named dedicated subfolder that is **explicitly commissioned by the
   user**; the principle "never write into the curated library unasked"
   otherwise remains untouched. A **path guard with a test** ensures
   that promotion writes exclusively below the configured
   subfolder. A location outside the root would require
   multi-root support (named surcharge, not v1).
5. **AI provenance, disclosed twice over (decisions 13/14):**
   - **UI:** badge/reference on the track ("Instrumental · AI-manipulated",
     wording/placement per UX rules), reference to the source title,
     insofar as linked. Source link: **DB primarily** (`provenance`),
     tag reference secondarily.
   - **File tags** (convention, documented — not an invented
     "standard"): Vorbis/FLAC/Opus fields `REPRISE_AI=vocals-removed`,
     `REPRISE_AI_MODEL=<name>@<version>`,
     `REPRISE_AI_SOURCE=<Artist> — <Title>` (+ optionally
     `REPRISE_AI_SOURCE_MBID`), additionally human-readable in the
     comment field "AI-manipulated: vocals removed (Reprise)"; ID3v2
     equivalently as COMM + `TXXX:REPRISE_AI*`, MP4 as
     `----:com.reprise:AI*`. lofty (the existing tag write path) can do
     all three. The source reference is **textual + optionally a
     MusicBrainz ID — never app-internal IDs in tags** (they survive
     no DB re-creation). The disclosure thus survives outside
     Reprise too and carries the rescan reconstruction.
   - **Naming (decision 14):** the title tag gets the suffix
     "(Instrumental)"; the **album tag stays unchanged** — the
     album view shows both versions side by side, badge + suffix
     disambiguate (deliberately accepted; no fragmented
     album list).
6. **Playback rule (decided): wait with a progress bar.** The player
   plays **exclusively finished files**. If the user clicks an entry that is
   still processing, the start blocks with visible
   render progress and begins after completion (no original fallback,
   no auto-skip). On hardware below ~1× real time this can take minutes for a
   4-minute track — deliberately accepted. Progressive
   early start ("start playing as soon as the render is safely ahead of the
   playhead") is a noted later optimization (section 8), is not
   designed.
7. **Conversion playlist = staging area (decisions 15, 18):** the
   view shows an **aggregate progress bar** (done/total + percent,
   fed from the job events) and per row the state
   (queued/processing/done — unsaved/saved/failed). **There is no further
   progress UI**: no sidebar/status-bar slot
   (the android-sync-V2 bottom slot is not touched), no toast.
   **Finished titles are immediately playable** (from staging), while
   others are still processing; "play playlist" plays the finished ones,
   a click on a processing entry follows the wait rule from
   point 6. Per row: save / discard; header row: "Save
   all". After saving, **the row switches to the
   promoted library title and stays** until the user tidies up —
   "all finished ones are playable in it". "Clear playlist" **warns**
   if undecided entries exist. Dragging an already
   converted track produces a **hint instead of a double job**
   (dedup from point 2). Since staging renders are not library titles,
   the view is technically a special view over `ai_jobs` +
   staging store (playback via file path), even if it feels like a
   playlist.
8. **Filter "hide AI music" (decision 17):** a library filter
   hides AI-manipulated (and in future AI-generated) titles.
   **Default: AI titles visible, filter opt-in** — the versions are
   wanted library citizens. The filter state is **sticky across
   sessions** like other view states. It keys on the
   **provenance flag in the DB** (row in `track_provenance`), never on
   folder paths — the folder is storage layout, the flag is the truth
   (files can move; tags carry the provenance across rescans).
   It fits into the **existing filter system**
   (`docs/ux-rules.md` section K: visible restriction in the
   filter row per FIL-1a, counted state per FIL-2 — grilled
   decisions, no parallel mechanism) and is implemented as a query clause in the
   core. **No shuffle/auto-queue special rule in v1**: the
   decided queue refill at the end of the queue refills from the
   **visible view** — with the filter active, AI titles are not
   visible and consequently are not refilled either. A
   long-form exclusion rule (meditation drone in the party shuffle)
   only comes about if generation becomes real — then as a new
   `[planned]` rule, not implicitly.
9. **Experimental + leanly packaged (decision 11):** visible only
   behind an "Experimental features" switch in the settings; rough
   edges are accepted. ML runtime weights are **not** bundled into the
   default build/Flatpak: **first-use download** on first
   activation, with checksum and license note (pattern of the
   cover download module); bundling is rejected (Flathub size,
   license exposure), a Flatpak "model add-on" package at most
   later (section 8). Model license gate: LICENSING.md requires
   redistribution/commercial use for the MIT engine path — the weights license
   is verified in the spike (package E); if it fails, the feature is
   blocked, not shipped "somehow".
10. **Generic pipeline, first job kind.** Schema, crate and API avoid
   instrumental-specific names where it costs nothing (`ai_jobs.kind`,
   `provenance.kind`, optional source track). Generalizable 1:1:
   job scheduler + progress events, provenance tag schema,
   staging-plus-promotion, folder-and-library-citizen pattern,
   AI hide filter (the provenance flag covers manipulated as well as
   generated), experimental gating, on-demand runtime/model download.
   What is saved in v1 is exclusively the instrumental track
   (decision 19) — models compute more stems internally, one is
   stored. Deferred job kinds: section 8.

### 2.5 New workspace members

| Crate | Binary | Purpose | Dependencies (permitted) | License (decided) |
|---|---|---|---|---|
| `crates/reprise-cli` | `reprise-cli` | Headless surface: playlists (incl. rename/delete), search, summary, scan, instrumental jobs, job status; features: `mpris` (Linux-only, zbus directly), `worker` (pulls in `reprise-stems`) | `reprise-core`, `clap` v4 (derive), `serde_json`; behind features: `zbus`, `reprise-stems` | MIT |
| `crates/reprise-mcp` | `reprise-mcp` | Local MCP server, stdio | `reprise-core`, official Rust SDK (`rmcp`, pinned), `serde`/`serde_json`, `tokio` (only here, forced by the SDK) | MIT |
| `crates/reprise-stems` | — (lib) | `StemSeparationBackend` implementation (ML inference; runtime per the spike: candle **or** ort; libtorch and Python subprocess rejected) | `reprise-core` + ML runtime | MIT (runtime/model licenses checked in the gate) |
| `crates/reprise-runtime-protocol` | — (lib) | The contract between the runtime and clients: snapshots, commands, bus name/object path, protocol version (section 9) | `zvariant`, `serde` | MIT |
| `crates/reprise-runtime` | — (lib) | The runtime itself: player, queue, device runs, jobs, DB writer; toolkit-neutral, platform work behind ports | `reprise-core`, `reprise-runtime-protocol`, `rusqlite` | MIT |
| `crates/reprise-runtime-client` | — (lib) | The client that **every** surface uses: handshake, reconnect, commands, deltas | `reprise-runtime-protocol`, `zbus` | MIT |

Rules (to be anchored in `scripts/check-architecture.sh`, package I):

- `reprise-cli`/`reprise-mcp` reference **only** `reprise-core` from the
  workspace. Decided, precisely delimited exceptions in the CLI
  (decision 3): `zbus` directly behind the Linux-only feature `mpris`
  (still without platform-linux) and `reprise-stems` exclusively
  behind the feature `worker`. The **default build of the CLI stays
  core-only** — enforced by a `cargo tree` probe.
- `reprise-stems` references only `reprise-core`; nobody except the
  binary hosts (app; CLI only behind `worker`) references
  `reprise-stems`. The feature thereby stays removable without touching the
  core seam.
- `reprise-runtime` references from the workspace only `reprise-core` and
  `reprise-runtime-protocol` and **no** toolkit family (gtk4,
  libadwaita, glib, gstreamer, zbus). The runtime is the reason why a
  second frontend — or none at all — can drive the application; a
  toolkit edge here silently couples it back to the GNOME process.
- `reprise-runtime-client` references **only**
  `reprise-runtime-protocol` from the workspace. Deliberately not
  `reprise-platform-linux`: the MCP server is a client and must not pull
  GStreamer into its dependency tree through it. Bus name and object path
  therefore live in the protocol, where they belong as part of the contract
  anyway, and not in the service.
- No SQL outside the core (existing gate extended);
  the MCP leak matrix from spec D19 applies verbatim (never paths, XDG, lyrics,
  serial numbers, credentials, raw listen events).
- `default-members` stays `reprise-gnome`; `cargo test --workspace` covers
  new crates automatically.

MCP determinations (adopted from spec D16/D18, not newly invented):
stdio-only, stderr logging, stdout protocol-pure, SDK pinned +
JSON-RPC fixtures as drift protection. Capabilities (decision 7):
`library:read`, `playlist:create`, `ai:create` — **fail-closed off** as
settings keys (`agent.capability.*`), read afresh per write call
(revocation takes effect immediately, new grants after a server restart —
spec semantics). Management: for the time being exclusively the
settings keys; a dedicated preferences subpage "Agent Access" is a
**named follow-up task after package F outside this plan**.

CLI determinations (decisions 3, 4): name `reprise-cli`; `clap` v4 derive;
everything additionally as `--json` (stable shapes); typed exit codes;
`--db <path>` for tests; destructive commands require `--yes`
(`playlist delete`); `SchemaTooNew` ⇒ "Database schema is newer than this
reprise-cli — please update." (English per the AGENTS.md rule)

### 2.6 Portability path (KDE/Qt, Windows, Android, iOS) — foundation only

Anchored is exclusively:

1. **The proof** through CLI + MCP as a second/third real surface over
   the same seam.
2. **A held property instead of hope:** core dependencies are already
   mobile/Windows-capable (rusqlite bundled, notify, ureq/rustls, lofty,
   image). Decided (12): CI check `cargo check` for
   `x86_64-pc-windows-msvc` and `aarch64-linux-android` — now, in
   package I (cheapest point in time).
3. **A documented direction:** KDE/Qt and Windows link the core directly
   (cxx-qt or similar); Android/iOS later via a UniFFI crate
   (`reprise-ffi`) with a handpicked API subset; one
   `reprise-platform-<os>` per OS for the core contracts. `reprise-stems` is
   deliberately laid out platform-neutrally so that the job pipeline stays
   portable.

**Out of scope:** any actual frontend code, the UniFFI crate,
async unification, daemon/IPC protocol, mobile packaging.

## 3. Feature slices end-to-end

### 3.1 Create (and maintain) a playlist

CLI: `reprise-cli playlist create "Name" [--tracks 1,2,3] [--json]` →
`open_migrated` → `playlists::create(_with_tracks)` (playlist **and**
`change_log` row in one transaction) → exit 0 with the ID. In addition
(decision 3): `playlist rename <id> "New"` and `playlist delete <id>
--yes` (without `--yes`: error message, no effect, exit ≠ 0). If the
app is running: the notifier wakes `external_changes` → sidebar/views
refresh ≤ 1 s, silently (decision 6). If it is not running: the next start reads
fresh anyway.

MCP: tool `music_create_playlist` (name + explicit track IDs; limits as per
the spec: ≤ 500 IDs, PRESENT semantics; response without paths), capability
`playlist:create`, fail-closed off. **Decided extension of spec
D17** (decision 2), which previously restricted writes to "playlist from an
approved mix draft": direct creation now; the draft path coexists
later under the same capability. The addendum note in the
spec document is a named task in package I.
**Overwriting/deleting via an agent remains excluded** — rename/delete
exists only in the CLI (operated by a human), never in MCP.

Read surface v1 (both, strictly over existing queries): library summary,
paginated track search, playlist list/content. MCP additionally as
resources `reprise://library/summary`, `reprise://playlists` (pulled forward
from D17).

### 3.2 Instrumental versions (experimental)

- GTK: context menu action (multi-selection → batch), conversion
  playlist as staging area (aggregate bar, row states,
  save/discard per row, "Save all", row switch after
  saving, warning on "Clear playlist" with undecided entries), badge +
  source reference on the promoted track, AI hide filter in the
  filter row (opt-in, sticky), wait-with-progress-bar on a click on
  something processing, experimental switch + model download flow in the
  settings. All UX rules `[planned]` first.
- CLI: `reprise-cli instrumental create <track-id…> [--stage] [--wait]`
  (the default **saves** the result directly — automation wants the
  end result; `--stage` forces the staging decision, decision
  15), `reprise-cli instrumental save|discard <job-id…>`,
  `reprise-cli jobs status [--batch <id>] [--json]`, as well as behind the
  cargo feature `worker`: `reprise-cli jobs work` — works through the queue
  without a running app (decision 3; lease coordination with the
  app worker, 2.4/2).
- MCP: tools `music_create_instrumental` (capability `ai:create`;
  registers jobs, returns immediately with job/batch IDs; parameter
  `save`, default `true`, `save=false` stages) and
  `music_get_job_status` (read-only). The running app shows new jobs,
  progress and finally the new track **live** (change_log →
  external_changes) — the showcase of the model. Responses follow the
  D19 leak matrix (no paths).
- If neither the app nor the CLI worker is running, jobs stay `queued`; the
  MCP/CLI response says so honestly and names both processing paths.

### 3.3 Control/read scope beyond that (decision 3)

- **Playback transport:** `reprise-cli playback play-pause|next|previous|
  status` as a thin MPRIS client behind the Linux-only feature `mpris`
  (zbus directly in the CLI, without platform-linux — decided gate exception).
  No new protocol; only works with a running app (clear
  message otherwise).
- **Scan trigger:** `reprise-cli scan` calls `scanner::scan_folder` (core)
  and is a good live propagation showcase; if the app is running, its
  watcher scans anyway (hint output, no double damage — WAL + retry).
- Not in v1 (both surfaces): tag writes from outside, track delete/
  trash, queue mutation, arbitrary settings writes. Playlist delete/
  rename is the only destructive CLI surface (with `--yes`); in MCP
  there are no delete/overwrite tools whatsoever (decision 2).

## 4. Migration plan — two tracks, parallel work packages

Hard rules: file ownership is exclusive — **no package running at the same
time touches another's files**. The root `Cargo.toml`, `db.rs` and
`core/src/lib.rs` belong to one agent in P0; afterwards track 2-D is the
sole `db.rs` owner (track 1 packages never need it).
`docs/ux-rules.md` edits serialize: first C (track 1), then F (track 2).
In the H wave, `crates/reprise-cli/**` belongs exclusively to H1 and
`crates/reprise-mcp/**` exclusively to H2. Every package: TDD, all
AGENTS.md gates, one commit per task, ledger line, rebase before merge.
Schema versions are to be understood as "the next free version" (base
today: v18; P0 assigns the next, D the one after — verify at package start
against `db.rs`, parallel branches can occupy numbers).

**Track 1 (architecture + playlist/MCP/CLI) is the deliverable. Track 2
(experimental instrumentals) hangs off P0, runs in parallel and may
slip without endangering the task.**

Wave picture: P0 → {A, B, C in parallel} → I → P3a (track 1); after P0
additionally {D, E in parallel} → {F after D+C; G after E; H1 after D+A;
H2 after D+B — in parallel, disjoint ownership} → P3b (track 2).

### P0 — foundation (1 agent, sequential; critical path)

Ownership: `crates/reprise-core/src/db.rs`, `lib.rs`, new module
`events/`, instrumentation edits in `library/playlists.rs`,
`library/playlist_delete.rs`, `library/settings.rs`, `modules.rs`,
`library/scanner.rs` (event append only), root `Cargo.toml` + empty stubs
`crates/reprise-cli|reprise-mcp|reprise-stems`.

- T0.1 next free schema version (currently v19): `change_log` (+ index)
  + `SchemaTooNew` guard (decision 8, fail-closed). Tests:
  fresh/upgrade identical, guard red with a raised `user_version`,
  existing migrations green.
- T0.2 `events`: record/read_since/prune/writer_token. Tests: atomicity
  (rollback ⇒ no event), ordering, prune, writer filter.
- T0.3 facade instrumentation (append-only, no signature change).
  Tests: exactly one correct event per facade; scan = one collective event.
- T0.4 `Notifier` (notify + 250 ms debounce + 2 s fallback, named
  constants — decision 5). Tests headless: a commit over a second
  connection wakes it; degradation ⇒ `None` instead of a panic.
- T0.5 crate stubs + workspace membership.

Done: all gates, core purity unchanged empty; expected new tests ≥ 25.

### Track 1 — P1: three parallel packages (after the P0 merge)

**A — CLI v1 (cut at maximum, decision 3).** Ownership:
`crates/reprise-cli/**`. Subcommands: `playlist list|show|create|rename|
delete` (delete requires `--yes`; without it: clear message, no effect,
exit ≠ 0), `search`, `library summary`, `scan` (hint output when
the app is presumably running — its watcher scans anyway), `events tail`
(debug), global `--json`, `--db`. Tests: unit + integration via
`CARGO_BIN_EXE_reprise-cli` against a temp DB (event row per mutation,
exit codes, `SchemaTooNew` message, `--yes` refusal, busy retry
against a held foreign write transaction, scan roundtrip against a
temp folder). Done: gates; `cargo tree` proof: default features
core-only; ≥ 28 tests.

**B — MCP v1.** Ownership: `crates/reprise-mcp/**`. Pulled-forward M1 +
subset: stdio server, resources (summary, playlists), tools
`music_search_tracks`, `music_create_playlist` (capability
`playlist:create`, fail-closed), pagination/limits, stderr logging,
stdout purity. Tests: JSON-RPC fixtures over a spawned process
(handshake, list/read, discovery, D19 leak negative matrix, capability off
⇒ refused, revocation takes effect per call, busy without a hang). Done: gates;
dependency boundary proven; ≥ 25 tests.

**C — GTK live refresh.** Ownership:
`crates/reprise-gnome/src/ui/external_changes/**` (new) + exactly
`ui/window/window_runtime_wiring.rs` (still exists; verified against
`797afa2dfa`); in addition `docs/ux-rules.md` (new section only,
append-only). Context after PR #29: browser navigation is centralized
(`ui/window/window_navigation.rs`, `ui/window/library_shell.rs`,
`ui/browse/**`), the library modes are replaced by a scoped track list;
today's refresh paths are `sidebar.refresh(reason)`
(`ui/sidebar/sidebar.rs`) and the track list reload paths
(`ui/track_list/track_list_reload.rs`). **This ownership list is to be
verified at package start against whatever the state is then and then pinned
down exclusively.** First `[planned]` rules (decision 6): externally
created content appears without a restart and **silently** (no toast, no
indicator); selection/scroll are preserved; no focus theft through a
background refresh; running playback/queue untouched. Then the runtime:
notifier consumption, coalescing (pure, unit-testable), channel → main loop
(pattern `ui/scan/scan_watcher.rs`), refresh over existing paths;
RefCell discipline. Tests: coalescing/filter logic headless; exactly one
isolated display test (playlist via a second connection ⇒ sidebar shows
it), only via `scripts/check-display-tests.sh`. Done: gates +
UX traceability; ≥ 12 tests.

### Track 1 — P2/P3: completion

**I — gates, docs, license, supersessions** (after P1). Ownership:
`scripts/check-architecture.sh`, `LICENSING.md`, `README.md`/
`README.de.md` (crate table), `TESTING.md` (cross-process section),
`CONTEXT.md` (terms "instrumental version", "AI provenance"),
`docs/plans/audio-character-mcp.md` and
`docs/superpowers/specs/2026-07-19-audio-character-mcp-design.md` (only
the two notes below). Content:

- Dependency rules mechanically (incl. red negative probe; CLI default
  core-only, `mpris`/`worker` exceptions exactly).
- Cross-target check (decision 12): `cargo check` for
  `x86_64-pc-windows-msvc` and `aarch64-linux-android` in CI.
- License lines CLI/MCP/stems = MIT (decision 9) + model gate paragraph.
- **Named task (decisions 2, 10):** in
  `docs/plans/audio-character-mcp.md` mark exclusively the M1 paragraph as
  "superseded by multi-frontend-core" (M2–M5/1B untouched);
  in the spec document note the documented addendum to D17
  (direct `music_create_playlist` now; the draft path coexists later
  under the same capability `playlist:create`).

**P3a — architecture acceptance** (serial): two-process smokes on the host
(headless recipe from AGENTS.md verbatim: CLI creates a playlist, the Xvfb app
shows it live), full merge readiness battery against `origin/dev`,
isolated display tests, adversarial review; the README roadmap line only
now. Document sandbox-refused sockets exactly as `deferred host check`.

Follow-up task (outside this plan, after package F): preferences subpage
"Agent Access" for capability management (decision 7); until then
the settings keys take effect.

### Track 2 — experimental instrumentals (after P0; may slip)

**D — feature core.** Ownership: `crates/reprise-core/src/db.rs`
(next free schema version after P0, currently v20: `ai_jobs`,
`track_provenance`, `playlists.role`), new modules `ai_jobs.rs`,
`provenance.rs`, `stem_separation.rs` (contract + fake),
extension of `library/playlists.rs` by the role playlist as well as a
query clause for the AI hide filter (`queries/clauses.rs`) (after
the P1 merge — D is the sole core owner in track 2). Content:
job state machine (lease/heartbeat/reclaim with an injected clock,
cancel, batch aggregates, dedup UNIQUE with skip-and-reference semantics,
decision 16), staging store (deterministic paths under the data dir,
discard, restart preservation — decision 15), promotion facade "staging ⇒
move + final tags + track + provenance + events atomically, no re-render"
incl. **path guard** (writes only below the configured
instrumental folder, decision 13), provenance registry (source
optional; original delete ⇒ the reference becomes provenance text),
conversion playlist semantics, tag schema writer/reader (lofty;
textual source reference + optional MBID, never app IDs — decisions
13/14) + rescan reconstruction best-effort,
provenance flag filter clause. Done: gates; ≥ 40 tests.

**E — ML spike (timeboxed, decision-obliged).** Ownership:
`crates/reprise-stems/**` (spike code) +
`docs/research/stem-separation-runtime.md`. Measures on the target machine:
real-time factor, peak RSS, model size, cold start for (a) candle +
Demucs class, (b) ort/ONNX + MDX class; checks weights licenses against
LICENSING.md and Flatpak offline build feasibility. libtorch and the
Python subprocess are rejected (decision 11) and are not
measured. The done criterion is the **report with a recommendation**, not
production code. Gates G; answers the last open runtime question
on the facts. (Possible in parallel with D — disjoint files.)

**F — GTK instrumental UX** (after D and track 1-C; the only
gnome-touching package of its wave). Ownership:
`crates/reprise-gnome/src/ui/instrumental/**` (new: worker host,
staging/conversion view with save/discard, badges,
wait state) + exactly named wiring files, state `797afa2dfa`
(2026-07-21) verified:

- `ui/window/window_runtime_wiring.rs` (worker/runtime start),
- context menu: `ui/track_list/track_menu.rs` (+
  `ui/strings_track_menu.rs`),
- AI filter: `ui/browse/browse_bar.rs`, `ui/browse/browse_filter_count.rs`,
  `ui/browse/filter_restriction.rs` (FIL-1a/FIL-2 mechanics), possibly
  `ui/track_list/track_list_filter_actions.rs`,
- experimental switch + model download: `ui/preferences/**`
  (module pattern `preference_plugins.rs`, registration
  `preferences_window.rs`),

— **this list is to be verified at package start against whatever the state
is then and then pinned down exclusively** — + `docs/ux-rules.md`
(new section incl. the section K addendum for the filter; decisions
15/17/18 become rules here: row switch after saving, warning
on "Clear playlist", hint instead of a double job, filter opt-in + sticky,
aggregate bar + row states only — no toast, no sidebar slot).
Progress exclusively from the job rows/events (the same numbers
as CLI/MCP). Tests: headless worker host with a fake backend; 3 isolated
display tests (batch progress; "finished ⇒ immediately playable, processing
⇒ wait rule"; the filter hides AI titles and the filter row counts
per FIL-2). Done: gates + UX traceability.

**G — stems backend productive** (after E). Ownership:
`crates/reprise-stems/**`. Spike recommendation implemented: inference, chunking
with overlap, cancel between chunks, progress callbacks,
deterministic output (instrumental track only, decision 19),
model download/verification (checksum, license note next to the file —
decision 11). Tests: synthetic/license-cleared short fixtures,
determinism across chunk boundaries, cancel latency, no network other than the
explicit download path. Done: gates; real-time factor report in the
release profile.

**H1 — CLI: instrumental, worker, playback** (after D + A; parallel to
H2). Ownership: `crates/reprise-cli/**` (exclusively H1 in this wave).
Content: `instrumental create <track-id…> [--stage] [--wait]` (the default
saves, decision 15), `instrumental save|discard <job-id…>`,
`jobs status [--batch <id>] [--json]`; cargo feature `worker` with
`jobs work` (the only path that pulls `reprise-stems` into the CLI;
the productive path uses G, the tests the fake from D); Linux-only feature `mpris`
with `playback play-pause|next|previous|status` (zbus directly, without
platform-linux — decided gate exception). Tests following the A pattern +
job roundtrip against a temp DB with a fake backend worker; two-worker lease
(CLI worker + simulated app worker: exactly one claimer per job, no
double render); feature matrix (`default`/`worker`/`mpris`) builds,
`cargo tree` probe: default stays core-only. Done: gates; ≥ 18 tests.

**H2 — MCP: instrumental surface** (after D + B; parallel to H1).
Ownership: `crates/reprise-mcp/**` (exclusively H2 in this wave).
Content: `music_create_instrumental` (capability `ai:create`; registers
jobs, returns immediately with job/batch IDs; `save` default `true`,
`save=false` stages) and `music_get_job_status` (read-only); an honest
answer when no worker is running (jobs stay `queued`; a hint pointing to the app
or `reprise-cli jobs work`). Tests following the B pattern (fixtures, capability
off ⇒ refused, D19 leak negative matrix) + job roundtrip against a temp DB
with a fake worker. Done: gates; ≥ 15 tests.

**P3b — feature acceptance** (serial): wire the real backend into the app
(a small isolated commit), end-to-end smoke headless (MCP creates a job,
the worker with a fake/real backend renders, the track appears live), gates,
review.

## 5. Test strategy

1. **Core headless, one process** (the reliable level): outbox atomicity,
   ordering, prune, guard, job state machine (lease/reclaim/cancel/
   batch, injected clock), provenance roundtrips, tag schema roundtrip,
   registration transaction incl. rollback, path guard of the
   instrumental folder, notifier with two connections in one process
   (wakes identically to foreign processes).
2. **Cross-process display-free:** integration tests spawn the real
   binaries (`CARGO_BIN_EXE_*`) against temp DBs — CLI/MCP roundtrips,
   stdio fixtures, busy under a held transaction, job creation
   CLI→DB→event, two-worker lease coordination (exactly one claimer).
   Generous, named timeouts.
3. **GTK deliberately minimal:** a few isolated display tests (live refresh,
   batch progress, wait rule, filter count), only via
   `scripts/check-display-tests.sh` (one test = one process). The
   default workspace run stays free of new display dependencies;
   nothing is built on the known MainContext flakiness.
4. **ML without real data:** only generated/license-cleared short fixtures;
   never `~/.local/share/reprise/reprise.db` or `~/Music`;
   temp XDG in every command (AGENTS.md recipe). Performance numbers are
   same-host evidence in the release profile, not CI thresholds
   (TESTING.md convention).

## 6. Risks and non-goals

Risks:

- **Busy window on a long scan** (one large transaction): external writes
  wait > 5 s. Mitigation: facade retry with jitter for CLI/MCP, clear
  error texts; scan transaction split deliberately NOT here
  (regression risk).
- **Version drift of separate binaries:** fail-closed via `SchemaTooNew`
  (decision 8), the message names the direction.
- **Two worker hosts** (app + `reprise-cli jobs work`, decision 3):
  a double claim is prevented by lease/`claimed_by`/heartbeat —
  exactly one claimer per job, reclaim only after lease expiry; explicitly
  tested in H1 (two-worker test).
- **Real-time factor of the separation** unproven until the spike; below ~1×
  real time the wait rule means minutes per track — accepted, but
  measured instead of asserted.
- **Model license/download:** weights can break the LICENSING gate
  (⇒ feature blocked); the download needs a checksum + offline error path;
  Flatpak: the model never in the bundle, the runtime must be buildable
  offline — a spike check point.
- **Writing into the library root** (instrumental folder): the only place where
  the app creates audio files. Safeguards (decision 13): only
  after an explicit user action, only below the dedicated
  subfolder (path guard + test), atomic move, idempotent rescan.
  The watcher loop (the app's own file triggers a scan) is harmless, but is
  tested (the scan sees a finished, already registered file ⇒ no-op).
- **notify edge cases** (network FS, watch limits): degradation to polling
  as with the library watcher; WAL on a network FS remains unsupported.
- **MCP SDK churn** (tier 2; revision `2026-07-28` most recently RC): pinning +
  fixtures; a revision update as a deliberate single commit.
- **Flatpak path divergence:** the sandboxed app uses `~/.var/app/…`, the
  host CLI `~/.local/share/…`. v1 documents that (+ `--db`);
  discovery order is release work.
- **Event/refresh storm** on mass writes: collective events (scan = 1),
  debounce, coalescing, progress throttle (≤ 2 writes/s);
  an acceptance criterion in C/F measures "one refresh per batch".
- **Staging storage cost (decision 15):** undecided renders
  are preserved — across restarts too — and cost space in the data dir
  (~20–60 MB FLAC per track). The cost is visible in the
  conversion playlist; tidying up is the
  save/discard decision, not a silent reaper.
- **Duplicate titles in album views** through instrumental versions in the
  same album: deliberately accepted per decision 14 — the album tag stays
  unchanged, badge + title suffix disambiguate.

Non-goals:

- No core daemon, no IPC protocol of our own, no HTTP/remote/
  OAuth MCP.
- No playback transport/queue/tag/delete tools in MCP; playback
  (via MPRIS) and playlist rename/delete are decided surfaces
  **in the CLI only** (decisions 2, 3).
- No "Agent Access" preferences page in this plan (a named
  follow-up task after package F; until then the settings keys take effect —
  decision 7).
- No new frontend code for KDE/Windows/Android/iOS, no UniFFI crate,
  no runtime extraction from `reprise-gnome` (only a documented
  direction).
- **No genre remixes** (dropped 2026-07-21, quality); **no
  DSP center-cancel "quick karaoke"** (below the quality threshold);
  **no global instrumental switch and no rolling
  render window with eviction** (the model was rejected in favor of explicit
  versions; the staging from 2.4 is decision-bound, no
  eviction machinery); **no progressive playback of half-finished
  renders** (section 8); **instrumental output only, no
  acapella/4-stem storage** (decision 19).
- **No AI music generation** — section 8, nothing built for it.
- The mix planner (stage 1B) and sound profile MCP tools (M2–M5) are here
  neither implemented nor blocked.

## 7. Decisions (grilled 2026-07-21)

All 19 open questions of the draft are decided. The numbering
corresponds to the question numbers of the draft; the decisions are worked into
the running text above — this list is the compact reference.

1. **Process model:** (i) embedded core + WAL + events. No daemon.
   MPRIS remains the playback IPC.
2. **MCP write:** direct `music_create_playlist` now; the draft path
   of the spec coexists later under the same capability
   `playlist:create`; noted in the audio-character spec document as a documented
   addendum to D17 (a named task in package I).
   Overwriting/deleting via an agent remains excluded.
3. **CLI scope v1 at maximum** (a deliberate user deviation from the
   plan recommendation): base (playlist list/show/create, search, library
   summary, events tail, `--json`, `--db`) **plus** `scan` **plus**
   playback via MPRIS (Linux-only feature `mpris`, zbus directly in the CLI,
   still without platform-linux — the intended gate exception applies)
   **plus** `playlist delete/rename` (delete requires `--yes`) **plus**
   the standalone worker `reprise-cli jobs work` behind the cargo feature
   `worker` — the only path that pulls `reprise-stems` into the CLI; the
   base CLI stays core-only. Packages A and H are cut correspondingly
   larger (A: scan + delete/rename; H: worker + playback — split here into
   H1/H2, section 4).
4. **CLI name:** `reprise-cli`.
5. **Wake-up:** notify on DB/WAL + 250 ms debounce + data_version check;
   degradation to 2 s polling. Numbers as named constants.
6. **External changes:** update silently; selection/scroll
   preserved, no focus theft — as `[planned]` UX rules in
   package C.
7. **Capabilities:** `library:read`, `playlist:create`, `ai:create`;
   fail-closed off; revocation takes effect per call immediately, new grants after
   a server restart (spec semantics adopted). Management: a dedicated
   preferences subpage "Agent Access" as a named follow-up task after
   package F — not part of this plan; until then the
   settings keys take effect.
8. **Schema guard:** fail-closed `SchemaTooNew` (P0).
9. **Licenses:** `reprise-cli`, `reprise-mcp`, `reprise-stems` all MIT.
10. **audio-character plan:** pull M1 forward; there mark **only** the M1
    paragraph as "superseded by multi-frontend-core" (a named task in
    package I); M2–M5/1B remain untouched.
11. **ML:** the spike (package E) decides candle vs ort on the facts;
    libtorch and the Python subprocess rejected. Weights:
    first-use download with checksum + license note + license gate; bundling
    rejected; a Flatpak add-on at most later.
12. **Cross-target check:** now in package I (`cargo check`
    `x86_64-pc-windows-msvc` + `aarch64-linux-android` in CI).
13. **Folder:** `<library_root>/Reprise Instrumentals/<Artist>/<Title>
    (Instrumental).flac`; configurable; path guard + test;
    rescan reconstruction from the embedded tags; source reference textual
    + optionally a MusicBrainz ID (no app-internal IDs in tags).
14. **Tags:** title suffix "(Instrumental)", album tag unchanged
    (the album shows both versions; badge + suffix disambiguate);
    source link DB primarily + tag reference.
15. **Staging:** renders are preserved until the user decision,
    across restarts too (disk cost visible in the
    conversion playlist, no silent reaper); the playlist row
    switches after saving to the promoted title and stays until
    tidying up; "Clear playlist" warns on undecided entries; dragging
    an already converted one gives a hint instead of a double job;
    MCP/CLI default `save=true`, `--stage`/`save=false` available.
16. **Duplicates/deletion:** skip + reference to the existing one
    (UNIQUE safeguard; a later `--force` conceivable, not v1); original
    deleted ⇒ the version stays standalone, the source reference becomes pure
    provenance text; instrumental deleted ⇒ a normal delete, recreatable
    at any time.
17. **Filter:** AI titles visible, filter opt-in; the filter state sticky
    across sessions like other view states; implementation per ux-rules
    section K (FIL-1a visibility, FIL-2 counting); no
    shuffle/auto-queue special rule in v1 (the refill follows the
    visible view); a long-form exclusion rule only comes about
    if generation becomes real — then as a `[planned]` rule.
18. **Progress:** only an aggregate bar + row states in the
    conversion playlist; no sidebar/status-bar slot
    (do not touch the android-sync-V2 bottom slot), no toast.
19. **Stems:** instrumental output only.

## 8. Later / idea parking lot — nothing of it is built, nothing foreclosed

- **AI music generation** (long form, e.g. two-hour
  meditation music) as a later job kind of the same pipeline
  (`ai_jobs.kind`, optional source track, prompt as provenance);
  realistically backed by an external service.
- **Remote sources + discovery** (user vision 2026-07-21):
  YouTube audio as a playback source for titles that are not available
  locally; extending similar artists (available locally since PR #23) to
  non-local suggestions; making new releases (the
  `new_releases` table exists) directly playable. A mandatory
  legal weighing in a **separate plan to be grilled on its own**:
  the official YouTube embed player (permitted; visible player, ads,
  no pure background audio) versus stream extraction (violates
  YouTube's ToS; precedent Spotube; for Flathub a deliberate
  distribution risk). The Spotify API does not permit
  third-party playback. The seams of this plan do not stand in the way
  (optional source in `provenance`, ID-based MCP responses,
  entity-generic events) — nothing more is done for it.
- **Progressive early start** of half-finished renders (start playing as soon
  as the render is safely ahead of the playhead).
- **D-Bus ping as a latency optimizer** in the platform layer, in addition to the
  notify wake-up call; likewise a later `org.reprise.Reprise1` service as an
  app-hosted extension point.
- **Multi-root scan support** (only needed once the instrumental folder
  is to leave the library root).
- **`--force` re-render** of existing versions (decision 16: not v1).
- **Flatpak "model add-on" package** for the weights (decision 11:
  at most later).
- **Long-form exclusion rule** for shuffle/auto-queue, should generated
  long-form titles become real (decision 17; then as a `[planned]` rule).

## 9. Runtime service — binding design

Addendum of 2026-07-28, from the thin-core headless-MCP execution plan, stage 1
task 1.1. That plan was held in-session and never committed, per the convention
in `AGENTS.md` that per-stage implementation plans stay out of the repository —
so there is no file to follow the reference to; `git log` and the ledger are
the record of what it produced. This section is from here on the truth about
ownership of runtime state; it supplements decision 1 (2.1) and does not
replace it.

### 9.1 Relationship to decision 1 — what stays, what is added

Decision 1 rejected a daemon **for data**, and that stays right:
library, playlists, settings, modules, podcast/radio subscriptions, concerts
and releases live in SQLite, every surface links `reprise-core` directly,
WAL carries n readers plus one writer, and `change_log` + notifier (2.2)
make foreign writes visible live. None of that gets IPC. The hot
window query path stays a function call.

What decision 1 does not cover is the state that is **not in the
database at all**: the GStreamer pipeline, the in-memory queue, running
device runs, running jobs. Decision 1 names that openly as a consequence
("no external access to the in-memory queue/position other than MPRIS") and
section 8 parks `org.reprise.Reprise1` as an extension point. The
forcing function of the thin-core plan — MCP must control playback, queue,
background tasks and device sync **without a running GTK window** —
fetches exactly this point off the parking lot.

The boundary is thereby sharp and checkable in both directions:

| State | Owner | Access of the surfaces |
| --- | --- | --- |
| Library, playlists, settings, modules, subscriptions, concerts/releases | SQLite | directly embedded via `reprise-core` |
| Player pipeline, position, volume | Runtime | command + snapshot via the runtime client |
| Queue (order, current index, refill) | Runtime | command + snapshot |
| Device runs (inspect → … → verify, generations, cancel) | Runtime | command + snapshot |
| Background jobs (scan, downloads, instrumental) | Runtime | command + snapshot |
| Write access to the DB during a runtime effect | Runtime (serialized) | effects write only via the runtime |

A frontend may continue to link pure queries and presentation values directly.
Runtime-bound effects go through the client without exception. This
split in two is not a transitional solution but the target picture: it keeps the
read path fast and portable and gives the effects a single owner.

### 9.2 Lifecycle

The runtime has four states. The transitions are complete; there is no
implicit fifth.

```text
        activate()                 handshake ok
Absent ------------> Starting ---------------------> Serving
   ^                    |                              |
   |                    | lease taken / wrong version  | last client gone
   |                    v                              | AND nothing active
   +---------------- Refused <---------------------+   v
   |                                                Draining
   +-------------------------------------------------- +
                        idle grace expired
```

- **Absent** — no process. Every command from a client triggers activation.
- **Starting** — the process is running, the lease is being claimed, the
  handshake still open. Clients wait with a timeout, they do not poll.
- **Serving** — lease held, commands are accepted, snapshots
  published.
- **Draining** — no client connected any more and no work active; the
  idle period is running. A new client or a new effect aborts draining
  immediately and leads back to Serving.
- **Refused** — the start was aborted because the lease is already held or
  the protocol major version does not match. The process terminates with a
  structured cause; it does not wait and does not restart anything.

### 9.3 Single-owner lease

Exactly one process owns the runtime. The lease is an exclusive
operating system lock on a file under `XDG_RUNTIME_DIR`, not a
database field and not a name service detail:

- The lock is claimed **before** GStreamer, devices and the
  write pool are opened. Whoever does not get it has never touched an effect.
- The kernel releases it when the process ends, including on `SIGKILL`. There is
  therefore no orphaned lock and no reaper.
- The file carries the PID and protocol version as its content, exclusively for
  diagnostic purposes. The authority is the lock, never the content.
- A second runtime process is a bug, not a special case: `Refused` is the
  only permissible outcome.

### 9.4 Auto-activation

On Linux the runtime is a D-Bus-activatable user service
(`org.reprise.Reprise1`) with a systemd user unit. A client sends its
first command and the bus starts the service. Three rules follow from that:

1. No client starts the service itself via `spawn`. There is exactly one
   start path, otherwise the lease argument is worthless.
2. The binary, the `.service` file and the unit are shipped **together**;
   an installation/release test proves that, otherwise activation is green on
   the development machine and dead at the user's.
3. Activation is platform-specific and lives in
   `reprise-platform-linux`. The client only knows "connect" and "error",
   never systemd.

### 9.5 Client reconnection

Clients are stateless with respect to the runtime; the runtime is the
truth. On connecting, the same sequence applies as on the first start:
handshake with the protocol version, then **one complete snapshot**, then
deltas. There is no replaying of missed events — the same reasoning as
with `change_log` (2.2): consumers refresh state, they do not replay
operations.

- A broken connection is expected behavior, not an error case: the
  client reconnects with a bounded backoff and replaces, on the snapshot,
  its entire runtime-bound state.
- As long as there is no connection, a surface shows no guessed
  state. Transport and device actions are visibly unavailable
  (RUN-2), no dummy.
- A command that was dispatched during a disconnection is **not**
  buffered and executed later. It fails in a structured way; the surface
  decides whether to offer it again after the snapshot. Delayed
  execution of old intentions is the more dangerous variant.

### 9.6 Idle shutdown

The runtime terminates only when **all** four conditions hold: no
client connected, no playback (not even paused with a loaded
track), no device run, no job. If one of them does not apply, the
idle period is not even started.

That is deliberately conservative: a service that aborts work in order to save
memory is a data-loss feature. The period is a named constant,
not a scattered number, and the transition to `Draining` is abortable (9.2).

### 9.7 Error semantics

Every client interaction ends in exactly one of four categories. They are
the same for surfaces **and** for agents:

| Category | Meaning | What the client does |
| --- | --- | --- |
| `Unavailable` | Runtime not reachable or not started | backoff, reconnect, visibly disable the action |
| `Refused` | Lease occupied or protocol major version foreign | do not retry; name the cause |
| `Rejected` | Command formally valid, not admissible on the merits (missing capability, unknown entity, forbidden transition) | do not retry; show the reason |
| `Failed` | The effect ran and failed (device gone, file not readable, codec missing) | show the result, retrying is a user decision |

Binding properties for all four:

- They are **structured**, never free text. MCP always delivers a
  tool response, never a transport abort; `stdout` stays pure MCP.
- They are **path-free**: no local file path leaves the runtime
  in the direction of an agent. Entities are named via IDs.
- They carry the writer token idea from 2.2 forward: every mutation is assigned
  to its trigger in the event log.

### 9.8 Capability matrix

Every resource is read or mutated; every mutation hangs off exactly one
capability. The "today" column records the state so that stage 3.5 closes
measurably instead of by feel.

Instrumental jobs left this table on 18.08.2026: `music_create_instrumental`,
`music_get_job_status` and the `ai:create` settings key were removed from
`reprise-mcp` on the owner's decision. Queueing and tending renders remains a
feature of the app and of `reprise-cli`; what went is the agent's reach into
it. The narrative sections above are left as the record of what was planned.

| Resource | Read | Mutation | Capability | today |
| --- | --- | --- | --- | --- |
| Library (tracks, artists, albums) | yes | — | `library:read` | present |
| Playlists | yes | create | `playlist:create` | present |
| Playlists | yes | rename, delete, change content | `playlist:manage` | present |
| Instrumental jobs | — | — | — | withdrawn |
| Podcasts, radio | yes | subscribe, remove, download | `sources:manage` | present |
| Concerts, releases | yes | refresh, filter, hide | `sources:manage` | open (3.5) |
| Device sync | yes | configure, start, cancel | `device:sync` | present |
| Playback | yes | play, pause, seek, volume | `playback:control` | present |
| Queue | yes | enqueue, reorder, clear | `playback:control` | partially |
| Settings, modules | open (3.5) | set | `settings:manage` | open (3.5) |
| Scan, maintenance | open (3.5) | start | `library:maintain` | open (3.5) |

Two rules above the table, both binding:

1. **No feature switch may make a central tool disappear.**
   An agent that does not see a tool cannot know that it exists.
   Today that is still violated: playback and device tools hang off the
   `mpris` feature of `reprise-mcp`, which is not in the default build —
   `scripts/check-architecture.sh` currently even requires that the
   default build depend on `reprise-core` alone. Task 3.4 resolves both
   together; until then this rule is a goal, not a current state.
2. **File-system-changing mutations are off by default**, need
   their own persisted capability and never return local paths.

### 9.9 Visible behavior

Everything a user can notice of this design stands as a
`[planned]` rule in section AG of the UX rulebook (RUN-1 to RUN-5) and
switches there individually to `[active]` as soon as the respective slice is
implemented. The question of whether closing the window ends playback
is marked there as a rule proposal and deliberately not yet
decided — it is a product decision, not an architectural consequence.
