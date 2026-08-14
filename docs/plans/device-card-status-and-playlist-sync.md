---
slug: device-card-status-and-playlist-sync
worktree: /home/marvin/Projects/reprise-sidebar-cancel-affordance
branch: feature/sidebar-cancel-affordance
phase: reviewed
codex_session:
created: 2026-08-14
---
# Device card status, cancel affordance, and playlist sync

phase: planned
date: 2026-08-14

Three findings from a live session on 2026-08-14, all diagnosed to root cause
before planning. They are independent and can land in any order, but they share
one theme: the sidebar device card and the device sync page disagree about what
is true right now.

## Finding 1 — the card leads with the one value it does not have

**Observed.** The phone was switched to MTP. The device page said "MTP
connected" immediately; the sidebar card read `Available space unk…` and looked
like it had not noticed the phone at all.

**Root cause.** The card *had* updated — it just says nothing useful. On
connect, `device_sync_device_list.rs` sets the phase to `ComputingDelta`. The
card's subtitle for that phase (`sidebar_device_card.rs`, `card_subtitle`) is:

```rust
PlannedSyncPhase::ComputingDelta => format!("{} · Checking…", free_space(device.storage.free_bytes))
```

`storage.free_bytes` is still `None` at that moment — the device page confirms
it in the same screenshot ("Free unknown"); free space only becomes known once
the on-connect contents scan has run. `free_space(None)` renders the 24-character
placeholder "Available space unknown", the detail label ellipsizes at the end
(`EllipsizeMode::End`), and the narrow sidebar drops the only informative half
("· Checking…").

The same shape appears in the `Finishing` and `Syncing` arms (both lead with
`free_space`), and in the `mirror_needs_attention` branch, which can render
"Needs attention · Available space unknown".

**Decision (user, 2026-08-14).** An unknown storage figure must never lead the
line — it must be omitted entirely while it is unknown, not rendered as a
placeholder.

```
before:  Available space unk…
after:   Checking changes…
after (once free space is known):  170.9 GiB free · Checking…
```

**Work.** Make "storage figure, if known" a prefix that collapses to nothing
when `free_bytes` is `None`, and apply it consistently to every subtitle arm
that currently interpolates `free_space`/`available_space` — `ComputingDelta`,
`Syncing`, `Finishing`, and the "Needs attention" branch. The existing
`free_space`/`available_space` helpers keep their current behaviour for the
device page, which has room for the full sentence and legitimately wants to say
"Free unknown"; this is a sidebar-side projection concern.

Prefer expressing it as a pure function next to the other card-text projections
(`sidebar_device_card_text.rs` already owns exactly this kind of wording, with
unit tests independent of any widget) rather than as inline `format!` calls in
the widget update path.

**Proof required.** A unit test per affected phase asserting that with
`free_bytes: None` the resulting subtitle contains neither the
`SPACE_UNKNOWN` string nor a leading separator, and still names the activity;
plus a test with a known figure asserting the figure is still present and still
leads. A regression guard that no card subtitle ever contains `SPACE_UNKNOWN`
is worth having, since this is the third arm to grow the same bug.

## Finding 2 — the cancel button reads as a decoration

**Observed.** The round button on a syncing card is not self-explanatory.

**Root cause.** It uses `process-stop-symbolic`. That icon exists in Adwaita,
but its geometry is a bare X with no frame of its own; the circle comes from
the `.circular` style class plus the accent background. At the button's 24px
size, a thin X inside a round accent frame reads as a ring-with-a-dot, not as
"stop". The tooltip and the AT-SPI label already say "Cancel", so only the
visual is at fault.

**Decision (user, 2026-08-14).** Use `window-close-symbolic` — a heavier X that
survives the small size and reads as cancel/close without a tooltip.

**Work.** Swap the icon name. Keep the tooltip, the accessible label, the
circular styling, the size, and the placement exactly as they are. If the
heavier glyph looks cramped in the existing circle, adjust padding rather than
shrinking the glyph.

**Proof required.** The device-card display test that covers the syncing state
should assert the icon name, so a future icon change cannot silently regress
the affordance. Visual confirmation on a real syncing card (the card only shows
this button while a sync is active).

## Finding 2b — the sidebar's four cancels are three different controls

**Observed (user, 2026-08-14).** "Cancel btn sollte in der linken spalte immer
gleich aussehen. Am besten ein Symbol mit Tooltip." The screenshot shows the
device card's round icon button stacked directly above a scan card whose cancel
is the words *Cancel Scan*.

**Root cause.** Finding 2 treats the device card's icon as the whole problem.
It is not — the column carries three different treatments of one verb:

