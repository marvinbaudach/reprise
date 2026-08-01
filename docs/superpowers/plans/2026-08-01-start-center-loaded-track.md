---
slug: start-center-loaded-track
worktree: /home/marvin/Projects/reprise-start-center-loaded-track
branch: feature/start-center-loaded-track
phase: planned
codex_session:
created: 2026-08-01
---
# START-1 Implementation Plan — library at startup, loaded track centered and marked paused

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A normal start always opens the library view with the remembered
sort, centers the loaded track in it, and marks that row exactly like a
mid-session pause.

**Architecture:** Three seams, each small. (1) A pure `startup_place` replaces
the persisted browser place at startup — always the library root, sort kept,
search/facets/anchor dropped. (2) `CurrentTrackChange::SessionRestore` starts
setting `playing_track_id` and the `.playback-paused` class, so the loaded
track carries the marker; its reveal policy drops to `MarkerOnly`. (3) A new
`center_loaded_track` runs *after* the startup routing and drives the existing
hardened centering scheduler (`schedule_centered_scroll_restore`). Because the
startup place carries no anchor, no second scroller competes.

**Tech Stack:** Rust, gtk4-rs / libadwaita, `reprise-core` (dependency-pure),
`reprise-gnome`. Tests are `cargo test --workspace`; display tests run under
`xvfb-run`.

**Spec:** `docs/superpowers/specs/2026-08-01-start-center-loaded-track-design.md`

## Global Constraints

- Base branch: `origin/dev`. Work in a worktree under `.worktrees/`, never
  under `/tmp` (`/tmp` is a 16G tmpfs; a cargo `target/` there lives in RAM).
- **English everywhere** in code, comments, log strings and commit messages.
  Design docs and specs stay German; `docs/ux-rules.md` is German.
- Tests that gate a binding rule are **rule-named**: `fn start_1_…`.
- Display tests carry `#[ignore = "requires a display; run via xvfb-run"]`.
- Gates — ALL must pass before every commit, from the repo root:
  ```bash
  cargo fmt --check
  cargo clippy --all-targets --workspace -- -D warnings
  cargo test --workspace
  cargo audit   # only accepted advisory: RUSTSEC-2024-0436 (paste, via lofty)
  ```
- Every code file created or substantially edited ends **< 800 lines**.
  Current sizes on `dev`: `current_track_selection.rs` 715,
  `window_runtime_wiring.rs` 787, `track_list_reload.rs` 563,
  `track_list.rs` 596. The 787 one matters — check after editing.
- Never point tooling at the real database `~/.local/share/reprise/reprise.db`.

## File Structure

| File | Responsibility after this plan |
| --- | --- |
| `crates/reprise-gnome/src/ui/session_restore.rs` | adds `startup_place` — the pure "where does a normal start land" decision |
| `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs` | routes to `startup_place` instead of the persisted place; calls `center_loaded_track` after routing |
| `crates/reprise-gnome/src/ui/track_list/current_track_selection.rs` | `SessionRestore` marks the loaded track and freezes its equaliser; reveal policy `MarkerOnly` |
| `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs` | adds `center_loaded_track`, next to the scheduler it drives |
| `crates/reprise-gnome/src/ui/track_list/track_list.rs` | `TrackList::center_loaded_track` pass-through |
| `crates/reprise-gnome/src/ui/track_list/start_restore_tests.rs` | **new** — START-1 display tests (keeps `current_track_selection.rs` under the size rule) |
| `docs/ux-rules.md` | START-1 rewritten, BROWSE-5 amended |

---

### Task 1: A normal start always lands in the library

**Files:**
- Modify: `crates/reprise-gnome/src/ui/session_restore.rs` (add `startup_place` + its unit test)
- Modify: `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs:641-659`
- Modify: `docs/ux-rules.md` (BROWSE-5, around line 3161)

