# The table stops following the music

Status: diagnosed, not fixed. Measured 2026-08-25 against the installed
build (`~/.local/bin/reprise`, 2006-track library) with
`REPRISE_SCROLL_PROBE=1 REPRISE_DEBUG_SCROLL=1`.

## Reported symptoms

1. The track table no longer centres on the playing track.
2. Clicking the title in the player bar jumps to the track and immediately
   jumps away again.
3. Double-clicking a YouTube episode moves the episode list.

One reproduction run produced **nine** `SCROLL JUMP-TO-TOP` records and
**zero** `current track centered` records.

These are three independent root causes. Each is stated with the evidence
that identifies it; none of them is inferred from the others.

---

## A. The view-state restore overwrites the reveal

`library_shell::route_to_place` runs two steps for a `RevealTrack` intent
(`library_shell.rs:347-351`):

```rust
sidebar.refresh_and_select(source, reason);                       // reload 1
let _ = track_list.restore_browser_place(place.browser_place());  // reload 2
```

Reload 1 centres the playing track. Reload 2 restores the destination
view's remembered anchor and wins, because it runs last:

```
SCROLLWRITE writer=centered.reveal.seed     want=19133.0 from=0.0     upper=805.0
SCROLLWRITE writer=centered.reveal.instant  want=19133.0 from=19133.0 upper=106318.0
SCROLLTO    writer=centered.reveal.anchor   position=361
SCROLLMODEL path=anchor.initial.apply anchor=Some((1502, 0.0)) position=Some(1162) target=61586.0
SCROLLWRITE writer=anchor.initial.hold_target want=61586.0 from=19133.0
SCROLLWRITE writer=hold                       want=61586.0 from=19133.0
SCROLLTO    writer=anchor.initial.scroll_to   position=1162
SCROLLWRITE writer=view_state_restore         want=61586.0 from=61586.0
ERROR SCROLL JUMP-TO-TOP
```

`19133.0` is the playing track (row 361). `61586.0` is where the user
stood before the click (anchor track 1502, row 1162).

The interlock for this already exists — `Shared::track_reveal_pending`,
which a pending reveal raises so a reload landing in the same turn defers
to it. `centered_scroll_restore` deliberately does not raise it (see its
module comment, "Why this occasion does not claim `track_reveal_pending`"),
and that exemption is what reload 2 walks through.

## B. The remembered row height certifies its own error

The persisted row height disagrees with the one GTK allocates:

| source | value | evidence |
|---|---|---|
| `settings.ui.row_height` | **53** | `sqlite3` on the live database |
| GTK's allocation | **45** | `upper=90270.0` = 2006 x 45.0 |
| realized row widgets | 45 | `SCROLLROWS ... distinct_heights=[0, 30, 45]` |

Every reload seeds `upper=106318.0` (= 2006 x 53.0) from the remembered
value; GTK's allocation pass corrects it back to `90270.0`. The log
pendulums between the two throughout the run, and `row_height=` alternates
between `53.0` and `45.0` within the same reproduction.

**Why it cannot heal:** `centered_scroll_restore::write_centered` calls
`ListGeometry::configure`, which writes `upper` *from the remembered
height*. `is_settled(upper, n_rows)` is then asked whether the geometry
agrees — against the `upper` this same path just wrote. The wrong value
certifies itself, so `remember_if_settled` never replaces it.

Commit `7a1e7aba11` deleted `track_list_geometry::forget_row_height`, the
only cache invalidation that existed, while moving row height from the
density projection to `.reprise-track-cell { min-height: ROW_MIN_HEIGHT }`
(36 px). Stale keys still in the database: `ui.row_height.comfortable=53`,
`ui.row_height.compact=0`.

Consequence beyond the pendulum: every centring is off by 53/45.

## C. The podcast/YouTube list reads scroll activity from `value-changed`

`podcasts_view_marker.rs:31`:

```rust
self.scroller.vadjustment().connect_value_changed(move |_| {
    view.last_scroll_activity.set(Some(Instant::now()));
});
```

The track table fixed exactly this and documented why
(`track_list_builder.rs`): *"'the user is scrolling right now' has to come
from what the user did, not from the adjustment moving. Every reload,
every anchor restore, GTK's own reset after `items_changed` — and the
centering glide itself — write this value, so reading activity off
`value-changed` marked the list as user-scrolled after every single
reload."* It uses an `EventControllerScroll` in the capture phase plus a
scrollbar drag gesture instead.

The podcast, YouTube and Radio views kept the wrong source, so any
programmatic scroll marks the list as user-scrolled for 1.5 s and
`LoadedItemChange::ChangedElsewhere` degrades to `MarkerOnly` — the list
stops following playback.

**Not yet measured:** `set_playing_episode` also runs `self.render()`
*before* deciding any policy and without pinning the viewport. The track
table pins exactly this mutation
(`now_playing_marker::reapply_now_playing_markers_pinned`). This is the
candidate for the double-click jump and needs the probe extended to the
podcast scroller before it is fixed.

---

## Scope

- A and B are proven and can be fixed now.
- C's `value-changed` half is proven by inspection against the track
  table's own documented fix; the `render()` half needs measurement first.

## Verification that would have caught this

The existing display tests assert the final scroll value. All three causes
produce a *correct* intermediate value and a wrong final one, or a value
that is right in one unit and wrong in another. Tests must therefore
assert the **ordered list of writers**, which `scroll_probe::trail`
already records under `cfg(test)`.
