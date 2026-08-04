# Implementation concept — Turn 6 and 7 (Podcasts, YouTube, Radio, MTP sync)

State: 2026-07-28 · branch `feature/podcast-channel-redesign` · base
`77b3f54545dadcf44850736d110792fae89ae428`

## 1. Purpose and source

Turn 6 **and Turn 7** of the design document `Tourdaten Varianten.dc.html`
(Claude design project `8fb24732-431c-447f-9a74-08d3229a0c33`) are the
binding template for the rebuild of the three online sources.

| Part | Subject | Issues |
| --- | --- | --- |
| 6a | RSS and YouTube separated, grouping by show/channel | #96, #98 |
| 6b | Channel detail: window, Shorts, download control, sync | #106 |
| 6c | Three separate add dialogs, one button pattern | #101, #102, #103 |
| 6d | Sync targets in the MTP device view | Sync |
| 6e | Offline concept | #107 |
| 6f | Empty views | #98 |
| 7a | Device view: storage by category, three content sources, diff | Sync |
| 7b | Settings "Online sources": three blocks on one level | #96 |
| 7c | The device card in the sidebar says what it means | Sync |
| 7d | Target folder freely selectable via a device browser | Sync |

Turn 7 is newer than Turn 6 and **refines 6d**: 7a/7d replace the folder
sketch from 6d, 7b replaces the preferences sketch from #96.

Other turns of the document (Tourdaten/Concerts, Releases) are explicitly
not part of this concept.

This document is a plan that outlives its own execution — later work must
follow it. It therefore belongs in `docs/plans/` and not in the session
context (`AGENTS.md`, section "Where we are RIGHT NOW").

`docs/ux-rules.md` remains the only binding UX source and takes precedence over
this plan. Where Turn 6 deviates from an `[active]` rule, the rule changes in
the same commit that implements the behavior and brings its rule-named test
with it.

## 1b. Constraint: no backwards compatibility

Confirmed by the owner on 2026-07-28: Reprise is **not released**, there are
**no existing installations**. Migrations, compatibility fallbacks and
duplicate write paths are therefore not a criterion anywhere in this work.

Practical consequence: where a clean data model and a backwards-compatible one
collide, the clean one wins. A second place of truth left behind is worse than
either of the two variants on its own. Directly affects G1 (YouTube gets its
own `ModuleDescriptor` instead of a nested `podcasts.youtube_enabled`) and E1
(named sync targets instead of a migration of the single managed device folder).

## 2. Current state — verified, not assumed

Before any effort estimate: a considerable part of Turn 6 already stands. The
following inventory was checked against the code, not taken over from the
ledger.

**Already in place:**

- `SRC-5` [active] — RSS and YouTube are separate library places with
  show-/channel-grouped source rows that expand their episodes. That
  implements the core of **6a** (`podcasts/podcasts_groups.rs`, 454 lines:
  `build_group`, `group_header`, `episode_row`).
- Already subscribed sources drop out of the search
  (`podcasts/search_results.rs::filter_unsubscribed`, `source_is_subscribed`
  including stable YouTube identity via handle/channel pairs).
- **6b** is largely implemented (`podcasts/youtube_channel_detail.rs`, 599
  lines): `INITIAL_WINDOW = 10`, `EXTENDED_WINDOW = 40`, `set_hide_shorts`,
  `is_short` via `SHORT_MAX_SECONDS = 180`, multiple selection (`set_selected`,
  `selected_ids`) and `update_batch_controls`.
- Source images have a surface of their own (`podcasts/source_image.rs`, 189 lines).
- Per-subscription device selection exists (`podcasts/podcasts_device_sync.rs`,
  `selected_for_groups`, rule `POD-8`).
- Empty states are **classified** per source
  (`podcasts_empty_state.rs::podcasts_empty_state_for` with
  `List/Empty/NoEpisodes/NoResults`, `radio_empty_state.rs` with
  `List/NoResults/Empty`).

**Missing:**

- **6c** entirely: today the add dialog searches **both** providers in one
  dialog (`add_dialog.rs:253`, `preferred_provider_order` returns
  `[PodcastKind; 2]`). There is no separate "Add channel" dialog. The
  row button is called `PODCAST_SUBSCRIBE` ("Subscribe"), radio has a pattern
  of its own.
- **6f** as a form: the classification exists, but there is no shared
  empty-state geometry, none of the described sentences, no "module off",
  "offline and empty" or "only Shorts" state and no radio shortcut chips.
- **6e** entirely: not a single `POD-`, `YT-` or `RAD-` rule set mentions
  offline. See #107.
- From **6b**: file sizes in the download column, the total, "Keep N
  downloaded", the "On phone" column.
- **6d**: three separate sync folders, size cap, "Remove from phone when
  deleted here", queue display in the device view.
- Real remote images in the dialogs (channel thumbnails, iTunes
  `artworkUrl600`, radio-browser `favicon`).
- The "Online sources" settings page and the global latch
  `online-sources-enabled`.

**Partly present — the sync is further along than the design suggests:**

