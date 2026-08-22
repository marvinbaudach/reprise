---
slug: issue-backlog-wave-1-3
worktree: /home/marvin/Projects/reprise-issue-backlog-wave-1-3
branch: feature/issue-backlog-wave-1-3
phase: planned
codex_session:
created: 2026-08-22
---
# Strand 3 — #620: the jump from the player bar lands at the top

Mother plan: `docs/plans/issue-backlog-wave-1.md`. Base `origin/dev` = `1515487599`.

This strand owns, and writes **only**:

```
crates/reprise-gnome/src/ui/track_list/**   except rating.rs and track_list_sort.rs
crates/reprise-gnome/src/ui/scroll_center.rs
crates/reprise-gnome/src/ui/list_geometry_layout.rs
crates/reprise-gnome/src/ui/view_session.rs
crates/reprise-gnome/src/ui/window/library_shell.rs
crates/reprise-gnome/src/ui/window/metadata_navigation.rs
docs/plans/one-centering-path-preseed-variant.md
docs/plans/issue-backlog-wave-1-3.md
```

If the design below turns out to need `crates/reprise-core/src/browser/navigation.rs`,
that is allowed — no other strand touches it. `track_list.rs` is written by
**nobody** in this wave; if a change there looks unavoidable, stop and say so in
`## Result` rather than taking it.

---

## Read this before planning: the issue's own premise is wrong

Issue #620 says the open question is "whether the player-bar reveal takes the
restore path or the anchor path", and assumes the answer leads to the preseed
work in `docs/plans/one-centering-path-preseed-variant.md`. It was measured on
2026-08-22 by reading the code end to end. **The answer is: the anchor path — and
the four-step fight that plan describes is not what is happening here.**

The chain, complete:

| step | where |
|---|---|
| player-bar title button `connect_clicked` | `ui/player_bar/player_bar.rs:204-211` |
| the installed callback | `ui/window/window_playing_source_wiring.rs:143-146` |
| `reveal_playing_track` builds `NavigationIntent::RevealTrack` | `ui/window/window_playing_source_wiring.rs:53-80` |
| the intent has no `source_target`, so the catalog branch runs | `ui/window/metadata_navigation.rs:125-153` |
| `route_to_place` takes its `else` branch and calls `restore_browser_place` | `ui/window/library_shell.rs:347-351` |
| `restore_browser_place` → `finish_track_source` | `ui/view_session.rs:155-179`, `237-254` |
| the place has an anchor, so `reload_with_anchor` | `ui/track_list/track_list_reload.rs:484-486` — fixed `ReloadViewport::PreserveAnchor` |
| `restore_reload_anchor` falls through to its default branch | `ui/track_list/track_list_reload.rs:281-287` |
| `reload_anchor_scroll::schedule` | `ui/track_list/reload_anchor_scroll.rs:106-149` |

And the target it computes, in `ui/track_list/reload_restore.rs:140-152`:

```rust
let (anchor_id, offset) = anchor?;
let position = current_ids.iter().position(|&id| id == anchor_id)?;
let target = layout.row_top(position) + offset;
```

The offset is **`0.0`**, set at `crates/reprise-core/src/browser/navigation.rs:243-259,415-419`,
where `RevealTrack` calls `set_explicit_track_anchor` → `TrackAnchor::new(track_id, 0.0)`.

So `target == layout.row_top(position)` — "put this row's top at the top of the
viewport". The row lands at the top because the code was **told** to put it there,
not because a later write beat the centring. There is no fight to collapse.

Two consequences for how this is fixed:

- The preseed plan (`docs/plans/one-centering-path-preseed-variant.md`) describes
  the *search-clearing restore*, a different occasion with a different mechanism.
  Its tasks 1-3 are **not** this strand's work. Do not implement them here.
- The edge snap that plan blames at `centered_scroll_restore.rs:55-59` no longer
  exists; that file's own module doc (lines 13-20) records its removal. Working
  from those line numbers would change unrelated code.

The contrast that proves the shape of the fix: switching source from the sidebar
does centre, because `TrackList::set_source` (`ui/track_list/track_list.rs:459-472`)
calls `center_playing_track_in_view` at line 470 **after** the restore. The
`route_to_place` branch the title click takes makes no such follow-up call.

**Do not fix it by adding that follow-up call.** Two writes to the same adjustment
for one navigation is exactly the shape this repository has already paid for four
times, and the second write is what produces the multi-step landings the preseed
plan measured. The reveal must land in **one** write.

## Task 1 — carry the centring intent into the reload

`NAV-10b` (`docs/ux-rules.md`): *"explicit metadata/reveal navigation always
selects, focuses, and centers."* `NAV-19` makes the same promise for source
switches, *in one move rather than through an intermediate position*.

