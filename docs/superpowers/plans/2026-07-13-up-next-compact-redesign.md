# Manual Up Next Queue + GNOME Compact Redesign — Implementation Plan

**Spec:** `docs/superpowers/specs/2026-07-13-up-next-compact-redesign-design.md`

**Baseline:** 674 passing workspace tests; 22 display-only tests ignored by
the normal workspace run (696 total listed tests).

## Global constraints

Work only on `feature/up-next-compact-redesign`, then merge locally to `main`
after the complete stage is green. TDD RED→GREEN for every behavior change.
Code, comments, logs, errors, UI strings and commits are English; this
plan/spec may use German where already established. Never touch real music or
the real database. Every app/display run uses private D-Bus, Xvfb, scratch
`XDG_DATA_HOME`/`XDG_CACHE_HOME`, forced X11, unset Wayland and
`REPRISE_AUDIO_SINK=fakesink`. Core stays free of GTK/libadwaita/GStreamer/
zbus. Every new or substantially edited file ends under 800 lines; the legacy
`queue.rs` stays at or below its approved 1,223-line exception, while
`player_controller.rs`, `strings.rs`, and `scripts/ptr-e2e/run.sh` must shrink
or delegate before receiving feature logic. Before every implementation
commit run fmt, strict clippy, workspace tests, audit and core purity after
core changes. Never push.

## Task 1 — Pure manual Up Next state and backward-compatible session fields

**Files:**

- new `crates/reprise-core/src/up_next.rs`
- `crates/reprise-core/src/lib.rs`
- `crates/reprise-core/src/library/session.rs`

**Interfaces:**

```rust
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UpNextQueue { /* private ordered ids */ }

impl UpNextQueue {
    pub fn append(&mut self, ids: &[i64]);
    pub fn ids(&self) -> &[i64];
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn pop_front(&mut self) -> Option<i64>;
    pub fn take_through(&mut self, position: usize) -> Option<i64>;
    pub fn move_item(&mut self, from: usize, to: usize) -> bool;
    pub fn remove_positions(&mut self, positions: &[usize]) -> usize;
    pub fn remove_ids(&mut self, ids: &[i64]) -> bool;
    pub fn truncate(&mut self, limit: usize);
}
```

`SessionState` gains serde-defaulted `up_next: UpNextQueue` and
`current_up_next: Option<i64>` without rejecting existing version-1 JSON.
Normalization caps the pending list at `QUEUE_LIMIT`.

**TDD steps:**

1. Add RED tests for ordered append, allowed duplicates, pop, take-through,
   stable multi-position remove, reorder guards, remove-all-occurrences by id,
   truncation, legacy session JSON and new-field roundtrip/normalization.
2. Observe missing-type/field failures.
3. Implement the smallest pure ordered type in the new sibling; export its
   module from `lib.rs` without growing legacy queue logic.
4. Add backward-compatible session fields and validate bounds.
5. Run targeted tests, full gates, purity, file sizes and adversarially review
   serde compatibility, duplicates and index handling.

**Commit:** `feat: model a manual up next queue`

## Task 2 — Route transport, Queue view and session through Up Next

**Files:**

- new `crates/reprise-gnome/src/ui/up_next_transport.rs`
- `crates/reprise-gnome/src/ui/player_controller.rs`
- `crates/reprise-gnome/src/ui/queue_transport.rs`
- `crates/reprise-gnome/src/ui/playback_faults.rs`
- `crates/reprise-gnome/src/ui/session_player.rs`
- `crates/reprise-gnome/src/ui/session_restore.rs`
- `crates/reprise-gnome/src/ui/track_list.rs`
- `crates/reprise-gnome/src/ui/track_list_activation.rs`
- `crates/reprise-gnome/src/ui/track_list_context_menu.rs`
- `crates/reprise-gnome/src/ui/track_list_dnd.rs`
- `crates/reprise-gnome/src/ui/window.rs`
- `crates/reprise-gnome/src/ui/mod.rs`
- `crates/reprise-gnome/src/ui/strings.rs` or a new sibling string module
- `po/reprise.pot`
- `po/de.po`

**Interfaces:**

```rust
pub(super) enum AdvanceReason { Automatic, Manual }

impl PlayerController {
    pub(super) fn advance_playback(&self, reason: AdvanceReason);
    pub(super) fn play_up_next_at(&self, position: usize);
    pub(super) fn remove_up_next_positions(&self, positions: &[usize]) -> usize;
    pub(super) fn up_next_ids_snapshot(&self) -> Vec<i64>;
    pub(super) fn up_next_len(&self) -> usize;
}
```

The existing `Queue` field remains the hidden playback context. New
`up_next: RefCell<UpNextQueue>` and `current_up_next: Cell<Option<i64>>`
fields are the only manual queue state. Queue rows/count/DnD use only Up Next.

**TDD steps:**

1. Add RED pure/controller-seam tests for context A→X→Y→B, automatic Repeat
   One, manual Next ignoring Repeat One, Previous from X returning A, empty
   context, preserving pending entries on a new context, current/pending
   session restore and bounded failure skipping.
