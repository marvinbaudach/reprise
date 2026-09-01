---
slug: one-missing-file-no-longer-ends-the-queue
worktree:
branch:
phase: planned
codex_session:
created: 2026-09-01
strands: a,b,c
merge_order: a,b,c
---
# One missing file no longer ends the queue

## Why

A screenshot from 2026-08-31 21:23 shows the Android queue stopped dead on
`A.I.` (Emmure) with `ERROR_CODE_IO_UNSPECIFIED: Source error` and 175 upcoming
tracks going nowhere. The measurements are in
`docs/plans/android-source-error-on-synced-track.findings.md` — read that first;
nothing there needs re-measuring.

The desktop sync had **deleted the file** at 21:15 ("no longer covered by the
selection") and only wrote it back at 21:27:51. For those twelve minutes the
Android library row pointed at a SAF URI with no bytes behind it. Three defects
turned that into a dead session instead of a half-second hiccup:

1. Every playback surface **stops** on a fault instead of skipping — against an
   active UX rule that says otherwise.
2. The error text discards the exception that would have named the cause.
3. The sync deletes and re-copies the same files run after run, which is what
   opened the window at all.

## The rule already exists

`docs/ux-rules.md` specifies the behaviour, `[active] [core]`, three times:

- **FB-6** — "Exception: the currently playing queue item faults → skip. A
  track shows one toast *Track unavailable — skipped*."
- **PLAY-5a** — "the playing track is never stopped by this (if the playing
  track itself faults, FB-6 applies: skip + one toast)."
- **PLAY-5b** — "**No background event (deleted, unmounted, sync removal,
  watcher) stops the playing track.**"

A sync removal deleting the playing track's file is verbatim the case PLAY-5b
names. Strand A restores specified behaviour; it invents none.

`crates/reprise-core/src/playback/fault_policy.rs` already carries the policy:
`playback_fault_policy()` returns `skip: true` in **both** branches, with
exactly one notice by construction. GNOME consumes it. Android and the runtime
service ignore it.

## Decisions (settled in the grill, 2026-09-01)

**D1 — `PlayerEvent::Error` has three independent consumers, not one shared arm.**

| surface | consumer | today |
|---|---|---|
| Android | `reprise-android-ffi/src/playback_session.rs:452` | stops |
| GNOME (in-process player) | `reprise-gnome/src/ui/playback/playback_faults.rs` | already correct |
| Headless runtime service | `reprise-runtime/src/transport.rs:433` | stops |

`reprise-android-ffi` depends only on `reprise-core` and `reprise-view`;
`reprise-gnome` plays in-process via `reprise_platform_linux::player::Player`
and uses `reprise-runtime-client` only for commands. There is no shared arm to
break, and no accidental blast radius.

**D2 — The runtime service is fixed too.** FB-6 is `[core]`, so the MPRIS/agent
surface is not exempt. Its Error arm calls `advance_past_failures`, which the
crate already has and which the `TrackFinished` arm already uses.

**D3 — The skip *guard* is lifted into core and GNOME is moved onto it in the
same strand.** One rule, one place — which is precisely why `fault_policy.rs`
exists. Leaving GNOME's private copy for a follow-up commit would leave the
rule duplicated in the tree.

Scope of that unification, stated so the code phase does not overreach: **only
`should_stop_skipping` becomes shared.** The *notice* stays per-surface and
will legitimately be produced three different ways after this plan — GNOME
through `playback_fault_policy` and `strings_issues.rs`, Android through a
direct mapping in `playback_session.rs` (D4), and the runtime service not at
all, because it reports `failure_kind`/`failure_track_id` on the wire and lets
its client phrase things. Unifying those three is a separate piece of work and
is not attempted here. Likewise `failure_limit` — GNOME's latching helper —
stays in GNOME; Android keeps its own small latch.

