---
slug: podcast-cover-gate-review-findings
worktree: /home/marvin/Projects/reprise-podcast-cover-gate-review-findings
branch: feature/podcast-cover-gate-review-findings
phase: planned
codex_session:
created: 2026-08-29
---
# Review findings on the collapsed-podcast-group artwork gate

Follow-up to PR #739 (`302aa4be6b`, "A collapsed podcast group asks for no
artwork"), which landed on `dev` before its review phase ran. The review found
four items and the owner accepted all four. This plan is the refactor input;
it changes nothing about the original plan's measured conclusions.

The original plan is `docs/plans/the-podcast-list-asks-for-every-cover-at-once.md`,
already on `dev` with `phase: shipped`. Its "Out of scope, deliberately"
section still holds in full — none of the work below reopens any of it.

## F1 (High) — a stale `images_allowed` latches a group to no-network

`crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs`, the per-group
`artwork_allowed` cell and the `connect_expanded_notify` handler.

The handler replays an `images_allowed` value cached at build/last-rebind time
instead of recomputing it live, and then latches `requested` permanently, so a
first expansion that happens to read a stale `false` can never be retried by
collapsing and expanding again.

The reachable sequence, every link verified in the checkout:

1. The Podcasts page is materialised while the Artwork module is off, so its
   groups cache `images_allowed = false`. Collapsed groups are the common case.
2. The user navigates away. `DeferredPage` keeps the materialised
   `PodcastsView` alive but unmapped
   (`crates/reprise-gnome/src/ui/window/content_stack.rs`).
3. The user enables Artwork in Preferences.
   `PodcastsView::refresh_visible_artwork`
   (`crates/reprise-gnome/src/ui/podcasts/podcasts_artwork_refresh.rs`)
   early-returns on `!self.root.is_mapped()` — "Hidden pages stay cold", which
   is deliberate and must stay that way. The cached value is not updated.
4. The user returns to Podcasts. `DeferredPage::materialize()` is a no-op after
   the first call and the stack's visible-child handler calls only
   `materialize()`, so **no render happens**. No `render()` call site in the
   crate is wired to "page became visible"; all of them are actions (search,
   filter, sync, subscribe, connectivity, failure).
5. The user expands a group for the first time. The handler reads the stale
   `false`, rebinds every row with it, and latches. Uncached covers stay on the
   fallback icon until an unrelated action forces a full `render()`.

This breaks the invariant stated in the file the change touches
(`source_image.rs`, the `SRC-11` / `NET-1a` note): *every caller of
`SourceImage::new`/`set_url` already recomputes `images_allowed` fresh from its
own live connection on every render pass — that is already the freshest signal
the app has.* This is the first `SRC-11` call site to use a snapshot instead.

**The fix:** recompute `images_allowed` from the live connection at the moment
of expansion, the way every other `SRC-11` call site does, rather than
replaying the cached cell. Keep the `requested` latch's purpose — a row must
still submit only once per expansion — but do not let a refused-by-gate
expansion count as the one that satisfies it.

**Scope note, so the severity is not overstated in the commit message:** a disk
cache hit is shown regardless of `images_allowed` (`source_image.rs`, the
`NET-1a` / `C1` note), so what the stale gate costs is *uncached* covers, not
all covers. Pre-#739 the same staleness window existed for rows already built;
what #739 added is the latch that makes re-expansion unable to recover.

**Regression test:** a group whose cached gate value is `false` but whose live
value is `true` requests artwork on its first expansion. Run the house mutation
probe on it — restore the cached read, confirm the new test goes red, paste
that output into the summary, revert the mutation. No `cfg(test)` switch in the
production path.

## F2 (Medium) — the line that implements the fix is untested

`crates/reprise-gnome/src/ui/podcasts/source_image.rs`, the
`if should_load { image.set_urls(request, |_| {}); }` branch in
`new_with_dimensions_when`.

All four tests added by #739 substitute a fake `episode_artwork` factory. They
genuinely pin the *decision* the expander machinery produces — that part is
sound and must not be undone — but nothing calls the real
`new_with_dimensions_when(..., false)` and asserts that no queue submission
happens. A regression reverting that one line to an unconditional `set_urls`
would pass every existing test **and** the mutation probe recorded for #739,
because that probe mutates the expander layer, one level upstream.

**The fix:** a direct test on `new_with_dimensions_when(request, icon, false)`
asserting no queue submission, using the `RegistrationContext` /
measurement-accessor pattern that `source_artwork_queue.rs` already uses for
its own tests. Use `StartupTiming::Immediate` explicitly (or release the quiet
gate first) — under `AfterQuiet` the closure is deferred and the test would
pass for the wrong reason, which is precisely the failure mode being fixed.

Add the matching `should_load = true` arm too, so the test pins a difference
rather than an absence.

## F3 (Medium) — the API shape invites the regression back

`crates/reprise-gnome/src/ui/podcasts/source_image.rs` (`new_with_dimensions`
as a `should_load: true` wrapper), `podcasts_row_interaction.rs`
(`episode_thumbnail(row, images_allowed, should_load)`), and the
`EpisodeArtworkFactory` closure type in `podcasts_groups.rs`.

Two separate traps:

- `new_with_dimensions` is the shorter and more discoverable name and defaults
  to eager loading, so nothing points a future lazy-context caller at `_when`.
- `episode_thumbnail` and the factory type take two adjacent plain `bool`s. A
  transposed call compiles silently and swaps "may use the network" with
  "should load at all".

**The fix:** make the load policy explicit at the type level — an enum rather
than a `bool` — and give the two flags distinguishable types so a transposition
cannot compile. Prefer removing the eager-by-default wrapper over keeping it;
if every call site must then state its policy, that is the point. Keep the
change mechanical: no behaviour may change, and the existing tests must stay
green without being rewritten to match a new shape.

## F4 (Low) — two new `#[allow(clippy::too_many_arguments)]`

`crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs`, on
`replace_with_sync_and_artwork` and `episode_row_with_artwork`.

The file already has the house pattern for this — `EpisodeRenderContext` and
`GroupRenderContext`. Group the artwork-gate parameters into a small context
struct and drop both lint suppressions. Mechanical; no behaviour change.

## Commits

Four commits, in order: F1, F2, F3, F4. F1 and F2 carry tests; F3 and F4 are
mechanical and must not change behaviour. Keeping them separate matters here
because F1 is the only one with a behavioural claim to attribute.

## Verification

The local gate list comes from `scripts/check-merge-readiness.sh`, never
hand-assembled. Capture its exit status in a file — never read a verdict
through a pipe, since `script | tail` reports tail's status and is always 0.

## Parallelität

**No cut. One strand.** All four findings touch
`crates/reprise-gnome/src/ui/podcasts/`, and three of them touch
`source_image.rs` or `podcasts_groups.rs` directly. F3 mechanically rewrites
the signatures F1 and F2 call, so it cannot run beside them. There is no
disjoint file group to cut.

**Merge order:** n/a, single branch.
**Post-merge cross-checks:** none, no seam.