Make "this arrival centres its anchor" a property of the navigation, carried from
where it is known to where the scroll target is computed:

1. `MetadataNavigator::navigate` knows the intent is a `RevealTrack`
   (`ui/window/metadata_navigation.rs:97-154`). That knowledge, not a re-derivation
   in the track list, is the source.
2. `library_shell::route_to_place` (`ui/window/library_shell.rs:324-352`) forwards
   it to `TrackList::restore_browser_place`.
3. `view_session::finish_track_source` (`ui/view_session.rs:237-254`) selects the
   viewport accordingly instead of the hard-coded `PreserveAnchor` at
   `track_list_reload.rs:484-486`.
4. `restore_reload_anchor` (`track_list_reload.rs:247-287`) gains the branch that
   centres the **anchored** row — not the playing row; NAV-10b is about the row the
   navigation named, and a reveal of a non-playing track must centre that track.

Compute the value through `ListLayout::centered_value` — the same function the
other centring path uses (`ui/scroll_center.rs`, `ui/list_geometry_layout.rs:167`)
— so section headers keep being counted. Do not add a second centring arithmetic;
there is one geometry model and it stays one.

Then hand that value to the write path that already exists:
`reload_anchor_scroll::apply` preseeds the geometry before writing
(`reload_anchor_scroll.rs:430-439`) and writes once (`:463`). Reuse it. The change
is *which value* is written, not *how* it is written.

`row_offset` stays what it is — a content offset in pixels. Do not encode
"centre me" as a magic offset value; the viewport height it would need is not
known where the anchor is built.

## Task 2 — do not break the occasions that already work

The same `restore_reload_anchor` serves the search-clearing restore and ordinary
place restoration. Those must keep landing exactly where they land today: a place
the user scrolled to is restored to that scroll position, not centred. Only an
**explicit reveal** centres.

Check every caller of `reload_with_anchor` and of `restore_browser_place` and say
in `## Result` which occasion each one is and which viewport it now gets.

## Task 3 — record the correction in the preseed plan

`docs/plans/one-centering-path-preseed-variant.md` is tracked on `origin/dev` (it
came in with `41aca1beeb`), and `docs/plans/open-issue-sweep-2026-08.md` tasks 3-4
point #620 at it. Add a short, dated section to the preseed plan saying that #620
was measured to the anchor path with a `0.0` offset, that it is not an instance of
the four-step fight, and that the preseed plan's own tasks remain open for the
search-clearing occasion. Correct its two stale line references
(`centered_scroll_restore.rs:55-59`, `reload_anchor_scroll.rs:52-80`) while you are
in there. Do not change its `phase:`.

## Acceptance — control-armed, one landing

A green test proves nothing here unless it was red before the fix. Both arms get
recorded.

1. **A display test for the actual reproduction.** Play a track, scroll away, then
   drive the player-bar title activation, and assert the row ends **centred** in
   the viewport — not merely visible. `nav_19_switching_source_centers_the_running_track`
   (`ui/track_list/source_switch_centering_display_tests.rs:90-132`) is the shape
   to follow for the assertion.
2. **One step, not several.** Assert the landing with the step recorder the search
   tests already use: `record_viewport_steps` and `viewport_steps`
   (`ui/track_list/search_viewport_display_tests.rs:375-407`), which sit on
   `scroll_probe::trail`. The reveal must produce a single landing, the way
   `search_16_clearing_after_a_play_reaches_the_track_in_one_step` (`:409-463`)
   requires for its own occasion. A test that only checks the final value would
   pass for a fix that scrolls twice.
3. **Control arm.** Run both new tests on the **unfixed** tree first and record
   that they fail, with the numbers. A run whose control arm never moved — the
   test passing before the fix, or the recorder capturing zero steps — is reported
   as **UNPROVEN**, not as passed. This is the acceptance #620 itself names.
4. **The occasions that must not change.** Run the existing centring and restore
   display tests — `source_switch_centering_display_tests.rs`,
   `search_viewport_display_tests.rs`, `start_restore_tests.rs` — and record
   passed/failed before and after. A regression here is a failed strand, not a
   detail.
5. **Displayless suite.** `cargo test --locked -p reprise-gnome`, fresh XDG roots,
   `REPRISE_AUDIO_SINK=fakesink`; record passed/failed.

**Forbidden in this strand:** claiming `Fixes #444` (§4C of
`docs/plans/queue-anchor-grill-followups.md` is not satisfied), touching
`ScrollAdoptionGeometry`'s parallel fields (#475 is sequenced after this strand),
and implementing the preseed plan's tasks 1-3.

---

## Result

*(Codex fills this in: the control-arm runs with their numbers, the fixed runs,
the caller-by-caller viewport table from task 2, and anything left unproven.)*
