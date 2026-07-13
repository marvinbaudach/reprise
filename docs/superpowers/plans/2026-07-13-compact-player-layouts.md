# Selectable Compact Player Layouts — Implementation Plan

**Spec:** `docs/superpowers/specs/2026-07-13-compact-player-layouts-design.md`

**Baseline:** 533 passing workspace tests; 8 display-only tests ignored by the
normal workspace run.

## Global constraints

TDD RED→GREEN for every behavior change. Code, comments, logs, errors, UI text
and commits are English; this plan/spec stay German/English as already
established. Never touch real music or the real database. Every app/display run
must use private D-Bus, Xvfb, scratch `XDG_DATA_HOME`/`XDG_CACHE_HOME`, forced
X11, unset Wayland and `REPRISE_AUDIO_SINK=fakesink`. `reprise-core` stays free
of gtk4/libadwaita/gstreamer/zbus. Every new or substantially edited file ends
under 800 lines; `window.rs` is already 790 lines and may only receive a small
module declaration/call replacement, never inline feature logic. Before every
implementation commit run fmt, strict clippy, workspace tests, audit and core
purity when core changed. Never push.

## Task 1 — Typed compact-view persistence

**Files:**

- `crates/reprise-core/src/library/settings.rs`
- new `crates/reprise-core/src/library/settings_compact_tests.rs`

**Interfaces:**

```rust
pub const WINDOW_VIEW_MODE_KEY: &str = "ui.window_view_mode";
pub const COMPACT_LAYOUT_KEY: &str = "ui.compact_layout";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowViewMode { Library, Compact }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactLayout { Bar, Cover, Pill, Card }

pub fn get_window_view_mode(conn: &Connection) -> WindowViewMode;
pub fn set_window_view_mode(
    conn: &Connection,
    value: WindowViewMode,
) -> Result<(), rusqlite::Error>;
pub fn get_compact_layout(conn: &Connection) -> CompactLayout;
pub fn set_compact_layout(
    conn: &Connection,
    value: CompactLayout,
) -> Result<(), rusqlite::Error>;
```

**TDD steps:**

1. Add five RED tests in the sibling file: fresh DB defaults to
   `Library`/`Bar`; both view modes round-trip; all four layouts round-trip;
   unknown strings fall back independently; a valid value is not affected by
   an unknown value in the other key. Register the sibling module under the
   existing `#[cfg(test)]` module.
2. Run
   `cargo test -p reprise-core library::settings::compact_tests -- --nocapture`
   and observe missing items/failed assertions.
3. Add the two enums, canonical string conversions and tolerant accessors by
   reusing `typed_value`/`set_setting`. Do not add a migration: the existing
   key/value table is the intended extension seam.
4. Re-run the target tests (5 new passing; workspace expectation 538 passing,
   8 ignored), all gates and the core-purity proof.
5. Adversarially check unknown/corrupt input, exact persisted tokens, public
   surface size and that no GTK type entered core.

**Commit:** `feat: persist compact player mode and layout`

## Task 2 — One compact surface on the existing controller state path

**Files:**

- new `crates/reprise-gnome/src/ui/compact_player.rs`
- new `crates/reprise-gnome/src/ui/compact_player_state.rs`
- `crates/reprise-gnome/src/ui/mod.rs`
- `crates/reprise-gnome/src/ui/player_controller.rs`
- `crates/reprise-gnome/src/ui/player_controller_wiring.rs`
- `crates/reprise-gnome/src/ui/now_playing_wiring.rs`
- `crates/reprise-gnome/src/ui/mpris_mirror.rs`

Task 2 deliberately builds only the `Bar` compact root. The remaining roots and
the layout menu belong to Task 3; this keeps controller fan-out reviewable on
its own.

**Interfaces:**

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CompactPresentation {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub state: PlaybackState,
    pub position_ms: i64,
    pub duration_ms: i64,
    pub transport_enabled: bool,
    pub shuffled: bool,
    pub repeat: Repeat,
    pub volume_percent: u8,
}

