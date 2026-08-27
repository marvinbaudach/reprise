---
slug: remembered-device-keeps-its-playlists
worktree: /home/marvin/Projects/reprise-remembered-device-keeps-its-playlists
branch: feature/remembered-device-keeps-its-playlists
phase: refactored
codex_session:
created: 2026-08-27
---
# A remembered device keeps its playlists

## The complaint

Open a remembered (unplugged) device page and everything describing the user's
own choices reads as empty: "Choose playlists" lists nothing but the synthetic
*Everything* row, the picker footer says `0 selected · 0 tracks · 0 B`, the
playlist card says `0 unique tracks · 0 B on device`, and "On this device" says
`0 playlists · 0 tracks · 0 B`. The user's words: *"warum merken wir uns dann die
Geräte, wenn wir diese Infos nicht behalten und bearbeiten können"*.

The selection is not lost. It is in the database:

```
device_settings: 59100DLCQ006SB | Pixel 10 Pro XL | ["playlist:2","smart:2"]
```

This is a presentation defect, top to bottom.

## The rule this plan follows

> Storage and free space are **measurements** — unknown without the cable, and
> they must say so. The playlist selection, the transfer profile, the target
> folder and the size limit are **settings** — they belong to the user, are
> stored locally, and must be readable and editable whenever the app is open.
> The last sync's inventory is **history** — shown with its timestamp, never as
> a current reading and never as a zero.

A zero is a claim. `0 playlists` asserts that nothing is selected. That assertion
is false, and it is the whole complaint.

## The mechanism (verified — do not re-derive)

Everything on the page reads `device.page` (`SyncPageState`):

- picker rows — `device_sync_picker_runtime.rs:35-50`, `.filter(|row| row.available)`
- picker *Everything* row — `device_sync_picker_runtime.rs:51-66`, built separately
  and unconditionally from the library DB, which is why exactly one row survives
  an otherwise empty page
- playlist card summary — `device_sync_playlist_card.rs:179-186`
- "On this device" balance — `device_sync_on_device.rs:257-264` via
  `summarize_playlist_selection`, which counts `available && selected`
  (`core/device_sync/selection.rs:53-66`) — no rows, no count

`device.page` is populated by exactly one function, `recompute_delta_silent`
(`device_sync_compact.rs:134-209`, assignment at :205), and every path to it is
gated on a live session:

- `refresh_contents_with_delta` (`device_sync_runtime.rs:449`) returns early
  unless `device.connected && device.session_state.opens_session()`
- `library_playlists_changed` (`device_sync_runtime.rs:240-243`) filters to
  `device.connected && !device.is_busy()`
- `apply_devices` (`device_sync_device_list.rs:6`) only queues a refresh for
  devices that `opens_session()`; unplugging sets `Remembered` and refreshes
  nothing

`opens_session()` is true only for `Active` (`core/device_sync/device_presence.rs:33`),
so both `Remembered` (unplugged) and `Inert` (plugged in while another device owns
the single session) keep `page` at `SyncPageState::default()`
(`device_sync_runtime.rs:113`, defaults at `core/device_sync/page.rs:90-94`).

**The gate is wrong, not the projection.** `recompute_delta_silent` does not need
the hardware: its playlists come from `load_mirror_playlist_snapshots(conn)`
(:161), its inventory from `load_device_files(conn, …)` (:157), and `available:
true` is set purely by presence in the library (`core/device_sync/page.rs:275`).
Only `storage` and `managed_files` come from the cable.

`device_sync_remembered.rs` is a 13-line placeholder whose own comment says
"Plan E replaces this narrow body with the remembered-state projection". That
work was never done. This plan is it.

### The second contradiction, visible in the same screenshot

The header says `Last synced 27.08.2026 at 10:36 · 4.3 GiB on device when last
verified` while the section below says **"Device contents never verified"** — and
"Check again" is enabled with no device present.

`project_contents_state` (`core/device_sync/device_view.rs:71-87`) derives the
state from `ever_inspected`, and `DeviceState::remembered`
(`device_sync_runtime.rs:117-130`) restores `last_sync` and `size_on_device_bytes`
but leaves `ever_inspected` at `new()`'s `false`. `can_scan` is `true` in the
`NeverVerified` arm (`device_sync_verification_copy.rs:13-17`). History was
loaded and then denied.

### Two inventories — never conflate them

| | source | offline |
|---|---|---|
| `device.managed_files` | MTP scan (`device_sync_runtime.rs:495`) | empty — genuinely unknown |
| `load_device_files(conn, …)` | `device_files` table | available — last verified truth |

`content_row` is built from the first (`project_category_content_row(&self.target,
self.managed_files.len(), …)`), which is exactly why it reads zero offline.

## Decisions (settled in the grill, 2026-08-27 — do not relitigate)

1. **`editable` is decoupled from `connected`**, by changing its meaning rather
   than adding a field: `editable: !active`. `can_start`, `can_cancel` and
   `can_eject` keep their `connected` term untouched. Verified beforehand: the
   three readers are all settings readers — the transfer-profile dropdown
   (`device_sync_page.rs:216`), the playlist toggles
   (`device_sync_playlist_card.rs:178`) and one field the MCP agent exports
   (`device_sync_agent.rs:188`). No hardware action is reachable through it.