**Interfaces:**
- Produces: `pub(super) fn startup_place(state: &SessionState) -> reprise_core::browser::BrowserPlace`
- Consumes: nothing from earlier tasks.

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` block at the bottom of
`crates/reprise-gnome/src/ui/session_restore.rs`:

```rust
    #[test]
    fn start_1_startup_place_is_always_the_library_root() {
        use reprise_core::browser::{LibraryScope, SortDirection, TrackCollection};

        let state = SessionState {
            source: SessionSource::Playlist(7),
            search: "leftover".into(),
            browse: reprise_core::queries::BrowseFilter {
                genre: Some("Metal".into()),
                ..reprise_core::queries::BrowseFilter::default()
            },
            sort_field: "year".into(),
            sort_dir: "desc".into(),
            browser_place: Some(reprise_core::browser::BrowserPlace::from(
                ViewSource::Playlist(7),
            )),
            library_root: Some(reprise_core::browser::BrowserPlace::from(ViewSource::Queue)),
            ..SessionState::default()
        };

        let place = startup_place(&state);

        let reprise_core::browser::BrowserPlace::Tracks(track_place) = &place else {
            panic!("a normal start must route into the track list");
        };
        assert_eq!(
            track_place.collection,
            TrackCollection::Library(LibraryScope::All),
            "where the last session ended is deliberately not restored"
        );
        assert_eq!(track_place.state.sort.field, "year");
        assert_eq!(track_place.state.sort.direction, SortDirection::Descending);
        assert!(
            track_place.state.search.is_empty(),
            "a stale search reads as a lost library on a cold start"
        );
        assert_eq!(
            track_place.state.browse,
            reprise_core::queries::BrowseFilter::default()
        );
        assert!(track_place.state.anchor.is_none());
        assert!(track_place.state.selected_ids.is_empty());
    }