pub(super) struct CompactPlayer { /* GTK widgets + guarded input state */ }

impl CompactPlayer {
    pub(super) fn new() -> Self;
    pub(super) fn widget(&self) -> &gtk4::Widget;
    pub(super) fn cover_image(&self) -> &gtk4::Image;
    pub(super) fn set_track(&self, title: &str, artist: &str, album: &str);
    pub(super) fn clear_track(&self);
    pub(super) fn set_state(&self, state: PlaybackState);
    pub(super) fn set_position(&self, position_ms: i64, duration_ms: i64);
    pub(super) fn set_transport_enabled(&self, enabled: bool);
    pub(super) fn set_shuffle_indicator(&self, active: bool);
    pub(super) fn set_repeat_indicator(&self, repeat: Repeat);
    pub(super) fn set_volume_indicator(&self, volume: f64);
    pub(super) fn connect_play_pause(&self, f: impl Fn() + 'static);
    pub(super) fn connect_seek(&self, f: impl Fn(i64) + 'static);
    pub(super) fn connect_previous(&self, f: impl Fn() + 'static);
    pub(super) fn connect_next(&self, f: impl Fn() + 'static);
    pub(super) fn connect_shuffle_toggled(&self, f: impl Fn(bool) + 'static);
    pub(super) fn connect_repeat_clicked(&self, f: impl Fn() + 'static);
    pub(super) fn connect_volume_changed(&self, f: impl Fn(f64) + 'static);
}
```

`PlayerController` gains exactly one `compact_player`, one compact cover
generation token, `compact_widget()`, and `sync_volume_indicator()`. Existing
`sync_track`, `sync_clear_track`, `sync_state`, `sync_position`,
`sync_transport_enabled`, `sync_shuffle_indicator` and
`sync_repeat_indicator` fan out to Bar, NowPlaying and Compact. `sync_cover`
uses the existing `CoverLoader`, not a new loader/cache.

**TDD steps:**

1. Add five RED pure tests for default presentation, clamped position,
   stopped reset, volume clamp/percent conversion and track clearing. The
   tests target `compact_player_state`, not GTK.
2. Run the targeted gnome tests and observe missing-module/items failures.
3. Implement the immutable presentation helpers, then the Bar-shaped compact
   widget with the same capture-phase/raw-pointer seek discipline as
   `PlayerBar`. Reuse icon constants and pure seek predicates instead of
   cloning fragile behavior.
4. Add the controller field and extend every central `sync_*` method. Replace
   direct MPRIS/bar volume indicator writes with `sync_volume_indicator`.
5. Add `wire_compact_controls`, mirroring the weak-controller and queue-borrow
   discipline of Bar/NowPlaying. Every compact intent must call the existing
   controller action; it must never manipulate the backend/queue independently.
6. Run targeted tests (5 new; expectation 543 passing, 8 ignored), full gates
   and adversarially sweep all `bar.set_*`/`now_playing_view.set_*` call sites
   to prove no surface bypasses the fan-out.

**Commit:** `refactor: project playback state to compact player`

## Task 3 — Four layouts and the shared accessible menu

**Files:**

- `crates/reprise-gnome/src/ui/compact_player.rs`
- new `crates/reprise-gnome/src/ui/compact_player_layouts.rs`
- new `crates/reprise-gnome/src/ui/compact_player_menu.rs`
- `crates/reprise-gnome/src/ui/compact_player_state.rs`
- `crates/reprise-gnome/src/ui/mod.rs`
- `crates/reprise-gnome/src/ui/strings.rs`
- `po/reprise.pot`
- `po/de.po`

**Interfaces:**

```rust
pub(super) const LAYOUT_NAMES: [(CompactLayout, &str); 4];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct LayoutMetrics {
    pub width: i32,
    pub height: i32,
    pub separate_header: bool,
    pub direct_shuffle: bool,
    pub direct_repeat: bool,
    pub direct_volume: bool,
}

pub(super) fn metrics(layout: CompactLayout) -> LayoutMetrics;

impl CompactPlayer {
    pub(super) fn layout(&self) -> CompactLayout;
    pub(super) fn set_layout(&self, layout: CompactLayout);
    pub(super) fn metrics(&self) -> LayoutMetrics;
    pub(super) fn set_on_restore(&self, f: Rc<dyn Fn()>);
    pub(super) fn set_on_layout(&self, f: Rc<dyn Fn(CompactLayout)>);
    pub(super) fn set_on_preferences(&self, f: Rc<dyn Fn()>);
}
```

`compact_player_menu::build` returns one `GtkPopoverMenu` plus the stateful
layout action group. The visible menu button, right-click on each free root,
`Menu` and `Shift+F10` open that exact popover. The menu contains a custom
native volume child and uses radio targets `bar`, `cover`, `pill`, `card`.

**TDD steps:**

1. Add eight RED pure tests: exact metrics/control visibility for all four
   layouts; token→layout mapping; all radio targets; active radio follows
   `set_layout`; missing album/year suppresses rows; Pill exposes a drag-only
   free region; context-menu predicate rejects interactive descendants.
2. Add four ignored display tests (one process each under Xvfb) asserting every
   layout's required cover/labels/previous/play/next/seek/menu/restore accessible
   names. Bar exposes shuffle/repeat/volume; Cover/Pill expose them through the
   popover; Card exposes all three directly. Each root's measured natural size
   must fit its metrics without clipping.
3. Observe RED failures, then build a `gtk::Stack` with Bar, Cover, Pill and Card
   roots. Pill uses an opaque `GtkWindowHandle` only around the free metadata
   region; controls are outside the drag handle. No transparency/always-on-top.
4. Build one shared menu/action group and inject weak restore/layout/preferences
   callbacks. Clone callbacks out of `RefCell`s before invocation. Do not attach
   right-click controllers to transport/seek/volume widgets.
5. Add complete English source strings and German gettext translations. Run the
   catalog coverage checker.
6. Run targeted tests (8 new passing + 4 new ignored; expectation 551 passing,
   12 ignored), each display test individually under fully isolated Xvfb, full
   gates and the file-size check.

**Commit:** `feat: add selectable compact player layouts`

## Task 4 — Fast mode switching, startup restore and geometry isolation

**Files:**

- `crates/reprise-gnome/src/ui/minimal_view.rs`
- new `crates/reprise-gnome/src/ui/compact_mode_controls.rs`
- `crates/reprise-gnome/src/ui/primary_menu.rs`
- `crates/reprise-gnome/src/ui/preferences.rs`
- `crates/reprise-gnome/src/ui/first_run.rs`
- `crates/reprise-gnome/src/ui/session_restore.rs`
- minimal call-site edits in `crates/reprise-gnome/src/ui/window.rs`
- `crates/reprise-gnome/src/ui/mod.rs`
- `crates/reprise-gnome/src/ui/strings.rs`
- `po/reprise.pot`
- `po/de.po`

**Interfaces:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ViewTransition {
    pub mode: WindowViewMode,
    pub layout: CompactLayout,
}

pub(super) fn startup_transition(
    persisted_mode: WindowViewMode,
    persisted_layout: CompactLayout,
    first_run: FirstRunDecision,
) -> ViewTransition;

pub(super) struct MinimalView { /* window/root/DB/geometry state */ }

impl MinimalView {
    pub(super) fn new(
        window: &adw::ApplicationWindow,
        full_root: &adw::NavigationSplitView,
        compact: Option<&CompactPlayer>,
        conn: Rc<RefCell<Connection>>,
        initial: ViewTransition,
        toast: Rc<dyn Fn(&str)>,
    ) -> Rc<Self>;
    pub(super) fn toggle(&self);
    pub(super) fn select_layout(&self, layout: CompactLayout);
    pub(super) fn apply_initial(&self);
    pub(super) fn geometry_guard(&self) -> Rc<Cell<bool>>;
}
```

**TDD steps:**

1. Add six RED pure tests: Library↔Compact toggle retains layout; first-run
   forces Library; completed/existing-library restores Compact; layout selection
   leaves mode Compact; failed mode persistence leaves root/state unchanged;
   failed layout persistence restores previous layout/metrics.
2. Add one ignored display test proving the full-header compact button enters
   the selected layout and its restore button returns in one activation while
   the same `AdwApplicationWindow` remains active.
3. Refactor `first_run::run` to consume one precomputed `FirstRunDecision`, so
   startup mode selection and wizard presentation share the exact decision.
4. Replace PlayerBar reparenting in `minimal_view.rs` with root switching between
   `NavigationSplitView` and `CompactPlayer`. Persist before committing a mode
   transition; on failure keep/reapply the prior root and toast. Apply layout
   metrics without touching the cached full geometry. Closing Compact must not
   force a Library transition.
5. `compact_mode_controls` installs the full-header one-click button and owns
   the small callbacks that would otherwise overflow `window.rs`. The existing
   `win.toggle-minimal-view`/`Ctrl+M`, Preferences row, compact restore button
   and both menus all call the same `MinimalView::toggle`.
6. Apply the startup transition after session/queue restoration but before
   `window.present()`. A restored current track remains `Stopped`. Update smoke
   hooks to accept `REPRISE_SMOKE_MINIMAL_VIEW=stay` plus
   `REPRISE_SMOKE_COMPACT_LAYOUT=bar|cover|pill|card` and log exact mode/layout.
7. Complete gettext, run targeted tests (6 new + 1 ignored; expectation 557
   passing, 13 ignored), the display test individually, all gates and an
   adversarial close-order/RefCell/action-state review.

**Commit:** `feat: restore compact player with fast switching`

## Task 5 — End-to-end QA and stage close-out

**Files:**

- `scripts/ptr-e2e/run.sh`
- `scripts/ptr-e2e/README.md` if invocation/evidence changes
- `docs/agent-workflow/MANUAL-QA.md`
- `.superpowers/sdd/progress.md`
- `docs/agent-workflow/STATUS.md`

**Steps:**

1. Extend the real mapped-pointer harness: use the full-header button to enter
   Compact, open the visible menu, select Bar→Cover→Pill→Card, capture each,
   open the same menu with right-click and `Shift+F10`, restore Library with the
   visible button, then repeat with `Ctrl+M`. Reject GTK/GLib criticals, panics
   and `RefCell` failures.
2. Run a fully isolated playing smoke with fixture music and fakesink. Record one
   track id, Playing state and position before switching; traverse Library and
   all four layouts; prove the same id/state and non-rewinding position.
3. Run a fully isolated two-start smoke on one scratch profile: first run closes
   in Card; second run reports Compact/Card, the same restored current track and
   `Stopped` without autoplay. Verify stored `ui.window_view_mode=compact` and
   `ui.compact_layout=card`; verify the full session geometry is unchanged.
4. Run all 13 display-only tests individually in separate isolated Xvfb
   processes, then `scripts/check-release.sh`, standalone core build/purity,
   audit and explicit touched-file line counts.
5. Whole-stage adversarial review against the spec: every input route, playback
   fan-out, cover generation, startup/close ordering, menu action state,
   accessibility and first-run behavior. Fix Important/Critical findings with
   their own RED regression and fix commit, then repeat affected gates.
6. Update `MANUAL-QA.md` with the four native GNOME/Wayland visual, drag, touch,
   HiDPI and WM checks. Append exact task/commit/test/smoke evidence to the
   ledger. Update STATUS to completed/current-plan-none, set the lock FREE and
   commit the release separately.

**Commit:** `docs: record compact player layout QA`
