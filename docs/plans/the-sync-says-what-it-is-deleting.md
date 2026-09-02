---
slug: the-sync-says-what-it-is-deleting
worktree: /home/marvin/Projects/reprise-the-sync-says-what-it-is-deleting
branch: feature/the-sync-says-what-it-is-deleting
phase: shipped
codex_session:
created: 2026-09-01
---
# The sync says what it is deleting

A sync that removes files from the phone already counts correctly — since #746
every removal is a work unit, the bar advances, the ETA is unit-based — but it
never says what it is doing. During the removal stage the dock prints a bare
device path under a heading that reads "Syncing", with no verb and no hint that a
file is leaving the phone rather than arriving on it.

This is a feature, not a repair. Nothing here is broken; the removal stage simply
has no voice. The plan gives it one, on the two surfaces that show a running sync,
and changes no scheduling, no counting and no I/O — the machine already emits
everything the labels need.

## What already works (do not "fix" it)

Worth establishing, because the missing labels look like counting bugs and are
not:

- `WorkLedger::for_plan` (`crates/reprise-core/src/device_sync/ledger.rs:17-35`)
  adds `plan.remove.len()` and `plan.playlist_removals.len()` into `total`, and
  every successful removal calls `complete_unit(0)` (`machine.rs:407,437,457`).
- A delete-only run starts: `can_start` (`page.rs:76-84`) requires no copies, and
  `phase_transitions::opening` falls through the transfer, analysis and playlist
  arms to `SyncStep::Removing` (`phase_transitions.rs:58-62`).
- The view is live: `publish_phase` runs after **every** dispatched event
  (`device_sync_planned.rs:195`), not only on the byte-progress path.
- The agent surface already names the step: `AgentDeviceSyncPhase::Removing`
  (`device_sync_agent.rs:289`).

So the work is presentation only, between the phase and the label.

## What is missing

**The dock throws the step away.** `DockReading::for_device` destructures the
running phase with a `..` that swallows `step` (`device_sync_dock.rs:74-88`), and
`DockReading::Running` has no field for it. `update` therefore sets the detail
label to the raw `current_track` (`:208-209`) with nothing in front of it. The
dock is the widest sync surface in the app and the only one that cannot tell a
deletion from a copy.

**A removal has no name, only a path.** Copies are named by `transfer_activity`
→ `"Immortal — Lorna Shore"` (`phase_transitions.rs:74-82`); removals by
`removal_path` → the raw `device_path` / `relative_path` (`:84-89`), e.g.
`Lorna Shore/Immortal/03 Immortal.opus`. Playlist removals show their `.m3u`
path (`machine.rs:626`) even though `DevicePlaylistRecord` carries the exact
`source_name` right beside it (`settings.rs:98-104`).

**The removal glyph is the only mute one.** `step_glyph`
(`sidebar_device_card_text.rs:251-258`) gives every other step a word —
`"⟳ transcoding ·"`, `"↑ analysis ·"`, `"≡ metadata ·"` — but abbreviates
removal to a bare `"−"`.

**The deferred removal shows the previous step.** `enter_deferred_removals`
(`machine.rs:677-687`) dispatches `Effect::RemoveReplacedFile` without touching
`self.phase`, so while the superseded file of a replacement is deleted the UI
still shows whichever step ran before it.

## Decisions taken in the grill

Settled, not open for reinterpretation during implementation:

1. **The name is read back out of the path, not looked up in the database.**
   `DeviceFileRecord` (`settings.rs:85-95`) holds only `track_id`, and
   `plan_mirror` is a pure function over `MirrorInput` with no database access
   (`mirror.rs:154`) — an exact title is not reachable where the phase text is
   built. It is derivable: this crate wrote the path, in `device_track_path`
   (`sanitize.rs:47-82`), as
   `{album_artist}/{album}/{number} {title}{suffix}.{extension}`. A lookup was
   rejected because `ManagedRemoval::Orphan` has no `track_id` at all, so the
   path-derived function would have to exist as its fallback anyway — the query
   would then buy only the hand-renamed-file case, at the price of a database
   read inside planning.
2. **Removal gets a wordmark, not a translated sentence.** `step_glyph` returns
   untranslated glyph vocabulary by existing design; `"− removing ·"` joins it at
   zero cost. A new `N_!` string would need an entry in all seven catalogs in
   `po/LINGUAS` plus finished `de` and `es` translations, because
   `scripts/tests/gettext-catalogs.sh` runs `msgcmp` against a freshly generated
   `.pot` and forbids untranslated entries in the complete locales. Not worth it
   for a label that already has a glyph slot.