2. **The library projection runs for every device**, `Remembered` and `Inert`
   alike, at every presence or change event: app start, plug, unplug, selection
   change, library change.
3. **Live measurements are discarded on unplug.** `device.storage` and
   `device.managed_files` are cleared on the transition to `Remembered`, so
   "device absent" looks identical whether it was just unplugged or the app was
   restarted. Free space is the most volatile value there is; a stale one is
   worse than an honest "unknown".
4. **The changes section shows a preview**, labelled as intent rather than
   measurement: what the next connection will do. Only the part that is sound
   offline may be stated (see task 5).
5. **Auto-sync is not touched.** The switch is the consent, and where the change
   was entered does not affect its validity. The preview does the disclosure.
6. **History comes from the `device_files` inventory** — row count, `device_size`
   sum, `last_verified_at` — and the header moves to that same source, so header,
   balance and preview cannot drift apart.
7. **A new state `DeviceContentsState::VerifiedEarlier(DateTime<Utc>)`** replaces
   the false `NeverVerified`, and `can_scan` is false for it.
8. **One plan, one strand, sequential.**

## Tasks

### 1 — Decouple editability from the cable

`core/device_sync/page.rs:66-87`. `editable: !active`; leave `can_start`,
`can_cancel`, `can_eject` exactly as they are.

`update_controls`'s `connected` argument is passed from `view()`
(`device_sync_runtime.rs:135`) as `self.connected && opens_session()`. That term
still governs the hardware actions, so an `Inert` device — plugged in, but not
the session owner — still cannot start or eject. Confirm this in a test rather
than by reading.

### 2 — Run the library projection without the cable

- Split the two jobs `refresh_contents_with_delta` fuses: the **backend scan**
  (`backend.inspect`) stays behind `connected && opens_session()`; the **library
  projection** (`recompute_delta_silent`) must be reachable for any device in
  `device_states`.
- Project on entry: a remembered device loaded at startup
  (`device_sync_runtime.rs:300-311`) and a device that becomes `Inert` must both
  have a populated `page` before the page is first opened.
- Project on unplug: `apply_devices` (`device_sync_device_list.rs:60-75`) sets
  `Remembered` and refreshes nothing. It must clear the live measurements
  (decision 3) and re-project.
- **The unplug branch also carries the resume contract — do not damage it.** The
  same block sets `resume_initiator` for reconnectable devices and calls
  `cancel_device_run` (`device_sync_device_list.rs:66-75`). Clearing `storage`
  and `managed_files` there is safe, and here is why: the resume decision runs
  *after* a completed refresh and requires `device.connected`
  (`device_sync_runtime.rs:614-630`), so reconnecting re-inspects and refills
  both before anything resumes. `resume_initiator` itself must survive the
  clearing untouched, and so must the `states.retain` predicate that keeps such
  a device in the list (`device_sync_device_list.rs:77-82`).
- `library_playlists_changed` (`device_sync_runtime.rs:240-243`): drop the
  `device.connected` filter, keep `!device.is_busy()`. A playlist deleted from
  the library must also be pruned from an unplugged device's stored selection —
  today that cleanup skips every such device, leaving dangling sources in
  `selection_json`.
- Confirm the projection stays honest with `storage =
  DeviceStorageSnapshot::default()` and `managed_files_scanned = false`: sizes
  and free space unknown, playlists known. No blocker and no storage state may be
  fabricated from an empty snapshot.

### 3 — The picker works offline

Tasks 1 and 2 give it rows and checkboxes. Then verify the round trip: open
offline, see `playlist:2` and `smart:2` ticked, toggle one, Save, reopen.
`save_picker` → `update_settings` → `save_settings` touches only the DB. The
footer must show the real count, not `0 selected`. Leave the *Everything* row
alone — it is correct, and its independence from `device.page` stops mattering
once `page` is populated.

### 4 — "On this device" shows history, not zeros

The section stays: it holds the folder and limit controls, which are settings and
remain usable offline.

- Feed the balance from the `device_files` inventory — row count and `device_size`
  sum — with `last_verified_at` as its timestamp, and move the header
  (`device_sync_page_copy.rs:69`) onto the same source so one number exists, not
  two. Follow the header's existing "when last verified" wording.
- A device with no recorded verification keeps an honest "never verified".
- "Check again" stays disabled without a device.
- **Timestamp and counts must describe the same event.** `record_device_verification`
  writes `last_verified_at` only on a successful `VerifySync` refresh
  (`device_sync_runtime.rs:505-545`), while `device_files` rows are written by
  the transfer itself. A sync that transferred and then failed verification
  therefore leaves fresh rows beside a stale timestamp, and pairing them would
  read as "1,842 tracks · 4.3 GiB, last verified 27.08. 10:36" for a state
  nobody verified. Either source both from the same recorded event, or detect
  the disagreement and drop to copy that does not date the counts.

