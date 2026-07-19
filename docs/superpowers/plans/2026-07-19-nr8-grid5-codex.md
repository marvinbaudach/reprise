# Codex Handoff — NR-8 and the GRID-5 focus

Two tasks on `main` (head `1a1a40d4`). Both are already diagnosed — do not
re-derive the analysis. **Never push. No attribution footers.**

## Task 1 — NR-8: enabling the module must trigger the first fetch

The rule is in `docs/ux-rules.md` section R as `[geplant]`. A failing
acceptance test already exists:
`crates/reprise-gnome/src/ui/new_releases/popover.rs`
`nr_8_enabling_the_module_reaches_a_fetch`, currently carrying
`#[ignore = "UX NR-8 [geplant] — enabling the module must trigger the first fetch"]`.
Remove that ignore in the implementing commit and flip NR-8 to `[aktiv]`.

### The defect

`module_effect` (`popover.rs:62`) computes
`button_visible: enabled && has_releases`. With opt-in modules default-off this
creates a standing state that has no entry point: enabled, never fetched, so no
sparkle → no popover → no "Fetch now" (its only caller is `popover.rs:153`) →
no releases → no sparkle. Nothing requests a fetch at startup either;
`ArtistNewsRuntime::setup` runs at `window.rs:163` but no `ArtistNewsRequest` is
ever sent outside the popover.

### Required behaviour

- `set_enabled(true)` for `NEW_RELEASES_MODULE` starts a fetch immediately.
  The enable path is `preference_plugins.rs` (see `:17` and `:229`).
- While the module is enabled and **no fetch has ever completed**, the sparkle
  stays visible and the popover shows an empty state: "Checking for new
  releases…" while a fetch is running, "No upcoming releases from your artists"
  once a fetch completed with no results.
- **Edge 1:** a failed first fetch (offline) keeps the button visible with a
  retry empty state. It must not disappear — that would recreate "switched on,
  but gone", the very bug NR-8 fixes.
- **Edge 2:** the first-run empty state carries **no** badge dot. The badge is
  a request for attention (P-1); this is feedback.
- After the first completed run, NR-5 applies again unchanged: no releases →
  no sparkle.

"Never fetched" needs persistence — a settings key, following the
`ONBOARDING_COMPLETED_KEY` pattern in `library/settings.rs:17` with its
`get`/`set` at `:96`/`:100`. Deriving it from row count does not work: a
successful fetch that finds nothing is indistinguishable from never fetching.

Translate the new strings into German in the same commit; `po/de.po` must stay
free of untranslated and fuzzy entries. Use the ellipsis character `…`, not
three dots — a recent commit had to normalise exactly that.

Commit: `feat(new-releases): enabling the module triggers the first fetch (NR-8)`

## Task 2 — GRID-5: the reveal scrolls but never focuses

`grid_5_reveal_scrolls_to_playing_album` has now failed three implementations.
The test carries a diagnostic assertion that splits the two conditions, and the
measured result is:

```
scrolled=true focused=false (adjustment value=8932 upper=9362 page=430)
```

**The scrolling works.** The previous fix (position through the adjustment,
then focus on the next main-loop turn) solved that half. What remains is only
that `grid.focus_child()` stays `None` — the focus never lands on the realized
tile.

Do not touch the scrolling path. Find why the focus does not land: the item is
positioned by setting the adjustment directly rather than via `scroll_to`, so
GtkGridView may never realize or focus it on its own; the fixture starts from a
deliberately focused player-bar button (`album_view.rs`, "fixture starts from a
focused player surface"), so focus has to be moved away from a live widget.

Keep the split diagnostic assertion — it is what made this tractable.

Commit: `fix(album-grid): focus the revealed tile (GRID-5)`

## Gates before every commit

- `cargo fmt --check`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
- `scripts/check-ux-traceability.sh`, `scripts/check-architecture.sh`
- Display tests one process each:
  `xvfb-run -a dbus-run-session -- scripts/check-display-tests.sh`
- Source files under 800 lines, UI orchestrators under 600.

## Standing rules

- Surfaces, geometry and reachability are verified **by result, not by
  declaration**. A test that establishes the target state and then asserts does
  not count where the plan asks for the path from the initial state — that is
  now a process rule in `docs/ux-rules.md` ("Erreichbarkeit").
- If a premise here turns out wrong, STOP, write `.codex-blocked.md` with the
  exact error, and end the run. Do not improvise a different design.
- UI copy English; `docs/ux-rules.md` and the ledger German; commits English.