2. Add RED track-list tests proving non-Queue activation builds a context,
   Queue activation dispatches its position, Queue context menu contains
   Remove from Queue (not Add to Queue), multi-position removal is stable,
   and DnD/view snapshots address only pending entries.
3. Implement selection in `up_next_transport.rs`; replace TrackFinished,
   Next, Previous, failure skip and stopped-toggle entry points with the one
   shared flow. Never hold queue/up-next borrows across playback callbacks.
4. Change Queue provider/count/reorder/purge to Up Next, add queue-position
   activation/removal callbacks, refresh the visible source and sidebar after
   every consume/mutation, and keep the context internal.
5. Persist/restore context, pending entries and a current manual title with
   DB validation and no autoplay; old sessions restore with an empty visible
   queue.
6. Complete English/German strings and catalog coverage.
7. Run targeted tests, full gates/purity/file sizes and an isolated smoke that
   proves visible 0→2→1→0 plus playback A→X→Y→B.
8. Adversarially review Repeat/Shuffle/MPRIS/session/error paths and fix every
   Important/Critical finding before committing.

**Commit:** `feat: separate up next from playback context`

## Task 3 — Restyle native compact layouts and make volume scroll-only

**Files:**

- `crates/reprise-gnome/src/ui/compact_player_layouts.rs`
- `crates/reprise-gnome/src/ui/compact_player_menu.rs`
- `crates/reprise-gnome/src/ui/compact_player.rs`
- new `crates/reprise-gnome/src/ui/compact_player_scroll.rs`
- `crates/reprise-gnome/src/ui/compact_mode_controls.rs`
- `crates/reprise-gnome/src/ui/window_decorations.rs`
- `crates/reprise-gnome/src/ui/mod.rs`

**Interfaces:**

```rust
pub(super) const VOLUME_STEP: f64 = 0.05;
pub(super) fn stepped_volume(current: f64, direction: f64) -> Option<f64>;

impl CompactPlayer {
    pub(super) fn connect_volume_changed(&self, f: impl Fn(f64) + 'static);
}
```

`connect_volume_changed` is retained but is fed only by scroll controllers on
declared free cover/metadata regions. Layout widgets no longer expose
`restore` or `volume`; restore remains one menu action.

**TDD steps:**

1. Add RED pure tests for ±5% clamped scroll steps, zero/non-finite no-op,
   exact layout metrics/direct-secondary-control policy, no visible restore/
   volume role and the menu's exact actions without a volume custom child.
2. Update the four ignored display tests to assert the new composition,
   accessible names, centered Cover metadata, absence of visible Restore and
   Volume controls, menu-only restore, natural sizes and one free scroll
   region per layout. Observe RED failures individually under isolated Xvfb.
3. Remove visible restore/volume widgets and menu volume row. Keep one weak
   restore callback only on the menu action.
4. Recompose Bar/Cover/Pill/Card with native Adwaita spacing matching the
   approved reference. Preserve Pill's metadata-only WindowHandle and all
   CSD/SSD projection points.
5. Wire scroll controllers from the extracted module to free regions only;
   use the existing controller/MPRIS volume callback and presentation value.
6. Update the one-activation display test to invoke menu restore instead of a
   removed button. Run all affected display tests separately.
7. Run full gates/file sizes and adversarially review input propagation,
   keyboard menu access, decoration modes and every former restore/volume
   reference.

**Commit:** `feat: refine native compact player layouts`

## Task 4 — Real-input QA, documentation and branch integration

**Files:**

- extract new `scripts/ptr-e2e/compact-flow.sh`
- shrink `scripts/ptr-e2e/run.sh`
- `scripts/ptr-e2e/README.md`
- `docs/agent-workflow/MANUAL-QA.md`
- `.superpowers/sdd/progress.md`
- `docs/agent-workflow/STATUS.md`

**Steps:**

1. Extract the existing Compact pointer flow before changing it so the
   797-line runner shrinks. Update it to prove menu-only return, all four
   layouts, right-click/Shift+F10 and real free-region scroll changing the
   exact scratch/MPRIS volume while seek remains unchanged.
2. Add an isolated full-app Up Next smoke with four fixture tracks proving
   visible count/order 0→2→1→0 and context A→manual X→manual Y→context B,
   plus a two-start current-manual/pending restore that stays Stopped.
3. Run all display-only tests individually in separate isolated Xvfb
   processes, the complete mapped-pointer harness, `scripts/check-release.sh`,
   standalone core build/purity/audit and touched-file line counts.
4. Whole-stage adversarial review: every activation/context-menu/DnD route,
   Repeat/Shuffle/Previous/Next/MPRIS/error/session path, compact input route,
   CSD/SSD mode, RefCell lifetime and old-session compatibility. Fix findings
   with RED regressions and separate fix commits.
5. Update manual QA with native Wayland queue/scroll/layout checks. Record
   exact commits/evidence in the ledger and update STATUS for completed work.
6. Merge `feature/up-next-compact-redesign` locally into `main`, rerun the
   combined release checker if main moved, release the lock in a final main
   coordination commit, and do not push.

**Commit:** `docs: record up next and compact redesign QA`
