# Handover — move the library-wide lyrics batch into `reprise-core`

**Why this exists.** `#189` added `crates/reprise-gnome/src/ui/lyrics/lyrics_batch.rs`
without moving the frontend-thinness budgets in the same commit, so
`scripts/check-frontend-thinness.sh` was red on `dev` until `#196` raised
`threads` 14→15 and `filesystem` 17→19. Raising them was the honest move for
that timebox, and the commit recorded the real answer as a follow-up: the batch
runs a provider chain and writes `.lrc` sidecars, which is engine work, and
`reprise-core::lyrics` already owns the providers, the cache and the circuit
breakers. **This document is that follow-up.**

**Status of the ground you are standing on.** Base is `dev` at `e836d125c1`
(after `#196` and `#197`). `scripts/check-merge-readiness.sh` passes completely
there — 4173 tests, 0 failures, all display batches green. A red you see is
yours.

**One correction up front, before you plan around a false premise.** Moving this
worker **does not automatically lower the `threads` budget.** See §5. If you
promise the owner a budget reduction you will not be able to deliver it without
also breaking the convention every other worker in this repo follows. Say what
this refactor actually buys instead: the *decision logic* leaves the frontend,
the batch becomes testable without a GTK context, and the duplicated skip-policy
gets one home.

---

## Things you must not relearn (hard-won)

**The "after completed library scans" trigger is not a call site.** LYR-6 names
three triggers. Only two are direct: window construction
(`ui/window/window_runtime_wiring.rs:521`, `lyrics_batch.start_after_cover(...)`)
and `recompute_enabled()` from `ui/preferences/preferences.rs:476`. The third is
**transitive**: `ui/cover/main_cover_download_progress.rs:85` wires
`scan_controls.set_on_complete(move || batch.start())` for the *cover* batch,
and `LyricsBatch::start_after_cover` (`lyrics_batch.rs:311-324`) subscribes to
the cover batch's progress with an always-true liveness probe (`:314`, `|| true`).
That subscription lives for the process, so it fires again on every later
cover-batch completion — including the one a rescan causes. **A rewrite that
treats this as a one-shot hook silently breaks a clause of an `[active]` rule,
and no test will tell you.** Test the re-arm explicitly.

**`Db` is deliberately neither `Send` nor `Sync`.** `crates/reprise-core/src/db_handle.rs:19`
says it outright: background work opens its own handle over the same path rather
than sharing the frontend's. `lyrics_batch.rs` sidesteps this today by querying
`query_live_track_summaries(&self.conn)` on the calling thread *before* spawning
(`:259`); the worker thread never touches `Db` at all — its four closures
(`local`, `needs`, `online`, `all_breakers_open`) are pure cache/file functions.
If your core design wants the batch to re-query inside the worker, it must take
a `db_path: PathBuf` and call `Db::open_migrated(Some(path))` in the thread, the
way `ui/scan/scan_worker.rs:240` and `ui/concerts/concerts_worker.rs:289-290`
already do. Never try to carry `Rc<Db>` across.

**The cover-download batch is the twin, not the template.**
`ui/cover/cover_download_batch.rs` (246 lines) is structurally near-identical to
`lyrics_batch.rs` — same state/progress shape, same `ProgressSubscribers`, same
`glib::spawn_future_local` reconciliation. It is *not* the pattern to copy; it
has the same problem. See §6 for what that means for scope.

**Exactly one line in `lyrics_batch.rs` is load-bearing GTK.** `glib::spawn_future_local`
at `:292` — the step that moves worker results onto the thread widgets may be
touched from. Everything else that *looks* like coupling is not: `async_channel`,
`std::thread::Builder`, `Rc`, `Cell`, `Arc<AtomicBool>`/`Arc<AtomicU64>` are all
dependency-pure and several already appear inside `reprise-core` (e.g.
`concerts/pipeline.rs`). Do not let the `Rc`s scare you into thinking this is a
big surgery.

