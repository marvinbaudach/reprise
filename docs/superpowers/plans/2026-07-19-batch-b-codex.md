# Codex Handoff — UI-Politur Batch B

Implements the Batch B scope of
`docs/superpowers/plans/2026-07-18-ui-polish-beschluesse.md` on
`feat/sidebar-visual-improvements` (base: your Batch A commits plus `80bf0f50`).
Section **U** already exists in `docs/ux-rules.md` with NAV-10, QUE-7, QUE-8 and
NPP-11 sitting at `[geplant]` — flip them in their implementing commits.

**Never push. No attribution footers.** Tasks strictly in order; TDD; the exact
test names below (the traceability gate matches on them).

## Standing rule for this branch

Surfaces, geometry and anything the user can see are verified **by result, not
by declaration** (STYLE-1, section S). A test asserting that a CSS class was
added or a setter was called does not count where the plan asks for a measured
outcome. Batch A shipped two such tests that stayed green while the widget was
invisible — do not add a third.

## B1 · Remove the dead queue button

The player-bar queue button is a no-op in the common case: `player_bar.rs:568`
→ `window_runtime_wiring.rs:166` → `NowPlaying::show_up_next()`, and
`panel_state.rs:32` `up_next_route_state()` returns a hardcoded
`(true, PanelTab::UpNext)`. It can only ever *show*, so when the panel is
already open on Up Next — the normal state — clicking does nothing. Its only
visible feedback is taking focus, which reads as "responded" while nothing
happened. The queue is reachable from the sidebar (ColumnView) and the panel
toggle, so the control is redundant rather than broken.

Remove: `queue_button` in `player_bar_layout.rs` (widget, tooltip, icon
constant, end-zone append), `connect_queue_clicked` in `player_bar.rs:566-569`,
its caller in `window_runtime_wiring.rs:166-168`, `TOOLTIP_QUEUE` and its
`po/de.po` entry. **Check whether `show_up_next()` has any other caller** — if
not, remove it too; that also removes the latent bug that it never syncs
`self.toggle` (unlike `apply_persisted_visibility`, `now_playing.rs:306`). If it
does have another caller, keep it and add the missing toggle sync.

Rewrite QUE-1 in `docs/ux-rules.md`: the player-bar icon no longer opens the
panel.

Test: `que_1_player_bar_has_no_queue_button`.
Commit: `feat(player-bar): drop the redundant queue button (QUE-1)`

## B2 · QUE-7 — virtual context tail

Today `window.rs:206` feeds the sidebar counter from
`queue_pending_len()`, so playing from the library badges "Queue 1,638" next to
"Music 1,663" — the queue reads as a second name for the library.

- Up Next = manual queue + a **virtual** context tail. The tail is never
  materialised as individual rows; it is a named section header with a count
  ("Playing from Music · 1,663 tracks"). Only the visible window renders
  (QUE-6).
- The sidebar "Queue" row counts **only the manual queue**. At zero it shows
  "Queue" with no number.

Tests: `que_7_sidebar_counts_only_the_manual_queue`,
`que_7_context_tail_is_not_materialised` (assert the model does not allocate a
row per context track).
Flip: **QUE-7 → [aktiv]**.
Commit: `feat(queue): keep the context tail virtual (QUE-7)`

## B3 · QUE-8 — drag reorder inside "Next in Queue"

Panel verbs stay light (jump, remove, reorder the manual section); the
ColumnView keeps the heavy ones (multi-select, clear, save-as-playlist, context
menu). Drop targets exist **only** in "Next in Queue". "Continuing" is not
reorderable; dragging an entry from it upwards means "play earlier" and
materialises exactly that one entry into the manual section. Needs drop targets
plus autoscroll.

Tests: `que_8_reorder_only_within_the_manual_section`,
`que_8_drag_from_continuing_materialises_one_entry`.
Flip: **QUE-8 → [aktiv]**.
Commit: `feat(queue): drag reorder within the manual section (QUE-8)`

## B4 · NAV-5 — per-view memory, anchored by ID

NAV-5 is still `[geplant]` (`ux-rules.md:113`) and **nothing implements it**.
NAV-10 depends on it: without a remembered position every view entry is a first
entry, and NAV-10 degenerates into the hard auto-follow its own rationale
forbids.

Remember scroll + selection per view for the session. The scroll anchor is a
**track/album ID plus offset, not a pixel value**, so re-sort and insert keep
the position. No `scrollIntoView`.

Tests: `nav_5_remembers_scroll_and_selection_per_view`,
`nav_5_anchor_survives_resort`.
Flip: **NAV-5 → [aktiv]**.
Commit: `feat(nav): per-view scroll and selection memory anchored by id (NAV-5)`

