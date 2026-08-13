---
slug: podcast-pointer-selection
worktree: ~/Projects/reprise-podcast-pointer-selection
branch: feature/podcast-pointer-selection
phase: planned
codex_session:
created: 2026-08-02
---
# Retire the episode checkboxes and the selection toolbar

Pointer selection is **already implemented** on this base (SRC-14:
`pointer_intent`, `SelectMode::Only|Toggle|Range`, double-click to play,
secondary-click-selects-first, Space/Shift+Space). The checkboxes and the
"N selected / Download selected / Remove selected" toolbar row are a second,
redundant path layered on top of it. This task removes that second path.

**Nothing about how selection works changes.** Do not touch `apply_select`,
`pointer_intent`, `SelectMode`, the rendered-order plumbing, or the Escape
controller. This is a subtraction task plus one small addition (the count
moving into an existing line).

## Base branch

This worktree is branched from `feature/podcast-escape-clears-selection`, which
adds `PodcastSelection::clear()`, `YoutubeChannelDetail::clear_selection()` and
the Escape key controller. All of it is already in your checkout. Build on it;
do not revert it.

## Work

### Package A — the checkbox leaves the row

1. Delete `podcasts_selection::episode_checkbox` and its call sites (the
   grouped view's row builder in `podcasts_groups.rs` and the channel detail's
   row builder in `youtube_channel_detail.rs`). Selection remains visible
   through the row's existing selected styling.
2. Close the gap the checkbox leaves: the row's leading edge is now the
   thumbnail. Keep the existing spacing rhythm — do not leave the old
   checkbox slot as empty padding, and do not re-tune unrelated row metrics.
3. The per-row accessible name that `strings::podcast_select_episode` supplied
   through the checkbox must not be lost. Move it onto the row widget itself,
   together with the row's selected state
   (`gtk4::accessible::State::Selected`), so assistive technology still reports
   both which episode a row is and whether it is selected.
4. Any row-widget struct field that only existed to hold the checkbox (for
   example the `checkbox` field the view tests reach through) goes away with
   it. Update those tests to assert selection through `PodcastSelection` /
   the row's selected style instead — do not keep a widget alive purely to
   keep a test compiling.

### Package B — the toolbar trio leaves both surfaces

5. Delete `SelectionControls` (`standalone()` in the grouped library view,
   `appended_to()` in the channel detail) and the toolbar row that hosted it.
   The `podcasts.download-selected`, `podcasts.remove-selected`,
   `podcasts.mark-played-selected`, `podcasts.mark-unplayed-selected` and
   `podcasts.delete-downloads-selected` actions all stay — the context menu is
   now their only entry point, and it already builds itself for the current
   selection.
6. The selection count is not lost. The grouped view's summary line
   ("2 channels · 54 episodes · 4 new") appends "· N selected" while a
   selection exists; the channel detail's toolbar summary does the same. Add
   one new translatable string for that fragment and format it through the
   existing summary path — never concatenate translated pieces by hand.
7. The escape display test on the base branch asserts through
   `selection_controls.actions_sensitive()`. That accessor and the widgets
   behind it are going away: rewrite that assertion against the selection
   state itself, keeping the test's intent (a first Escape clears and is
   consumed, a second proceeds) exactly as it is.

### Package C — strings and rules

8. `strings::YOUTUBE_SELECT_EPISODES` (the checkbox tooltip) becomes unused.
   Remove the constant and its `.pot`/`.po` entries the way this repo retires
   any other string. `YOUTUBE_DOWNLOAD_SELECTED` and `YOUTUBE_REMOVE_SELECTED`
   stay — the context menu uses them as its labels.
9. `docs/ux-rules.md`:
   - **SRC-12** currently describes bulk selection with its shared batch
     actions. Amend it so the batch actions live in the context menu and
     nowhere else, and so it no longer implies a checkbox or a toolbar trio.
     Keep the Escape sentence the base branch added.
   - **SRC-14** describes the selection mechanics and stays true as written.
     Only touch it if some clause names the checkbox.
   - Grep the whole file for any other rule that names the checkboxes or the
     selection toolbar and fix those too, rather than leaving the document
     describing chrome that no longer exists.

### Package D — proof

10. The existing SRC-14 selection tests must keep passing untouched in
    behaviour — they are the proof that removing the checkbox did not disturb
    the mechanism.
11. Add one pure test that the summary line reports the selection count and
    drops the fragment at zero.
12. Display-gated tests stay `#[ignore = "requires a display; run via
    xvfb-run"]`. Do not un-ignore them and do not claim they ran.

## Verification

```
cargo test -p reprise-gnome podcasts
cargo test -p reprise-gnome youtube_channel_detail
cargo build -p reprise-gnome
cargo clippy -p reprise-gnome --all-targets -- -D warnings
```

Plus whatever string-extraction check this repo runs, since a string is being
retired.

## Out of scope

- Any change to selection mechanics, ranges, anchors, or activation.
- Drag-and-drop of a multi-selection.
- The track list, playlists, radio.
