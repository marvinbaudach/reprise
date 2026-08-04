---
slug: audio-character-mcp
worktree: /tmp/reprise-audio-character-mcp
branch: feat/audio-character-mcp
base: 35045a33
phase: reverted
created: 2026-07-19
---

# Sound profile and agent-capable playlist planning — implementation plan

> **Void since 2026-07-23.** Stages 1A and 1B of this plan were fully
> implemented and merged on 2026-07-19/20, and then removed wholesale by
> `eda0edaebb` ("remove Song Analysis, Create Similar Mix, and Related Artist
> Discovery", 76 files). Migration v27
> (`crates/reprise-core/src/db_drop_audio_analysis_mix.rs`) drops
> `track_audio_analysis`, `mix_drafts` and `mix_draft_tracks`. No production
> code from this plan is left in `origin/dev` — only this document. Stage 2
> (MCP) never happened; `crates/reprise-mcp` grew out of
> `docs/plans/multi-frontend-core.md` instead.
>
> The status `reverted` is new for this: neither `shipped` (nothing is left)
> nor `planned` (it went much further) would have been honest.

This plan implements
[`docs/superpowers/specs/2026-07-19-audio-character-mcp-design.md`](../superpowers/specs/2026-07-19-audio-character-mcp-design.md).
The specification is binding. In case of conflict, `docs/ux-rules.md` wins
next, then the specification, then this plan.

The stages are deliberately separated:

- **Stage 1A** is the next executable section and ends with a
  joint review.
- **Stage 1B** (native mix planner) does not begin automatically. After
  stage 1A it requires an explicit user instruction.
- **Stage 2** (MCP) does not begin automatically. After stage 1B it requires
  an explicit user instruction.
- **Stage 3** (semantic atmosphere/embeddings) is not a planned
  implementation section, but a later research goal.

No task accesses real music, a real Reprise database, a live desktop,
accounts or network services. Audio integration uses exclusively
versioned redistributable fixtures.

## Execution rules

For every task, strictly:

1. Read the plan, the specification, the affected UX rules and the current
   git state.
2. Write a failing test and observe the expected red run.
3. Write the smallest correct implementation.
4. Run the focused tests green.
5. Run all mandatory gates from `AGENTS.md`.
6. Prove core purity after every core change.
7. Keep edited/created code files under 800 lines.
8. Check the diff adversarially against the specification and the task; fix
   findings.
9. Commit with exactly the given message.
10. Update `.superpowers/sdd/progress.md` with commit and base.
11. Move on to the next task of the same stage without a review pause.

The schema version number is determined at the beginning of task 2 from the
`main` current at that time. The plan deliberately does not name "v18",
because parallel branches can take the number.

## Stage 1A — Sound profile foundation

### Task 1 — UX contract and test/license gates

**Commit:** `docs(ux-rules): define local audio character analysis`

**Goal:** The new visible and compute-intensive feature is described
normatively before production code exists.

**Changes:**

- `docs/ux-rules.md`: a new append-only section with `[planned]` rules for
  local/opt-in analysis, four dimensions, coverage/uncertainty,
  background control and the Now Playing sound profile tab.
- Derive rule IDs from the document current at that time, do not invent them
  in advance.
- `scripts/check-ux-traceability.sh`: no logic change, unless a new
  prefix were, contrary to expectation, not detected automatically; for that,
  a red negative probe first.
- `LICENSING.md`: record that audio analysis in the MIT engine path must not
  pull in AGPL/non-commercial models.
- `TESTING.md`: document fixture provenance and the audio analysis benchmark
  as future gates.

**Red evidence:** a negative fixture with a new rule wrongly set to
`[active]` without a rule-named test must fail at the traceability gate.

**Acceptance:** rules stay `[planned]`; no product code or UI switch is
faked.

### Task 2 — Versioned persistence and staleness

**Commit:** `feat(core): persist versioned audio character analysis`

**Goal:** Core can store, invalidate and load audio evidence and sound
profiles without a decoder and project them as coverage/pending work.

**Failing tests:**

- Migration from the predecessor version determined live preserves tracks,
  waveforms, playlists and `listen_events`.
- A fresh DB and an upgraded DB have the same analysis schema.
- `save_analysis` round-trips evidence, profile, versions, confidences and
  fingerprint.
- the same mtime/size/version is current; every relevant deviation is
  pending.
- a path/inode change with the same mtime/size does not invalidate.
- missing/removed become neither pending nor mix-eligible.
- Track deletion cascades the analysis.
- Coverage counts the denominator and current profiles correctly and
  distinguishes library tracks from listen events.
- Failure kind and retry state round-trip; an unknown kind falls back safely
  to `unknown`.

**Implementation:**

- new focused core modules `audio_analysis` and `sound_profile`;
- validate data values as finite numbers; never persist NaN/Inf;
- enforce `0.0..=1.0` via the constructor;
- pending/coverage queries get suitable partial/join indexes;

**Adversarial checks:** manipulated DB values, negative mtime/size,
unknown version, empty library, exclusively missing tracks.

### Task 3 — Streaming accumulator and reproducible fixtures

**Commit:** `feat(core): extract deterministic audio character evidence`

**Goal:** Pure core mathematics processes bounded PCM chunks and produces
raw waveform values, evidence and profiles without platform or file access.

**Failing tests:**

- Chunk boundaries do not change the result within defined tolerances.
- Silence, a constant signal, a low/high sine, a click track, a crescendo and
  broadband noise produce the expected orderings.
- 60/90/120/180 BPM click tracks land within the tolerance window; half/double
  tempo is handled stably via confidence or the canonical range respectively.
- Zero chunks, a final partial chunk, an extremely short file and a very long
  synthetic stream stay free of panics/NaN.
- Profiles always lie in `0..=1` and a pure projection version change
  needs no PCM.
- The waveform output stays exactly 1,000 `u8` peaks and compatible with the
  existing player bar.

**Implementation:**

- `AudioEvidenceAccumulator` accepts mono PCM plus sample rate block by block;
- a fixed FFT/hop size and bounded ring buffers;
- robust percentile/histogram aggregation instead of complete sample lists;
- projection with named constants and documented calibration;
- fixture generator as code, real binary fixtures only with a license note.

**Gate:** a deterministic memory/chunk test proves that memory does not grow
with track duration.

### Task 4 — Native GStreamer analysis adapter

**Commit:** `feat(platform): stream audio into the character analyzer`

**Goal:** Linux decodes supported formats block by block, without a
`gst-launch-1.0` subprocess or a complete stdout buffer.

**Failing tests:**

- FLAC/WAV fixtures deliver evidence and 1,000 peaks.
- a missing, an empty and an undecodable file yield typed errors.
- Cancellation ends the pipeline in bounded time and delivers no partial
  ready result.
- the production adapter processes several chunks; a one-chunk fake cannot
  pass the test by accident.
- existing waveform tests stay green and the player bar contract unchanged.

**Implementation:**

- a core `AudioAnalysisBackend` with a single `analyze(path, cancellation)`
  call;
- a Linux `GstreamerAudioAnalysisBackend` via AppSink/callbacks and bounded
  PCM blocks;
- an internal shared decoder path for analysis and on-demand waveform, without
  coupling the two public capabilities;
- no GTK/GLib MainContext access in the worker.

**Benchmark:** decode/analysis time and peak RSS for short and long fixtures;
a release report, not an uncalibrated marketing claim.

### Task 5 — Durable, cancellable single-worker scheduler

**Commit:** `feat(gnome): run controllable local audio analysis`

**Goal:** Enabled analysis works after a scan and at startup, resumable,
bounded and independent of GTK borrows.

**Failing tests:**

- when disabled, no **sound profile** work starts; the existing unconditional
  waveform backfill stays functional;
- when enabled it takes exclusively current pending tracks;
- exactly one track is analyzed at a time;
- pause stops before the next track, resume continues, cancel ends it;
- a fingerprint change during work discards the result;
- an error state prevents a startup retry loop;
- "Retry failed" resets only failed rows;
- a second start does not create a second worker;
- scan completion signals new work, but analyzes outside the
  scan transaction;
- if waveform and profile are missing while analysis is enabled, exactly one
  coordinated decode arises; disabled analysis still produces waveforms;
- shutdown joins or cancels without a UI hang.

**Implementation:**

- scheduler state in a focused runtime module, not in `scanner.rs`;
- a dedicated DB connection per worker;
- generation/cancellation token;
- progress coalescing to the GTK main loop;
- the existing four-worker waveform backfill is migrated to the shared,
  bounded path.

### Task 6 — Settings and analysis progress

**Commit:** `feat(gnome): expose local audio analysis controls`

**Goal:** The user can understand, enable and control local analysis.

**Failing tests:**

- The settings toggle is off on a fresh install and round-trips.
- Enabling starts analysis, disabling stops new work and keeps
  profiles.
- Coverage, Running, Paused, Failed and Complete have unambiguous states.
- Retry appears only on errors; Reanalyze demands confirmation.
- UI strings name local processing and no upload.
- RefCell borrows do not cross scheduler/GTK callbacks.
- narrow width and Reduced Motion preserve operability.

**Implementation:** a dedicated Preferences subpage under Library, gettext
strings, shared sidebar activity only if the existing slot fits semantically;
otherwise no unplanned new global progress stack.

**UX flip:** the rules for opt-in, control and progress become `[active]`
together with their rule-named tests.

### Task 7 — Sound profile in the Now Playing panel

**Commit:** `feat(gnome): show audio character in now playing`

**Goal:** The user sees four profiles, BPM/confidence and analysis state
without an objective mood claim, in the existing right-hand panel of the
loaded track.

**Failing tests:**

- Ready shows four labeled scales and optional BPM.
- Pending/Disabled/Failed/Stale differ in their wording.
- A `None` tempo shows no `0 BPM` fake.
- Color is redundant with label/value/position.
- Screen reader names contain dimension and value.
- A track change uses a generation and never shows the profile of the previous
  track.
- User paths and internal versions do not appear.
- Up Next/Lyrics/Audio Character share NPP-11's adaptive switcher; the new
  tab stays unambiguously labeled in icons-only mode and reachable by keyboard.
- Without a loaded track the tab shows a neutral empty state.

**Implementation:** a reusable profile view, but for now wired up only in the
Now Playing panel; no new library details dialog and no new
context menu action.

**UX flip:** the Now Playing/sound profile rules become active in the same
commit.

### Task 7A — Stage 1A acceptance

**Commit:** No commit of its own, provided the review produces no findings.

After task 7: the full mandatory gates, audio fixture/memory/performance
report, isolated Now Playing/settings display tests and an adversarial
standards/spec review. Display socket blockers are documented exactly as
`deferred host check`. Findings get their own precise
fix commits.

**STOP:** A joint review follows. Stage 1B does not begin automatically.

## Stage 1B — Native mix planner (separate approval required)

### Task 8 — Mix contract and safety boundaries

**Commit:** `docs(ux-rules): define audio character mix planning`

**Goal:** Mix preview, determinism, coverage, draft approval and later
agent capabilities are planned before the planner gets production code.

**Changes:** new `[planned]` rules in the existing sound profile section;
`TESTING.md` adds a mix/MCP safety matrix; a negative probe proves that a
premature `[active]` flip without a rule-named test fails.

### Task 9 — Mix intent and bounded candidate query

**Commit:** `feat(core): validate sound-profile mix intents`

**Goal:** Core has a canonical, serializable mix intent and a
safe candidate projection.

**Failing tests:**

- The JSON/type round-trip is canonical and stable.
- unknown fields, NaN, values outside `0..=1`, zero/negative durations,
  oversized ID lists and contradictory conditions are rejected.
- library/playlist/artist/album/track ID sources use the existing
  grouping/PRESENT semantics.
- unanalyzed/stale/missing/removed are excluded.
- minimum confidence and exclusions take effect before scoring.
- The SQL pre-selection delivers at most 500 stable candidates and reads no
  waveform/PCM BLOBs.

**Implementation:** typed enums/validated scalars; no free
field/operator/SQL string from MCP or GTK.

### Task 10 — Deterministic, diverse mix planning

**Commit:** `feat(core): plan deterministic audio-character mixes`

**Goal:** A pure/DB core path produces an explainable,
reproducible mix draft from candidates.

**Failing tests:**

- an identical candidate/source snapshot, intent and seed yields a
  byte-identical draft;
- the weighted profile distance orders predictably;
- a stable track ID tiebreak;
- no duplicates;
- an artist spacing of four when satisfiable, with a diagnostic when not;
- familiarity/variety modes change only documented score components;
- duration stops at the smaller deviation and overshoots by at most
  one final track;
- rise/fall/arc orders the chosen membership, it does not replace it;
- hard impossibility is an error, soft underfill a partial draft;
- selection reasons name structured top contributions without free-text logic;
- empty/small/large candidate sets stay deterministic.

**Performance gate:** 100,000 profile rows plus SQL pre-selection and planning
are reported reproducibly; the greedy phase sees at most 500 candidates.

### Task 11 — Durable drafts and atomic approval

**Commit:** `feat(core): approve durable mix drafts atomically`

**Goal:** Preview and saving demonstrably use the same selection.

**Failing tests:**

- Draft head/positions/reasons round-trip in order.
- The draft stores fingerprints of the selection, hard source conditions and
  the profile version.
- stale/expired/already-approved is rejected; a new or changed
  uninvolved track has no consequences.
- Approval revalidates PRESENT, fingerprint, analysis and source membership
  only for the selected set, without rescoring.
- Approval atomically creates a manual playlist with exactly the draft IDs.
- An FK/insert error rolls playlist and approval back completely.
- the same idempotency key delivers the same playlist result.
- a different key on an approved draft creates no second playlist.
- an existing name may, under today's manual semantics, yield a second
  playlist, but never overwrites the existing one.
- a bounded cleanup query deletes only expired, unapproved drafts.

**Implementation:** `mix_planner` encapsulates the existing
`playlists::create_with_tracks`; callers must not send a track list along with
the approval.

### Task 12 — Native mix builder with a truthful preview

**Commit:** `feat(gnome): build playlists from audio character drafts`

**Goal:** GTK creates and saves mixes through the same core interfaces as
MCP later.

**Failing tests:**

- Presets set editable intent values.
- Invalid/unsatisfiable shows precise errors and creates no draft.
- The preview shows order, duration, coverage and diagnostics.
- Save is correctly enabled/disabled with no draft/a currently stale draft.
- Save sends only `draft_id`, name and idempotency key.
- the saved playlist matches the visible preview exactly.
- Changing a control invalidates the old draft.
- Navigation to the new playlist uses the normal sidebar/history path.
- narrow layout, keyboard, screen reader and Reduced Motion are covered.

**UX flip:** the draft-before-save and mix builder rules become active.

### Task 13 — Coverage-honest My Stats projection and stage 1B acceptance

**Commit:** `feat(stats): summarize listened audio character`

**Goal:** My Stats gains a small, honest sound profile evaluation and
stage 1B ends fully checked.

**Failing tests:**

- The aggregate joins exclusively current profiles onto the `listen_events`
  of the chosen period.
- repeated plays weight the listened title according to the events.
- Thresholds: at least 20 analyzed plays and 70 % coverage.
- below that no insight; above it text plus "based on N analyzed plays".
- a period/timezone change follows the existing stats contracts.
- A deep link opens the mix builder with the displayed profile direction.
- no paths/model terms/objective emotions in the UI.

**Final stage 1B gates:**

- `cargo fmt --check`
- strict workspace clippy
- workspace tests
- `cargo doc --workspace --no-deps` with denied warnings
- UX traceability, motion, architecture, QA and file size gates
- core purity
- `cargo audit` only with the existing permitted exception
- audio fixture/memory/performance reports
- isolated GTK display tests; if sandbox sockets prevent them, document
  exactly `deferred host check`
- adversarial standards/spec review and fix pass

**Commit after review fixes:** If findings require production changes,
one commit per coherent fix with a precise message; do not squeeze them into
the stage 1B closing ledger.

**STOP:** A joint review follows. Stage 2 does not begin automatically.

## Stage 2 — Local MCP adapter (separate approval required)

Before M1, the stable MCP revision, the state of the official Rust SDK, its
license and conformance support are re-checked live. The plan's assumption is
the stable revision `2025-11-25`, local stdio and a pinned official tier-2
Rust SDK. If `2026-07-28` is final by then and fully supported in the SDK, the
target revision is updated in M1 with documentation; the tool domain does not
change.

### M1 — Separate stdio crate and read-only resources

> **superseded by multi-frontend-core** (2026-07-21): `crates/reprise-mcp` is
> founded there (package B), which brings this M1 forward and extends the
> tool domain. Only this M1 paragraph is superseded; M2–M5 and stage 1B remain
> untouched.

**Commit:** `feat(mcp): expose local read-only library resources`

**Failing tests:** workspace/license boundary, JSON-RPC handshake,
`resources/list/read`, pagination, stdout protocol purity, no paths or
settings leaks, unknown URI and DB errors.

**Implementation:** `crates/reprise-mcp`, a dependency only on core + the
pinned official SDK, stdio, `stderr` logging, library summary,
sound profile vocabulary and playlist overview.

### M2 — Bounded read-only tools

**Commit:** `feat(mcp): add bounded audio character query tools`

**Failing tests:** tool discovery and structured outputs for
`music_search_tracks` and `music_get_sound_profiles`; limits of 100 IDs,
pagination, validated sort/filter values, PRESENT semantics and a complete
leak negative matrix.

### M3 — Mix planning through the shared core

**Commit:** `feat(mcp): plan explainable playlist drafts`

**Failing tests:** `music_plan_playlist`/`music_get_mix_draft` match
direct core results byte-for-byte and structurally; `music_get_mix_draft` is
annotated read-only, `music_plan_playlist` deliberately is not, because of the
durable draft row; invalid/stale/partial diagnostics; no mutation of
playlist/queue/playback; the tool schema contains no free SQL or
prompt surface.

### M4 — Capability-guarded playlist creation

**Commit:** `feat(mcp): create playlists from approved mix drafts`

**Failing tests:** the capability is off on a fresh install; without approval
the tool is either not exposed or fail-closed (in line with the stable
spec/SDK convention); activation, revocation while running, a stale draft,
idempotency, atomic creation, no overwrite/deletion and correct
non-destructive/idempotent annotations.

GTK settings names exactly which data and operations local agent access
receives. No HTTP/OAuth surface.

### M5 — MCP conformance, packaging and stage 2 acceptance

**Commit:** `test(mcp): gate local agent playlist access`

**Changes and gates:**

- versioned JSON-RPC fixtures and the official inspector/conformance path;
- `scripts/check-architecture.sh`: MCP may reference only core;
- `scripts/check-release.sh`: binary, license notices and stdio smoke;
- a security matrix for all resources/tools and capabilities;
- a 100k metadata response/pagination benchmark;
- README/README.de: change the roadmap line from planned to shipped only now;
- no agent/LLM network test and no real library.

After the fix pass, stage 2 ends for the joint review.

## Deliberately not planned

- semantic happy/sad/aggressive/relaxed models;
- lyrics sentiment;
- CLAP/audio-text embeddings;
- cloud analysis or model download;
- "similar to this" over large embedding BLOBs;
- MCP over HTTP/OAuth;
- playback, queue, tag, delete, trash, sync or history tools;
- autonomous modification of existing playlists;
- learning from user feedback.

Each of these points needs a new specification or stage approval
respectively and is not implicitly authorized by this plan.