| Card | File | Cancel today |
|---|---|---|
| Device sync | `sidebar_device_card.rs` | `process-stop-symbolic`, circular overlay, tooltip |
| Scan / batch | `scan/scan_progress.rs` | **text button** `CANCEL_SCAN` = "Cancel Scan" |
| Library Doctor | `library_doctor/progress_card.rs` | `window-close-symbolic`, flat, tooltip |
| Missing-files relink | `issues/missing_progress.rs` | `window-close-symbolic`, flat, tooltip |

Swapping only the device card's glyph (Finding 2) leaves the text button
standing — which is the control the user actually pointed at.

**Decision (user, 2026-08-14).** Unify all four on `window-close-symbolic` with
a tooltip. The device card keeps its circular, accent-tinted ground: it is an
overlay on a clickable card surface and needs its own ground to read at all.
The wording stays per-card ("Cancel Scan" vs "Cancel") and moves into the
tooltip and the AT-SPI label, not a visible text link.

**Work.** Hold the icon name in one shared constant rather than four literals —
`scan_card_css.rs` already hosts the cards' shared geometry
(`JOB_CARD_HEIGHT_PX`) and is visible to the sidebar module, so it is the
natural home. Point all four call sites at it, including the two that already
use the right glyph, so the column cannot drift apart again.

**Constraint, measured 2026-08-14 — do not rediscover this.** `JOB_CARD_HEIGHT_PX`
(85) is a hard ceiling: three display tests assert
`measure(Vertical, 232).0 == JOB_CARD_HEIGHT_PX` *exactly*. The job cards'
status row takes its height from the percent label beside the button (~19px).
Giving `.scan-card-cancel` a `min-height` of the GNOME-standard 24px pushed the
scan card to **88px** and the doctor and relink cards to **90px**, i.e. it
silently resized every job card in the sidebar. Style the hit target with
`min-width` only and leave `min-height: 0`; growing the cards is a geometry
decision of its own and is out of scope here.

**Proof required.** Each of the four cards asserts its cancel button's icon name
against the shared constant, and the scan card additionally asserts that its
tooltip still reads "Cancel Scan" and that the button carries no visible label.
The three `npp_1_*_job_card_minimum_width_fits_the_sidebar` tests must stay
green — they are what catches the height regression above.

## Finding 3 — a deleted playlist stays on the sync page

**Observed.** Deleting playlist "88" from the sidebar showed the "Deleted
playlist "88"" toast and removed it from the sidebar, but the device sync page
kept listing it under Playlists.

**Root cause.** Not a timing problem — there is no wire at all.
`delete_playlist` (`sidebar_export.rs`) calls `rebuild(...)`, which rebuilds
*the sidebar only*. The device page's rows come from `page.playlists`, which is
refreshed exclusively by `recompute_delta` (`device_sync_compact.rs`, via
`playlists::list(conn)`). Its trigger inventory is: device connect, picker
selection change, settings/target-folder change, and sync completion. A library
mutation is not among them, and the `DeviceSyncRuntime` subscribes to no
library-change channel — there is no `PlaylistsChanged`/`LibraryEvent` anywhere
in the crate. The stale row therefore persists until an unrelated trigger fires.

**Work.** Give library playlist mutations a way to tell the device sync runtime
that its playlist projection is stale, and have the runtime recompute the delta
for every connected device in response.

The runtime is already in reach: `Sidebar::bind_device_sync` (`sidebar.rs`)
receives the `Rc<DeviceSyncRuntime>`, so a notification hook can be installed
there and stored on the sidebar's `Shared` — no new global, and no direct
dependency from `sidebar_export` on the device-sync module.

Cover the mutations that change what the device page would show, not only
deletion: creating a playlist, renaming one, deleting one, and changing its
membership all move rows or their subtitles. Deletion is the reported case and
the minimum bar; the others share the same missing wire and should not be left
to be rediscovered one at a time.

Constraints:

- Recompute must be cheap enough to run on a library mutation and must not
  block the GTK main thread — `recompute_delta_silent` is the existing
  non-toasting entry point.
- A device that is mid-sync must not be disturbed; `is_busy()` already guards
  the settings path and the same guard applies here.
- A failed recompute must not surface a user-facing error for what was, from
  the user's point of view, a successful playlist deletion — log it.

**Proof required.** A test that mutates a playlist through the library path and
asserts the connected device's `page.playlists` no longer lists it, failing
against today's code. State plainly in the handover which mutations ended up
covered.

## Out of scope

- Why MTP free space is unknown until the contents scan runs. That is correct
  behaviour today (the figure needs an open session) and Finding 1 is about how
  the card handles the gap, not about closing it.
- The device page's own "Write access unknown · … · Free unknown" line. It has
  the width to say "unknown" honestly, and the user did not raise it.

## Verification

Per the project's standing rules: run the display suite unfiltered, and treat a
red result as a signal to check `origin/dev` first — several display tests are
known red there and are not this change's fault. The sidebar card work touches
CSS-dependent measurements, so display fixtures must install the app CSS or the
heights they measure are meaningless.