**Strings are already clean.** `lyrics_batch.rs` has zero `strings::` references.
All translated text (`LYRICS_BATCH_CHECKING`, `_COMPLETE`, `_FAILED`) sits in
`ui/strings_news.rs:198-223` and is consumed only by `lyrics_batch_progress.rs`,
which stays in the frontend. No gettext problem to solve.

**The online gate is already core.** `network_allowed()` (`lyrics_batch.rs:403-408`)
is a thin wrapper over `reprise_core::online_sources::network_allowed_or_off`
(`crates/reprise-core/src/online_sources.rs:113`) plus
`reprise_core::modules::ONLINE_LYRICS_MODULE`. Nothing to port.

**`lyrics_worker.rs` is a different thing and is out of scope.**
`ui/lyrics/lyrics_worker.rs` (203 lines) is the single-track, on-demand lookup
behind the Lyrics tab (`LyricsRuntime`, used by `player_lyrics.rs` /
`lyrics_view.rs`). It is unrelated to the library-wide batch. Leave it alone.

---

## What is actually there today

`crates/reprise-gnome/src/ui/lyrics/lyrics_batch.rs`, 417 lines. Public surface
is all `pub(in crate::ui)` — nothing crosses the crate boundary:

| item | line |
| --- | --- |
| `LyricsBatch::new(conn: &Rc<Db>) -> Rc<Self>` | `:197` |
| `recompute_enabled(self: &Rc<Self>)` — restarts on an off→on transition | `:221` |
| `cancel(&self)` | `:235` |
| `subscribe_progress(&self, is_alive, callback)` | `:244` |
| `start(self: &Rc<Self>)` | `:253` |
| `start_after_cover(self: &Rc<Self>, cover_batch: &Rc<CoverDownloadBatch>)` | `:311` |
| `LyricsBatchState`, `LyricsBatchProgress` (both `Copy`) | `:20`, `:28` |

`start()` re-checks the gate, queries live tracks on the calling thread (`:259`),
bumps a `generation` counter, ships the whole `Vec<BatchTrack>` plus closures to
a dedicated thread named `"reprise-lyrics-batch"` (`:169-175`) over
`async_channel`, and drains results back onto the main loop. The per-track loop
is `run_request()` (`:345-384`), strictly serial, checking cancellation and
generation staleness before each item (`:348`, `cancelled()` at `:410`).

Policies it implements, and where the constants really live:

- **opt-in** — `network_allowed()` `:403-408`
- **serial** — one thread, `for track in &request.tracks` `:347`
- **cancellation** — `ScanCancellation` plus an `Arc<AtomicU64>` generation counter
- **local/cache skip** — `:352-356`
- **seven-day synchronized-upgrade throttle** — constant is *in core*:
  `NEGATIVE_TTL_SECONDS = 7*24*60*60` at `crates/reprise-core/src/lyrics/cache.rs:8`,
  reached via `NeedsFetch::RetryForSynced` (`cache.rs:48-55`)
- **all-breakers-open termination** — checked twice, `:362-366` (before spending a
  call) and `:371-373` (after a failure), via `lyrics::all_network_breakers_open()`
- **250 ms per-host spacing** — already fully core-side (`lyrics/lrclib.rs:18`,
  `lyrics/netease.rs:17`), untouched by this move

What it already takes from core (`crates/reprise-core/src/lyrics/mod.rs` is the
228-line facade): `local_hit`, `needs_fetch`, `load_or_fetch_with_options`,
`all_network_breakers_open`.

---

## The design decision you must make first

**Core does not expose a single "give me the next work item, honouring the skip
policy" entry point.** `load_or_fetch_with_options` (`lyrics/mod.rs:58-142`)
internally re-derives the same local/cache decision (`best_local` `:149-168`,
`cached_result` `:170-195`) that the batch computes *itself* beforehand via
`local_hit`/`needs_fetch`. The batch duplicates it deliberately: it needs the
classification (`Skip` / `Fetch` / `RetryForSynced`) **up front**, for progress
counting, before deciding whether to pay for a network-capable call at all.
`load_or_fetch_with_options` never surfaces that classification.

