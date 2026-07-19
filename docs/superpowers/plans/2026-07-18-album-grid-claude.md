# Claude Code Handoff — Album-Grid Playing, Keyboard, Overlay, Reveal

Implement the complete current stage described by
`docs/superpowers/plans/2026-07-18-album-grid-taskplan.md`. The binding product
decisions and edge cases are in
`docs/superpowers/plans/2026-07-18-album-grid-beschluesse.md` (German). Read
both completely; do not substitute the screenshot or this summary for them.

## Repository and scope

- Worktree: `/home/marvin/Projects/reprise/.worktrees/album-view-improvements`
- Branch: `feat/album-view-improvements`
- Planned base: `main@e0493d0` (also `origin/main` when the plan was written).
  Re-read live Git state; commits and the progress ledger are ground truth.
- This stage ends after T7 plus its progress entry. Do not begin `ALB-2` or
  another roadmap stage. Never push.

Before editing, completely read `AGENTS.md`, `TESTING.md`, the tail of
`.superpowers/sdd/progress.md`, both album-grid plan documents,
`docs/ux-rules.md` sections C/D/M and the navigation rules, and recent Git
history. Claim the repository lock required by the workflow. Preserve all
unrelated user changes.

The old parallel-lane block at the end of `AGENTS.md` is stale on this base:
the two lanes named by it were merged as `c785a32` and `eddd0f7`, satisfying
the block's own removal condition. T1 removes only that expired block before
touching `docs/ux-rules.md`.

## Autonomous execution contract

1. Execute **T1 → T7 strictly in order**. Continue automatically until the
   whole stage is complete. Do not stop for routine choices, compiler/test
   failures, refactors needed for the 800-line rule, or manual review between
   tasks.
2. For each behavior task, perform real TDD: write the failing test, run it
   and observe the intended failure, implement the smallest correct change,
   then run the targeted test green. Never write implementation and test in
   one unobserved batch.
3. Use the exact rule-test names from the plan:
   - `grid_1_playing_badge_persists_without_hover`
   - `grid_2_enter_opens_detail_ctrl_enter_plays`
   - `grid_2_space_is_global_playpause_not_album`
   - `grid_3_focus_ring_and_overlay_on_focus`
   - `grid_4_hover_uses_bottom_gradient_not_tooltip_box`
   - `grid_5_reveal_scrolls_to_playing_album`
   - `nav_9a_ctrl_l_reveals_current_track_origin`
4. Flip a UX rule from `[geplant]` to `[aktiv]` only in the implementation
   commit named by the plan and only after its rule-named test passes. Rule
   IDs are append-only. `ALB-1` and `NAV-9` become replacement records;
   `ALB-2` remains untouched.
5. Run every repository gate before every commit, including `cargo audit`, UX
   traceability, architecture, file-size checks and core purity where
   applicable. Only RUSTSEC-2024-0436 is accepted. Use the fully isolated
   display-test command from the taskplan; never run the app or GTK smoke
   checks on the user's live desktop/session bus/database.
6. Adversarially review each task diff against the plan and decisions before
   committing. Fix findings and rerun affected tests/gates.
7. Use every task commit message exactly as specified. No attribution
   footers. After T7, perform the full-branch adversarial review, make only
   concrete `fix(album-grid): ...` commits if findings exist, append the
   stage summary to the progress ledger, commit it with the specified message,
   release the lock, and verify a clean tree.

## Non-negotiable semantics

- Playing is persistent without hover: shared EQ top-left plus inner cover
  ring. Paused freezes EQ but keeps ring; reduced motion is static.
- Focus is the real GtkGridView child focus, not a second focusable card.
  Focus ring is outside the cover; playing ring is inside; overlay is another
  layer. All can coexist.
- The overlay button is pointer-only. Current playing album toggles pause,
  current paused album resumes without queue rebuild; other/stopped rebuilds
  canonical album queue. `Ctrl+Enter` and menu Play always rebuild/start at
  track 1. Space always remains global.
- Canonical order is schema v12 `disc_no`: NULL disc behaves as 1, then track
  number with NULL last, then stable path/id. Do not add a visible disc field
  and never scan real music.
- Album context menu is exactly five entries: Play, Play next, Add to queue,
  Go to artist, Edit tags.... Both pointer and keyboard use the same model and
  target identity. Edit tags uses the existing batch editor for present album
  tracks.
- Player-bar and NPP cover/title activate GRID-5 by click or Enter and let
  Space propagate. They are focus-visible link-like surfaces named
  `Reveal playing album`.
- GRID-5 visibly clears search/filter, records a deduplicated Album-grid
  history route, uses `GtkGridView::scroll_to`/`ListScrollFlags::FOCUS`,
  focuses the card, and applies a generation-safe ~1 s pulse. Missing album
  falls back to NAV-9a without a dialog. Ctrl+L remains NAV-9a track-origin
  reveal only.

## Adaptation policy

The plan was checked against live code at `e0493d0`. Adapt private function
names or gtk4-rs call syntax when the checked-in API requires it, while
preserving the tested behavior and module boundaries. Prefer the existing
seams (`PlayerController::play_next`, `ArtistView::select_artist_callback`,
batch tag-edit flow, NavHistory, shared EqBars and accent token). Do not invent
a parallel transport, navigation stack, EQ widget, tag editor, or accent
pipeline.

Stop only for one of the explicit `AGENTS.md` stop conditions: destructive
or real-user-data access, a genuinely active lock, a new security advisory,
missing external permission/dependency with no safe local alternative, or a
material product conflict not resolved by the decision document. If blocked,
record concise evidence, attempted safe alternatives and the exact needed
decision in `.codex-blocked.md`; do not silently narrow the specification.