- A **podcast sync exists in full**, not merely as a sketch:
  `device_sync/podcasts.rs` provides `PodcastSyncCandidate`, `PodcastSyncPlan`
  with `to_copy`/`to_remove`/`bytes`, `query_candidates_for_device` and
  `build_plan`; it is wired up via `device_sync_runtime.rs`,
  `device_sync_planned.rs` and `device_sync_compact.rs`. `PodcastSyncSource`
  already knows `Rss` **and** `Youtube`.
- Of that, however, only RSS is in operation: PCR-1 deliberately made the
  phone-sync opt-in RSS-only and defensively cleared it for YouTube sources.
  The YouTube branch is thus prepared, but switched off.
- What is missing is therefore **not** the podcast transfer, but: named
  targets instead of a single managed device folder (`78e379fd`), unlocking the
  YouTube branch, size caps, the diff by category, the device browser and
  the "Device contents never verified" state.

That noticeably shifts the cut of block E: E2 and E4 build on an existing
planning and transfer layer instead of reinventing it.

## 3. What Turn 6 decides

Two decisions overturn earlier commitments and must stay visible:

1. **Images are v1.** Turn 6c states explicitly: "decision F6 thereby
   overturned: images are now v1". That entails a remote image module.
2. **Channel listing is keyless.** `videos.xml?playlist_id=UULF…` (channel ID
   with `UC` → `UULF`) returns long-form without Shorts, 15 entries, without an
   API key. yt-dlp remains only for the audio track (`-x --audio-format opus`)
   and for "Load more" (`--flat-playlist -I 1:40`).

Further load-bearing sentences from Turn 6:

- **Hard separation** of the dialogs: "no mixed result, no shared
  search". Three dialogs with an identical structure — title, one field,
  result list, source footnote.
- **One button pattern** everywhere: the same small `+ Add`. Already subscribed
  = inactive "✓ Added", and the source drops out of *later* searches.
- **Sync responsibility split:** *where and how* syncing happens is
  device configuration; *what* comes along is decided by the channel toggle.
- **Offline is a state, not an error.** Online actions are queued and executed
  automatically the next time there is a network. Radio is the exception.
- **Empty states:** glyph, one sentence on *what* lands here, one sentence on
  *where from*, primary button, the URL path below it. No filter row, no
  counter, no "0 of 0". Never a generic placeholder graphic, never a spinner
  without a job, and never keep showing the same explanation after the first
  subscription.

### 3b. What Turn 7 decides

Turn 7 makes the MTP sync a task in its own right. It is **redesigned**,
and the sync for YouTube audio tracks and podcast episodes is built with it.

**Folders (resolves the earlier open question O-6):**

| Source | Target folder | Cap | Rationale in the design |
| --- | --- | --- | --- |
| Playlists | `/Music/Reprise` | no cap | existing behavior |
| YouTube audio tracks | `/Music/Reprise-YouTube` | 8 GiB, oldest first | the folder name is Android's only sorting aid — audio tracks do **not** belong under `/Music/Reprise` |
| Podcast episodes | `/Podcasts/Reprise` | 4 GiB | Android's own `/Podcasts` folder; the media scanner recognizes it and keeps it out of the music library |

The defaults are suggestions: 7d gives every source a device browser with
storage selection, "New folder", target preview and a warning when the chosen
folder lies inside the playlist target. Files that have already been synced
stay where they are and are **moved, not copied twice**, on the next sync.

**MTP reality that binds the design:**

- MTP knows no paths. Folders are object handles under a `StorageID`.
  What is persisted is `StorageID` + path string; handles are re-resolved on
  every reconnect because they are not stable. Hence a browser instead of
  text entry.
- Sizes and changes come from `GetObjectPropList` — one roundtrip per
  folder instead of file-by-file queries.
- Deleting and copying run serially, one transfer at a time; progress
  from the send callback.
- Creation via `SendObjectInfo` (association). Some devices forbid that in the
  root directory — then show an error and suggest a subfolder.
- A folder cannot be moved across storage boundaries. A change means
  copying anew and cleaning up the old target.

**Transfer profile:** stays for music ("Music · Opus 160 kbit/s", lossless is
transcoded, lossy stays untouched). Podcast and YouTube audio is **always
copied 1:1** — it is already Opus or AAC.

**States that are missing today:** "Device contents never verified" as a
checkable state with the action "Scan device"; the storage bar segmented by
category including a hatched "Incoming this sync"; diff by category
(`0 new · 3 removed`, `source off`, "Unavailable, kept on phone");
"Sync automatically when this phone connects".

**7b — Settings "Online sources":** one page, three blocks on one level, per
block one master switch and at most three rows. At the very top a global
latch "Use online sources"; off means local player, no requests, no
downloads, and the three sidebar entries disappear. Subscriptions and
favorites are preserved, they are never deleted.

State after the design update of 2026-07-28 — **four** blocks in this
order. Note: "Phone sync" carries **five** rows and thereby breaks the
earlier "at most three rows" description; this table is authoritative.

