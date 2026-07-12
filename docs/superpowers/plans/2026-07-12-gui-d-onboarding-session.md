# GUI-D: First-run Wizard + Session Restore — Implementation Plan

**Goal:** Guide a truly new user into setup and restore a validated previous
window/view/queue session without autoplay.

**Baseline:** 440 passed; 1 ignored. Core purity, strict gates, one commit per
task, adversarial review, isolated smokes, and `<800` lines are mandatory.

## Task 1 — Exact validated QueueSnapshot

**Files:** modify `crates/reprise-core/src/queue.rs`.

Add serde derives to `Repeat` and:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueSnapshot {
    pub ids: Vec<i64>,
    pub order: Vec<usize>,
    pub position: Option<usize>,
    pub repeat: Repeat,
    pub shuffled: bool,
}

pub fn Queue::snapshot(&self) -> QueueSnapshot;
pub fn Queue::restore_snapshot(&mut self, snapshot: QueueSnapshot)
    -> Result<(), QueueSnapshotError>;
```

Validation: equal lengths, `order` is a permutation of `0..ids.len()`, empty
implies no position, non-empty position is in range. Validate before mutating,
so failure leaves Queue unchanged.

RED tests: exact shuffled/reordered roundtrip; duplicate/out-of-range order
rejected without mutation; invalid position rejected.

Expected 443 passed; 1 ignored.

Commit: `feat: snapshot and restore validated queue state`

## Task 2 — Versioned bounded SessionState persistence

**Files:** create `crates/reprise-core/src/library/session.rs`; modify library
mod and settings/constants.

Use serde/serde_json already in core. Define `SessionStateV1` with geometry,
plain source enum, search, BrowseFilter, sort, QueueSnapshot. Do not serialize
GTK types. `load` returns default on missing/corrupt/unknown-version data and
logs; `save` writes canonical JSON to `ui.session.v1`.

Bounds: width 600..8192, height 400..8192, search <= 1024 bytes, queue <=
`QUEUE_LIMIT`, sort whitelist only. Playlist/Smart IDs must be positive.

RED tests: roundtrip all fields; corrupt/unknown version default; bounds and
unknown sort normalize; oversized queue defaults safely.

Expected 447 passed; 1 ignored.

Commit: `feat: persist bounded versioned UI session state`

## Task 3 — Restore queue metadata without autoplay

**Files:** modify `player_controller.rs`, `queue_transport.rs`, possibly
`now_playing_wiring.rs` and `mpris_mirror.rs`.

Add:

```rust
pub(super) fn session_queue_snapshot(&self) -> QueueSnapshot;
pub(super) fn restore_session_queue(&self, snapshot: QueueSnapshot);
```

Restore snapshot, remove IDs no longer present in DB, sync shuffle/repeat,
transport sensitivity and current track labels/covers. Set MPRIS state to
Stopped. Never call `Player::play`, `play_track_id`, seek, or toggle_pause.

Add a pure `restore_should_start_playback() -> false` regression test and
controller-level smoke log `session queue restored ... playback=Stopped`.

Expected 448 passed; 1 ignored.

Commit: `feat: restore queue metadata without starting playback`

## Task 4 — Snapshot and restore TrackList/Sidebar view state

**Files:** modify `browse_bar.rs`, `track_list.rs`, `track_list_sort.rs`,
`sidebar.rs`, `window.rs` only via extracted helper if needed.

Add plain snapshot accessors. Restore order: set search text, browse raw filter,
sort state/visible indicator, then request Sidebar source selection; Sidebar's
existing fallback handles vanished IDs. Non-Library source ignores browse.
Callbacks must not reload more than once at final application; use an update
guard/batched restore helper.

RED pure tests: unknown sort -> title/asc; vanished source decision reuses
existing fallback; Browse restore preserves empty-string Unknown values.

Add `REPRISE_SMOKE_VIEW_SESSION` to apply/log exact source/search/browse/sort.

Expected 451 passed; 1 ignored.

Commit: `feat: restore validated track view and sidebar source`

## Task 5 — Window geometry and close-time session orchestration

**Files:** create `ui/session_restore.rs`; modify `ui/mod.rs`, `window.rs` by
extraction, strings only if a toast is required.

Load SessionState before window builder and use clamped default width/height +
maximized. After all widget callbacks exist, apply view and queue restore.
Connect close-request with weak widget/controller references; clone plain state,
save once, return `Propagation::Proceed` even on error.

Permanent hooks:

- `REPRISE_SMOKE_SESSION_SEED=<fixture>` applies a deterministic view/queue/
  geometry state then closes through the real save handler.
- `REPRISE_SMOKE_SESSION_REPORT=1` logs the loaded state and playback status.

Two-start isolated E2E reuses one temporary XDG_DATA_HOME and proves exact
restore plus no autoplay.

Expected 452 passed; 1 ignored.

Commit: `feat: save and restore window and application session`

## Task 6 — First-run decision and onboarding persistence

**Files:** create `ui/first_run.rs`; modify core settings for
`ONBOARDING_COMPLETED_KEY`; add strings.

Pure decision:

```rust
enum FirstRunDecision { ShowWizard, ExistingLibrary, AlreadyCompleted }
fn decide(completed: bool, library_root: Option<&str>) -> FirstRunDecision;
```

Existing library auto-persists completion. Missing/invalid settings degrade to
ShowWizard, never hide setup accidentally.

RED tests cover all three decisions and boolean persistence.

Expected 456 passed; 1 ignored.

Commit: `feat: detect first run without prompting existing libraries`

## Task 7 — Native wizard reusing existing actions and scan button

**Files:** implement GTK in `first_run.rs`; modify `primary_menu.rs` to expose
stable action constants if needed; wire from `window.rs` with one call.

Build AdwDialog with welcome/privacy copy, Cover download and read-only
Rhythmbox check rows, Skip and Set Up responses. Completion handler:

1. changes the existing cover-download stateful action only if opted in;
2. activates the existing Rhythmbox import action only if opted in;
3. persists onboarding complete;
4. closes; real setup emits `scan_button.clicked()`.

`REPRISE_SMOKE_FIRST_RUN=skip|setup-options` invokes the exact completion
handler but suppresses the undrivable portal picker. Verify scratch settings,
cover module flag/import layout, one wizard presentation, and no real dconf.

Expected 458 passed; 1 ignored (pure option/response helpers).

Commit: `feat: add native first-run setup wizard`

## Task 8 — GUI-D close-out

Run full gates, audit, core standalone/purity, touched sizes, isolated first-run
smokes and two-start session E2E. Whole-branch review: no autoplay path,
snapshot validation-before-mutation, deleted-ID queue cleanup, source fallback,
single final view reload, weak lifecycle captures, close always proceeds.

Manual: real wizard/portal, geometry/maximize, queue/current metadata, physical
Play after restore, no autoplay, existing-library upgrade suppression.

Update STATUS to Release and ledger.

Commit: `docs: close GUI-D onboarding and session restore stage`