3. **The dock's title stays `syncing_files(copied, total)`.** It covers the whole
   run, removals included; making the heading swap per step would let it jump
   between stages.
4. **Playlists take the exact route.** They are named by `source_name`, because
   it is already in the record and nothing has to be guessed.
5. **The disconnect toast is out of scope.** `previous_device`
   (`device_sync_feedback.rs:144-153`) records progress only during
   `SyncStep::Copying`, so a delete-only run that is unplugged names no figure.
   Real, adjacent, deliberately not part of this change.
6. **The sidebar card changes too, and that is intended.** Decision 2 turns its
   `"−"` into `"− removing ·"`, so the existing expectations at
   `sidebar_device_card_text.rs:472-508` are *edited*, not merely relocated.

## Tasks

TDD throughout: each test goes in first and must fail for its stated reason
before the change lands.

### Task 1 — a removal names what it removes

In `crates/reprise-core/src/device_sync/phase_transitions.rs`, beside
`transfer_activity`:

```rust
/// The reverse of `sanitize::device_track_path` — good enough to name a file
/// this crate itself wrote, and honest about the paths it did not.
pub(super) fn removal_activity(removal: &ManagedRemoval) -> String

/// A playlist is named by the source the user picked, never by its file.
pub(super) fn playlist_removal_activity(record: &DevicePlaylistRecord) -> String
```

`removal_activity` takes the path — `device_path` for `Inventory`,
`relative_path` for `Orphan` — and then:

- last component, extension stripped → candidate title; drop a leading
  `NN ` / `NNN ` track-number prefix and a trailing ` (n)` collision suffix;
- first component → album artist, but only when the path has at least three
  components (`artist/album/file`), which is the shape `device_track_path`
  produces;
- both present → `format!("{title} — {artist}")`, the same shape
  `transfer_activity` builds;
- anything else — no separator, two components, empty after stripping → the path
  unchanged. **Never return an empty string:** `sync_activity`
  (`device_sync_strings.rs:154-159`) treats empty as "no name" and would print a
  lone wordmark.

`playlist_removal_activity` returns `source_name` when non-empty, else
`device_path`.

**The function reproduces the sanitized form, not the original metadata, and
that is accepted.** `sanitize_component` (`sanitize.rs:21-45`) rewrites
`/ \ ? * : " < > |` and controls to `_`, trims dots and whitespace, and
truncates at `MAX_COMPONENT_BYTES` (120). None of that is invertible: a band
written `AC/DC` is on the device as `AC_DC` and comes back as `AC_DC`; a title
past the cap comes back cut. That is the price decision 1 knowingly paid, and it
must not be "repaired" by reaching for the database on the fallback path.

Wire both in, replacing `removal_path` at its two call sites — `enter_removals`
(`machine.rs:672`) and `opening` (`phase_transitions.rs:62`) — and replacing the
bare playlist `device_path` at `enter_playlist_removals` (`machine.rs:626`) and
`opening` (`:59`). `removal_path` then has no callers: delete it.
`removal_track_id` is unrelated and stays.

New test file `crates/reprise-core/src/device_sync/phase_transitions_tests.rs`,
declared in `device_sync.rs` in the established form
(`#[cfg(test)] #[path = "device_sync/phase_transitions_tests.rs"] mod …`, beside
`machine_tests` at `:426-428`). Cases, each named for what it protects:

- a path produced by *calling* `device_track_path` comes back as
  `"Immortal — Lorna Shore"` — building the input through the writer is what
  makes the test break if that format ever moves;
- a three-digit track number (`100 Title.opus`) loses its prefix;
- a collision suffix (`03 Immortal (2).opus`) is dropped;
- a two-component path (`Album/03 Title.opus`) keeps the title and gains no
  artist;
- a bare file name with no separator returns itself unchanged;
- an orphan path outside the naming scheme returns itself unchanged;
- **a lossy component stays lossy**: build the path through `device_track_path`
  from an album artist `AC/DC` and assert the result is `"… — AC_DC"`, not
  `"… — AC/DC"`. This is the test that pins the paragraph above; without it the
  first reader who notices the mismatch will try to fix it;
- a playlist record with a `source_name` uses it; one with an empty `source_name`
  falls back to its device path.

### Task 2 — the deferred removal gets its own phase

Failing test in `machine_tests.rs`, beside the existing removal sequences
(`a_failed_removal_still_lets_a_superseded_path_be_cleaned_up`): drive a plan
with a replacement to the point where `Effect::RemoveReplacedFile` is emitted and
assert the phase is `Syncing { step: SyncStep::Removing, .. }` naming that file.
Today it still reads whichever step ran before.