So a straight port inherits the duplication. Decide explicitly, and write the
decision into the commit message:

- **(a)** keep the two-stage pre-filter as-is inside core — smallest diff,
  duplication survives but is now in one crate instead of across the boundary;
- **(b)** refactor `needs_fetch` / `load_or_fetch_with_options` so the
  classification is produced once and shared — the honest fix, larger blast
  radius, touches an `[active] [core]` rule's implementation (LYR-5).

(b) is better on the merits. (a) is defensible if you keep the follow-up written
down rather than silent — that is exactly the mistake this whole document exists
to clean up. **Do not choose by accident.**

---

## The shape to build

The precedent for "worker lives in core, driven from GTK" is the **library
scanner**, not the cover batch:

```rust
// crates/reprise-core/src/library/scanner.rs:129
pub fn scan_folder_with_progress(
    db: &Db, root: &Path, mut on_progress: impl FnMut(ScanProgress),
) -> Result<ScanOutcome, ScanError>
```

Plain, synchronous, no threads, no channels — just a callback. The frontend
(`ui/scan/scan_worker.rs:58-110`) owns the `thread::spawn`, the `async_channel`s
and the `glib::spawn_future_local` reconciliation, and re-opens its own `Db`
inside the thread (`:235-247`).

For **cancellation**, core already has the right type:
`reprise_core::concerts::CancellationToken` (`concerts/pipeline.rs:29-41`) —
`Clone + Default`, wraps `Arc<AtomicBool>`, `.cancel()` / `.is_cancelled()`.
It is structurally identical to the frontend's `ScanCancellation`
(`ui/scan/scan_controls.rs:42-56`). Two identical wrappers in two places is
itself a small smell; converging on one (likely promoting `CancellationToken`
out of `concerts::pipeline` into a shared core location) is a natural part of
this task — but it is a design decision, so make it deliberately.

`concerts::refresh_cancellable` supplies only the cancellation half of the
template — it has no per-item progress callback, only a final `RefreshSummary`.
Take cancellation from Concerts and the progress-callback shape from the
scanner.

**What stays in `reprise-gnome`:** the thread spawn, the channels, the
`glib::spawn_future_local` reconciliation, `ProgressSubscribers<P>`
(`ui/progress_subscribers.rs`, `Rc`/`RefCell`, single-threaded by construction),
and the whole of `lyrics_batch_progress.rs` — which is presentation, translating
progress into `ScanControls::show_batch_progress(title, detail, fraction)`
(`:80-84`) plus an auto-hide timer (`:90`). `ScanControls` itself is a GTK type
(`ui/scan/scan_controls.rs:59-263`, wraps `gtk4::Button`, `ScanProgressView`, a
`WeakRef<gtk4::ToggleButton>`) owned by the window. None of that moves.

---

## §5 — Budget reality, stated plainly

Current pins in `scripts/check-frontend-thinness.sh`: `rusqlite=112`,
`filesystem=19`, `threads=15`, `workers=7`. Measured contribution of the files in
question, replicating the script's own comment-stripping and `#[cfg(test)]`-skipping:

| file | `threads` | `filesystem` |
| --- | --- | --- |
| `lyrics_batch.rs` | **1** (`:169`, `thread::Builder::new()`) | 0 |
| `lyrics_batch_progress.rs` | 0 | 0 |

Consequences, and none of them is the one you would guess:

- **`filesystem` stays at 19.** Both files contribute zero. The two matches that
  pushed that budget up in `#189` were in
  `ui/device_sync/device_sync_effects.rs:335,393` (the `.lrc` sidecar probes) and
  are unrelated to this task. **Nothing here lowers `filesystem`. Do not claim it.**