| Block | Subtitle | Rows |
| --- | --- | --- |
| **Phone sync** | Same rules for every device — folders stay per device | Sync playlists `Selected playlists` · Sync YouTube audio `Marked channels · cap 8 GiB` · Sync podcast episodes `Off` · Music transfer profile `Opus 160 kbit/s` · Target folders `Per device →` |
| YouTube | Channel feeds, audio via yt-dlp | Episodes per channel `Latest 10` · Hide Shorts `On` · `yt-dlp 2026.07.04` with `Update` |
| Podcasts | RSS feeds, search via Apple Podcasts | Episodes per show `Latest 25` · Download new episodes `Off` · Delete played episodes `After 7 days` |
| Radio | Directory: radio-browser.info | Search order `Most voted` · Report plays to the directory `On` |

The subtitle of the Phone sync block is the whole contract in one sentence: the
**rules** apply to every device, the **folders** stay per device. The last
row "Target folders · Per device →" is the jump-off into the device view.

Technically: three booleans plus a global `online-sources-enabled` as an
**AND condition before every request** — explicitly for covers, portraits
and lyrics too, so that "off" really means off.

**Addendum of 2026-07-28 — sync rules are global.** This change came after
the first version of 7a/7b and takes precedence over the paragraphs above:

- 7b gets a block of its own, **"Phone sync"**: syncing playlists,
  YouTube audio tracks and podcast episodes, caps and the music transfer
  profile. These rules apply **to all devices**; in the settings there is
  **no** device selection. Last row of the block:
  "Target folders · Per device →".
- 7a now shows the same values **read-only** ("rules from Preferences",
  "Same on all devices"). Per device only the target folder picker remains.
- **7e** records this: the rules are global, per device only the target folder
  and sync state — folder structures differ between phone and DAP.
  Defaults are set automatically so that the picker mostly stays untouched.

Consequence for the task cut: **E1 gets smaller** (the targets now carry only
the folder and sync state per device, the rules live globally), **G1 gets
bigger** (the "Phone sync" block belongs in the settings page). 7b and 7e are
to be read in full before block E starts; the paragraphs above describe the
superseded device-local design.

**7f — sync in two visible phases.** Solves the problem that a sync
needs files that have not been downloaded yet:

1. **Preparation** — the sync overview lists "2 files to download · 312 MiB"
   with titles and carries a switch "Download missing files before syncing"
   (on by default when online). The primary button is then called
   **Download & sync**, otherwise **Sync now**.
2. **Transfer** — "Step 1 of 2 · Downloading 1 of 2 · 62%" with a bar, then
   the actual transfer. Cancelling keeps finished downloads.

The decisive trick against mental work: "Sync to phone" on an episode
**without** a file sets `wanted_on_device`, and the download follows
automatically. Nobody has to reason through "download first, then select".
Preparation uses the same download manager with priority, not a second path.

Edge cases that carry the contract:

- **Offline** the sync still runs: existing files go across, missing ones
  are skipped with a note ("2 episodes skipped · not downloaded") and
  stay queued. That is the sync expression of `NET-3` (block F).
- On a **metered connection** preparation is offered, not started.
- With **online sources switched off** (`online-sources-enabled` off, G1b)
  the phase does not exist at all.

`wanted_on_device` is new persistent state and therefore belongs in E1, not
in the presentation.

**7c — device card:** names the direction instead of a meaningless balance.
Four states: "14 to copy · 2.6 GiB · 3 to remove", "3 to remove · frees
148 MiB" (here 0 B is correct and must not look like "nothing to do"),
"Up to date · synced 12 min ago", "Tap to scan device contents". The tooltip
carries the full balance; during the sync a thin progress line at the bottom
of the card replaces the text.

## 4. Impact on `docs/ux-rules.md`

Rule IDs are append-only; replaced rules stay in place as
`[replaced by <ID>]` and their tests are re-pointed in the same
commit.

