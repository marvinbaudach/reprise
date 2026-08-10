---
slug: queue-section-preseed
worktree: /home/marvin/Projects/reprise/.worktrees/list-geometry-service
branch: feat/list-geometry-service
phase: implemented-awaiting-display
codex_session:
created: 2026-08-10
---
# Pre-seed the sectioned Queue

Follow-up to `docs/plans/list-geometry-service.md`, decision 5 / task M4.

## Goal

Returning to the Queue must receive the same atomic adjustment-range pre-seed
as an unsectioned list. That requires uniform section headers and a header
height that is known before GTK allocates the restored view.

## Display measurements that motivated the work

Measured on 2026-08-10, not inferred:

```text
HEADERPROBE immediate ([0, 0], false, None)
HEADERPROBE settled   ([20, 34], false, None)

QUEUEPROBE headers=["Play Next", "Now Playing"] rows=2276 row_h=34.0
           expected=37480 final=37454 samples(n=71 first=507 min=507 max=37454)
```

The immediate header widgets are bound but unallocated. After layout, the
plain `gtk4::Label` header is 20 px while the Play Next `gtk4::Box` is 34 px
because it contains the real flat Clear button. Consequently the live header
measurement is unavailable in the pre-seed window and non-uniform afterward.
The sampled 507 px value is the old view's `upper - page`: the Queue visibly
visits the top before climbing to its anchor.

## Decisions

### B1. Grow the plain header; do not shrink the button row

`SECTION_HEADER_MIN_HEIGHT` is a floor above the measured 34 px natural button
row. The plain label and Play Next box receive
`queue-section-header-row`; the existing `queue-section-header` label class is
retained for text styling and test discovery. The Clear button keeps its
natural padding and real `GtkButton` hit target.

If a theme or large system font grows one variant beyond the token, the
measurement becomes non-uniform. The service then writes nothing and retains
the last good value. This is degradation, not corruption.

### B2. Persist header height per density

The keys are:

- `ui.section_header_height.comfortable`
- `ui.section_header_height.standard`
- `ui.section_header_height.compact`

They use the row-height encoding: `0` means unloaded, `-1` invalidated, and a
negative usable value means `Assumed`. `TrackList::apply_list_density` remains
the single invalidation entry point. `RowHeight` is reused because both values
are positive pixel heights.

The pure formula remains `rows * row_height + sections * header_height`; only
the source of `header_height` changes from a pre-layout live measurement to the
cache.

### B3. Trust only a complete settled measurement

All three conditions must hold:

1. Every accepted `ListHeader` height is at least
   `SECTION_HEADER_MIN_HEIGHT`; zero and partial allocations are filtered out.
2. All accepted header heights are uniform.
3. `settled_content_row_height` also closes the complete adjustment equation.

Rows and headers are then persisted in one database transaction and promoted
to measured cache entries together. A non-uniform reading never clears a prior
good value.

### B4. Cold start uses an assumed lower bound

Without persistence, both CSS tokens are `Assumed`, never measured truth. If
either the row or header source is assumed, the complete content height is
assumed. `preseed_upper` may grow an undersized range to that lower bound but
must never shrink an already larger live range.

This does not promise an exact first-ever restore from an empty database. It
does ensure that the estimate cannot make the old behavior worse; a settled
visit replaces it with measured values.

### B5. Settled measurement remains live

`settled_content_row_height` and `is_settled` continue to inspect allocated
widgets. Only `ListGeometry::content_height` reads the cache first. There is
one cache-reading seam for pre-seed and one live-writing seam after layout.

### B6. File limits are binding

Geometry settings live in `settings_geometry.rs`; header filtering/cache logic
lives in `list_geometry_header.rs`; `Shared.last_row_height` is replaced 1:1 by
one `ListGeometryCache`. Cohesive tests and TrackList adapters may be extracted
when required to keep Rust files below 800 lines and `track_list.rs` below 600.

### B7. Strengthen QUE-1

Uniform height belongs to the existing Queue section surface, so QUE-1 gains a
sentence and the display proof is named
`que_1_queue_section_headers_share_one_height`. No new rule ID is introduced.

## Tasks and results

### T0. Blocking probe

Temporary `section_header_report` and call-site instrumentation compiled
against the exact acceptance-test target, then was fully reverted. This
sandbox has no X server and display execution was explicitly excluded, so T0
could not produce positive runtime evidence that an existing call sees
allocated headers before return pre-seed.

The implementation therefore took the fail-closed branch: T3 includes an
explicit Queue-wide post-layout adjustment subscription, independent of anchor
restore. An early `configure()` emission re-arms the subscription until a
settled allocation is observed.

### T1. Uniform section headers

- Add the authored floor token and feature-owned CSS.
- Apply the shared class to the actual child passed to `ListHeader::set_child`
  in both branches.