Fix: in `enter_deferred_removals` (`machine.rs:677-687`), set

```rust
self.phase = phase_transitions::syncing(&self.ledger, SyncStep::Removing, device_path.clone());
```

before returning the effect. No `begin_unit`, no `complete_unit` — the unit
belongs to the `replace` entry and is already counted (`machine.rs:460-465`).
Leave a one-line comment saying so, or the next reader will "fix" the missing
count.

### Task 3 — one glyph vocabulary, and the dock joins it

Two failing tests first:

- in `device_sync_page_tests.rs`, which already builds `DockReading` from a phase
  (`:150, :211, :575`): a `PlannedSyncPhase::Syncing` with
  `step: SyncStep::Removing` and a named track must produce a dock detail
  beginning with `− removing ·`, and the copying case must keep producing `↑`;
- in `device_sync_strings_tests.rs`: `step_glyph(&SyncStep::Removing)` is
  `"− removing ·"`.

Then:

- move `step_glyph` out of `sidebar_device_card_text.rs:251-258` into
  `device_sync_strings.rs` as `pub(in crate::ui) fn step_glyph(&SyncStep)`, and
  change its `Removing` arm to `"− removing ·"`. The sidebar keeps calling it
  through `device_sync_strings::`, which it already imports. The move exists
  because the dock is about to become the second caller — one vocabulary, two
  surfaces — not for tidiness;
- move the glyph expectations from `sidebar_device_card_text.rs:442-521` into
  `device_sync_strings_tests.rs`, **editing** the `Removing` cases at `:472-508`
  to the new wordmark (decision 6);
- add `step: SyncStep` to `DockReading::Running` and stop discarding it in
  `for_device` (`device_sync_dock.rs:74-88`);
- in `update`, build the detail as `sync_activity(step_glyph(step), name)`
  instead of the bare name (`:208-209`). With no current name `sync_activity`
  returns the wordmark alone, which is why task 1 must never produce an empty
  one.

Leave the dock's title untouched (decision 3). Leave the card's rate suffix
copy-only as well: `sidebar_device_card_text.rs:195-203` appends ` · {rate}/s`
only for `Copying | WritingAnalysis`, and that is right — a removal moves no
bytes, so there is no rate to show. The asymmetry is deliberate, not an
oversight this task forgot.

## Verification

- `cargo test -p reprise-core device_sync` — tasks 1 and 2 fail first, each for
  its stated reason, then pass.
- `cargo test -p reprise-gnome device_sync` — task 3; the relocated glyph tests
  are green in their new home with their new expectation.
- `cargo clippy --workspace --all-targets` clean — in particular no dead-code
  warning for `removal_path`, which must be gone, not left behind.
- `bash scripts/tests/gettext-catalogs.sh` green **without touching `po/`**. If it
  goes red, a translated string was introduced and decision 2 was silently
  reversed: stop and say so rather than editing catalogs.
- Manual, and this one is the operator's step, not the implementer's: with the
  phone attached, drop a playlist from the selection so the next run only
  deletes, then sync. The dock must read `Syncing · n / m files` over a detail
  line `− removing · Immortal — Lorna Shore`, the bar must advance per file, and
  the sidebar card must show the same wordmark and name.

## Parallelität

**No cut. One strand.**

The cut was attempted and does not survive its own compile order. The obvious
seam is "core naming" (task 1) against "GNOME surfaces" (task 3), but task 3's
dock test asserts exactly the text task 1 produces — a strand verifying against a
`removal_activity` that exists only on the other strand cannot go green before
the merge *in principle*, which is the failure this section exists to prevent.
Task 2 lives in `machine.rs` and calls task 1's function, so it cannot leave that
strand either.

Nothing else is left to split: the one part of the original draft that shared no
file with the rest — the disconnect toast — was dropped in the grill
(decision 5).

File ownership for the single strand:

- `crates/reprise-core/src/device_sync/phase_transitions.rs`
- `crates/reprise-core/src/device_sync/phase_transitions_tests.rs` (new)
- `crates/reprise-core/src/device_sync.rs` (the test-module declaration only)
- `crates/reprise-core/src/device_sync/machine.rs`
- `crates/reprise-core/src/device_sync/machine_tests.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_dock.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_strings.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_strings_tests.rs`
- `crates/reprise-gnome/src/ui/device_sync/device_sync_page_tests.rs`
- `crates/reprise-gnome/src/ui/sidebar/sidebar_device_card_text.rs`

No post-merge cross-checks: with one strand, every verification step reads only
files that strand owns.