| Rule | Change |
| --- | --- |
| `SRC-2` | Stays for the toolbar. The new compact `+ Add` in result rows is a **different** surface and needs a rule of its own — `SRC-2` continues to apply to the toolbar button. |
| `SRC-3` | "Every source has exactly one add dialog" stays true and becomes even stricter through the hard separation. The sentence "search returns grouped results" must be restricted to one provider → `[replaced by SRC-3a]`. |
| `SRC-5` | The part "group YouTube hits by channel and hide already subscribed …" stays. The code test `src_5_search_orders_the_calling_library_source_first` checks the old two-provider order and must be replaced. |
| **new `SRC-6`** | Hard provider separation of the add dialogs. |
| **new `SRC-7`** | Unified `+ Add` / "✓ Added" row pattern including an accessible name. |
| **new `SRC-8`** | Shared empty-state grammar for all three sources (6f). |
| **new `NET-3`** | App-wide offline presentation contract (6e, #107) — consolidates `NR-6`, `NR-8`, `CONC-4b`, `LYR-3`, `INST-12`. |
| `POD-5` | The cleanup policy meets "Keep N downloaded" from 6b — decided: global is the default, the channel value wins (O-5). |
| `NET-1` | Is **extended** by 7b: beyond today's four network modules comes a global latch `online-sources-enabled` as an AND condition before every request, for covers, portraits and lyrics too → `[replaced by NET-1a]`. Remote images (block C) fall under it as well. |
| **new `SET-*`** | Settings page "Online sources": three blocks of equal rank, each with a master switch and at most three rows; a switched-off block hides its sidebar entry, stops requests and deletes nothing (7b). |
| **new `MTP-*`** | Three named sync targets with `StorageID` + path, freely selectable via a device browser, a cap per target, diff by category, "Device contents never verified" as a checkable state (7a, 7c, 7d). Existing `MTP-` rules are to be checked for collisions beforehand. |

## 5. Task cut

Every task is one commit, test-first, with the full gate battery
(`cargo fmt --check`, `cargo clippy --all-targets --workspace -- -D warnings`,
`cargo test --workspace`, `cargo audit`, core purity, file size < 800 lines)
and one line in the ledger `.superpowers/sdd/progress.md`.

### Block A — dialogs (6c) · Issues #101, #102, #103, #99

- **A1 · Hard provider separation.** `preferred_provider_order` →
  `dialog_provider(kind) -> PodcastKind`. `add_dialog::search` searches only
  its own provider. Dialog title and placeholder per source: "Add podcast" /
  "Search by name or paste a feed URL" versus "Add channel" / "Search or paste
  a channel URL". New rule `SRC-6`, test
  `src_6_each_dialog_searches_only_its_own_provider`. Replaces
  `src_5_search_orders_the_calling_library_source_first`.
  *Open:* what a foreign-source URL does in the dialog — see O-1.
- **A2 · Unified row pattern.** `+ Add` in all three dialogs,
  "✓ Added" as the inactive state, footnote "Subscribed channels drop out of
  later searches." with "Show". Translated accessible name per action
  (`Subscribe to {channel}` / `Add {station}`), keyboard order,
  Enter/Space, focus handover after success, repeatable error message. New
  rule `SRC-7`. Closes #102.
- **A3 · Unjam the dialog layout.** Vertical scrolling only, fixed header and
  footer, artwork and action keep their size, title/subtitle
  ellipsize, end spacing to the overlay scrollbar, the last row scrolls
  fully past the footer. Test at normal, narrow and short
  allocation. Closes #101.
- **A4 · Radio results into a bounded scroller.** Today radio hangs its
  `GtkListBox` directly into the vertical dialog; search and
  result summary stay fixed, only the results scroll. Closes
  #99.
- **A5 · Subscriber counts.** `1.2M subscribers · audio only` in
  channel search results and the URL preview. Zero/hidden/erroneous is
  omitted — never an invented zero, never "unknown". One bounded
  yt-dlp process, no N+1. Closes #103.

### Block B — empty states (6f) · Issue #98

- **B1 · Shared empty-state geometry.** One module, three sources inherit:
  the glyph of the sidebar entry, title, one sentence on *what*, one sentence
  on *where from*, exactly one primary button, the secondary path below it. No
  toolbar, no filter row, no counter in a genuine empty state. New rule
  `SRC-8`. The existing classifications from `podcasts_empty_state.rs` and
  `radio_empty_state.rs` are lifted onto the shared surface, not replaced.
- **B2 · Further empty states.** "Nothing matches these filters" with
  "Clear filters" and a visible filter row, "Only Shorts here — show them
  anyway?", "Nothing downloaded yet …", "YouTube is turned off" with
  "Enable in Preferences" instead of an add button.
- **B3 · Radio shortcut chips.** "Metal in DE", "Top voted", "Near you" as
  one-click searches, always visible. "Near you" reads the existing,
  consented location (O-4) and filters by it; without a usable location the
  click instead opens the location setting in Preferences,
  rather than disappearing or searching unfiltered (O-4 addendum, `RAD-5`).

### Block C — remote images

- **C1 · Image module.** Channel `thumbnails`, iTunes `artworkUrl600`,
  radio-browser `favicon`; cache, bounded size, fallback to the
  source glyph, no fetch without the consent `NET-1` demands.
  Blocks the image parts of A2 and B1, which are therefore shipped first with
  the existing `source_image` surface and glyph fallback.

### Block D — complete the channel detail (6b) · Issue #106

- **D1 · Download column with file sizes** and a header total
  ("10 of 487 · 3 downloaded · 1.2 GB").
- **D2 · "Keep N downloaded"** as a channel property, aligned with the
  existing cleanup policy from `POD-5`: global is the default, the
  channel value wins (O-5).
- **D3 · "On phone" column** — a mirror of the sync state, writable only via
  the channel toggle.
- **D4 · Download errors classified and sanitized.** A visible reason,
  reachable without a pointer; clean retry through
  queued/downloading/downloaded; fresh provider output on retry; cleanup
  of `.part` and post-processor leftovers; no signed URLs, query strings,
  credentials or local paths in the UI or normal logs. Closes #106.

### Block E — MTP redesign and sync for YouTube and podcasts (6d, 7a, 7c, 7d)

The largest block. Today the existing MTP sync covers only playlists; two
content kinds and a new device view are added here.

- **E1 · Sync target model in the core.** A single managed device folder
  becomes three named targets, each with a `StorageID`, path string,
  activation and an optional size cap. Pure data layer. **No migration** — see
  section 1b; the old single target is replaced, not carried over.
  Unit tests for resolution and cap computation; no UI.
- **E2 · Content selection per source.** Playlists ("2 of 4 selected"), YouTube
  ("2 of 6 channels · latest 5 each") and podcasts ("Unplayed downloads only")
  each yield the target set of files. The channel toggle from 6b and the
  show selection feed in exactly here.
- **E3 · Diff by category.** The sync plan is broken down per source
  (`0 new · 3 removed`, `source off`, "Unavailable, kept on phone") and yields
  the balance "To copy / To remove / Playlists rewritten". Explicitly
  test-driven, because this is exactly where 7c lies today.
- **E4 · Transfer.** Music continues to follow the transfer profile; podcast and
  YouTube audio is copied 1:1. Cap enforcement "oldest first" when
  exceeded. Serial transfer, progress from the send callback.
- **E5 · Device view (7a).** Segmented storage bar with
  "Incoming this sync", "Device contents never verified" as a checkable state
  with "Scan device", a "Content" section with target folder, selection and cap
  per source, "Next synchronization" with diff and balance, "Remove from phone
  when deleted or unsubscribed here", "Sync automatically when this phone
  connects", "Eject". **Addendum:** the switch was at first only specified and
  rendered here, but read by no code — a dead switch. Its behavior
  (automatic sync start after a verified scan and planned work,
  silent on refusal/error) is now implemented (`MTP-30`).
- **E6 · Target folder browser (7d).** Storage selection internal/SD, tree from
  `GetObjectPropList`, "New folder" via `SendObjectInfo`, target preview, a
  warning for a folder inside the playlist target, "Reset to default". Files
  that have already been synced are moved on the next sync instead of copied
  twice. Error path for devices that forbid creation in the root
  directory.
- **E7 · Sidebar device card (7c).** Four states with a direction, the full
  balance only in the tooltip, a progress line during the sync.

- **E8 · Sync rules onto the device page (consequence of `E-6`).** Selection
  per source, caps and transfer profile become operable where today they only
  stand as a display; the cross-reference "rules from Preferences" / "Same on
  all devices" disappears, the planned "Phone sync" block in 7b is dropped
  without replacement, and the settings keep only the "Online sources" latch.
  The data model stays unchanged — what moves is the control surface, not the
  storage. `MTP-28` records the abolished separation as `[active]` and is
  superseded via `[replaced by …]`, and so is `SET-8`'s requirement for the
  Phone sync block. **Most dangerous spot in the task:** display becomes
  control, and that is exactly where three switches have already appeared on
  this branch that render and store but are never read. Every relocated
  setting needs a test that proves the *behavior* differs between its two
  positions.

- **E9 · Visible two-phase sync (7f).** So far only `wanted_on_device` is
  modelled (E1); the two phases from section 3 are missing entirely.
  *Preparation*: the overview lists "2 files to download · 312 MiB" with
  titles, carries the switch "Download missing files before syncing" (on by
  default when online) and names the primary button **Download & sync**
  accordingly instead of **Sync now**. *Transfer*: "Step 1 of 2 · Downloading
  1 of 2 · 62%" with a bar, then the transfer; cancelling keeps finished
  downloads. Preparation uses the existing download manager with priority —
  **no second path**. The three edge cases carry the contract and are the
  actual substance: offline the sync still runs and skips missing
  files with a note ("2 episodes skipped · not downloaded"), they stay
  queued (`NET-3`); on a metered connection preparation is *offered*,
  not started; with online sources switched off the phase does not exist at
  all. Depends on R1, because only then does the waiting list have real numbers.

- **R1 · wire `MTP-45` up live** (finding of the external review). The pure
  projection `selection::select_episodes` satisfies the rule and is tested,
  but the live pipeline never calls it: `query_candidates_for_device` does not
  filter by played, and `files_waiting_for_download` is hard-coded 0.
  Consequence: played episodes travel to the device anyway, even though the
  device page claims "Unplayed downloads only", and a queued episode without a
  file disappears from the balance — since `MTP-30` that means a device reports
  "Up to date" and skips its automatic sync while work is pending. The
  rule-named test checks only the label and would stay green if the selection
  behavior were deleted.

- **R2 · MCP parity for block D/E/F/G** (finding of the external review) —
  identical to H-D through H-G below, noted here only as an open item:
  `device_dto` still knows only the old aggregate view, so an agent can trigger
  a sync but cannot see what it would do.

### Block F — offline (6e) · Issue #107

- **F1 · Contract `NET-3`** for cached, empty, queued, interrupted,
  authentication, rate-limit, provider-failure; migration of `NR-6`, `NR-8`,
  `CONC-4b`, `LYR-3`, `INST-12` onto the shared contract.
- **F2 · Queue instead of greying out.** Download and sync actions are
  accepted, listed as "Queued offline" and executed automatically in turn once
  there is a network.
- **F3 · Radio exception.** Stations stay listed, play reports
  "No connection · Retry" instead of queueing.
- **F4 · Add dialogs offline.** Search field disabled with a one-line
  reason; pasting a URL still works and the subscription comes into being
  at the next fetch.

### Block G — hierarchy and grouping (6a) · Issue #96

- **G1 · Settings page "Online sources" (7b).** One page, three blocks on
  one level, each with a master switch and at most three rows, with the exact
  row set from section 3b. A switched-off block hides its
  sidebar entry and stops its requests, but deletes neither subscriptions nor
  favorites.
- **G1b · Global latch `online-sources-enabled`.** AND condition before every
  request, explicitly for covers, portraits and lyrics too. That extends
  `NET-1` beyond today's four network modules and needs its own
  test per call path, otherwise "off" is not provable.
- **G2 · Remaining reconciliation of 6a.** Check the column set
  `Show · Latest · Episodes`, "Show all N episodes", the header
  "4 shows · 41 episodes · 7 new" against the existing group renderer and
  close only the gap.

### Block H — MCP parity (cross-cutting)

Owner's requirement: **everything the GUI can do must also be available via
MCP.** Today's state does not cover that — `source_tools.rs`
knows only `music_manage_podcasts` and `music_manage_radio` (add/edit/remove/
refresh) plus the cached resources `reprise://podcasts` and
`reprise://radio`. There is **no** discovery, download, sync or
settings surface.

Every block therefore gets its MCP task; it follows the respective
GUI task so that both use the same core function instead of building two
paths:

- **H-A** · Discovery: a source-separated search tool (`SRC-6`) with the
  candidate fields including the optional subscriber count (`SRC-9`). Network
  and subprocess work is capability-gated like the mutations; "already
  subscribed" is filtered exactly as in the GUI.
- **H-B** · Empty states are pure presentation and need no tool; the
  state can be derived from the existing resources.
- **H-D** · Channel detail: window, Shorts filter, download states with
  sizes and the batch actions.
- **H-E** · Sync: target folders, caps, diff by category, `wanted_on_device`
  and triggering a sync.
- **H-F** · Offline: the state from `NET-3` must be readable via MCP so
  that an agent does not blindly run into a queued action.
- **H-G** · Settings: the three master switches, the global latch and the
  "Phone sync" block.

Ground rules for all of them: no signed URLs, credentials or local paths
in responses. **Query strings are exempt** (owner decision of
2026-07-28): `SRC-5` proves with a test of its own that the query string can be
part of a feed's identity — stripping it wholesale would let a
subsequent `add` point at the wrong feed. Instead, userinfo
and fragment are removed, non-HTTP(S) schemes are rejected and artwork URLs are
omitted entirely. It further holds: mutations and network access behind
capabilities; the same core facade as the GUI, so that behavior does not
diverge.

## 6. Order

```
A1 → A2 → A3 → A4 → A5        Dialogs first: self-contained, no data model
        ↘                     image parts wait for C1
G1 → G1b                      settings + global latch: prerequisite for
                              the "module off" state in B2
B1 → B2 → B3                  empty states; B2 needs G1 and F1
C1                            unblocks the image parts of A2 and B1
F1 → F2 → F3 → F4             offline contract, before D and E inherit it
D1 → D2 → D3 → D4             complete the channel detail
E1 → E2 → E3 → E4             MTP core: targets, selection, diff, transfer
        ↘ E5 → E6 → E7        device view, folder browser, sidebar card
R1 → E8 → E9                  selection live, rules onto the device page,
                              then the visible two-phase sync
R2 (= H-D…H-G)                MCP parity, independent of the GUI blocks
G2                            remaining reconciliation of 6a
```

Rationale for the head: block A is self-contained, depends on no
data-model rebuild and closes #101, #102, #103 and #99 in one go. Block F
deliberately stands **before** D and E so that they inherit the offline
contract instead of having it retrofitted later — exactly the mistake that the
stage review note PCR names.

Two dependencies are hard:

- B2 ("Offline & empty") requires F1.
- The image parts of A2 and B1 require C1. Both are therefore
  shipped first with a glyph fallback and caught up in C1; that is
  not rework, but the fallback Turn 6f demands.

## 7. Verification

Three levels, none replaces another:

1. **Pure unit tests** for every projection — provider selection,
   empty-state classification, offline state derivation, size formatting.
   That is the level `search_results.rs` and `podcasts_empty_state.rs`
   already sit on today.
2. **Isolated display tests** for widget construction and allocation, under
   Xvfb, with a private `XDG_DATA_HOME`/`XDG_CACHE_HOME`, a private D-Bus and
   `REPRISE_AUDIO_SINK=fakesink`.
3. **CUA scenarios** under `scripts/cua-e2e/` for the visible states. The
   harness is workable — a complete `responsive-window` run
   produced 213 evidence files with AT-SPI trees and screenshots. New
   scenarios follow the invariant: a fresh `get_window_state` before every
   action and another one after it.

For #104 and #106 the boundary from the issue filing still applies: name causes
only after a red, deterministic fake-provider run. Live YouTube
is not a regression gate.

One precondition: the `responsive-window` scenario currently fails
(#108). The evidence points to a race condition in the harness — two
actions without a settling assertion between `responsive_window.sh:189-190` —
not to a product bug. That should be fixed before the first new CUA scenario,
otherwise every new scenario inherits the same pattern.

## 7b. What only a human can sign off

The automatic gates prove projections, widget construction and
state transitions. They do **not** prove how something looks, feels or
behaves on real third-party hardware. `AGENTS.md` explicitly reserves this
category for rendering, pointer gestures, media keys, Wayland behavior and
the lock screen. What from this work belongs to it:

- **Add dialogs (block A).** Does the compact `+ Add` as a row action feel
  correctly weighted next to the footer bar? Is the acknowledged "Added"
  recognizably done without looking like an error? Does the one-line hint
  for a foreign-source URL read as an explanation rather than a rejection?
- **Empty states (B1).** Do the three surfaces look "unused" rather than
  "broken" — that is the actual promise of 6f and no test can check it.
- **Channel detail (D1).** Are the download column and the header total still
  legible with long titles and a narrow window?
- **Online sources (G1).** The most important point: switch off, then walk
  through podcasts, YouTube, radio, covers, portraits and lyrics — does
  really nothing happen on the network? The promise reads "no requests, no
  downloads, nothing hidden".
- **MTP sync (block E).** A real device run with a phone plugged in. The
  test double proves the order and the bookkeeping, but neither
  handle resolution after a reconnect nor the behavior of the Android media
  scanner with the three target folders.
- **Offline (block F).** Disconnect the network and check whether downloaded
  episodes play unchanged, non-downloaded ones read a dimmed "Needs network"
  and radio offers "No connection · Retry" instead of a queue.

## 8a. Decided

Decided by the owner on 2026-07-28; these points are no longer open
questions and go into the rules `SRC-6` and `SRC-7` in this form.

- **E-1 (formerly O-1) · A foreign-source URL is rejected.** A YouTube URL in
  "Add podcast" — and conversely a feed URL in "Add channel" — is not
  evaluated. The dialog reports in one line that the source belongs to the
  other place, and creates nothing. No silent dialog switch. The hard
  separation from 6c thus applies to search **and** the URL path.
- **E-2 (formerly O-2) · `+ Add` carries icon and text.** Turn 6c is the newer
  source and wins over the icon-only wording in #102; #102 is updated
  accordingly. The accessible name nevertheless remains
  mandatory, because the label alone does not name the source
  (`Subscribe to {channel}` / `Add {station}`).
- **E-4 (formerly O-6) · Folders are decided and freely selectable.** Turn 7
  replaces the sketch from 6d: `/Music/Reprise` for playlists,
  `/Music/Reprise-YouTube` for audio tracks and `/Podcasts/Reprise` for
  episodes, each as a *suggestion* that the device browser from 7d can
  override. The podcast folder deliberately does not sit under `/Music`,
  because Android's media scanner recognizes `/Podcasts` and keeps it out of
  the music library; audio tracks do not sit under `/Music/Reprise` for the
  same reason. The existing managed device folder (`78e379fd`) is migrated
  onto the "Playlists" target (task E1).
- **E-3 (formerly O-3) · "Later" means: from the next submitted search on.**
  A source that has just been added stays in the current hit list as an
  inactive "✓ Added", so that the success is visible. Only the next
  submitted search query filters it out. Today's immediate removal
  of the row (`remove_candidate_result`) is dropped and its test
  `src_5_successful_subscribe_removes_the_result_row` is re-pointed at the new
  state.

- **E-5 · Exactly one MTP device.** Reprise supports one connected device, not
  several. Turn 7e (multiple devices) is dropped without replacement. The
  owner's rationale on 2026-07-29: too much complexity for too rare a case.
  Multi-device operation does not cost a single feature, it drags the question
  "which device does this apply to?" into every rule, every settings row and
  every status display.

  **The data model nevertheless stays device-scoped**: `device_settings` by
  serial number and three `SyncTarget`s per device are built, tested and
  free, and the reason from `E-4` still applies — folder structures
  differ between phone and DAP. Storage is per device;
  what is *managed* is not several. No dismantling of the model.

- **E-6 · Sync rules live on the device page.** Replaces the addendum of
  2026-07-28, whose global-vs-per-device cut was justified exclusively by
  multiple devices ("applies to all devices, no device selection in the
  settings"). With `E-5` that justification is dropped, and with it the
  cross-reference: today the device view shows "rules from Preferences" and
  "Same on all devices" and thereby points at a block that does not exist —
  with one device, "Same on all devices" is a statement about nothing.

  So: the rules (playlists/YouTube/podcasts, caps, transfer profile) move to
  where the device is. The settings keep only the privacy latch
  "Online sources" (`NET-1a`, `SET-8`). The planned "Phone sync" block in 7b
  is dropped as a surface of its own. `MTP-28` records the abolished separation
  as `[active]` and bindingly so, and is therefore superseded regularly via
  `[replaced by …]`, not silently reinterpreted.

- **O-4 · decided on 2026-07-29 · "Near you" uses the existing location
  source.** The question was posed wrongly: Reprise already has a
  *consented* location — `reprise_platform_linux::location::PortalLocation`
  via the XDG desktop portal, plus a city search, used today for the
  concert/tour dates. Inventing a second source would be the forbidden second
  truth (`AGENTS.md`). Two follow-up jobs belong to it: the keys sit
  as `concerts.location_lat/_lon/_name` in the namespace of a single feature
  and are hoisted to app level as soon as radio is the second user (without
  installations that costs nothing); and radio-browser filters by
  **country code**, whereas what is stored is latitude/longitude plus a
  display name — the city search supplies the code anyway, only the portal
  path needs reverse geocoding once, which like every network path falls under
  `NET-1a`. Without a location set the chip does not appear; it never asks of
  its own accord.

- **O-4 addendum · decided on 2026-07-29 · The hoist carries the
  consent; "Near you" stays visible.** Two points on O-4 above that the owner
  sharpened up on the same day, both binding:

  Firstly, moving the location keys out of the `concerts.` namespace
  to app level (`reprise_core::location`, keys `location.lat` /
  `location.lon` / `location.name` / `location.country_code`) **does in fact
  carry the existing consent forward**. Radio never asks for the location on
  its own and calls neither the portal nor a geocoder itself —
  it reads exclusively the same value that Concerts writes and reads via
  `reprise_core::concerts::config::location` (today a pure forwarding
  call onto the same place). There is no second copy;
  Concerts' write path (city search, "Use current location", "Clear") remains
  the only one.

  Secondly, the last sentence of O-4 above changes: reverse geocoding for
  the portal path is **not** built — an additional network call purely
  to procure a country code for "Near you" would be exactly the new,
  unconsented network flank that this decision was meant to avoid.
  A location set via the portal therefore honestly stays without a country
  code; the code comes exclusively from Nominatim's `addressdetails`, an
  extension of the same forward geocoding request that the city search
  already makes anyway. And: "Without a location set the chip does not
  appear" no longer holds — the chip is **always** visible. If a
  usable location is missing (none set, or one without a country code),
  a click instead opens the location setting in Preferences, via
  the same deep link (`PreferencesContext::present_location_settings`)
  that `present_plugins` already uses for the online lyrics settings button —
  rather than disappearing or passing off an unfiltered search as "near
  you". Rule `RAD-5` carries both halves.

- **O-5 · decided on 2026-07-29 · Global is the default, the channel wins.**
  "Keep N downloaded" on the channel wins where it is set; the global
  cleanup policy applies to all remaining channels. Today's `KeepLast5`
  thereby becomes the global default "Keep 5" — "Keep N" is only its
  generalization anyway. Explicitly **not** the minimum of the two: a channel
  on "Keep 20" with a global "Keep 5" would be silently capped, i.e. show 20
  and keep 5 — the same kind of untruth as a balance that reports
  "nothing to do" while three files are being deleted.

- **O-7 · decided on 2026-07-29 · No OPML import.** The podcast
  empty state stays with the feed URL path as it ships today. The
  line "or import an OPML file" from Turn 6f is not adopted, because no
  import path exists and an empty state must not promise anything that does
  not exist. The question is closed, not deferred.
- **E-9 · decided on 2026-07-29 · Zero means "unlimited" everywhere.**
  On the device page two zeros with opposite meanings stood
  side by side: the size cap `0 GiB` = *unlimited* (that is how `cap_bytes`
  has been modelled as an `Option` since `MTP-38`) versus `MTP-36`'s
  "latest N" `0` = *nothing from this channel*. `MTP-36` is the outlier and
  was not yet implemented, so the alignment cost one line of rule text instead
  of a rebuild. "Nothing from this channel" is still said by the channel
  switch from 6b — that is not a quantity, so it is not expressed as a
  quantity either. `MTP-36` carries the decision in its rule text.
- **E-10 · decided on 2026-07-29 · `MTP-41`'s YouTube clause was wrong,
  not the code.** An external review flagged as P1 that
  `query_selection_candidates_for_device` admits only already downloaded or
  explicitly `wanted_on_device` episodes as candidates, while
  `MTP-41`'s text promised "latest N episodes … regardless of download state".
  The owner has decided: the code is right, the rule text was wrong — an
  unwanted, missing episode must never silently trigger a download, only to
  fill an N-episode quota. `MTP-41` becomes `[replaced by
  MTP-45]` in accordance with the append-only contract; its text stays in
  place as a record of what it once said. `MTP-45` takes over
  playlists, podcast episodes and the waiting-versus-ready distinction
  unchanged and corrects
  only the YouTube clause to what the code has always done: N
  limits only the set of already downloaded or wanted episodes per channel,
  not "every episode ever published".

## 8. Open questions — do not decide locally

**None at present.** O-1 through O-7 as well as E-9 and E-10 are decided and
set out with their rationale in section 8a. New cases that no rule covers still
come here — as a `[planned]` draft with `<!-- REVIEW: rule proposal -->`, not
answered in code (`AGENTS.md`).


## 9. Explicitly not included

- All other turns of the design document.
- #100 (selection versus now-playing highlight) — standalone, source-independent.
- #104 and #105 (streaming stalls, buffered range in the waveform) —
  playback, not source management.
- #97 and #108 (responsive edges, CUA scenario) — relevant only as a
  precondition for the CUA coverage in section 7.