- **`threads` stays at 15** if you follow the scanner/concerts convention, because
  the `thread::Builder::new()` call simply relocates to whatever gnome-side
  adapter replaces `lyrics_batch.rs`. It drops to 14 only if *core* owns the
  thread — which no other worker in this repo does. If you want the drop, you are
  proposing a new convention; argue for it explicitly or accept 15.
- **`workers` stays at 7** — `lyrics_batch.rs` does not match the `*worker*.rs`
  glob. **Watch the naming**: calling the surviving adapter `lyrics_batch_worker.rs`
  pushes the count to **8** and forces you to *raise* a budget, which is the exact
  opposite of this task's point. Name it something like `lyrics_batch_runtime.rs`.

So: **this refactor is not a budget-reduction task.** Its value is that the
decision logic becomes engine code, testable without GTK, next to the cache and
breakers it already depends on. Sell it as that.

---

## §6 — Scope: one batch or two?

`ui/cover/cover_download_batch.rs` is the same construction with the same
problem. Doing lyrics alone leaves an identical twin behind and means the second
move re-litigates every decision you make here.

Two honest options — **ask the owner, do not decide silently**:

- **Lyrics only.** Smaller, reviewable, proves the pattern. Record the cover twin
  as an explicit follow-up in the ledger *and* in the commit message, or it will
  rot exactly like this one did.
- **Both, sequentially, same branch or two stacked PRs.** More work, but the
  shared core abstraction (`CancellationToken` placement, the progress-callback
  signature) gets designed once against two real consumers instead of one. A
  shape validated by a single caller is a guess.

The second is better engineering; the first is likelier to land. That is the
same trade this document's own origin story is about — so at minimum, make the
choice visible.

---

## Tests

`ui/lyrics/lyrics_batch_tests.rs` — **11 `#[test]`, none ignored, all pure logic**
(in-memory `test_db::open()` or nothing; no display needed). These should move to
core essentially as-is:

- `progress_counts_checked_downloaded_and_unavailable_without_counting_skips` `:64`
- `cover_completion_starts_lyrics_only_after_the_subscription_is_armed` `:78`
- `every_open_network_breaker_fails_before_a_provider_call` `:87`
- `cancellation_keeps_the_first_completed_lookup_and_never_starts_the_second` `:109`
- `net_1a_switching_the_module_off_mid_run_stops_before_the_next_request` `:140`
- `net_1a_the_batch_gate_follows_the_global_online_sources_switch` `:175`
- `lyr_6_enabling_the_module_starts_the_batch_once_and_nothing_else_does` `:200`
- `a_synced_retry_only_counts_when_it_actually_improves_the_cached_result` `:232`
- `a_dead_progress_subscriber_stops_being_called_and_is_pruned` `:258`
- `local_and_cache_hits_skip_network_but_still_advance_progress` `:278`
- `lyr_7_the_batch_runs_the_sidecar_writing_lookup_for_the_whole_library` `:307`

`ui/lyrics/lyrics_batch_progress_tests.rs` — **4 `#[test]`**, of which two are
GTK display tests that **cannot** move (they instantiate `ScanControls`):

- `running_progress_is_determinate_and_names_lyrics_counts` `:16` — pure
- `terminal_progress_auto_hides_and_idle_stays_hidden` `:29` — pure
- `lyr_6_scan_controls_show_live_lyrics_batch_progress` `:48` — `#[ignore]`, needs a display
- `lyr_6_the_card_cancel_stops_the_batch_without_sharing_the_scan_flag` `:70` — `#[ignore]`, needs a display

`cover_completion_starts_lyrics_only_after_the_subscription_is_armed` (`:78`) is
the one guarding the permanent-subscription trap. Keep it, and consider adding a
sibling that asserts a *second* cover completion re-arms the run — today nothing
covers that, and it is the clause most at risk.

---

## The rule

`docs/ux-rules.md:2213-2224` — **LYR-6 `[active] [gtk]`**:

> With the Online Lyrics module enabled, a cancellable serial background run
> fills the lyrics cache for the present library after the cover batch, after
> completed library scans, and the moment the module is switched on — switching
> it on starts the run once; a further settings change while it is already on
> never restarts a run in progress, and switching it off only stops one. Tracks
> with local lyrics, complete positive cache entries, or fresh negative entries
> are skipped; a cached plain result is retried for synchronized text at most
> once per seven-day negative-TTL window. Provider requests keep at least 250 ms
> between calls to the same host. The shared ScanControls card reports checked,
> cached, and unavailable counts; if every provider breaker is open, the run
> fails immediately while already cached entries remain.

`LYR-7 [active] [core]` (`:2225`) also runs through this file — its
sidecar-writing test lives in `lyrics_batch_tests.rs:307`.

**This is a pure refactor: LYR-6's text must not change and no user-visible
behaviour may move.** The rule's substance is layer-agnostic except one clause —
"the shared ScanControls card reports…" — which is inherently presentation.

But the **level tag** does become wrong. Per `docs/ux-rules.md:23-30` the tag
records *where the rule is testable*: "Testing happens at the **lowest level
that can disprove the rule**." Once the loop, the skip policy and the breaker
termination live in core, most of LYR-6 is disprovable in the core suite, and
`[gtk]` understates it. The rulebook already has both the precedent for
tag-only changes ("If a `[manual]` rule later becomes automatable, only its tag
changes, never its ID", `:29-30`) and the precedent for dual tags
(`MTP-37 [core] [gtk]`, `:654`).

So: **change the tag to `[core] [gtk]` in the same commit that moves the code**,
and leave the ID and the text alone. Rule IDs are append-only — do not split
LYR-6 into two IDs just because its implementation now spans two crates.

Note that `scripts/check-ux-traceability.sh:79-88` only requires *some*
rule-named `#[test]` to exist anywhere under `crates/` — it does not check that
a rule's tag matches where its tests physically live. **The gate will not catch
you if you get this wrong.** That is a reason to be careful, not a reason to skip it.

---

## Definition of done

- `lyrics_batch.rs`'s decision logic lives in `reprise-core`; the surviving
  gnome-side adapter owns only thread, channels and `glib::spawn_future_local`.
- `cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` is empty.
- All 11 pure tests moved and green in the core suite; the two GTK display tests
  still present and green in `reprise-gnome`.
- LYR-6 retagged `[core] [gtk]`, text and ID unchanged, in the same commit as the move.
- The three LYR-6 triggers still fire — including the transitive rescan one. Prove
  the re-arm with a test.
- `scripts/check-frontend-thinness.sh` passes. If a budget moved, the commit
  message says which and why; if none moved, say that too, so the next reader
  does not go looking for a reduction that was never available.
- Full `scripts/check-merge-readiness.sh` green, test count accounted for
  (tests move between crates, so the workspace total should be unchanged — state
  the before/after numbers).
- One line appended to `.superpowers/sdd/progress.md`.
- Squashed PR against `dev`. Never push to `dev` or `main`.

## Context you will want and cannot find on `dev`

`docs/plans/wave-0-agent-handover.md` and `docs/plans/consolidation-plan.md` are
referenced by the `#196` commit and PR but are **not on `dev`** — they live on
the unmerged branch behind PR #195. If you need the wider consolidation context,
read them there; do not assume they are missing.

Related, still open, and deliberately not folded into this task:
`scripts/check-lyrics-smoke.sh` is red on `dev` and has been since before `#196`
(verified against untouched `origin/dev`). Two separate causes: its internal
`timeout 15s` also covers the `cargo run` build, so a cold run always dies at
exit 124 before the app starts; and once pre-built, the LRCLIB provider is never
asked at all (`line_count=0`, the fixture request log is never written). It is
not part of the merge gate. Full diagnosis is in the comment on PR #196. If your
refactor touches the batch's provider path, this smoke test is worth fixing
first so you have a working end-to-end signal — but it is not a prerequisite.