```

- [ ] **Step 2: Run the test and watch it fail**

```bash
cargo test -p reprise-gnome start_1_startup_place_is_always_the_library_root
```
Expected: FAIL — `cannot find function 'startup_place' in this scope`.

- [ ] **Step 3: Implement `startup_place`**

Insert into `crates/reprise-gnome/src/ui/session_restore.rs`, directly after
`apply_initial_geometry`:

```rust
/// The place a normal start routes to (START-1): always the library root,
/// carrying only the remembered sort.
///
/// Where the last session ended — a playlist, the queue, a podcast channel —
/// is deliberately not restored, and neither is a leftover search or facet.
/// On a cold start a stale refinement does not read as "the filter I chose"
/// but as "my library is gone", and the player is opened to hear music, so
/// the library is the honest destination. The sort survives because it is a
/// preference rather than a refinement.
pub(super) fn startup_place(state: &SessionState) -> reprise_core::browser::BrowserPlace {
    use reprise_core::browser::{
        BrowserPlace, LibraryScope, SortDirection, TrackCollection, TrackSort, TrackViewState,
    };

    let direction = if state.sort_dir == "desc" {
        SortDirection::Descending
    } else {
        SortDirection::Ascending
    };
    BrowserPlace::tracks(
        TrackCollection::Library(LibraryScope::All),
        TrackViewState {
            sort: TrackSort::new(state.sort_field.clone(), direction),
            ..TrackViewState::default()
        },
    )
}
```

- [ ] **Step 4: Run the test and watch it pass**

```bash
cargo test -p reprise-gnome start_1_startup_place_is_always_the_library_root
```
Expected: PASS.

- [ ] **Step 5: Route the startup through it**

In `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`, replace this
block (it begins right after the `restore_runtime` call):

```rust
    let restored_place = session_state
        .browser_place
        .clone()
        .unwrap_or_else(|| BrowserPlace::from(ViewSource::Library));
    let restored_root = session_state
        .library_root
        .clone()
        .unwrap_or_else(|| BrowserPlace::from(ViewSource::Library));
    nav_history.restore(restored_place.clone(), restored_root);
    nav_history.begin_back();
    super::library_shell::route_to_place(
        &crate::ui::nav_history::NavPlace::browser(restored_place),
```

with:

```rust
    // START-1: a normal start ignores the persisted place and opens the
    // library. Both fields are still written on close (schema unchanged, the
    // way back stays open) — they are simply no longer read here.
    let startup_place = super::session_restore::startup_place(session_state);
    nav_history.restore(startup_place.clone(), startup_place.clone());
    nav_history.begin_back();
    super::library_shell::route_to_place(
        &crate::ui::nav_history::NavPlace::browser(startup_place),
```

The remaining arguments of the `route_to_place` call stay exactly as they are.
`BrowserPlace` and `ViewSource` are still used elsewhere in the file (lines 277
and 467), so no import becomes unused.

- [ ] **Step 6: Amend BROWSE-5 in the rulebook**

In `docs/ux-rules.md`, replace the body of **BROWSE-5** with:

```markdown
- **BROWSE-5** [active] [core] — **Session restore is limited.** The
  remembered sorting and the structured playback origin are restored; the
  start always opens the library root (START-1). The last visited location,
  history, open search surfaces, utilities, and raw widget focus do not
  survive a restart.
```

- [ ] **Step 7: Run the gates**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
```
Expected: all green. Note the new total test count for the next task.

- [ ] **Step 8: Commit**

```bash
git add crates/reprise-gnome/src/ui/session_restore.rs \
        crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs \
        docs/ux-rules.md
git commit -m "feat(start): always open the library view on a normal start"
```

---

### Task 2: The restored track is marked like a paused song

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/current_track_selection.rs:35-43` (`reveal_policy`), `:230-240` (`update_current_track`), plus its `mod tests`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: after a `CurrentTrackChange::SessionRestore`,
  `Shared::playing_track_id` holds the loaded track id and the `ColumnView`
  carries the `playback-paused` CSS class. Task 3 relies on both.

- [ ] **Step 1: Write the failing test**

Append to the `mod tests` block in
`crates/reprise-gnome/src/ui/track_list/current_track_selection.rs`:

```rust
    /// START-1: the restored track is loaded, not running. It gets the marker,
    /// but the viewport belongs to the startup centering — never to this
    /// callback, which fires before the target view even exists.
    #[test]
    fn start_1_session_restore_marks_without_moving_the_viewport() {
        assert_eq!(
            reveal_policy(CurrentTrackChange::SessionRestore, false),
            TrackRevealPolicy::MarkerOnly
        );
        assert_eq!(
            reveal_policy(CurrentTrackChange::SessionRestore, true),
            TrackRevealPolicy::MarkerOnly
        );
    }
```

- [ ] **Step 2: Run the test and watch it fail**

```bash
cargo test -p reprise-gnome start_1_session_restore_marks_without_moving_the_viewport
```
Expected: FAIL — `assertion \`left == right\` failed: left: Center, right: MarkerOnly`.

- [ ] **Step 3: Move `SessionRestore` to `MarkerOnly`**

Replace `reveal_policy` in the same file with:

```rust
fn reveal_policy(change: CurrentTrackChange, user_scrolling: bool) -> TrackRevealPolicy {
    match change {
        CurrentTrackChange::PlaybackStarted | CurrentTrackChange::SessionRestore => {
            TrackRevealPolicy::MarkerOnly
        }
        CurrentTrackChange::AutomaticAdvance if user_scrolling => TrackRevealPolicy::MarkerOnly,
        CurrentTrackChange::AutomaticAdvance | CurrentTrackChange::ExplicitTransport => {
            TrackRevealPolicy::Center
        }
    }
}
```

- [ ] **Step 4: Run the test and watch it pass**

```bash
cargo test -p reprise-gnome start_1_session_restore_marks_without_moving_the_viewport
```
Expected: PASS.

- [ ] **Step 5: Set the marker and freeze the equaliser on restore**

In `update_current_track` (same file), replace this block:

```rust
        if matches!(
            change,
            CurrentTrackChange::PlaybackStarted
                | CurrentTrackChange::AutomaticAdvance
                | CurrentTrackChange::ExplicitTransport
        ) {
            self.shared.playing_track_id.set(Some(track_id));
        }
```

with:

```rust
        // Every change carries a loaded track, including the session restore:
        // NAV-10a asks for the marker on every visible instance of the
        // *loaded* track, not only the running one.
        self.shared.playing_track_id.set(Some(track_id));
        if change == CurrentTrackChange::SessionRestore {
            // START-1: a restored track is loaded but not running, so its row
            // must look exactly like a mid-session pause — same marker, same
            // frozen equaliser. `restore_session_queue` fans out a
            // `Stopped` before this runs (session_player.rs), which is why
            // the class is set here and not earlier. The first real `Playing`
            // drops it again (`on_playback_state`).
            self.set_playback_paused(true);
        }
```

Note the placement: this stays **above** the `let Some(position) = … else`
early return, so a loaded track that is not in the current view still gets its
marker.

- [ ] **Step 6: Run the full suite**

```bash
cargo test --workspace
```
Expected: PASS, one test more than Task 1's total. The existing
`nav_10a_row_activation_marker_does_not_move_selection_or_viewport` and
`fil_9_filter_changes_center_the_visible_playing_track` are display tests and
stay `ignored` in this run.

- [ ] **Step 7: Run the gates and commit**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
git add crates/reprise-gnome/src/ui/track_list/current_track_selection.rs
git commit -m "feat(start): mark the restored track like a paused song"
```

---

### Task 3: Center the loaded track after the startup routing

**Files:**
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs` (add `center_loaded_track`)
- Modify: `crates/reprise-gnome/src/ui/track_list/track_list.rs` (add the `TrackList` method, next to `restore_browser_place`)
- Modify: `crates/reprise-gnome/src/ui/track_list/current_track_selection.rs` (register the new test module at the very bottom)
- Create: `crates/reprise-gnome/src/ui/track_list/start_restore_tests.rs`
- Modify: `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs` (call it after `route_to_place`)
- Modify: `docs/ux-rules.md` (START-1, around line 1092)

**Interfaces:**
- Consumes: `Shared::playing_track_id` and the `playback-paused` class from
  Task 2; the anchor-free `startup_place` from Task 1.
- Produces: `pub(in crate::ui) fn center_loaded_track(shared: &Shared)` in
  `track_list_reload`, and `TrackList::center_loaded_track(&self)`.

- [ ] **Step 1: Write the failing display tests**

Create `crates/reprise-gnome/src/ui/track_list/start_restore_tests.rs`:

```rust
//! START-1 display tests: a normal start marks the loaded track like a paused
//! song and centers it, without touching selection or focus.
//!
//! Included as a child module of `current_track_selection` (see the bottom of
//! that file) for two reasons: the tests drive its private
//! `update_current_track`, and that file is already close to the project's
//! 800-line ceiling.

use std::rc::Rc;
use std::time::Duration;

use gtk4::prelude::*;

use super::*;

/// A realised track table over `rows` synthetic tracks, in a window big
/// enough to scroll — centering needs `upper > page_size` to mean anything.
fn synthetic_track_list(rows: i64) -> (TrackList, gtk4::Window) {
    let conn = crate::test_db::open().unwrap();
    let fixture_conn = crate::test_db::connection(&conn);
    let tx = fixture_conn.unchecked_transaction().unwrap();
    for id in 1..=rows {
        tx.execute(
            "INSERT INTO tracks (id, path, title, artist, added_at) \
             VALUES (?1, ?2, ?3, 'Synthetic Artist', 0)",
            (
                id,
                format!("/synthetic/{id:03}.flac"),
                format!("Track {id:03}"),
            ),
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let track_list = TrackList::new(
        Rc::new(conn),
        Box::new(|_, _, _, _| {}),
        |_, _, _, _| {},
        crate::ui::track_list::queue_sections::QueueViewModel::default,
        crate::ui::cover_download_worker::setup_for_test(),
    );
    let window = gtk4::Window::builder()
        .default_width(900)
        .default_height(320)
        .child(track_list.widget())
        .build();
    window.present();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        track_list
            .shared
            .column_view
            .vadjustment()
            .is_some_and(|adjustment| adjustment.upper() > adjustment.page_size())
    });
    (track_list, window)
}

fn centered_value(track_list: &TrackList, position: u32) -> Option<f64> {
    let adjustment = track_list.shared.column_view.vadjustment()?;
    scroll_center::centered_scroll_value(
        position,
        track_list.shared.model.n_items(),
        adjustment.upper(),
        adjustment.page_size(),
    )
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_1_loaded_track_is_centered_and_marked_paused() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = synthetic_track_list(100);

    let position = 60_u32;
    let track_id = track_list.shared.model.track_at(position).unwrap().id;

    // Exactly what a normal start does, in order: the session restore marks
    // the loaded track, then the startup routing hands the viewport over.
    track_list.update_current_track(track_id, None, CurrentTrackChange::SessionRestore);
    track_list.center_loaded_track();

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    crate::ui::test_settle::settle_until(crate::ui::test_settle::DISPLAY_TEST_TIMEOUT, || {
        centered_value(&track_list, position)
            .is_some_and(|target| (adjustment.value() - target).abs() < 0.5)
    });

    let expected = centered_value(&track_list, position)
        .expect("a 100-row list in a 320px window must have centering geometry");
    assert!(
        (adjustment.value() - expected).abs() < 0.5,
        "a normal start must center the loaded track: actual {}, expected {expected}",
        adjustment.value()
    );
    assert!(
        track_list.shared.column_view.has_css_class("playback-paused"),
        "the restored row must look like a paused song, not a running one"
    );
    assert_eq!(
        track_list.shared.selection.selection().size(),
        0,
        "START-1 marks and centers; it never takes the selection (NAV-10a)"
    );

    window.close();
}

#[test]
#[ignore = "requires a display; run via xvfb-run"]
fn start_1_absent_loaded_track_leaves_the_list_at_the_top() {
    let _main_context = crate::ui::test_main_context::lock_main_context();
    gtk4::init().unwrap();
    let (track_list, window) = synthetic_track_list(100);

    // A loaded id the library view does not contain — the session ended on a
    // podcast episode, or the track was removed since.
    track_list.shared.playing_track_id.set(Some(9_999));
    track_list.center_loaded_track();
    crate::ui::test_settle::settle_for(Duration::from_millis(200));

    let adjustment = track_list.shared.column_view.vadjustment().unwrap();
    assert!(
        adjustment.value().abs() < 0.5,
        "an unresolvable loaded track must leave the list at the top, actual {}",
        adjustment.value()
    );

    window.close();
}
```

Register it at the very bottom of
`crates/reprise-gnome/src/ui/track_list/current_track_selection.rs`, after the
closing brace of the existing `mod tests`:

```rust
#[cfg(test)]
#[path = "start_restore_tests.rs"]
mod start_restore_tests;
```

- [ ] **Step 2: Run the tests and watch them fail**

```bash
xvfb-run -a cargo test -p reprise-gnome start_1_loaded_track_is_centered_and_marked_paused -- --ignored --exact --test-threads=1
```
Expected: FAIL to compile — `no method named 'center_loaded_track' found for struct 'TrackList'`.

- [ ] **Step 3: Implement `center_loaded_track`**

Append to `crates/reprise-gnome/src/ui/track_list/track_list_reload.rs`, right
after `schedule_centered_scroll_refinement`:

```rust
/// START-1: centers the loaded track once the startup routing has built the
/// library view.
///
/// Called after `route_to_place`, which is the one moment nothing else owns
/// the viewport: the startup place carries no anchor
/// (`session_restore::startup_place`), so `view_state_memory`'s anchor
/// restore bails out, and an untouched list produces a no-op reload anchor.
/// A no-op when nothing is loaded or the loaded track is not part of the
/// view — the list then simply starts at the top.
pub(in crate::ui) fn center_loaded_track(shared: &Shared) {
    let Some(track_id) = shared.playing_track_id.get() else {
        return;
    };
    let current_ids = shared.current_view_ids();
    if !current_ids.contains(&track_id) {
        tracing::debug!(
            track_id,
            "startup centering skipped: loaded track is not in the library view"
        );
        return;
    }
    schedule_centered_scroll_restore(
        shared.column_view.clone(),
        Some(track_id),
        current_ids,
        SCROLL_RESTORE_MAX_ATTEMPTS,
    );
}
```

Add the pass-through to `crates/reprise-gnome/src/ui/track_list/track_list.rs`,
directly below `restore_browser_place`:

```rust
    /// START-1: centers the loaded track after the startup routing built this
    /// view. See `track_list_reload::center_loaded_track` for why this is the
    /// only scroller running at that moment.
    pub(in crate::ui) fn center_loaded_track(&self) {
        super::track_list_reload::center_loaded_track(&self.shared);
    }
```

- [ ] **Step 4: Run the display tests and watch them pass**

```bash
xvfb-run -a cargo test -p reprise-gnome start_1_loaded_track_is_centered_and_marked_paused -- --ignored --exact --test-threads=1
xvfb-run -a cargo test -p reprise-gnome start_1_absent_loaded_track_leaves_the_list_at_the_top -- --ignored --exact --test-threads=1
```
Expected: PASS, one test each. Run them **one process at a time** — this
project's display suite is unreliable when a whole batch shares a process.

- [ ] **Step 5: Call it from the startup wiring**

In `crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs`, immediately
after the `nav_history.end_back();` that follows the startup
`route_to_place(…)` call, insert:

```rust
    // START-1: the routing above owns the model; this owns the viewport.
    // Order matters — the view must exist before its rows can be centered.
    track_list.center_loaded_track();
```

- [ ] **Step 6: Rewrite START-1 in the rulebook**

In `docs/ux-rules.md`, replace the **START-1** entry with:

```markdown
- **START-1** [active] [gtk] — Normaler Start: immer die Bibliotheksansicht
  mit der gemerkten Sortierung, ohne Suchtext und ohne Facetten. Der geladene
  Track ist darin zentriert und markiert, sein Equalizer eingefroren wie bei
  einer Pause; Auswahl und Fokus bleiben unangetastet (NAV-10a). Kommt er in
  der Bibliothek nicht vor, startet die Liste oben. Wiedergabe pausiert auf
  dem letzten Track (Position wiederhergestellt), der Startup-Reconcile läuft
  still (Karte nur bei echter Arbeit).
```

- [ ] **Step 7: Run the gates and commit**

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
wc -l crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs \
      crates/reprise-gnome/src/ui/track_list/current_track_selection.rs
```
Expected: gates green; both files below 800 lines.

```bash
git add crates/reprise-gnome/src/ui/track_list/track_list_reload.rs \
        crates/reprise-gnome/src/ui/track_list/track_list.rs \
        crates/reprise-gnome/src/ui/track_list/current_track_selection.rs \
        crates/reprise-gnome/src/ui/track_list/start_restore_tests.rs \
        crates/reprise-gnome/src/ui/window/window_runtime_wiring.rs \
        docs/ux-rules.md
git commit -m "feat(start): center the loaded track in the library view"
```

---

### Task 4: Verification pass

The implementing agent's sandbox usually has no Xvfb, so the display half of
this plan is easy to leave unproven. This task exists so that never happens
silently: if any command here cannot run, say so explicitly in the handoff
rather than reporting the task as done.

**Files:** none — this task only runs and reports.

- [ ] **Step 1: Re-run every START-1 test in its own process**

```bash
for t in start_1_startup_place_is_always_the_library_root \
         start_1_session_restore_marks_without_moving_the_viewport \
         start_1_loaded_track_is_centered_and_marked_paused \
         start_1_absent_loaded_track_leaves_the_list_at_the_top; do
  echo "== $t"
  xvfb-run -a cargo test -p reprise-gnome "$t" -- --ignored --exact --test-threads=1 \
    || xvfb-run -a cargo test -p reprise-gnome "$t" -- --exact --test-threads=1
done
```
Expected: each run reports `1 passed`.

- [ ] **Step 2: Re-run the two neighbouring display tests that could regress**

```bash
xvfb-run -a cargo test -p reprise-gnome nav_10a_row_activation_marker_does_not_move_selection_or_viewport -- --ignored --exact --test-threads=1
xvfb-run -a cargo test -p reprise-gnome fil_9_filter_changes_center_the_visible_playing_track -- --ignored --exact --test-threads=1
```
Expected: `1 passed` each. These cover the reveal policy and the filter-change
centering that Task 2 touched.

- [ ] **Step 3: Headless smoke run of the real startup**

Fully isolated — every one of these variables is required; without
`XDG_DATA_HOME`/`XDG_CACHE_HOME` this writes to the user's real database:

```bash
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  REPRISE_SMOKE_SESSION_SEED=deterministic:1,2,3 \
  REPRISE_SMOKE_SESSION_REPORT=1 \
  cargo run -p reprise-gnome 2>&1 | grep -E "session restore report|current track|startup centering"
```
Expected: the log shows the session restore report and no
`startup centering skipped` line for a seeded track that exists.

- [ ] **Step 4: Report**

State plainly which of Steps 1–3 ran and what they printed. If Xvfb or
`dbus-run-session` is unavailable in the sandbox, list the unrun commands so
they can be run on the host.

---

## Follow-up (not in this plan)

The general form of this rule is *the start view follows the loaded item, not
the last visited view*: track → Music, episode → Podcasts, stream → Radio.
Podcasts and Radio need the source-list reveal from
`docs/superpowers/specs/2026-07-31-source-list-reveal-design.md`, which is
being built on its own branch. Until that lands, episode and radio listeners
start in the library. Revisit once it is merged.