### 5 — Preview instead of an unprovable empty diff

`view()` clears `page.changes` when `!shows_diff()` (`device_sync_runtime.rs:134`),
so the section renders `0 new · 0 updated · 0 removed · …` — a claim the app
cannot check. Open that path for a device without a live session and label the
result as intent.

What is sound offline and what is not:

- **sound** — copies, replacements and playlist writes, planned against the
  persisted `device_files` inventory, plus the transferred byte total
- **not sound** — removals of files the app has not seen since, and anything
  derived from a storage reading

**The mechanism matters more than the intent here, so it is specified rather
than left open.** `SyncChangeSummary` (`core/device_sync/page.rs:27-35`) is a
flat struct of seven `usize` counters, all printed by one renderer
(`projection::change_summary` via `device_sync_page_copy.rs:87`). Simply leaving
`removals` at zero and reusing that renderer reproduces the exact defect this
plan removes — "0 removed" is the same unprovable claim in a new place.

So the offline preview gets **its own copy function, which does not take the
removal counters as arguments at all** — only additions, replacements,
`playlist_writes` and `transfer_bytes`. Structurally unable to print a removal
count, rather than merely instructed not to. Do not add `Option<usize>` to the
shared struct: every live-path call site would have to unwrap it, for one
offline case.

The copy states plainly that removals are settled when the device is next
inspected. Getting that sentence wrong turns a helpful preview into a false
promise — it is the one piece of copy worth writing twice.

### 6 — `VerifiedEarlier`, strings, tests

- Add `DeviceContentsState::VerifiedEarlier(DateTime<Utc>)`
  (`core/device_sync/device_view.rs`) and give `project_contents_state` what it
  needs to return it. Six files reference the enum, including
  `sidebar_device_card.rs` and `sidebar_device_card_text.rs` — the sidebar gets
  the state too and must read sensibly. `verification_copy` returns `can_scan =
  false` for it. A *failed* scan must not become `VerifiedEarlier`.
- Every new or changed user-facing string goes through `device_sync_strings.rs`
  with `N_!`, then `po/reprise.pot` is regenerated. Never hand-edit `.po` files.
- Extend the existing display suite rather than starting a parallel harness:
  `device_sync_page_display_tests.rs`, `device_sync_remembered_tests.rs`,
  `device_sync_picker_tests.rs`, `device_sync_playlist_rows_display_tests.rs`.

New coverage, each with a control arm:

- a remembered device with `Sources([playlist:2, smart:2])` renders both rows,
  ticked, with a non-zero selected count — control: the same state today renders
  zero;
- the picker offline lists the library's playlists and persists a toggle;
- a device with `last_verified_at` never renders "never verified", and its
  "Check again" is disabled;
- the preview names copies and playlist writes, and the rendered string contains
  **no removal count at all** — assert its absence, never `removals == 0`, which
  is the defect itself;
- a sync interrupted by unplugging still resumes on reconnect (guards the
  measurement clearing in task 2 against the resume contract);
- **regression guard:** a connected `Active` device renders exactly as before;
- **regression guard:** an `Inert` device still cannot start a sync or eject,
  while its selection is visible and editable;
- **regression guard:** unplugging clears the storage reading rather than leaving
  the last one on screen.

## Out of scope

- Syncing without a cable, in any form. No transfer, no scan, no eject.
- The single-session arbitration in `project_device_sessions`.
- The storage bar itself. Unknown storage stays unknown and keeps its copy.
- Auto-start behaviour (decision 5).

## Verification

- `cargo test -p reprise-gnome device_sync` and `cargo test -p reprise-core device_sync`
  — full output to `$SCRATCH`, judged by grep, never read whole.
- `cargo clippy --workspace --all-targets` clean at the repo's pinned level.
- Manual control arm: the real database already carries the reproducing state
  (`59100DLCQ006SB`, `["playlist:2","smart:2"]`, device unplugged). Before/after
  on that exact page is the evidence — not a screenshot of a fixture.

## Parallelität

**One strand. The cut does not survive contact with the file list.**

The obvious split — "core projection" against "GNOME presentation" — is cut
through by the dependencies:

- task 1 changes `core/device_sync/page.rs` *and* its readers in
  `device_sync_playlist_card.rs` / `device_sync_page.rs`; a strand owning one
  side leaves the workspace uncompilable;
- task 6 adds an enum variant in `core/device_sync/device_view.rs` whose `match`
  arms live in `device_sync_verification_copy.rs` and both sidebar files — same
  problem, in the other direction;
- tasks 2, 4 and 6 all edit `device_sync_runtime.rs`, tasks 2 and 4 both edit
  `DeviceState::remembered`;
- tasks 3, 4 and 5 only become *testable* once task 2 populates `page`. Their
  verification would have to read files they do not own — exactly the failure
  mode the strand rules exist to prevent (measured 2026-08-11, Flathub wave,
  strand D).

A three-way cut would produce three branches editing `device_sync_runtime.rs`
and one merge conflict per strand. Sequence instead: 1 → 2 → 3/4/5 → 6.
