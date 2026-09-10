---
slug: the-devices-section-sits-on-the-sidebar-floor
worktree: /home/marvin/Projects/reprise-the-devices-section-sits-on-the-sidebar-floor
branch: feature/the-devices-section-sits-on-the-sidebar-floor
phase: shipped
codex_session:
created: 2026-09-10
---
# The DEVICES section sits on the sidebar floor

**Complaint.** In the running app the `DEVICES` section floats about 95px above
the bottom edge of the sidebar column instead of resting on it.

## What the layout intends

`sidebar_root::build_root` stacks three children in one vertical box:

| child | file | expansion |
| --- | --- | --- |
| navigation scroller | `sidebar_navigation_scroller.rs:9` | `vexpand(true)` |
| activity slot (DEVICES) | `sidebar_activity_slot.rs:30` | natural height |
| bottom region (Issues + job cards) | `sidebar_issues_section.rs:42-46` | `vexpand(false)`, `valign(End)` |

The scroller takes every spare pixel, so the activity slot and the bottom
region are pushed against the sidebar's floor. With no issues and no running
job the bottom region should measure **0px** and `DEVICES` should touch the
bottom edge. The expansion chain is correct and
`fb_8_progress_region_reaches_split_view_bottom` already guards it.

## What actually happens

Measured live over AT-SPI against the running process (window coordinates,
sidebar is 239px wide, no issues shown, one remembered device, no job running):

```
panel   x=0 y=821 w=239 h=45     activity slot — DEVICES section
  panel x=0 y=829 w=239 h=29       heading row
  panel x=4 y=852 w=231 h=0        present devices (empty)
  group x=0 y=858 w=239 h=0        remembered devices (collapsed revealer)
panel   x=0 y=866 w=239 h=95     bottom region  ← reserves 95px
  panel x=0 y=866 w=239 h=0        Issues block (hidden)
  panel x=0 y=866 w=239 h=95       progress dock
    panel x=0 y=866 w=239 h=0        flexible spacer
    group x=0 y=874 w=239 h=87       job-card revealer, visible, nothing drawn
```

The sidebar's inner bottom edge is y=961. The 95px are `8px` dock margin
(`.sidebar-job-card-dock`) plus an **87px job-card revealer that is `visible`
while it reveals nothing**. That is the whole gap.

## Why an idle card can reserve height

The three dock cards (scan, Library Doctor, missing-file relink) are
`GtkRevealer`s with `RevealerTransitionType::Crossfade`. Measured with GTK
4.22.4:

| transition | `reveal_child` | child `visible` | measured height |
| --- | --- | --- | --- |
| `SlideDown` | false | true | **0** |
| `Crossfade` | false | true | **85** (the child's full request) |
| `Crossfade` | false | false | **0** |

A slide revealer scales its measurement with the reveal position; a crossfade
one does not. `sidebar_activity_slot.rs:92-96` already documents this trap and
answers it with bookkeeping: `sync_revealer_visibility` keeps the *revealer's*
`visible` in step with `reveals_child() || is_child_revealed()`, driven by the
`reveal-child` and `child-revealed` notifications.

The live process shows that bookkeeping has drifted into
`visible = true, reveal_child = false, child_revealed = false` — the card's
child is still `visible`, so the collapsed crossfade revealer keeps asking for
its 85px. Which of the three cards drifted, and by which path, could not be
established from the outside: the collapsed revealer's child is unmapped and
therefore absent from the accessibility tree, and it carries no distinguishing
attributes. Replaying the plausible sequences (show → finish, redundant show,
unmap mid-animation, window hidden during the crossfade) against real GTK
revealers reproduced no drift; the only unguarded direct write is
`scan_progress.rs:435` (`revealer.set_visible(true)` in `begin_visibility`).

## Proposed fix

Make an unrevealed dock card contribute zero height **regardless of the
revealer's own `visible` flag**, by keeping the revealer's *child* in step as
well as the revealer:

```rust
fn sync_revealer_visibility(revealer: &gtk4::Revealer) {
    let should_be_visible = revealer.reveals_child() || revealer.is_child_revealed();
    if let Some(child) = revealer.child() {
        if child.is_visible() != should_be_visible {
            child.set_visible(should_be_visible);
        }
    }
    if revealer.is_visible() != should_be_visible {
        revealer.set_visible(should_be_visible);
    }
}
```

The invariant becomes `child.visible == (reveals_child || is_child_revealed)`,
so the measured live state (`visible` revealer, nothing revealed) measures 0px
whatever set the revealer visible. The show path stays intact: a hidden child
implies `reveal_child == false`, so the next `set_reveal_child(true)` always
notifies and brings the child back before the crossfade runs.

The structural alternative — switching the three dock cards from `Crossfade`
to `SlideUp` — removes the bookkeeping entirely (GTK then measures 0 by
construction) but changes the cards' animation, which is a deliberate choice in
this repo and therefore the owner's call.

### Who owns the flag afterwards

The fix makes the activity slot write `visible` on a widget that three other
modules build. State the contract in the code: **the activity slot owns each
dock card's container `visible`; the card modules must not set it.** Scan,
Library Doctor and relink only ever touch their own inner rows today, so
nothing has to change there.

There is a second owner of the *revealer's* flag: `ScanProgressView`'s
constructor connects its own `connect_child_revealed_notify` that hides the
revealer (`scan_progress.rs:238-243`), duplicating `sync_revealer_visibility`.
It is harmless today — same condition, same result — but a duplicated
invariant is how this class of bug returns. Collapse it into the slot's sync
and leave `revealer.set_visible(false)` in the constructor as the initial
state.

`begin_visibility` (`scan_progress.rs:434-436`) may keep its
`revealer.set_visible(true)`: after the fix it costs nothing, because the
height follows the child.

## The regression test that is missing

`fb_8_progress_region_reaches_split_view_bottom` asserts that the bottom region
*ends* at the sidebar's floor, which is true whether the region is 0px or 95px
tall. It cannot see this bug. Two assertions to add, both with the three cards
attached and none revealed, and a visible DEVICES section:

1. **Idle.** Nothing forced. The bottom region measures 0px and the DEVICES
   section's bottom plus its 8px `margin_bottom` equals the sidebar root's
   height. This is the user's complaint, and the state is unambiguously
   reachable.
2. **Drifted.** One card's revealer forced to `set_visible(true)` without a
   reveal — the state measured in the live process above. Same two
   assertions must hold.

## Verification scope

Only the sidebar dock changes; do not run the whole workspace gate. Relevant:

- `cargo fmt --check`, `cargo clippy --locked --all-targets -p reprise-gnome -- -D warnings`
- `cargo test -p reprise-gnome` for the non-display tests
- the display-suite tests that exercise a *revealed* card, since the fix now
  gates the child's `visible`: `doc_5e_every_job_card_docks_at_the_same_place_and_height`,
  `device_and_scan_activity_stack_in_stable_bottom_slot_order`,
  `fb_8_progress_region_reaches_split_view_bottom`, plus the two new ones.

Note for the show path: in `measured_job_card` the scan card runs `show_batch()`
*before* `slot.set_scan_card(card)`, so at attach `reveals_child()` is already
true and the sync must leave the child visible — that ordering is what proves
the fix does not swallow a card that is already revealed when it docks.