## B5 · Extract the shared playing marker

GRID-1 calls it "das gemeinsame EQ-Badge", but there is no shared component:
the only implementation lives in `album_card` (see
`album_card_tests.rs:grid_1_playing_badge_persists_without_hover`), and the
player bar's mini-EQ is a second, independent path. "One marking language"
(ALB-2) needs an **extraction**, not a third copy.

Extract one reusable playing-marker widget (EQ glyph + accent treatment, using
the **playback** accent role per STYLE-3) and make album_card consume it.

Test: `marker_1_single_implementation_serves_grid_and_bar`.
Commit: `refactor(ui): extract the shared playing marker`

## B6 · Mark the playing element in the remaining views

Artists (ART-1 is `[geplant]`) and playlists have no playing marker at all.
Apply the extracted marker so every view shows the running track/album/artist
persistently, independent of hover and focus.

Tests: `nav_10_playing_marked_in_all_views`.
Flip: **ART-1 → [aktiv]**.
Commit: `feat(artists,playlists): persistent playing marker (ART-1)`

## B7 · NAV-10 — cross-view context anchor

Three parts: persistent marking (B5/B6); auto-scroll onto the running context
**only on the first entry** into a view in the session, later switches restore
NAV-5's remembered position without a yank; explicit reveal (now-playing
cover/title, "Go to album/artist") always jumps deterministically. Selection
never follows playback; a clicked non-playing track's context is reachable only
via "Go to album/artist". Playing marker and selection highlight stay separate
treatments.

Note GRID-5 already implements the album direction of the explicit reveal — but
see B8, its display test currently fails.

Tests: `nav_10_first_entry_lands_on_playing_context`,
`nav_10_subsequent_switch_restores_remembered_position`,
`nav_10_reveal_always_jumps`.
Flip: **NAV-10 → [aktiv]**.
Commit: `feat(nav): cross-view context anchor (NAV-10)`

## B8 · GRID-5 — decide feature or test, then fix the right one

`grid_5_reveal_scrolls_to_playing_album` fails on clean `main@b0965905` too, so
it predates Batch A. It has now failed in **two** different test formulations —
the original and your rewritten wait-loop version, which still fails at
`album_view.rs:517`: within 500 ms the adjustment does not move past 0, or the
grid gets no `focus_child`.

Two failed test rewrites make "the test is wrong" the less likely explanation.
Determine whether the reveal actually works in a running app. **If the feature
is broken, fix the feature** — GRID-5 standing at `[aktiv]` while its display
test fails is not an acceptable end state. State in the commit body which side
was wrong and what the evidence was.

Reproduce:
`xvfb-run -a dbus-run-session -- cargo test --locked -p reprise-gnome grid_5_reveal_scrolls_to_playing_album -- --ignored`

Commit: `fix(album-grid): GRID-5 reveal` (adjust to whichever side you fixed)

## B9 · NPP-11 — centred view switcher

Move the view tabs to a centred `AdwViewSwitcher` title widget with adaptive
degradation: `AdwViewSwitcherBar` at the bottom, or `AdwInlineViewSwitcher`
collapsing to icons-only, driven by `AdwBreakpoint`. This reverses the earlier
left-aligned decision; the reason for it (a rigid centre widget reserves
`2×max(left, right)` and squeezes narrow windows) is neutralised because search
now lives in its own bar below the header and the switcher itself squeezes.

Tests: `npp_11_switcher_is_centred_when_wide`,
`npp_11_switcher_degrades_when_narrow`.
Flip: **NPP-11 → [aktiv]**.
Commit: `feat(shell): centred adaptive view switcher (NPP-11)`

## Gates before every commit

- `cargo fmt --check`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
- `scripts/check-ux-traceability.sh`, `scripts/check-architecture.sh`
- Display tests one process each:
  `xvfb-run -a dbus-run-session -- scripts/check-display-tests.sh`. If the
  sandbox blocks it, report them pending rather than faking a green.
- Translate new UI strings in the same commit; `po/de.po` free of untranslated
  and fuzzy entries. Never mark glyphs with `N_!`.
- Source files under 800 lines, UI orchestrators under 600.

## Known non-failures

`nav_9a_ctrl_l_reveals_current_track_origin` fails in a full workspace run and
passes alone — GTK display tests need one process each. Do not "fix" it.

## Policy

If a premise here turns out wrong, STOP, write `.codex-blocked.md` with the
exact error, and end the run. Do not improvise a different design. UI copy
English; `docs/ux-rules.md` and the ledger German; commit messages English.
