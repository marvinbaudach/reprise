---
slug: the-band-the-window-does-not-see
worktree: /home/marvin/Projects/reprise-the-band-the-window-does-not-see
branch: feature/the-band-the-window-does-not-see
phase: shipped
codex_session:
created: 2026-09-16
---
# The band the window does not see

Successor of `the-real-window-passes-the-ux-audit` (A #970, B #971). Input:
`HANDOFF-2026-09-16-the-band-the-window-does-not-see.md` (the tour, the
numbers, the reproduction recipe). Base: `origin/dev` (`772f807563` at the
end of the plan phase; nothing under `ui/sidebar`, `ui/window`, `ui/scan`,
`ui/issues`, `ui/library_doctor` or `docs/ux-rules.md` changed since the
handoff's read of `1f808182f1`). Single strand; the cause was measured in
the plan phase, so the tasks below are concrete.

## The defect

Headless tour, dev = release build of `1f808182f1`, pinned block visible
(two seeded `import_errors` rows, the start-up cover check's card revealed):
162 px of empty sidebar under the block at 1280×720, 42 px at 1024×600, the
block's painted bottom at y = 465 in both; the band stays after the cover
card has gone. Block hidden: the navigation scroller fills to the player bar.

## The cause, measured (plan phase, 2026-09-16)

An in-process probe on the composed window (real `import_errors` row,
`Sidebar::refresh` after the map, 1280×720, gate environment) reproduced it
and the widget dump names the owner — shape **F4**: the root box fills the
page (h = 586), the pinned scroller is docked at the bottom (bottom = 633 =
page bottom), the navigation scroller has real surplus (319 px, not its
232-px floor) — and inside `progress_root` (y 447–633, 186 px) the two
production job-card revealers that were never revealed are **visible with
an unrevealed child**:

```
activity slot child #2: GtkRevealer [sidebar-job-card-dock] h=85 visible=true  child_visible=true  mapped=true
  child #0: GtkBox [scan-card]                              h=85 visible=true  child_visible=false mapped=false
activity slot child #3: GtkRevealer [sidebar-job-card-dock] h=85 visible=true  child_visible=true  mapped=true
  child #0: GtkBox [scan-card]                              h=85 visible=true  child_visible=false mapped=false
```

Two crossfade revealers, 170 of the 186 px, painting nothing. Before the
refresh (block hidden) both were `visible=false`. The scan card (child #1)
stays hidden correctly.

**Mechanism.** `sync_revealer_visibility`
(`sidebar/sidebar_activity_slot.rs:118-128`) decides
`should_be_visible = reveals_child || is_child_revealed` and then writes the
flag only when it differs from **`is_visible()`** — which is
`gtk_widget_is_visible`, *ancestor-aware*: false whenever any ancestor is
hidden. The relink card (`window.rs:224`) and the doctor card
(`library_doctor/mod.rs:216`) are docked while the pinned scroller is still
hidden (`build_scrollable_issues_section` hides it until the listbox or a
card is visible), so at that moment `is_visible()` is already false,
`false != false` skips the write, and both the revealer and its child keep
GTK's default `visible=true`. When `sidebar_rebuild.rs:342` turns the
listbox on and `sync_visibility` shows the scroller, the two revealers
become visible with their full crossfade height — a crossfade revealer keeps
its child's size while unrevealed. Nothing ever notifies them again unless
they are revealed, so the band stays. The scan card is the control arm:
`scan/scan_progress.rs:240` sets `revealer.set_visible(false)` at
construction, so its own flag is already right.

**Why every test was green.** `fb_8_the_real_sidebar_leaves_no_band_under_the_pinned_block`
seeds `issues_listbox.set_visible(true)` *before* it docks its cards (the
scroller is visible, the sync writes), and its `diagnostic_job_card` uses
`RevealerTransitionType::None`, which is 0 px unrevealed anyway. The fixture
`fb_8_progress_region_reaches_split_view_bottom` pins its own height with
`set_size_request(240, 470)`. `fb_15_…` measures the minimum.

**A second finding, in the scaffold.** `build_real_window` ends with
`set_size_request(MIN_WIDTH, MIN_HEIGHT)` (`real_window_tests.rs`, last
lines). Without a window manager that shrink is honoured on the next
main-loop iteration: any test that settles after construction silently
measures ~640×550, not the size it asked for. The four existing tests read
their allocations without another iteration and so never noticed. T1 must
re-pin the size after seeding and assert it.

## Decisions (binding)

- **D1 — The fix is the mechanism above, at its source:** the visibility
  sync compares against the widget's *own* flag, not the ancestor-aware
  one. No `queue_*` sprinkling, no per-card `set_visible(false)` copies.
- **D2 — One instrument.** The plan-phase `sidebar_report` (already in the
  worktree, see T0) is the report every assertion attaches; `chain_report`
  stays.
- **D3 — The regression test seeds the block the production way, after the
  map:** an `import_errors` row and `Sidebar::refresh` (the `rebuild` path
  through `sidebar_rebuild.rs:342`), the cards docked while the scroller is
  hidden — exactly the start-up order. The existing pre-map test stays
  untouched as the control arm.
- **D4 — FB-8 is amended, no new rule.** FB-15 and A's minimum-height work
  are not touched (`LIBRARY_BLOCK_MIN_HEIGHT`, the pinned scroller's
  scrolling below the LIBRARY floor, `MIN_HEIGHT`); `fb_15_…` stays green.
- **D5 — Display tests run one process each** (the exact invocation in T4
  or `scripts/check-display-tests.sh`); never a filter bundle.
- **D6 — Builds stay `-p reprise-gnome`.** The tour re-run needs a release
  build and the real DB — post-landing, user-authorised.
- **D7 — Assertions on allocations are exact; only the painted gap carries
  a tolerance of one issue row.** One size (1280×720), two states (card
  revealed; card gone, Import Errors row only).

## Tasks (in this order — each its own commit)

**T0 — The instrument (already in the worktree, uncommitted).** The
plan-phase probe left `window/real_window_tests.rs` with
`build_real_window_with_db` (returns the `Rc<Db>` so a test can seed rows
after the map; `build_real_window` is its thin wrapper), `sidebar_report`
(ancestor chain window → root box, the root box's subtree, the player bar,
`gap_raw`/`gap_painted` lines) and the probe test
`layout_probe_the_pinned_block_after_the_map`. Review it, keep
`build_real_window_with_db` and `sidebar_report`, **delete the probe test
and its rung-only helpers** (T2 replaces it; the rung reports are in this
plan), keep `chain_report` for the existing tests, and commit as T0. No
production code changes in this commit.

**T1 — The regression test**
`fb_8_a_card_docked_behind_a_hidden_block_reserves_no_height` in
`real_window_tests.rs`, `#[ignore = "requires a display; run via xvfb-run"]`:

1. `build_real_window_with_db(1280, 720, SidebarSeed::default())` — block
   hidden. Immediately re-pin the size: `set_size_request(1290, 730)`,
   `settle_until(5 s, || window.height() == 720 && window.width() == 1280)`,
   assert it (this is the scaffold finding above).
2. Dock two production cards **while the block is hidden**, the way
   `window.rs:223-224` does: `ScanProgressView::new()` via
   `append_scan_card` and the relink view's revealer (build the real one:
   `issues/missing_progress.rs`'s view, `widget()` → `&gtk4::Revealer`; if
   its constructor needs context the test cannot supply, the doctor card
   from `library_doctor/progress_card.rs` is the second) via
   `append_relink_card`/`append_doctor_card`. Do **not** reveal them.
   Assert the block is still hidden.
3. Insert one `import_errors` row (`INSERT INTO import_errors(path,
   reason_kind, reason_detail, first_seen, last_seen)`), then
   `handles.sidebar.refresh("import errors found")`,
   `settle_until(5 s, || issues_listbox.is_visible())`, `settle_for(200 ms)`.
4. **State 1 — a card revealed:** `set_reveal_child(true)` on the scan card,
   `settle_until(child_revealed)`, `settle_for(300 ms)`. Assert.
5. **State 2 — the card gone:** `set_reveal_child(false)`,
   `settle_until(!child_revealed)`, `settle_for(300 ms)`. Assert again.

Assertions per state, each with `sidebar_report` attached:

- `root.height == sidebar_page.height`;
- `pinned.bottom == sidebar_page.bottom`;
- `pinned.height == pinned.measure(Vertical, pinned.width).natural`;
- `region.bottom == pinned.bottom`;
- `navigation_scroller.height == sidebar_page.height − pinned.height`;
- **every revealer in `progress_root` that is not revealing is
  `get_visible() == false`, and `progress_root.height` equals the sum of its
  visible children's heights** — this is the line that is red today;
- `gap_painted ≤ issues_listbox.row_at_index(0).height()`.

**Run it before T2 and paste the failing assertion plus the two revealer
lines of the report into T1's commit message** — the mutation proof. A T1
that is green before T2 does not reproduce and is wrong.

**T2 — The fix.** In `sidebar/sidebar_activity_slot.rs`,
`sync_revealer_visibility`: compare against the widget's own flag —
`child.get_visible()` and `revealer.get_visible()` (`gtk_widget_get_visible`)
instead of `is_visible()` — so the write happens whether or not an ancestor
is currently hidden. Update the doc comment above `track_progress_visibility`
to name the trap (ancestor-aware `is_visible` made the initial sync a no-op
for cards docked behind the hidden block). Leave
`scan_progress.rs:240` as it is (harmless, and the comment there already
says so). Nothing else changes in production code. T1 is green after this
commit; every other display test in T5 stays green.

**T3 — Rulebook.** Amend **FB-8** in `docs/ux-rules.md` (no new rule) with
an `*Amended 2026-09-16.*` paragraph: the block's bottom edge sits above the
player bar *at every moment it is visible, including when it appears after
the window is already laid out* — the normal case, since the start-up scan
reports import errors and job cards reveal seconds after the map. On
`1f808182f1` the relink and Library Doctor cards, docked while the block was
still hidden, kept GTK's default `visible` because the sync compared against
the ancestor-aware `is_visible`; when the block appeared they reserved
170 px of crossfade height without painting (the headless tour of
2026-09-16: 162 px at 1280×720). The sync now writes the widget's own flag.
Tests: `fb_8_the_real_sidebar_leaves_no_band_under_the_pinned_block` (cards
docked into a visible block — the control) and
`fb_8_a_card_docked_behind_a_hidden_block_reserves_no_height` (docked behind
the hidden block, appearing after the map). `scripts/check-ux-traceability.sh`
must pass.

**T4 — Verification, all inside the worktree.**

- `cargo test -p reprise-gnome` (non-display suite) and
  `cargo clippy -p reprise-gnome --all-targets` clean.
- **One process each**, under the gate's environment — fresh
  `XDG_DATA_HOME/XDG_CACHE_HOME/XDG_CONFIG_HOME/XDG_RUNTIME_DIR`,
  `GIO_USE_VFS=local GTK_USE_PORTAL=0 GSK_RENDERER=cairo GDK_BACKEND=x11
  WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink dbus-run-session -- xvfb-run
  -a cargo test -p reprise-gnome -- --ignored --exact --nocapture <path>`
  (`scripts/check-display-tests.sh:269-277` is the reference): the new T1
  test; every `#[ignore]` test in `real_window_tests.rs`;
  `fb_8_progress_region_reaches_split_view_bottom`;
  `fb_8_a_finished_scan_card_occupies_no_space_again`; the `#[ignore]`
  tests of `sidebar_layout_tests.rs` and `sidebar_tests.rs`;
  `set_19_pages_scroll_inside_a_short_window`. Read each verdict from the
  process exit status, never through a pipe.
- Mutation proof after the fix: put `is_visible()` back in
  `sync_revealer_visibility` (both occurrences — count them), run T1 → red,
  restore, run T1 → green.
- `scripts/check-ux-traceability.sh` green.

## Out of scope — do not fold in

- `AdwBreakpointBin width: requested 1170 px, 1014 px available` at
  1024×600 — separate finding.
- `writer_is_free_during_every_trash_callback` (`reprise-android-ffi`) —
  its own plan.
- The scaffold's trailing `set_size_request(MIN_WIDTH, MIN_HEIGHT)` in
  `build_real_window` stays (fb_15 relies on the minimum); T1 re-pins
  explicitly. Reworking the scaffold is a later cleanup.

## Post-landing (user-authorised — needs a release build and the real DB)

`docs/plans/HANDOFF-2026-09-16-tour-run-arm.sh dev <release binary> 1280 720
"" issues` (and 1024×600) under `heavy-run light`: gap ≤ one row with the
block visible, also after the cover card has gone; control = the pre-fix dev
build; `exceeds AdwApplicationWindow height` still 0. The new test's first
CI execution is on `dev` after the merge (display shards are skipped on
PRs) — watch that run.

## Parallelität

**One strand — no cut.** T0–T2 all touch `window/real_window_tests.rs` and
`sidebar/sidebar_activity_slot.rs`; T1 cannot go green before T2 in any
other branch; T3 (`docs/ux-rules.md`) is a ten-line edit whose text names
T1's test and T2's cause. Merge order and post-merge cross-checks reduce to
the post-landing tour above.