- Add the ignored QUE-1 display proof that gathers allocated `ListHeader`
  heights, requires at least two positive values, and requires uniformity.

The token is 36 px: the smallest integer above the supplied 34 px natural
button-row measurement. Re-measurement under the real display remains required.

### T2. Persist section-header geometry

- Extract row-height settings into `settings_geometry.rs` without changing the
  public call paths.
- Add validated density-keyed header settings.
- Add GTK-free floor, cache, persistence, invalidation, and fail-closed tests.

### T3. Switch the geometry service

- Replace the single Shared cell with `ListGeometryCache`.
- Persist settled row/header pairs transactionally and cache them together.
- Use cached heights or assumed token floors for content height.
- Let the weaker source control pre-seed behavior.
- Invalidate both values through the existing density entry point.
- Schedule Queue header measurement after layout for every Queue model swap;
  re-arm after an early pre-layout adjustment change.
- Emit `QUEUEPROBE preseed header_source=...` in test builds so display
  acceptance distinguishes measured cache use from an assumed fallback.

### T4. Acceptance and rules

- Extend QUE-1 and update active ownership.
- Compile/list every display proof without executing it in this sandbox.
- Run all non-display gates and compare the complete suite count with the
  pre-change baseline.
- Leave real Xvfb acceptance, counterprobe, and screenshot verification
  explicitly outstanding.

## Required display acceptance

Run each test in a separate process with fresh XDG roots, a private D-Bus
session, `GDK_BACKEND=x11`, unset `WAYLAND_DISPLAY`, and `fakesink`, following
`scripts/check-display-tests.sh`.

1. This exact test must execute once and pass:

   ```text
   ui::track_list::track_list_reload::queue_section_geometry_display_tests::nav_back_to_a_large_sectioned_queue_never_visits_the_top
   ```

2. The same test with `REPRISE_NO_PRESEED=1` must execute once and fail, with a
   sample minimum near 507. A green counterprobe means the positive test is
   insensitive and proves nothing.

3. This exact test must execute once and pass:

   ```text
   ui::track_list::queue_section_header_display_tests::que_1_queue_section_headers_share_one_height
   ```

   Its output must show `HEADERPROBE settled ([X, X], true, Some(X))`. The
   Queue acceptance log must show a measured header source at return pre-seed.

4. Run the 12 named viewport display tests individually, never as a flaky
   group. The selection patterns are:

   ```text
   navback_anchor_display_tests
   queue_section_geometry_display_tests
   delete_follow_display_tests
   reveal_track_display_tests
   search_viewport_display_tests
   track_list_reload_display_tests
   delete_tracks_large_block_display_tests
   context_menu_scroll_display_tests
   ```

5. Capture before/after Queue screenshots. Tests alone do not approve the
   visible increase in every plain section-header row.

After every Xvfb run, execute `xvfb-orphan-gc --apply`.

## Non-display acceptance

- `cargo fmt --check`
- `cargo clippy --all-targets --workspace -- -D warnings`
- isolated `cargo test --workspace`, with the suite count compared to the
  pre-change baseline
- Core dependency purity after Core edits
- `cargo audit` (only accepted advisory: RUSTSEC-2024-0436)
- `scripts/check-architecture.sh`
- `scripts/check-ux-traceability.sh`
- `scripts/ci-quality.sh`

## Remaining risks and manual checks

- A truly cold, immediate Queue restore with no persisted geometry still has
  only a conservative lower bound. A dedicated display test may be warranted.
- The 36 px token is based on the supplied 34 px measurement; uniform rendered
  height under the actual theme is unverified until the display probe runs.
- The visible header growth requires screenshot review.

## Verification record — 2026-08-10 Codex sandbox

Passed:

- pre-change and final isolated workspace runs both reported 60 suites and no
  failed suite;
- `cargo fmt --check`;
- strict all-target workspace Clippy;
- Core dependency purity;
- cached `cargo audit`, with only accepted RUSTSEC-2024-0436;
- all edited Rust files are below their limits: `list_geometry.rs` 778,
  `track_list_reload.rs` 778, `track_list.rs` 598/600;
- the 12 named viewport display tests and the new QUE-1 display test compile
  and are discoverable at their exact paths.

Not executed in this no-X-server sandbox:

- the sectioned-Queue acceptance test;
- its required `REPRISE_NO_PRESEED=1` failing counterprobe;
- the uniform-header display test;
- the 12 individual viewport display runs;
- the before/after screenshot review.

Existing unrelated gate failures remain:

- `scripts/check-architecture.sh` stops on the unchanged 803-line
  `crates/reprise-core/src/library/session.rs`;
- `scripts/check-ux-traceability.sh` stops on existing release tests that still
  reference replaced NR-20 and NR-25; it reports no QUE-1 gap;
- `scripts/ci-quality.sh` was not run because it necessarily executes display
  suites and also requires a clean, up-to-date integration worktree.