**D4 — The FB-6 sentence is produced in Rust, in `snapshot.error`.**
`fault_policy.rs`'s doc comment asks frontends to translate the notice at their
presentation edge, and GNOME does that in `strings_issues.rs`. Android has **no
i18n at all** (no `R.string` lookups anywhere in the app) and `snapshot.error`
already carries finished display text from Rust. Deliberate deviation: no new
FFI field, no uniffi regeneration, and the strand cut stays disjoint.

**D5 — The banner shows FB-6's sentence; the cause chain goes only to logcat.**
Android has no toast or snackbar — `BrowseErrorLine`, fed from
`snapshot.error`, is the only surface. Putting `FileNotFoundException` in it
would be developer text in a user UI and would diverge from FB-6. Strand B
therefore keeps all of its value and moves it to the right surface.

**D6 — Strand C fixes the deletion, not the smart list.** The churn has two
halves: a capped, volatile-ordered smart list whose membership legitimately
moves, and a planner that deletes anything outside today's selection with no
hysteresis. Whether "Top rated" *should* be a stable set is a product question;
that the mirror thrashes files is a correctness one. C does the second, with a
**stability margin on the cap** — chosen over a grace period (`device_files`
has no timestamp column, so that needs a migration) and over honouring `pinned`
(dead outside one test, no UI, and no help against automatic churn).

## The cut

Three strands, at the cap, disjoint by file.

**Strand A — `docs/plans/one-missing-file-no-longer-ends-the-queue-a.md`**
Every playback surface obeys FB-6.
Owns `crates/reprise-core/src/playback/**`, `crates/reprise-android-ffi/**`,
`crates/reprise-gnome/src/ui/playback/playback_faults.rs`,
`crates/reprise-runtime/**`.

**Strand B — `docs/plans/one-missing-file-no-longer-ends-the-queue-b.md`**
The fault path says what actually happened.
Owns `android/app/src/main/java/de/reprise/spike/Media3PlaybackPort.kt`,
`android/app/src/test/java/de/reprise/spike/**`.

**Strand C — `docs/plans/one-missing-file-no-longer-ends-the-queue-c.md`**
The sync stops deleting what it is about to re-copy.
Owns `crates/reprise-core/src/device_sync/**`.

A is much larger than B and C. That is accepted deliberately: the core guard
Android needs is created in A, so splitting A would put two strands in
`crates/reprise-core/src/playback/**` and break the very disjointness the cut
exists to guarantee.

A and C both live in `reprise-core` but in disjoint module trees
(`src/playback/**` vs `src/device_sync/**`). **Neither strand may edit
`crates/reprise-core/src/lib.rs`** — both module trees are already declared.

## Merge order: A, B, C

A first: it owns the banner text that B's logging complements, so B is never
merged into a tree where that path is still dead. C is independent of both and
lands last because it is the one with a behavioural judgement call in it.

## Post-merge cross-checks

None of these can run inside a strand.

1. **A + B together on the real phone.** The banner text A produces and the
   logcat line B writes are one feature, and neither strand can check the pair —
   A owns no Kotlin, B owns no Rust. After both land, rename a queued track's
   file out from under the player and confirm: one skip, the queue still
   running, FB-6's sentence in the banner, and the real exception in
   `adb logcat`.
2. **Full workspace build.** A and C both change `reprise-core`; only a build
   after both have merged proves the module trees really were disjoint.
   `cargo test --workspace`.
3. **UX traceability.** `scripts/check-ux-traceability.sh` reads
   `docs/ux-rules.md` together with every crate's tests, so A's rule-named tests
   cannot be checked from inside A's own scope. Run it after A lands.
4. **The `deleted` control arm.** After C lands, run a real device sync twice
   in a row and read `sync_runs` and `sync_events`. The discriminating
   assertion is **zero re-transfers of tracks the first run already placed** —
   not "zero transfers", because runs 87/88 also carried unrelated
   case-collision failures and a run can legitimately have work to do. The
   control arm is `deleted`: it must not become permanently zero, or the fix
   has simply disabled removals.

## Out of scope, deliberately

- Making smart-list membership itself stable (D6).
- The queue search that does not filter, and the filename-shaped titles — both
  recorded in the findings document as unrelated to this bug.
