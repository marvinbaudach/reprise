# Reprise — UX rulebook (binding)

This document is the project's single source of UX truth. In case of
conflict, it beats the existing code — for `[active]` rules immediately
(deviation = bug), for `[planned]` rules it binds all future work in that
area. Acceptance tests reference rules via their IDs.

## Process rules

**Status.** Every rule carries exactly one status:
`[active]` = enforceable — the code conforms, a rule-named test is
green and is a merge blocker. `[planned]` = an agreed target state, not yet
enforceable. A rule switches to `[active]` **in the same commit** that
implements the behavior or proves it with a test — never after the fact.
Half-implemented rules are immediately split into sub-rules (a/b), so that
no test is ever written against half a rule.

**IDs.** IDs are append-only: never renumber, never reuse. If the meaning
changes, a new (sub-)rule replaces the old one; the old one remains as
`[replaced by <ID>]` — as a signpost, never as deletable ballast. Tests
against replaced rules are re-hung onto the new rule in the same commit.

**Test levels.** Every rule carries a level tag: `[core]` (reprise-core,
workspace suite), `[gtk]` (widget/logic tests in reprise-gnome), `[e2e]`
(cua-e2e harness against the real app), `[manual]` (RELEASING.md checklist,
which references the same rule IDs). Testing happens at the **lowest level
that can disprove the rule**. Timing numbers (100 ms, 150 ms, …) are design
intent, not assertions: the *what* (feedback exists) is automated, the
*how fast* is checked manually. If a `[manual]` rule later becomes
automatable, only its tag changes, never its ID.

**Traceability.** A test carries **exactly one primary rule ID in its
name** (Rust: `fn play_1a_…`, cua-e2e scenario: `play-1a-…`). If a scenario
happens to cover further rules along the way, that does not count — the
second rule needs its own test. `#[ignore = "UX <ID> [planned] — …"]` is
only allowed on `[planned]` rules. `scripts/check-ux-traceability.sh` (part
of the merge gate) enforces: every `[active]` rule has ≥ 1 test — for
`[manual]`, a literal ID reference in `RELEASING.md` instead · neither a
test nor the checklist references an unknown or replaced ID · no disabling
ignore on `[active]` · every disabling ignore on a rule-named test follows
the format above. Only real `#[test]` functions or executed cua-e2e lines
count as coverage — a helper fn of the same name or a comment does not
green the gate. The display-runner marker
`#[ignore = "requires a display; run via xvfb-run"]` is not a disabling
ignore: such tests run as a merge blocker via
`scripts/check-display-tests.sh --rule-named` and count as coverage even
for `[active]` rules.

**Reachability.** For every action whose visibility is bound to a state,
the test question applies: **How does the user get into the state that
shows it?** If the answer is "via exactly this action" or "not at all",
the rule is incomplete — regardless of how correct each individual
condition is. A rule-named test must walk the path **from the starting
state**, not construct the target state and then check it. Two findings
forced this: "Hide" was only reachable from the digest, which only
appeared on overflow; and New Releases could never populate itself,
because ✦ requires entries, "Fetch now" sits behind ✦, and no initial
fetch existed (NR-8). Both times every individual test was green — the
bug sat between the rules, because each test pre-established the target
state.

**Language.** This document and the design docs are German — the
project's working language. Tests and scripts are code and therefore
English (AGENTS.md); rule IDs and status tokens are quoted verbatim
there.

**Changes.** If you encounter a case while implementing or testing that no
rule covers: **add a rule, don't decide locally.** Agents do this by
adding a `[planned]` draft with the next free ID in the affected section,
marked with `<!-- REVIEW: rule proposal -->` — the decision rests with the
human. Rationale for changes lives in the git history.

## A. Core principles

- **P-1** [planned] [manual] — Every feedback role has exactly one
  mechanism: announcing an event = toast · state of a view =
  StatusPage/inline · running process = progress card · open request =
  badge. An event may serve multiple roles at once (disconnect →
  toast + StatusPage), but never two mechanisms in the same role (never
  two toasts, never toast + dialog as a duplicate announcement).
- **P-2** [planned] [gtk] — Click responds instantly: every click
  produces visible feedback (state change, spinner in the button,
  selection), target < 100 ms. Never a click into the void. The *what* is
  automated (state after click ≠ starting state); the 100 ms are a manual
  checklist item.
- **P-3** [planned] [gtk] — Hover never navigates: hover shows (tooltip,
  pills, +3% area), click acts. No hover-to-open.
- **P-4** [planned] [manual] — Nothing shifts uninvited: layout shifts
  only as a direct consequence of a user action or a process started by
  them (sync removals collapse, force-show of the filter row on the
  user's own search input, FIL-2). Fade-ins (device card, ISSUES, chip
  contents of the filter row) fade in without reflowing neighboring
  content; for dynamically appearing elements (the old-value row in the
  tag editor) space is reserved. Background events (scan, watcher, mount)
  never shift visible content under the cursor.
- **P-5** [replaced by BROWSE-6] — The earlier rule coupled the local
  listening history to the library entry. BROWSE-6 separates historical
  events from the current catalog.
- **P-6** [active] [core] — Evidence rule: what is provably present is
  shown/healed (mount event, resurrect); what is provably gone is marked
  honestly right away (eject). Assumptions (unmounted) are never grounds
  for deletion.

## B. Navigation model

- **NAV-1** [replaced by BROWSE-1] — Sidebar = places, content = mode.
  The sidebar chooses the place (Music, Queue, Playlists, My Stats,
  Devices, Issues). Within "Music" the switcher toggles the mode:
  Tracks | Albums | Artists.
- **NAV-2** [planned] [core] — One global history stack across the
  entire content area, even across place boundaries (Queue → Artist
  detail → Back → Queue). Content clicks (NAV-3) always push; Alt+← /
  mouse-back / header-‹ pop. A sidebar click replaces the stack (places
  are restarts, not stack entries). The sidebar highlight follows the
  topmost stack entry — if the stack shows Artist detail after a Queue
  click, "Music" is highlighted.
- **NAV-2a** [planned] [core] — The stack does not survive the session
  (session restore only restores the topmost view, START-1 unchanged);
  Back with no stack entries is disabled, never a no-op.
- **NAV-3** [planned] [e2e] — Clickable metadata is the same everywhere:
  in every track list (Library, Playlist, Queue, Album detail, Top
  Tracks) the following applies: click on artist name → artist detail;
  click on album name/cover → album detail; both push the global stack
  (NAV-2). Hover shows an underline as affordance. Also applies in the
  player bar (there: artist/album click per this rule, cover/title per
  GRID-5).
- **NAV-4** [planned] [gtk] — Double-click on a row = play in the
  context of the visible list (see PLAY-2). Single click = select.
  Enter = like double-click. Exception Queue view: double-click jumps to
  the track (playhead) per QUE-3, instead of rebuilding the queue.
- **NAV-5** [replaced by BROWSE-2] — Mode memory (scroll + selection per
  Tracks/Albums/Artists) applies only within the session; sidebar/place
  changes also preserve the scroll + selection of the mode being left.
  The scroll anchor consists of track/album ID plus offset, never a raw
  pixel value; re-sort and insert therefore keep content at its
  position. START-1 restores, across restarts, only the last active
  view including scroll position; all other modes start at the top,
  unselected.
- **NAV-6** [active] [e2e] — Search (Ctrl+F) filters the current view
  live; Esc clears and closes. Search never navigates on its own.
- **NAV-7** [active] [e2e] — Hamburger menu: "Scan Library" → starts the
  scan, stays in the view (card appears). "Preferences" → Preferences
  window. "Keyboard Shortcuts" → shortcuts overlay. "About Reprise" →
  About dialog. No menu item silently switches the content view.
- **NAV-8** [planned] [gtk] — My Stats is a sidebar place like any
  other: full content area, the header bar with search stays put (search
  there being disabled/hidden is allowed, but the bar remains).
- **NAV-9** [replaced by NAV-9b/GRID-5] — Originally cover/title in the
  player bar and Ctrl+L shared the same jump to the home of the playing
  track. Split into track origin via Ctrl+L (NAV-9a) and album-grid
  reveal via player surfaces (GRID-5).
- **NAV-9a** [replaced by NAV-9b] — Ctrl+L navigates to the origin view
  of the loaded track, selects its row and centers it without
  scrollIntoView edge-sticking. The jump pushes onto the global history
  stack; Back returns to the previous place.
- **NAV-9b** [replaced by BROWSE-4] — Ctrl+L and player metadata were
  previously treated as a shared track jump. BROWSE-4 separates track,
  album, and artist intents app-wide and keeps the explicit track jump
  for Ctrl+L and the player title.
- **NAV-11** [active] [gtk] — Every operable sidebar entry exposes its
  own label, an interactive role, and a triggerable action to assistive
  technology. Section headings remain non-operable, but are exposed
  semantically as headings.
- **NAV-12** [replaced by NAV-2] — The global back history as a named ‹
  button in the header bar (disabled without a previous place, active
  after a navigation, restores the previous place including focus when
  triggered) belongs, in the single-track browser, to the NAV-2 history
  complex; Album and Artist are no longer separate views but scopes of
  the music list.
- **NAV-13** [replaced by NAV-10a] — Starting playback is not
  navigation: Enter or double-click on a track row leaves selection,
  keyboard focus, and viewport unchanged; only the now-playing marker
  changes. The separation of marking and scrolling in the one track
  browser is now governed by NAV-10a.

## C. Playback, queue, shuffle, filter

- **PLAY-1** [active] [gtk] — Queue source = visible track list. "What
  you see is what plays": double-click/Play all/Shuffle in a track list
  build the queue from the currently visible (filtered, sorted) list.
  PLAY-1a applies for container buttons.
- **PLAY-1a** [planned] [core] — Container play (play button on cover,
  Play all/Shuffle in hero areas) builds the queue exclusively from the
  container in its canonical order (Album: disc/track number; Playlist:
  position order; Artist "Play all": albums by year, then track number
  within). The visible grid filter only determines which containers are
  reachable, never the queue content.
- **PLAY-2** [active] [core] — Double-click plays the row and appends
  the rest of the visible list from this position into the queue.
- **PLAY-3** [replaced by PLAY-3a/PLAY-3b] — Original umbrella rule
  "filter constrains shuffle"; split per the process rule for
  half-tested rules into hit-shuffle (3a) and filter-afterward (3b).
- **PLAY-3a** [active] [core] — Filter constrains shuffle —
  intentionally. Filtered playlist + shuffle = shuffle over the hits
  ("shuffle my 90s tracks"); the queue is exactly the hit set, no track
  from outside it.
- **PLAY-3b** [active] [gtk] — Changing the filter afterward does not
  touch an already-built queue (the queue is a snapshot; visible in
  "Queue").
- **PLAY-4a** [active] [core] — Missing in lists: list playback and
  queue advance skip Missing silently.
- **PLAY-4b** [active] [gtk] — Double-click on a concrete Missing row:
  toast "File missing since …" + button "Show in Missing files".
  Enqueueing (Play next/Add to queue) is disabled for Missing.
- **PLAY-5** [replaced by PLAY-5a/PLAY-5b] — Original queue-hygiene
  umbrella rule; split during hardening into the sub-rules deleted (5a)
  and unmounted (5b).
- **PLAY-5a** [active] [core] — Deleted hygiene: externally deleted
  tracks leave the queue silently; the playing track is never stopped by
  this (if the playing track itself faults, FB-6 applies: skip + one
  toast).
- **PLAY-5b** [active] [core] — Unmounted hygiene: unmounted tracks stay
  gray in the queue, are skipped on advance, and heal on the mount event
  (P-6). No background event (deleted, unmounted, sync removal, watcher)
  stops the playing track — explicit user actions (double-click, Play
  all, OS-open) naturally change playback.
- **PLAY-6** [planned] [gtk] — Shuffle/Repeat are global player states
  (player bar), not view states. Repeat cycles: off → all → one.
- **PLAY-7** [replaced by PLAY-7a] — The player bar is a structural
  delineation, not an overlay: it claims its own height in the layout,
  and no content element (track list, sidebar, right info column) ever
  runs under or behind it. Its background is opaque.
  <!-- REVIEW: rule proposal -->
- **PLAY-7a** [replaced by PLAY-7b] — Header, open search, and the
  player bar sit as global glass zones over all library views. The
  content runs visibly underneath; its scroll start and end receive
  exactly the actually allocated height of the overlaying top/bottom
  zone as a scroll inset, so that no last row remains hidden or
  unusable. The player bar works mirrored, top and bottom.
- **PLAY-7b** [active] [gtk] — The player bar is once again a
  structural delineation instead of an overlay: it claims its own
  height at the top or bottom in the layout, and no content element
  runs under or behind it. Its background is opaque.

- **PLAY-8** [active] [core] — **Playback is an immutable snapshot.** At
  start, ordered track IDs, cursor, complete browser origin, and its
  display name are frozen. Later navigation, search, facets, or even
  refining down to zero hits change neither the snapshot nor the running
  track. After the last track, playback ends with Repeat off, unless an
  explicit Up Next entry follows; deletion hygiene is governed by
  PLAY-5a/5b.
- **PLAY-9** [active] [gtk] — Play/Pause, with playback stopped and no
  loaded title, queue snapshot, or "Play Next", immediately starts a
  randomly chosen existing library title. For this, an immutable
  snapshot is created from all existing library titles in random order;
  Missing and deleted titles are excluded. With an empty library,
  Play/Pause stays disabled and playback stays stopped.

## D. Albums & artists view

- **ALB-1** [replaced by GRID-2/GRID-4] — Original shared album-grid
  rule for hover overlay, activation, container play, and context menu;
  split into operation/actions (GRID-2) and overlay appearance (GRID-4).
- **ALB-2** [planned] [gtk] — Album detail: hero with cover + dominant
  color area (accent pipeline), Play all/Shuffle pills (PLAY-1a), track
  list by disc/track number. Playing track: accent row + EQ icon + bold
  — identical in every list in the app (one marking language).
- **GRID-1** [replaced by BROWSE-1] — Persistent playing state: the
  loaded album shows, independent of hover and focus, the shared EQ
  badge in the top left of the cover and a 1.5px inner ring around the
  cover. Both use `@reprise_player_accent`. On pause, the ring stays and
  the EQ motion freezes; with `gtk-enable-animations=false` the glyph is
  static.
- **GRID-2** [replaced by BROWSE-1] — Operation and actions: the native
  GtkGridView moves focus two-dimensionally with arrow keys. Enter opens
  the album detail source as a history push, Ctrl+Enter replaces the
  queue with the album in canonical disc/track order and starts at
  track 1. Space remains global Play/Pause. Menu key and Shift+F10 open,
  at the focused tile, the same menu as right-click, exactly with Play,
  Play next, Add to queue, Go to artist, and Edit tags….
- **GRID-3** [replaced by BROWSE-1] — Visible focus and state
  composition: keyboard focus draws a 2px outer ring in `@accent_color`
  only around the cover and shows the same play affordance as hover.
  Playing, focus, and hover remain separate state layers: playing
  inside, focus outside, interaction overlay on top; combined states do
  not obscure each other.
- **GRID-4** [replaced by BROWSE-1] — Bottom gradient overlay: hover or
  focus fades in a bottom-anchored darkening gradient instead of a
  floating tooltip box. It contains a thin meta line ("13 tracks ·
  47 min") and, bottom right, a Play/Pause button in
  `@reprise_player_accent`; album and artist stay below the cover. The
  cover's center stays free. The card container has no metadata tooltip;
  only actually ellipsized title/artist labels show their full text.
- **GRID-5** [replaced by BROWSE-4] — Revealing the playing album:
  activating cover or title in the player bar or now-playing panel
  switches, if needed, to the album view, clears a visible search field
  including album filter, scrolls via GtkGridView/Adjustment to the
  loaded album tile, focuses it, and highlights it for about 1 s. The
  place change is a history push; if already in the album grid, no
  duplicate is created. If the album tile is missing, NAV-9b applies
  without an error dialog. `gtk-enable-animations=false` shows a static
  highlight for the same duration.
- **GRID-6** [replaced by BROWSE-2/BROWSE-4] — Return focus: Back from
  an album detail into the album overview restores keyboard focus onto
  exactly the previously activated album tile and scrolls it into view
  if needed.
- **GRID-7** [replaced by BROWSE-1] — The album overview carries, behind
  its cards, a subtle texture of the currently playing cover. The cover
  is downscaled to 32px and pre-rendered blurred exactly once per track
  change; when drawing, only this cached texture is scaled, never is a
  live blur run over the list. Without a cover, after stop, and in High
  Contrast the texture stays invisible. It is not interactive and uses
  the cover content, but does not tint any chrome surface.
- **GRID-8** [replaced by BROWSE-1] — The album overview fills the
  entire available height of the library area, independent of the
  number of visible cards. Ambient layer, content, grid page, and
  scroller stay vertically expanded after switching from Tracks or
  Artists; card rows are neither cut off nor constrained to their
  natural total height.
- **ART-1** [replaced by BROWSE-1/BROWSE-4] — Artist list: click selects
  and shows detail on the right; selection NEVER follows playback, the
  playing artist only shows a mini EQ.
- **ART-2** [planned] [gtk] — Artist detail: hero glow (precomputed
  texture, 250 ms crossfade on change), album row (hover like ALB-1),
  Top Tracks (double-click plays per PLAY-2 in the context of "Top
  Tracks"). "Show all N tracks ›" → Tracks mode in the artist scope; its
  visible and, via ×, removable scope chip is already active via FIL-1c.
- **FX-1** [planned] [manual] — All effects respect
  `gtk-enable-animations=false` (hard switch) and only run GPU-cheap
  (opacity/transform, pre-rendered glows). No live blurs in lists.

## E. MTP / Sync

- **MTP-1** [active] [gtk] — A newly connected Android MTP device
  produces a device-name-specific connected toast and a device card in
  the sidebar. It never automatically navigates away from the current
  view.
- **MTP-2** [replaced by MTP-13]
- **MTP-3** [active] [gtk] — The device card and an open device page
  project the same device-related runtime state. Syncs of different
  devices may run in parallel; start and cancel act exclusively on the
  named device, and a late progress event of a cancelled run is
  discarded via its generation.
- **MTP-4** [active] [gtk] — Eject lives exclusively on the device
  page. It is only active for a connected, non-syncing device; during
  sync and finishing it is disabled and explains the reason in a
  tooltip.
- **MTP-5** [active] [gtk] — On unplugging, the device card disappears,
  an open device page remains readable as a "Device disconnected"
  status, and a running device-related sync is cancelled. A
  reconnect-capable device resumes the remaining safe mirror plan upon
  reconnecting; incomplete `.part` files are cleaned up before the next
  publish.
- **MTP-6** [active] [gtk] — Finishing is projected as complete
  progress. Afterward, the lifecycle toast shows the completion or
  error status, and the device card switches back, without a separate
  100% hold state, to the current idle/synced state.
- **MTP-7** [active] [gtk] — The device page presents fully known
  storage as a theme-colored segment bar of Music, planned after-sync
  growth, Other, and Free; the same values remain available as text.
  With incomplete or inconsistent capacity, the bar disappears, and the
  text states "unknown" instead of inventing proportions.
- **MTP-8** [active] [gtk] — The device page offers exactly three
  transfer profiles: Opus at 160 kbit/s as the recommendation and
  default, MP3 at 256 kbit/s as a compatibility fallback, and unchanged
  original files. A lossy source format, or one not unambiguously
  recognized as lossless, is copied unchanged under every profile and
  never transcoded into another lossy format.
- **MTP-9** [active] [gtk] — The device page names the write access, as
  reported by GIO, of the chosen target storage as "Writable",
  "Read-only", or "Write access unknown". A confirmed write-protected
  target locks the sync start and explains the reason; unknown values
  are not reported as write capability and do not block preemptively.
- **MTP-10** [active] [gtk] — An error-free transfer stays "Finishing"
  until Reprise has re-read the managed device folder. Only this
  successful read-back produces the completion toast and a page summary
  labeled "Verified" with the actually found count of managed tracks; a
  failed read-back does not claim success.
- **MTP-11** [replaced by MTP-29] — Described an idle device card without a
  playlist selection, showing the write status instead of a call to action.
  Turn 7c replaces that single formulation with four named states (`MTP-29`),
  of which "Tap to scan device contents" supersedes the former write-status
  line; genuine scan, sync, warning or selection errors still retain "Needs
  attention" (now part of `MTP-29`).
- **MTP-12** [active] [gtk] — Every available playlist row on the
  device page names its last sync time verified on this device, in
  local time. Without a reliable timestamp, it explicitly states "No
  verified sync time". A timestamp is only stored after a successful
  device readback; failed or only partially published runs do not
  overwrite it.
- **MTP-13** [active] [gtk] — The entire device card is exactly one
  native keyboard and pointer entry point into a non-modal full device
  page in the main window and does not start a sync directly. The
  primary menu item opens the same page for one device, and a compact
  selection first for multiple devices. The page contains no song or
  device file list, and the transfer profile as its only setting; it
  shows every playlist with a visible, markup-safe name, selection,
  last verified sync, and the target size projected for the active
  profile, as well as, during a running sync, a progress bar and
  current smoothed MTP transfer rate.
- **MTP-14** [active] [gtk] — The full device page has the information
  hierarchy of a device dashboard, not that of a preferences page:
  device identity, MTP status, last device sync, device storage, and
  actions form a shared, simple hero head. Playlists with
  profile-dependent target size and last playlist sync form the main
  content; transfer profile, delta, and running progress remain a
  compact secondary overview. Locally known playlists appear and stay
  selectable while Reprise is still checking the MTP storage; only the
  sync start waits for this check.
- **MTP-15** [active] [gtk] — The playlist workspace and sync overview
  have the same stable top and bottom card edges independent of delta,
  track, and speed text; changing status text wraps within a bounded
  overview width and does not shift any column. The current MTP
  transfer speed appears during copy as its own labeled line next to
  the track text. The sidebar device card states the free device
  storage early enough, even during Checking, Sync, and Finishing, that
  ellipsizing does not hide it.
- **MTP-16** [active] [gtk] — A change to the transfer profile is saved
  immediately, per device, and restored for the same device both after
  a reconnect and after an app restart. A new device still starts with
  Opus 160 kbit/s.
- **MTP-17** [active] [core] — `Music/Reprise` — the playlists target from
  `MTP-38` — is fully authoritative for music and playlists. After
  successfully publishing all desired tracks and playlists, all remaining
  safe files are removed there, even if they are not in the Reprise
  inventory; desired track and playlist paths are preserved. Nothing is
  written, moved, or deleted outside this subfolder, and a missing or
  invalid playlist target state schedules no destructive work. The two
  other targets (YouTube audio, podcast episodes) are **not** authoritative
  in the same way — they diff additively against their own candidate list
  instead of removing every unknown file (`MTP-23`).
- **MTP-18** [active] [core] — A running sync always names the step it is
  actually performing. The run opens on the step that will do the first
  visible work — its first transfer, or its playlists, or its removals —
  never on a step scheduled for later.
- **MTP-19** [active] [core] — A failed track holds back only what depends
  on it. A playlist is left unwritten exactly when it would point at a track
  that never arrived; every other playlist is published. Removals wait until
  every planned playlist has been rewritten, because an older playlist left
  on the device may still reference a file that is about to be deleted.
- **MTP-20** [active] [core] [gtk] — Every synchronization run is recorded,
  so what a past run did can be answered afterwards instead of guessed. The
  entry opens when the run starts and names the device, the transfer profile
  and how many files were planned; it closes with the balance of what was
  copied, skipped, removed and failed, and with the reason when the run did
  not complete. A run whose session ends without closing it — the app died,
  the cable was pulled — is marked interrupted by the next run rather than
  left open or dropped, because "it never finished" is itself the answer.
  Successful copies are counted, not listed; every file that deviated —
  skipped, failed, removed, or kept in its original format — is recorded
  individually with its device path and reason, removals included, since the
  mirror owns `Music/Reprise` and what it deleted is the question that gets
  asked. The device page shows the recorded runs newest first, one
  expandable row each with its deviations inside. Recording never blocks a
  sync: a log write that fails is dropped, not propagated. Only the most
  recent thirty runs are kept.
- **MTP-21** [active] [core] — A file counts as transferred only once it is
  proven to be on the device under its final name. Transfers publish through
  a `.part` file and rename it at the end; that rename is confirmed
  afterwards — the target must be there with the bytes that were sent —
  because a device can acknowledge a rename it never performed, which strands
  the audio under a name no media scanner reads while the inventory claims it
  arrived. An existing target is removed before the rename instead of being
  overwritten by it, since overwriting is what the device mishandles. A
  transfer that cannot be proven fails the file honestly and leaves no partial
  behind, so the next run copies it again.
- **MTP-22** [active] [core] — The sync plan is read per category (E3):
  either a numbered balance "N new · M removed", or one of two states in
  their own right — "source off" (the global rule or the device target is
  disabled; nothing was examined, which is a different thing from "examined
  and unchanged") and "Unavailable, kept on phone" (nothing local to compare
  against; existing device files are left untouched rather than guessed at —
  derived from `Connectivity`, `NET-3a`, without inventing a second notion of
  offline). Copying and removing each keep their own file and byte count, in
  the row and in the overall balance alike ("To copy 14 files · 2.6 GiB",
  "To remove 3 files · 148 MiB", "Playlists rewritten 2"); whether a category
  has work is decided by the file count, never by the byte value. A removal-only
  run moving 0 B in total therefore stays distinguishable from "nothing to do"
  as "3 to remove · frees 0 B" — the earlier sidebar card, which combined
  "N changes" with a single amount counting copied bytes only and so displayed
  "3 changes · 0 B", is that gap. A size cap (`MTP-39`) acts as an additional
  removal above the selection balance and never changes the copy count.
- **MTP-23** [active] [core] [gtk] — The actual transfer writes and deletes
  through the three named targets from `MTP-38`, no longer through a single
  managed folder (finally superseding the `78e379fd` resolution): playlists
  still under the playlists target (default `/Music/Reprise`, `MTP-17`),
  YouTube audio under the YoutubeAudio target (default
  `/Music/Reprise-YouTube`), podcast episodes under the PodcastEpisodes target
  (default `/Podcasts/Reprise`). A disabled target (`SyncTarget::enabled`)
  yields an empty intended set for its category rather than writing something
  to the wrong place. Deleting and copying run serially, one transfer at a
  time; progress comes from the transport's send callback. The MTP transport
  layer is an injectable abstraction (`DeviceBackend`); the real GVfs/MTP
  binding and a recording test double share the same contract, so the
  regression gate runs without a phone attached.
- **MTP-24** [active] [core] — Music still follows the transfer profile
  (`profile.rs`): lossless source material is re-encoded to Opus 160 kbit/s or
  MP3 256 kbit/s, lossy material is left untouched. Podcast and YouTube audio
  is **always** copied 1:1, never re-encoded — it is already Opus or AAC, and a
  second encoding step would be pure quality loss for no gain.
- **MTP-25** [active] [core] — The size cap from `MTP-39` is real: before a
  YouTube-audio or podcast-episode target with a cap forms its intended set
  from candidates, the oldest subset (by source file age) leaves that set until
  the total is at most the cap again. A candidate already on the device but now
  excluded thereby becomes an ordinary removal — no separate cleanup step is
  needed. The playlists target has no cap (`MTP-38`) and is unaffected by this
  rule.
- **MTP-26** [active] [core] [gtk] — "Device contents never verified" is a
  real, checkable state (7a) rather than a silent fact in the scan bookkeeping:
  `NeverVerified`, `Verifying`, `Verified` and `Failed(reason)` are four values
  in their own right, not variants of a bool. The device view shows the state
  with a "Scan device" action; it is disabled while a scan is already running,
  but stays active after a failed scan so another attempt is possible. No new
  scan mechanism: the same inspect call that already runs automatically before
  every sync is merely made visible here.
- **MTP-27** [active] [core] [gtk] — The device view's storage bar is segmented
  by category — Music, YouTube audio, Podcasts, Other — rather than only
  Music/After-sync/Other/Free like the existing bar in the compact sync dialog
  (`MTP-7`, unchanged). The bytes this sync will write carry their own, clearly
  **hatched** "Incoming this sync" segment rather than merely a different alpha
  value. A free-space line reads "175.0 GiB free → 172.4 GiB after this sync";
  if the pending sync does not move free space, the arrow is dropped and only
  "X free" remains. On incomplete or inconsistent capacity the bar disappears
  entirely rather than inventing a proportion — the same rule as in `MTP-7`.
- **MTP-28** [replaced by MTP-37] — The device view gains a "Content" section
  with one row per named target (`MTP-38`): target folder path, selection
  summary, size on the device, cap and a switch. **Addendum of 2026-07-28,
  binding:** the selection summary and the cap are read-only displays of the
  global rules from Preferences (7b/7e) — labelled "rules from Preferences" /
  "Same on all devices" — and are not editable here; there is no second place
  to operate them. The only switch in this section is `SyncTarget::enabled`
  (`MTP-38`) — whether this device has an active place for the category at all
  — explicitly distinguished from a global "sync this content kind" rule, which
  does not yet exist (7b "Phone sync" block, see `T6-G1`). A "Next
  synchronization" panel below it reads `MTP-22`'s line per category ("To copy
  14 files · 2.6 GiB", "To remove 3 files · 148 MiB", "Source off",
  "Unavailable, kept on phone") and carries the same balance aggregated
  beneath. **Addendum of 2026-07-28 (E6), binding:** the target folder path is
  no longer read-only text; `MTP-31` gives it a browser.
- **MTP-29** [active] [gtk] — When idle, the sidebar device card states one of
  exactly four directional sentences rather than a single blocking number:
  "14 to copy · 2.6 GiB · 3 to remove", "3 to remove · frees 148 MiB" (0 B
  moved is correct here and must not look like "nothing to do" — this also
  supersedes `MTP-11`'s earlier write-status line), "Up to date · synced 12 min
  ago", and "Tap to scan device contents" for `MTP-26`'s
  `NeverVerified`/`Failed`. A real problem (a blocker other than "nothing
  selected", a warning, a scan or sync error) takes precedence and still shows
  "Needs attention". The card carries only the leading sentence; the full
  balance from `MTP-22` lives in the tooltip. During sync and finishing, the
  thin progress line at the bottom of the card (`MTP-6`, unchanged) remains the
  only status indicator.
- **MTP-30** [active] [core] [gtk] — The switch "Sync automatically when this
  phone connects" (7a, `DeviceSettings::sync_automatically`, default **on**)
  means: as soon as the sync plan is settled after connecting, the sync starts
  by itself, with no button press. This applies exclusively to the first
  refresh after connecting (new connection or reconnect) — a manual "Refresh"
  or the verification refresh after a sync never triggers it. An automatic
  start requires a verified scan **and** an error-free planned sync (no
  `scan_error`, no planning error); a device that has not been verified yet
  (`MTP-26`) never starts automatically. It is also skipped when the device is
  already busy, or when there is simply nothing to do according to the existing
  balance (`MTP-22`, `SyncBalance::has_work`) — the latter is reused, never
  derived a second time. A refused or failed automatic start stays entirely
  silent apart from a `tracing::warn!` log — no modal, no error banner, because
  nobody clicked anything; the manual sync button keeps its existing error
  display unchanged. The decision itself is a pure function over the gathered
  facts (`reprise_core::device_sync::auto_start::should_auto_start`); the GTK
  runtime only gathers those facts and obeys.
- **MTP-31** [active] [core] [gtk] — Design 7d, E6: "Change folder…" next to
  each target folder path in the Content section (`MTP-37`) opens the device
  folder browser. The browser offers storage selection (internal/SD card, from
  the storages found on the device), a folder tree with navigation into a
  folder and one level back, "New folder", and a target preview ("Files will be
  stored at ⟨Storage⟩ → ⟨path⟩") which resolves only once a storage has been
  chosen and otherwise reads "once a storage is chosen", or "no longer
  available" for a storage that has disappeared. If the chosen folder sits on
  the same storage inside the playlists target (`MTP-17`'s authoritative tree),
  a warning appears — nesting would expose the files there to the playlists
  cleanup. "Reset to default" resets path and storage to
  `SyncTargetKind::default_path` without touching `enabled` or `cap_bytes` — a
  folder reset is a different operation from a cap or activation reset
  (`cap_bytes` has been independently editable through the Content section's
  spin button since `MTP-37`, which is exactly why "Reset to default" must not
  drag it along as a side effect). If a device refuses to create a folder
  directly in a storage's root, the browser shows that error inline rather than
  swallowing it silently or claiming success. MTP knows no paths (`MTP-38`):
  every browser call resolves storage and folder contents freshly through the
  `DeviceBackend`, never through a stored object handle. Every decision —
  target preview, conflict warning, reset outcome — is a pure, GTK-free
  function in `reprise_core::device_sync::browser`; the GTK side only gathers
  facts (storage list, folder contents) and displays them. The real GVfs/MTP
  binding and a recording test double share the same `DeviceBackend` contract
  (`MTP-23`), so no test needs an attached phone.
- **MTP-32** [active] [core] [gtk] — If the browser (`MTP-31`) changes the
  target folder of a target already resolved to a storage, and stays on the
  same storage, files already synchronized there are moved to the new location
  by an MTP move on the next step rather than being copied a second time. A
  storage change still goes through `MTP-38`'s existing copy-and-orphan path,
  because a folder cannot cross MTP storage boundaries by moving. The decision
  (Unchanged / MoveFolder / CopyAndOrphanPrevious) is the pure function
  `reprise_core::device_sync::browser::target_relocation_action`, which reuses
  `MTP-38`'s `target_storage_transition` instead of duplicating it; a move that
  fails on the device does not block saving the new folder — the next sync then
  simply copies afresh, but logs a warning.
- **MTP-33** [active] [core] — The switch "Remove from phone when deleted or
  unsubscribed here" (design 7a, `DeviceSettings::remove_deleted`) really
  decides whether a podcast episode or YouTube audio track that is no longer
  wanted leaves the device on the next sync plan: switched off, such a file
  stays, and `podcasts::build_plan`'s `to_remove` stays empty for the affected
  category. Both additive targets (YouTube audio, podcast episodes, `MTP-38`)
  read `DeviceSettings::remove_deleted` of that same device for this, never a
  hardcoded value. The playlists target is untouched by this switch — there,
  `MTP-17`'s complete cleanup remains in force regardless of how this switch is
  set.
- **MTP-34** [active] [gtk] — Design 7d's device folder browser (`MTP-31`)
  carries a generation token for every navigation — row activation, "Up",
  storage change, "Reset to default" — exactly like `cover_loader.rs`'s
  protection against recycled rows. A folder listing or an error arriving late
  for a navigation that has already been superseded is discarded rather than
  appended to the folder view now on screen — otherwise the children of a
  slowly loading folder could appear under a different folder opened in the
  meantime, and selecting one would yield the wrong path.
- **MTP-35** [active] [gtk] — Design 7d's "Save" shows a refused persist
  (`set_target_folder` fails, e.g. a running sync or a device unplugged in the
  meantime) inline in the same error area a refused folder creation already
  uses (`MTP-31`), and leaves the dialog open instead of closing it as it does
  on success. A refused save must never look like a successful one; the chosen
  selection stays visible so that nothing is discarded silently.
- **MTP-36** [planned] [core] — `MTP-41`'s YouTube rule "the latest N episodes
  per enabled channel, regardless of download state" names N; this is where
  that value lives. **Decided 2026-07-29:** a global default (default **5**),
  overridable per channel — the same shape as `O-5` for "Keep N downloaded", so
  that both quantity limits share a single mental model rather than two
  different ones. N is **device-independent**: since `E-5` there is exactly one
  device, and storing it per device would be effort without meaning. **Decided
  2026-07-29 (the zero question):** a value of 0 means **unlimited**, here and
  in every other numeric sync setting — the size cap has modelled 0 that way
  since `MTP-38` (`cap_bytes` is an `Option`), and two adjacent numbers on one
  page must not read 0 in opposite directions. "Nothing from this channel" is
  what the channel toggle from 6b says; it is not a quantity, so it is not
  expressed by a quantity.

  Until 6b's channel surface sets this value, the live pipeline
  (`device_sync_compact::recompute_delta_silent`) treats N as unlimited
  (`EpisodeSelectionRule::LatestPerChannel { latest: usize::MAX, .. }`). That is
  deliberately the existing behaviour and not a silent change; meanwhile the
  intended set is bounded solely by `MTP-41`'s candidate limit (downloaded
  episodes plus the missing ones explicitly wanted via `wanted_on_device`,
  `MTP-40`). This rule becomes `[active]` once the persistence and the surface
  exist — not before.
- **MTP-37** [active] [core] [gtk] — Replaces `MTP-28`'s addendum of 2026-07-28
  (Turn 6/7 plan `E-6`, `E-8`): Reprise supports exactly one MTP device
  (`E-5`), so the sole justification for a global Preferences surface — "applies
  to all devices, no device picker in the settings" — falls away. The device
  view keeps a "Content" section with one row per named target (`MTP-38`):
  target folder path (unchanged, `MTP-31`'s "Change folder…"), selection
  summary, size on the device, and cap. Two things change:
  1. **The cap becomes operable here.** A spin button in GiB (0 = no cap)
     replaces the read-only text; every change persists immediately through
     `DeviceSyncRuntime::set_target_cap` into `SyncTarget::cap_bytes`
     (`MTP-38`) and takes effect on the next sync plan through the existing
     oldest-first eviction (`MTP-39`/`MTP-25`) — no second cap mechanism, only
     one place to operate the one that already exists. Playlists have no cap
     (`MTP-38`'s `default_cap_bytes`) and keep the spin button permanently
     disabled rather than feigning an effect no eviction path delivers.
  2. **The selection summary becomes a real, honest live value** instead of a
     display of global rules or a static text. Playlists already read
     `selection::summarize_playlist_selection` (`MTP-41`); YouTube and podcasts
     now likewise read "N of M channels selected" and "N of M shows selected ·
     unplayed downloads only" live from
     `podcasts::phone_sync::selection_summary` — the same selection that is
     operated on the podcast and channel pages and in the playlist list
     (`POD-12`). This line gets **no** second place to operate it: toggling
     individual channels and shows stays where it is, so that two places can
     never claim the same selection. Without `MTP-36`'s (still `[planned]`)
     persisted "latest N per channel", the "latest K each" suffix is omitted
     for YouTube rather than asserting a number that enforces nothing.

  The cross-reference "rules from Preferences" / "Same on all devices"
  disappears with nothing in its place — with one device, "Same on all devices"
  is a statement about nothing. The only switch this section ever had
  (`SyncTarget::enabled`, `MTP-38`) is unchanged; it now simply gains the cap
  field and the selection line as equal, real neighbours instead of read-only
  text beside it. The planned "Phone sync" block in Preferences (`SET-8`) is
  dropped with nothing in its place, not merely "as long as its sync foundation
  does not exist" — see `SET-9`.
- **MTP-38** [active] [core] — Reprise knows three named sync targets per
  device — playlists, YouTube audio, podcast episodes — rather than a single
  managed folder (supersedes `78e379fd`, no migration: see Turn 6/7 plan §1b).
  Each target carries an optional `StorageID`, a path string, an activation and
  an optional size cap; the proposed values are `/Music/Reprise` (no cap),
  `/Music/Reprise-YouTube` (8 GiB) and `/Podcasts/Reprise` (4 GiB) — changeable
  through the device browser (7d, `MTP-31`). The rules for *what* is
  synchronized to a target live on the device page (`MTP-37`) and are expressly
  **not** part of this type. MTP knows no paths: folders are object handles
  under a StorageID and are not stable across reconnects — which is why only
  StorageID plus path is persisted, never a handle. If a target's StorageID
  changes, the folder cannot move with it; the previous storage copy counts as
  orphaned and is cleaned up once the new copy is in place. The wiring into the
  actual MTP transfer is `MTP-23`.
- **MTP-39** [active] [core] — When a sync target with a size cap exceeds its
  limit, the oldest files leave the device first until the total is at most the
  cap again; removal stops as soon as that is reached and never takes more than
  necessary. The selection is a pure function over size and age per entry,
  independent of target kind or transport.
- **MTP-40** [active] [core] — Every episode carries a persistent
  `wanted_on_device` state (7f). "Sync to phone" on an episode without a local
  file sets that state instead of refusing the action; the download follows
  automatically — immediately when online, marked as pending when offline,
  through the existing `NET-3a` contract (`deferrable_action_outcome`), without
  duplicating its online/offline decision. An already downloaded episode needs
  no download step. The downloader that actually works off the pending state is
  not part of this rule (E2/E4).
- **MTP-41** [active] [core] — The intended set per sync category is a pure
  projection from the selection rule and the library state (E2). Playlists
  yield "N of M selected · K tracks"; YouTube audio limits each enabled channel
  (channel toggle from 6b) to its latest N episodes regardless of download
  state ("N of M channels · latest K each"); podcast episodes want every
  unplayed, already downloaded episode of enabled shows with no upper bound
  ("Unplayed downloads only"). A wanted episode without a local file
  (`wanted_on_device`, `MTP-40`) counts as waiting, never as ready to copy —
  the intended set keeps the two visibly apart instead of silently filtering a
  waiting episode out of the result.
- **MTP-42** [active] [core] — Design 7f's preparation phase
  (`reprise_core::device_sync::preparation`) is a pure projection over
  `MTP-41`'s waiting set, `NET-1a`'s global gate, `NET-3a`'s connectivity, and
  the device's own "prepare before sync" switch. It resolves in this order:
  1. `online-sources-enabled` off (`NET-1a`) means the phase does not exist at
     all — not an empty phase, not a disabled switch, nothing shown.
  2. No `wanted_on_device` episode is missing its file: nothing to prepare.
  3. Offline (`NET-3`): the sync still runs and skips these files, which stay
     marked wanted for the next attempt.
  4. A metered connection, or the device's switch off: offered to the user
     but not started.
  5. Otherwise: planned, and starts alongside the sync.

  Offline is checked before metered/switch-off on purpose: offline is a fact
  about whether the download can run at all, metered and the switch are
  policy about whether it should run given that it could. Offering a
  download that cannot run either way would be a lie dressed up as a choice,
  so a missing connection always wins over policy. Only the planned state
  changes the primary sync button to "Download & sync"; every other state,
  including offered, leaves it as a plain "Sync now".
- **MTP-44** [active] [gtk] — Device-sync preparation (7f, E9) downloads a
  `wanted_on_device` episode (`MTP-40`) by giving the existing podcast
  download manager a priority lane, never a second download path: a
  high-priority request is served ahead of any ordinary request already
  queued in front of it, but both still run through the one
  `PodcastsOperation::Download` and the one worker thread. The priority lane
  is only a queue in front of that single executor, not an alternate one.
  Priority work never permanently starves the ordinary lane — once the
  priority lane runs dry, the worker resumes ordinary requests exactly where
  it left off.

## F. Settings & modals

- **SET-1** [planned] [gtk] — Preferences = one window with vertical
  navigation (pages: General, Library, Playback, Audio, Sync, Plugins).
  Clicking a page switches the content on the right; no tab overflow, new
  features = new page or section.
- **SET-2** [planned] [gtk] — Subpages (e.g. scrobbler configuration) are
  navigation pages in the same window with a ‹-Back in the header — no
  new windows.
- **SET-3** [planned] [gtk] — Modal layers: maximum two. Layer 1 = one
  window above the main window (Preferences OR Tag Editor OR Shortcuts —
  never two at once). Layer 2 = exactly one dialog above that
  (FileChooser, confirmation). A dialog never opens another dialog. Esc
  always closes the topmost layer.
- **SET-4** [active] [gtk] — Settings take effect immediately (no
  Apply/OK). Destructive toggle, concretely: if Auto-clean is enabled
  while the Deleted group already contains rows past the chosen
  threshold, a dialog appears once: "This will remove N tracks now
  (deleted more than 30 days ago), including their ratings, playlist
  entries, and device sync state. Listening history stays in My Stats.
  Remove now / Start counting from today." The latter stores the
  activation date as the cutoff (`auto_clean_armed_at`); only what
  breaches BOTH the threshold AND the cutoff gets deleted. Both delete
  dialogs in the app (this one and "Remove all N") name the cascade
  explicitly: ratings, playlist entries and device sync state go;
  listening events remain (BROWSE-6).
- **SET-5** [active] [gtk] — The content of every Preferences main page
  begins with the compact standard spacing directly below the content
  header. Short pages are not vertically centered; unused space stays
  below the last group.
- **SET-6a** [active] [gtk] — The Plugins page groups by user intent:
  "Local Features", "Online Content" and "Connected Services". Scrobbling
  appears there exactly once as a navigation entry and opens a
  navigation page in the same Preferences window with ‹-Back. There is
  no global scrobbling switch.
- **SET-6b** [active] [gtk] — The scrobbling subpage lists ListenBrainz
  and Last.fm as independent providers; both may be active at the same
  time. Activation, account, status, errors and queue stay
  provider-specific. With bundled app credentials, Last.fm offers the
  normal browser login directly; custom API credentials sit collapsed
  under "Advanced setup".
- **SET-7** [active] [gtk] — "New Releases" and "Concerts" are peer
  Preferences main pages in the vertical navigation. For these two
  features, the Plugins page keeps only the activation switches; scope,
  provider, location and similar options live exclusively on their
  respective main pages and are not operable while the module is
  disabled.
- **SET-8** [replaced by SET-9] — A Preferences main page of its own, "Online
  sources" (turn 7b), bundles the three network sources: at the very top a
  global gate "Use online sources" with the subtitle "Off makes this a local
  player only: no requests, no downloads, nothing hidden — the three entries
  disappear from the sidebar."; below it three equal blocks for YouTube,
  Podcasts and Radio, each with its own master switch and the rows laid down in
  `docs/plans/podcasts-youtube-radio-turn6.md` section 3b. YouTube is a module
  equal to Podcasts and Radio, not a sub-option of Podcasts (issue #96). A
  switched-off block hides its own sidebar entry and stops its requests, but
  deletes neither subscriptions nor favorites — the footer carries the same
  assurance: "Each block is self-contained: turning one off hides its sidebar
  entry and stops its requests; subscriptions and favorites are kept, not
  deleted." The fourth block, "Phone sync" from 7b, is deliberately not part of
  this as long as its sync foundation (block E) does not exist — its rules
  follow with that block.
- **SET-9** [active] [gtk] — Replaces `SET-8`: the same Preferences main page
  "Online sources" (turn 7b) with the same rows for YouTube, Podcasts and
  Radio, unchanged — a global gate "Use online sources" with the subtitle "Off
  makes this a local player only: no requests, no downloads, nothing hidden —
  the three entries disappear from the sidebar.", below it three equal blocks
  with their own master switch, YouTube equal to Podcasts and Radio (issue
  #96), and the footer "Each block is self-contained: turning one off hides its
  sidebar entry and stops its requests; subscriptions and favorites are kept,
  not deleted." The single correction (turn 6/7 plan `E-6`, `E-8`): the fourth
  block "Phone sync" from 7b will **not be built** — not "as long as its sync
  foundation does not exist", which is what `SET-8` left open, but finally.
  `E-5` reduces Reprise to a single MTP device, and with it the justification
  for sync rules needing a cross-device Preferences surface at all; they live
  on the device page instead (`MTP-37`). "Online sources" remains the only
  Preferences main page this area will ever get.

## G. Feedback vocabulary

- **FB-1** [planned] [core] — Two-class toasts (pill, bottom-centered,
  one line, max 1 action button, 4 s / 10 s with Undo; only for
  completed actions or events): Actionless event toasts replace one
  another — at most one waits, the newest wins, no backlog noise. Toasts
  WITH an action (Undo) are undismissable and run their full 10 s; event
  toasts wait that long.
- **FB-2** [replaced by FB-2a/FB-2b] — Original shared progress-card
  rule; split during the Relink expansion into the fully delivered
  Relink contract (2a) and the not-yet-uniformly-delivered card for the
  remaining long-running tasks (2b).
- **FB-2a** [replaced by FB-8] — The Relink scan runs off-thread in the
  existing progress card, stackable with Scan/Sync, **inside** the
  bottom-anchored Issues area. Its order is: heading "ISSUES" → running
  cards → Import errors / Missing files; running cards and issue rows
  form a shared block at the bottom edge with no flexible gap between
  them. Card: spinner + title + % on the right (tabular) + 3px bar +
  ellipsized detail line. Clicking the card → Missing files; the visible
  Cancel button checks for abort before each audio file.
- **FB-2b** [planned] [gtk] — Scan, Sync and playlist import use the
  same full card contract from FB-8 for every run > ~1 s, including
  visible Cancel and navigation to the associated view.
- **FB-3** [active] [core] — Errors: individual errors during a run are
  collected, never toasted individually. At the end, ONE toast with
  "N failed · Details" → Details opens the relevant view/dialog.
  Persistent problems live as a badge + ISSUES entry, not as recurring
  toasts.
- **FB-4** [active] [core] — Badges only count entries newer than the
  last time the respective view was opened (`last_viewed` timestamp per
  view in the settings store): Missing counts
  `missing_since > last_viewed`, Import Errors counts
  `first_seen > last_viewed` — excluding dismissed rows and excluding
  notice rows ("imported without metadata"), because only what asks the
  user for something is counted. Reactivating a dismissed row (file
  changed) starts a new episode: `first_seen = now`, `seen_count = 1` —
  so it badges again. Opening the view = badge gone, the total count
  lives in the view.
- **FB-5** [replaced by FB-5a/FB-5b] — Original combined StatusPage
  rule; split during implementation into the deliverable Missing empty
  state (5a) and the unavailable state that only becomes deliverable
  with the Root-Guard UI (5b).
- **FB-5a** [active] [gtk] — The empty Missing-files view shows the
  StatusPage "No missing files ✓" with no competing next action.
- **FB-5b** [active] [gtk] — An unavailable library root shows the
  StatusPage "Library folder unavailable — Retry" with exactly this
  next step.
- **FB-6** [active] [core] — File deleted (externally, watcher): no
  toast per file (noise) — row turns gray/disappears per the Missing
  rules, the ISSUES badge counts up. Exception: the currently playing
  track faults → skip + one toast "Track unavailable — skipped".
- **FB-7** [active] [core] — "Remove from library" does not delete but
  sets `removed_at` (tombstone); the row with ratings, play counts and
  playlist positions stays fully intact for 10 s, Undo only resets
  `removed_at = NULL` — that's why the restoration is exact (same id, no
  race with scans running in parallel). The remove toast always carries
  Undo (FB-1, 10 s). After the toast expires, it is hard deleted
  (cascade: playlist entries, ratings, sync state); the standalone
  listening history remains per BROWSE-6. App exit within the window →
  the deletion is committed on the next launch, never rolled back
  ("7 removed" must stay true). Auto-clean (opt-in, default off, deleted
  tracks only) hard-deletes without a toast and without Undo — it fires
  no earlier than 30/90 days after disappearance (SET-4).
- **FB-8** [active] [gtk] — Scanner and Relink scans run off-thread in
  the existing progress cards, stackable with Sync/Doctor, in the
  bottom-anchored area. As long as at least one progress card is
  visible, the card block **replaces** the full Issues block; the
  heading "ISSUES" and Import errors / Missing files are neither visible
  nor do they occupy extra space. Fully inactive progress cards likewise
  occupy no space; only active or still-fading-out cards take part in
  the layout. The bottom edge of the visible card block sits directly
  above the player bar, while all free sidebar height stays above the
  block. After the last card has fully faded out, the Issues block
  returns. Persistent device status remains visible independently of
  this.
  Card: spinner + title + % on the right (tabular) + 3px bar +
  ellipsized detail line. Clicking the card → Missing files; the visible
  Cancel button checks for abort before each audio file.

## H. File association & OS integration

- **OS-1** [planned] [e2e] — A file opened (double-click in the file
  manager): the main window opens in the last-used view (session
  restore), playback starts immediately, the player bar shows the
  track. No special view, no mini-player autostart.
- **OS-2** [planned] [core] — File in the library → normal track (with
  history/rating). File outside → transient track: plays, appears in
  the queue/player bar with a subtle "not in library" chip, is NOT
  imported, leaves no DB row beyond the session. The context menu
  offers "Add to library…".
- **OS-3** [planned] [e2e] — Multiple files (selection → "Open with
  Reprise"): replaces the queue with the selection in file-manager
  order, plays the first one, toast "12 files queued". A second
  invocation while an instance is running: same semantics (replace the
  queue, don't append — predictable beats clever). Replacing during
  ongoing playback is an explicit user action and does not violate
  PLAY-5.
- **OS-4** [planned] [e2e] — Single instance: a second launch passes
  files through to the running instance and focuses its window.
- **OS-5** [planned] [e2e] — MPRIS always mirrors the player state;
  playback from file association is identically visible there.

## I. Start state

- **START-1** [planned] [e2e] — Normal start: last view + scroll
  position, playback paused on the last track (position restored),
  startup reconcile runs silently (card only for actual work).
- **START-2** [planned] [gtk] — Start with an unavailable library root:
  StatusPage per Root-Guard, no mass Missing marking; library views
  show the last known holdings normally (Root-Guard hasn't marked
  anything), only the StatusPage/card reports the state. No blank
  screen.

## J. Queue view

- **QUE-1** [active] [gtk] — A shared queue model feeds two surfaces
  with different depths: the sidebar row "Queue" opens the ColumnView
  as a management surface with sections, DnD reorder, right-click,
  Clear and StatusPage. The panel toggle opens "Up Next" as a viewing
  surface of the same queue with sections, jump and remove. The player
  bar has no redundant queue icon. No surface maintains its own second
  list.
- **QUE-2** [active] [gtk] — The panel divides the future into exactly
  two conditional sections: **Next in Queue** for manually enqueued
  tracks and **Continuing from "<Album/Playlist>"** for the automatic
  context from `play_origin`. A header appears only if its section has
  entries; an empty manual section leaves only "Continuing …" standing.
  Their visible order is also the playback order; as long as something
  is playing, the queue never shows two empty sections.
- **QUE-3** [active] [core] — Played manual entries silently disappear
  from "Next in Queue" on track change: no strikethrough and no
  lingering. The section contains only the still-pending future.
  "Remove" in the panel removes exactly that entry from the queue,
  never from the library.
- **QUE-4** [active] [core] — The queue footer formats track counts
  with the same shared thousands-separator function as the library;
  there is no second formatting path.
- **QUE-5** [active] [core] — Jumping to a queue entry sets the
  playback position and consumes only the clicked entry. Manual entries
  before it stay in "Next in Queue" and play afterward; there is
  neither silent discarding nor a dialog nor a queue history. "Remove"
  removes from the queue, never from the library.
- **QUE-6** [active] [core] — Both surfaces read a shared queue model.
  Metadata comes from a single batched query over the queue IDs, never a
  per-row query; row recycling and loading only the visible window
  bound widgets and work independent of queue length. With the panel
  closed or another tab active, track changes and reorders only update
  the model and render no panel rows.

## K. Filter & search visibility

- **FIL-1a** [active] [gtk] — One truth about restrictions (track
  lists): anything that restricts the visible track list appears as a
  chip in the filter row directly above the list — including the
  header-bar search (chip ⌕ "falling" in any field, own ×-click target
  ≥ 20 px; the × removes only the search, Esc per NAV-6). Applies in
  every track source (Library, Playlist, Smart, Queue, Missing). The
  search is global across track sources and travels along on location
  change; its chip appears everywhere it actually restricts — in
  sources without search effect (Import Errors: own panel rows) no chip
  appears. Facet chips and "+ Add filter" stay library-only. An
  invisible active filter is a bug. Per-location scoping of the search
  would be its own future rule, not part of this one.
- **FIL-1b** [planned] [gtk] — Albums/Artists mode: the global search
  already works there (grid filtering); the same chip row incl.
  counting and "Clear all" will follow there per the pattern of
  FIL-1a/FIL-2. Until then, the gap is named here instead of silently
  broken.
- **FIL-1c** [active] [gtk] — Artist, Album and Genre scopes of the
  track list carry their own scope-chip class in the filter row
  alongside search and facet chips: "<Artist>", "<Album> — <Artist>" or
  "<Genre>" respectively, with its own ×-click target of at least
  20 px. The × leaves the scope via a regular NAV-2 history push to the
  Library; there, its remembered search and facets are restored. The
  counting follows FIL-2 and puts the scope hits in relation to the
  whole library. Playlist, Smart, Queue, Missing and standalone panels
  carry no scope chip. "Clear all" still only clears search and filters
  and never changes location.
- **FIL-2** [active] [gtk] — Counting is state: the filter row is the
  permanent list header of every track source — it never appears or
  disappears (no layout shift by design, P-4). Idle as quiet as
  possible: only the neutral total count on the right (dim, caption),
  in the Library additionally the "+ Add filter" pill; no "FILTER"
  label. With an active restriction: "FILTER" label + chips +
  accentuated hit count ("15 of 1,664 tracks", hit count in accent
  color, bold) + "Clear all ×" (clears search and all filters in one
  click). The row's hide preference only governs the idle state — with
  an active restriction the row always appears (force-show; the shift
  is a direct consequence of the user's own input, P-4-compliant). The
  status overlay in the bottom right always shows the neutral library
  statistic; its "X of Y" variant is dropped — the filter row speaks
  about the view, the overlay about the library. Clarification: outside
  the Library the overlay doesn't appear at all — there the filter row
  is the only counting (decided 2026-07-17).
- **FIL-3** [active] [gtk] — End-of-results row: below the last row of
  a restricted list (≥ 1 hit) sits, centered, "End of results — 1,649
  tracks hidden by search "falling"" + pill "Show all 1,664 tracks"
  (= Clear all). It visually belongs to the end of the list: directly
  below the last row when the list is shorter than the viewport; for
  longer lists it only appears once the end of the list scrolls into
  the viewport; it never floats over rows (not sticky). Implementation
  as a positioned overlay — the ColumnView's virtualization stays
  untouched; input-transparent except for the pill; position is
  recalculated on scroll, model/filter and resize changes.
- **FIL-4** [replaced by SEARCH-3] [gtk] — The search field carries its
  state: as soon as the field contains text, it gets an accent border +
  tinted background — even unfocused.
- **FIL-5** [active] [gtk] — Hit highlighting: the search term is
  highlighted in all searched, visible text columns (Title, Artist,
  Album, Genre; accent bold, Pango-escaped). If the only matching
  column is hidden, the row stays unmarked — an accepted remaining gap.
  Chip wording stays "in any field".
- **FIL-6** [active] [gtk] — Zero-hit empty state: StatusPage with
  exactly one button "Show all 1,664 tracks" (= Clear all) —
  FB-5-compliant; the one step guaranteedly leads to content, never
  into a second empty state. "Clear all ×" (filter row), "Show all N
  tracks" (end row, empty state) fire the same action — two
  context-appropriate names, one behavior.
- **FIL-7** [active] [gtk] — "Hide AI music" is an **opt-in** filter:
  visible by default (AI versions are welcome library citizens, INST
  section), on request it hides AI-manipulated (and, in the future,
  -generated) titles. It keys on the **provenance flag in the DB**
  (`track_provenance.ai`), never on folder paths — the folder is
  storage layout, the flag is the truth. When active it inserts itself
  as a visible restriction into the filter row per **FIL-1a** (own chip
  with ×-click target) and into the counting per **FIL-2** ("15 of
  1,664 tracks", force-show); like the facet chips it is
  **library-only** and is implemented as a query clause in Core
  (`queries::query_track_window_browsed_ai`). The filter state is
  **sticky across sessions** like other view states. **No
  shuffle/auto-queue special rule** in v1: queue refill follows the
  visible view — with the filter active, AI titles are not visible and
  are not refilled. Only available while the Experimental switch is on
  (INST-11). (Decision 17)
- **FIL-8** [active] [core] [gtk] — "Recently added" is its own library
  scope over all currently existing tracks whose `added_at` is at most
  seven days ago; there is no 50-track limit. The source initially
  sorts by `added_at` descending and carries a dismissible scope pill
  in the filter row per FIL-1c. Its × leaves the scope via the normal
  history push and restores the remembered, unrestricted library.
- **FIL-9** [active] [gtk] — When a search or facet filter is set,
  changed or removed and the loaded track belongs to the new result
  set, its marked row is vertically centered instead of anchored to the
  top table edge. Selection and keyboard focus remain unchanged.
  Without a loaded track visible in the target, the existing
  ID-plus-offset anchor is retained.

## L. Tag editor

- **TAG-1** [active] [gtk] — Save is navigation-neutral: saving changes
  neither scroll nor the library's view (NAV-5 holds through the
  dialog); there is no "jump to next song". After closing, focus sits
  on the library, selection = the **written** tracks (on partial
  failures, the successful ones; unchanged after Cancel/Discard) —
  feedback about the user's own action is allowed, jumping to
  uninvolved tracks is not. Mechanics at the root: the reload secures
  selection via track IDs and scroll via an anchor (track ID + offset,
  never pixels) and restores both — for every trigger (Save, watcher
  reconcile, sorting, rating). During the asynchronous tag-editor save,
  the scroll anchor is captured before the dialog opens and reused
  after its worker completes. A pure rating save that affects neither
  sorting nor filters nor source membership updates the cache and the
  realized star cells without a model signal and thus without scroll
  movement. Deleted IDs silently drop out; a deliberate reset is
  explicit, never a side effect.
- **TAG-2** [active] [gtk] — Multi semantics: fields with an identical
  value show it normally; differing ones show a mixed placeholder
  (italic, dashed border) — with ≤ 2 different values the values
  themselves ("Mixed — Ambient, Post-Rock"; empty counts as its own
  value), from 3 on the count ("Mixed — 8 different values"), next to
  it the counter ("2 values"). No value is prefilled and no field is
  locked: the first typed character arms it (accent border, revert in
  the field, "will be applied to all N"). Backspace/Delete in the
  placeholder arms it as well — as "clear for all N", with full review
  treatment. Nothing is silently swallowed.
- **TAG-3** [active] [gtk] — Per-track fields are read-only in multi
  mode: Title and Track number show "—" with the tooltip "Per-track
  field — edit tracks individually". A bulk title is always an
  accident.
- **TAG-4** [active] [gtk] — Paging discards nothing: if the editor
  opens with exactly one track, ‹ › (Ctrl+Page Up/Down) page through a
  snapshot of the visible list taken at open time — via track IDs,
  never indices, so "Track 3 of 12" stays stable while re-sorting
  happens underneath. Pending changes are held per track; Save writes
  all pending tracks, Cancel discards all (confirmation from one change
  on). An invalid numeric field (Year/Track) blocks both paging and
  Save.
- **TAG-5** [active] [core] — The diff sits at the field, not in a
  second dialog: every effectively changed field shows the old value
  below it ("was: …", struck through, dimmed), border in accent; the
  space for it is always reserved (P-4). Above the Save area, a summary
  line ("2 fields · 30 tracks affected"), and in multi mode with
  cross-field pending changes, additionally an expander "Review
  changes" with one line per field
  (`Artist: Suicide → Suicide Silence · 30 tracks`). Only **tracks
  whose value actually changes** are counted; no-op writes drop out
  (exact comparison, no trim/case normalization). All numbers speak the
  same currency — tracks: the Save button ("Save 30"), progress
  ("Saving… 12/30") and toast ("Tags updated · 30 tracks"). Without an
  effective change, Save is disabled and names the reason (P-2).
- **TAG-6** [active] [core] — Autocomplete source for Artist, Album,
  Album Artist and Genre: distinct values from the user's own library
  with track count, case-insensitive; prefix hits before substring
  hits, within those sorted by track count descending; maximum 6 rows,
  dropdown from 2 characters, section title "FROM YOUR LIBRARY". The
  last row is always "Use 'X' as new artist…" — a new value is never
  blocked.
- **TAG-7** [replaced by TAG-7a/TAG-7b] — Inline ghost. Split because
  the mechanics are enforceable while the ghost's actual appearance
  cannot be proven headlessly (TESTING.md: Xvfb proves construction,
  signals and CSS, not the final rendering) and stays disabled until a
  visual check. A single `[active]` would have lied about one half.
- **TAG-7a** [active] [gtk] — Ghost mechanics: the suggested ghost is
  the best prefix match, in the same ranking as dropdown row 1
  (tiebreak track count) — ghost and row 1 never name different
  values; a pure substring match is never ghosted. Tab accepts **only**
  a visible ghost; without a ghost, Tab is a pure focus change — there
  is no silent takeover of the first dropdown row. The Tab badge
  renders only with a visible ghost. The ghost is pure display and
  never becomes a change unless someone accepts it. The popover anchors
  to the entry and never steals focus: typing continues uninterrupted.
  Applies unchanged while the ghost is disabled (then simply none is
  ever visible).
- **TAG-7b** [planned] [manual] — The ghost actually appears: dimmed,
  flush behind the typed text, at the cursor position. Cannot be proven
  headlessly (TESTING.md: Xvfb proves construction, signals and CSS,
  not the final rendering), so `GHOST_ENABLED = false` stays until a
  visual check on a real display confirms it — "no half-broken ghost in
  the release". The target picture is decided, only the delivery awaits
  sign-off; flipping it then costs a constant, not code.
- **TAG-8** [active] [gtk] — Keyboard semantics. **Enter:** with the
  dropdown open it accepts the highlighted suggestion (dropdown closes,
  focus stays in the field); with it closed it jumps to the next
  editable field; in the last field it focuses the Save button, so the
  next Enter deliberately saves. Enter **never** saves directly from a
  text field — too easily triggered while typing through suggestions.
  Ctrl+Enter saves from anywhere (Ctrl+S is the same, just an
  unadvertised alias — one action for both). **Esc cascade:** first it
  closes the popover (text stays), then it reverts the armed field,
  then the dialog layer takes over (discard question from one change
  on, otherwise close) — each stage destroys at most what the next one
  can bring back; if a save is currently running, the dialog layer
  ignores Esc entirely (the batch is atomic, no abort). The discard
  question counts tracks ("Discard changes to 3 tracks?") and has two
  answers: Keep editing (default) and Discard (destructive). No Save in
  the prompt: saving is never the way out of a closing gesture.

  <!-- REVIEW: rule proposal -->
- **TAG-9** [planned] [manual] — The autocomplete popover consistently
  uses the raised, theme-provided popover surface. Inner lists don't
  paint their own dark view surface on top of it; selection and accent
  highlighting stay legible on light and dark themes.
## M. Tooltips

<!-- The section letters K (filter & search visibility) and L (tag editor)
     are already assigned; tooltips are therefore section M. -->

Tooltips are labeling, not a feedback mechanism — they never carry the
sole statement (TIP-3) and therefore do not fall under P-1's role model.
When an entire container is disabled, TIP-2a/b applies to the container
statement, not to each child individually (the empty player bar is its
own statement).

- **TIP-1a** [replaced by TIP-1c] — Existence follows the label:
  icon-only buttons always have a tooltip; buttons with a visible text
  label get none — the label is the statement, a repeating tooltip is
  noise. Exception: ellipsized/truncated labels show the full text in
  the tooltip.
- **TIP-1b** [planned] [manual] — Form: verb + object („Eject Pixel 8",
  „Toggle sidebar"); the object may be omitted if the button itself makes
  it unambiguous („Play", „Shuffle"). If a shortcut exists, it follows in
  parentheses („Play (Space)").
  <!-- Flip criterion TIP-1b: „Previous"/„Next" in the tag editor
       (tag_editor_form.rs, ownership feat/tag-editor-rework) and „Back"
       in browse_bar (ownership feat/global-search-rework) are still
       nouns. [active] only once both have been updated. -->
- **TIP-1c** [replaced by TIP-1d] — Existence follows label and action:
  icon-only buttons always have a tooltip; buttons with a visible action
  label get none. A compact metadata label may name the hidden action
  (player-bar performer: „Go to artist"). Ellipsized labels still only
  show the full text when actually truncated.
- **TIP-1d** [active] [gtk] — Existence follows label and action:
  icon-only buttons always have a tooltip; buttons with a visible action
  label get none. A compact metadata label may name the hidden action
  along with its matching shortcut (player-bar performer: „Jump to now
  playing (Ctrl+L)"). Ellipsized labels still only show the full text
  when actually truncated.
- **TIP-2a** [active] [gtk] — Disabled explains itself (icon-only): a
  disabled icon-only control keeps its tooltip and adds the reason
  („Eject device — Sync in progress"). Never a dead button without a
  named reason (a specialization of P-2).
  <!-- Player bar prev/next: NO individual tooltips. They are only
       disabled when the queue is empty — and then the whole bar is
       disabled (bar_should_be_sensitive), so the container clause
       above applies: the empty bar is its own statement. -->
- **TIP-2b** [planned] [manual] — Disabled explains itself (labeled): a
  disabled labeled control states its reason visibly via label,
  subtitle, or hint line („Requires same artist & album across
  selection", „Everything in sync") — never only via tooltip (TIP-3:
  the reason would otherwise be exclusive hover information).
  <!-- Flip criterion TIP-2b: Save/„Change cover…" in the tag editor
       (feat/tag-editor-rework) and the disabled „Add filter" state in
       browse_bar (feat/global-search-rework) are still unexplained dead
       ends. [active] only once both have been updated. -->
- **TIP-3** [active] [manual] — Tooltips are redundant, never exclusive:
  every piece of information in a tooltip must also be reachable
  without hover (view, dialog, visible label). Hover details (sync
  card: „28 of 82 · ~2 min left") are comfort duplicates of a reachable
  view — touch operation never sees tooltips.
- **TIP-4** [active] [manual] — Menu entries get no tooltips. In
  popover/context menus the label carries alone; a fixed subtitle line
  („M3U · PLS · XSPF") is allowed. If a menu item needs a tooltip, it
  is misnamed or belongs in a dialog.
- **TIP-5** [active] [manual] — GTK default behavior: no custom delays,
  no interactive/rich tooltips; dynamic values (percent, time,
  ellipsized full text) are allowed.
- **TIP-6** [active] [gtk] — Shortcut hints stay action-matched: if the
  control action named in the tooltip already has a documented keyboard
  shortcut, it appears in parentheses after the label. Shortcuts of
  other actions are not appended to neighboring controls; controls
  without a matching shortcut remain unchanged.

## N. Track context menu

- **CTX-1** [active] [gtk] — One builder, one context enum. All
  track-row menus are produced by a single pure function
  `build_track_menu(context, selection)` (GMenu sections), never from
  five hand-copied menus. Contexts: `LibraryTracks | AlbumDetail |
  ArtistDetail | Playlist | Queue`. The missing view and smart
  playlists render as `LibraryTracks`.
- **CTX-2** [active] [gtk] — Selection actions only. No global entry in
  the track menu (no „Rescan library" — that lives in the hamburger
  menu). Right-clicking an unselected row selects it first; the menu
  always applies to the visible selection. Shift+F10 / menu key open
  on the keyboard selection.
- **CTX-3** [active] [gtk] — No „Play" entry. The primary action is
  double-click/Enter (PLAY-2). The first menu entry is „Play next" (in
  the queue: „Move to top").
- **CTX-4** [active] [gtk] — Navigation only with an unambiguous target.
  „Go to album"/„Go to artist" are omitted when the context IS the
  target (album detail shows no „Go to album", artist detail no „Go to
  artist"). With multi-selection, active only if all tracks share the
  same album or the same (album) artist, otherwise grayed out — never
  hidden, the menu stays shape-stable. The graying-out carries the
  meaning alone; no tooltip (TIP-4).
- **CTX-5a** [active] [gtk] — Destructive belongs to the context.
  Playlist → „Remove from playlist", queue → „Remove from queue" (both
  immediate, without a dialog). „Remove from library…" and „Move to
  Trash…" exist ONLY in library contexts (LibraryTracks/AlbumDetail/
  ArtistDetail), never in playlist or queue. „Move to Trash…" is the
  only red/destructive-marked entry.
- **CTX-5b** [planned] [gtk] — „Remove from library" becomes immediate
  + undo toast (FB-7); the ellipsis „…" and the confirmation dialog are
  dropped in the same commit that builds the undo toast. Until then the
  entry remains „Remove from library…" with a dialog (CTX-5a).
- **CTX-6** [active] [gtk] — Count currency only for destructive
  entries. Only destructive entries carry the selection count: „Remove
  3 from playlist", „Remove 3 from queue", „Remove 3 from library…",
  „Move 3 to Trash…". All other entries stay unnumbered; „Edit tags…"
  opens the multi-editor, which itself titles „Editing 3 tracks".
- **CTX-7** [planned] [manual] — Hover neutral (white ~10%); the
  accent color remains reserved for selection and the playing track.
  The menu fits without scrolling into the window (GTK popover flips
  at the edge).
- **CTX-8** [active] [gtk] — Missing rows in the selection. „Play
  next"/„Add to queue"/„Move to top" are disabled (not playable = not
  queueable, PLAY-4b); „Show in Files"/„Move to Trash…" are disabled
  (file missing). An additional entry „Show in Missing files" appears
  as soon as the selection contains missing rows and the view itself is
  not the missing view, and jumps to the issues view. „Edit tags…"
  only acts on existing files: disabled for a purely-missing selection,
  on a mixed selection it acts on the existing ones (the editor title
  counts only these). „Remove from playlist/library" stay active.
- **CTX-9** [active] [gtk] — „Add to playlist ▸". The submenu lists
  playlists alphabetically, „New playlist…" at the end. The currently
  open playlist is grayed out (no duplicate insertion into itself via
  the menu; DnD remains free).
- **CTX-10** [active] [gtk] — „Show in Files" is active when all
  selected files exist and are in the same folder (a single Nautilus
  multi-selection in one window), otherwise grayed out.

## O. Motion & Transitions

<!-- Section letter: M (tooltips) is assigned on main; N is claimed by
     feature/context-menu-unification („N. Track context menu").
     Motion therefore takes O; the letter situation was verified
     against the main state when this section was inserted. -->

Motion illustrates, it never informs exclusively: every transition
confirms a state change that would also be fully visible without it —
`gtk-enable-animations=false` is the proof (MOT-7). Animations follow
direct user actions; background processes switch hard or fade in
place (MOT-2, the motion reading of P-4).

- **MOT-1** [active] [gtk] — Four tokens, no free-floating numbers:
  every animation configured by Reprise itself uses one of four tokens
  from `ui/motion.rs`: **Micro** 150 ms ease-out for control state
  (icon swap Play⇄Pause, hover pills, chips, rating, press-scale; icon
  crossfades run as two Micro halves of 75 ms each) · **Standard**
  250 ms ease-out-cubic for surfaces (sidebar/panel reveal, toast in,
  card collapse, crossfades cover/StatusPage⇄list) · **Ambient** 400 ms
  ease-out-cubic for atmospheric, non-interactive transitions
  (accent-color crossfade) · **Spatial** = AdwSpringAnimation with Adw
  default spring parameters for directed navigation, added in code
  starting with the first directed navigation case. Ease-in only for
  what is leaving (toast out, Micro duration); linear only for genuine
  progress bars. Adw-internal widget animations without a duration API
  (OverlaySplitView, NavigationSplitView, ToastOverlay, Banner, Dialog,
  Popover — e.g. the push/pop slides of the settings subpages) count
  as system-given and are exempt from the token requirement.
  <!-- Flip criterion MOT-1: all call sites from the motion plan's
       audit inventory consume tokens; scripts/check-motion-tokens.sh
       is strict and without a leftover allowlist. -->
- **MOT-2** [active] [gtk] — User action animates, background never:
  transitions follow direct user actions. Scan/watcher/mount/sync
  switch hard or fade without displacement (P-4 in motion language).
  Exception: the process card started by the user may fill/pulse.
- **MOT-3** [active] [gtk] — Symmetry: same pattern = same widget + same
  token. Specifically: the left library sidebar uses exactly the same
  widget and thus exactly the same transition as the right info column
  (`adw::OverlaySplitView`, position start — the trigger for this
  section); the StatusPage⇄list stacks crossfade with the Standard
  token like the outer Library/Stats/Device stack.
- **MOT-4** [replaced by MOT-8] — Lists do not move: no stagger/
  fade-in per row (windowed model, 200-item window, libraries beyond
  1,600 rows). Allowed: a crossfade of the entire surface on a view
  change, as long as two dense sources do not become simultaneously
  readable; Podcasts⇄Music therefore switches hard. Named exception:
  the queue may animate DnD drop and single remove.
  <!-- The queue exception is permissive, not mandatory; its
       implementation lives in a follow-up branch and does not block
       the MOT-4 flip. -->
- **MOT-5** [active] [gtk] — The player bar lives, but quietly: Play→
  Pause = icon crossfade (two Micro halves) + scale pulse (1.0→0.92→
  1.0, Micro); track change = cover/title crossfade; the waveform
  crossfades to the new track instead of dropping to 0; pause slightly
  desaturates the waveform fill (at draw time), play reverses it — the
  accent pipeline (`cover_accent`) stays untouched. The EQ indicators
  (track list, mini-player) run only during active playback; the idle
  bar is static — no permanent loop without playback.
- **MOT-6** [active] [gtk] — Nothing blocks: the model changes at frame
  0, the animation only illustrates. A second action during a running
  animation jumps to the end state via `AdwAnimation::skip()` and then
  starts the new one; animation slots (track crossfade, icon
  crossfade, accent fade) call `skip()` instead of silently dropping
  the old handle.
- **MOT-7** [active] [gtk] — `gtk-enable-animations=false` wins without
  exception: every token degrades centrally in `ui/motion.rs` to a
  hard switch (`follow-enable-animations-setting` or the central gate
  helper `animations_enabled()`), not at 30 call sites. Also applies
  to our own tick callbacks (waveform position smoothing: set the
  position hard; progress interpolation) and pulse timers.
  `gtk::Spinner` and GTK-internal CSS mechanics are system behavior and
  are not gated.
- **MOT-8** [active] [gtk] — Lists do not move: no stagger/fade-in per
  row (windowed model, 200-item window, libraries beyond 1,600 rows).
  View changes keep the Standard token. Between two dense sources
  (Podcasts/YouTube⇄Music), the outgoing surface is fully faded out before the
  stack change and only the incoming surface is faded in over the
  Standard duration: visible motion without a hard cut and without two
  simultaneously readable tables. The queue exception from MOT-4
  remains permissive; `gtk-enable-animations=false` switches hard per
  MOT-7.

## P. Now-playing panel

<!-- Section letter: O (motion) is the last section assigned on main;
     P follows seamlessly. These rules stem from the grilling on
     2026-07-18 for design 21/21a; the decision ledger at
     docs/superpowers/plans/2026-07-18-npp-beschluesse.md holds the why
     and the detail decisions below the rule level. -->

The right column belongs to the **playing** track, never to the
library selection: it is not an inspector, but the stage for the
current piece. A paused track counts as loaded and stays put; without
a loaded track the panel shows a calm placeholder instead of closing
on its own (P-1 for volume still applies: no volume control lives in
the panel).

- **NPP-1** [active] [gtk] — Geometry is a **pixel** contract and
  deliberately unequal: left sidebar fixed **240 px**, right panel
  fixed **300 px**, both pinned rather than as a range. The panel
  collapses with the same slide transition as the sidebar (MOT-3,
  Standard token). Two pitfalls, both measured rather than assumed:
  `AdwOverlaySplitView` calculates without `sidebar-width-unit = Px` in
  `sp`, and a child without `ellipsize` forces a minimum width via its
  text width that `max-sidebar-width` cannot go below — a status
  element in the sidebar must never dictate its width.
- **NPP-2** [active] [gtk] — Layout from top: cover 168 px (radius 12,
  shadow + 1 px inset hairline) → title 15 px bold → „Artist · Album"
  12 px white 55% → **pill toggle** (segments, no tab-bar widget) →
  tab content → footer 10.5 px white 35%, whose content is provided by
  the active tab. No panel header: closing runs via the app-header
  toggle, a retry belongs in the tab's error state. **No volume
  control** (P-1).
- **NPP-3** [active] [gtk] — Glow instead of full tint: a radial
  gradient of the cover accent color sits in the upper third behind
  the cover and fades down into neutral panel-dark. The reason is
  legibility — the base surface stays neutral so the lyrics contrast
  stays constant over the whole height. Fallback is the theme accent
  (petrol), idle shows no glow. Rendered as a gradient, never
  live-blurred.
- **NPP-4** [active] [gtk] — Tab memory only for the session (NAV-5); a
  restart lands on Up Next. Panel *visibility* continues to persist
  across restarts — tab and visibility are separate states.
- **NPP-5** [active] [gtk] — Line hierarchy in the lyrics tab: active
  line 15 px bold white with accent underline (26 × 2.5 px, centered,
  color = cover accent), neighbors stepped white 45% (±1) / 32% (±2) /
  28% (further). All lines centered, 13 px, generous spacing. Whole
  LRC lines, no karaoke word highlighting.
- **NPP-6** [active] [gtk] — Line change: the new line fades to
  white+bold, the old one back (Micro token); at the same time the
  list slides the active line to center (Standard token,
  ease-out-cubic — no spring, lyrics should run calmly). The underline
  does not travel, it belongs to the active line and fades with it.
- **NPP-7** [active] [gtk] — Manual scrolling wins: user scroll pauses
  auto-scroll for 4 s and resets the timer on every further event,
  after which the list glides back to the active line; a running
  glide-back is aborted in the process. The highlight keeps running
  during the pause — only the scroll is paused. Programmatic scrolls
  never reset the timer, otherwise the panel would lock itself out.
- **NPP-8** [active] [gtk] — Clicking a line seeks to its timestamp
  (synced only); hover lifts to white 65% with a pointer cursor. This
  is the only click interaction in the lyrics tab, and the text is not
  selectable. A seek — from here or from the waveform — jumps
  immediately to the new active line, without the 4-s timer from
  NPP-7.
- **NPP-9** [active] [gtk] — Fallbacks without a dead end: unsynced →
  static scrollable text (white 65%), no highlight, no auto-scroll,
  footer „lyrics · tags"; no lyrics → subtle empty state without a
  search CTA; error → inline retry in the tab. Instrumental gap (> 10 s
  without a line) holds the active line and dims it to 60% instead of
  losing the highlight.
- **NPP-10** [replaced by NPP-13] — A track change is not a place
  change: cover, title block, glow, and tab content crossfade
  **together** in one transition (Standard token, MOT-5), never as a
  slide; the lyrics then start at line 0 and position it per LYR-4.
  `gtk-enable-animations=false` switches hard here too (MOT-7).

## Q. Search

- **SEARCH-1** [active] [gtk] — At rest, search occupies only a
  magnifying-glass icon in the header bar. The search field lives in a
  second, collapsed-by-default top bar and is never shown as a
  permanent wide field.
- **SEARCH-2** [replaced by SEARCH-2a] — Clicking the magnifying
  glass, Ctrl+F, or typing directly opens the search bar and focuses
  the field. It is a full-width strip flush under the header bar, has
  its own surface with a bottom divider line, and pushes the content
  down on reveal; the search field within it is clamp-centered at
  approximately 450 px. The bar slides with the central Standard
  duration (MOT-1/3); for GTK-native revealers their default applies,
  provided it matches the Standard token.
- **SEARCH-2a** [replaced by SEARCH-2b] — Clicking the magnifying
  glass, Ctrl+F, or typing directly opens the search bar and focuses
  the field. Header and search are one continuous upper glass zone
  with a shared neutral blur, tint, and exactly one bottom hairline;
  content keeps scrolling underneath both. The reveal enlarges the top
  scroll inset by the actually allocated search-bar height, and by an
  additional top player-bar height if one is present. The search field
  is clamp-centered at approximately 450 px. The bar slides with the
  central Standard duration (MOT-1/3); for GTK-native revealers their
  default applies, provided it matches the Standard token.
- **SEARCH-2b** [active] [gtk] — Clicking the magnifying glass, Ctrl+F,
  or typing directly opens the search bar and focuses the field. It is
  a full-width, opaque strip flush under the header bar with its own
  surface and a bottom divider line; on reveal it structurally
  reserves its own height. The search field is clamp-centered at
  approximately 450 px. The bar slides with the central Standard
  duration (MOT-1/3); for GTK-native revealers their default applies,
  provided it matches the Standard token.
- **SEARCH-3** [active] [gtk] — The magnifying glass is a ToggleButton
  and carries the `:checked` accent style when the search bar is open
  **or** an active non-empty query exists. A query remains visible
  even when the search bar is collapsed: its search chip persists. The
  magnifying glass gets no badge dot; dots remain reserved exclusively
  for the request role (FB-4, P-1).
- **SEARCH-4** [active] [gtk] — Esc is two-stage and applies to the
  whole search bar: with text present, the first Esc clears the query,
  leaves the bar open, and keeps the field focused; with an empty
  field, Esc collapses the bar. A query is never made invisible by
  collapsing without its chip carrying it.
- **SEARCH-5** [active] [gtk] — Collapsing ends only the input, not the
  filter. Query, results, and search chip are preserved until the user
  explicitly removes them via Esc, chip, or „Clear all".

## R. New releases

- **NR-1** [replaced by NR-1a] [core] — A library-wide MusicBrainz
  pipeline is the sole source of truth for new releases and later
  artist-news views. Artist MBIDs come first from tags, otherwise from
  a persisted name resolution including negative results; artists are
  prioritized by play count. Per artist, at most five regular albums
  or EPs from the last 90 days remain, plus exclusively future
  singles; incomplete data is never treated as future, secondary types
  stay out.
- **NR-1a** [active] [core] — A library-wide MusicBrainz pipeline is
  the sole source of truth for new releases and later artist-news
  views. Artist MBIDs come first from tags, otherwise from a persisted
  name resolution including negative results; artists are prioritized
  by play count. Per artist, at most twenty regular albums or EPs from
  the last 90 days remain, plus exclusively future singles; incomplete
  data is never treated as future, secondary types stay out.
- **NR-2** [active] [gtk] — Release covers load lazily via Cover Art
  Archive (`/release-group/{mbid}/front-250`). A missing cover is the
  normal state and immediately shows an equally sized tile made of the
  stored artist accent color plus initials — never a hole or a
  permanent spinner.
- **NR-3** [replaced by NR-3a] [gtk] — The header ✦ appears only
  when entries exist and carries a badge exclusively for `seen_at IS
  NULL`. Opening stamps the listed episode as seen; it never badges
  again, only a later newly found entry produces a badge again (FB-4).
- **NR-4** [replaced by NR-12] [gtk] — „See all" opens a real digest
  location with back/forward history, but without a sidebar entry.
  Releases can be hidden there; existing hidden entries keep „See
  all" reachable, and the footer „N hidden · Show" makes them
  recoverable. A future „Remind me" remains explicitly outside this
  rule until it has its own scheduler.
- **NR-5** [replaced by NR-5a] [gtk] — The popover is transient and
  never changes the navigation stack. Only „See all" navigates
  normally into the digest location; closing returns to the current
  view without losing state.
- **NR-5a** [replaced by NR-5b] [gtk] — The popover is transient;
  opening/closing never changes the navigation stack. Only explicit
  row actions (Show in library) navigate normally and close the
  popover; the history is a popover-internal subpage without
  navigation.
- **NR-6** [active] [gtk] — „Fetch now" replaces its refresh icon with
  a spinner during the fetch and otherwise shows the age of the last
  update. Offline or error still show the last cache along with its
  age and only a subtle inline note in the footer — never an error
  banner. The underlying principle has been named app-wide since `NET-3`
  (`cached`/`interrupted`); this rule remains the authoritative version for
  New Releases' own spinner mechanics and is not superseded by it.
- **NR-7** [active] [gtk] — New Releases is a plugin on the plugins
  page, off by default, with the privacy subtitle „contacts
  MusicBrainz" and a choice of „Top artists only / all artists". With
  the module switched off there is neither fetch nor ✦; the cover,
  portrait, and lyrics modules do not belong to this rule and are
  governed in the follow-up branch `feat/network-opt-in`.
- **NR-8** [active] [gtk] — Turning the module on is the consent and
  therefore immediately triggers the first fetch: `set_enabled(true)`
  kicks off a fetch. As long as no fetch has ever succeeded, ✦ stays
  **visible** and carries an empty state („Checking for new
  releases…" while running, then „No upcoming releases from your
  artists"). Only after the first completed run does NR-5 apply
  normally again.
  Two edge cases: a **failed** first fetch (offline) keeps ✦ visible
  with a retry empty state, instead of letting the button disappear —
  otherwise „turned on, but gone" happens again. And the initial empty
  state carries **no** badge dot: it is feedback, not a request (P-1).
  *Reason:* NR-5 was formulated when population was guaranteed. Opt-in
  has created the permanent state „active, never populated", for
  which there was no entry point — ✦ appears only when entries exist,
  „Fetch now" sits in the popover behind ✦, and there is no starting
  fetch. NR-8 closes this loop without overturning NR-5. Privacy-wise
  unchanged: network traffic only arises after explicit activation,
  just immediately instead of never.
  The offline edge of the second edge case (a failed first fetch keeping ✦
  visible with a retry) follows the same "a cache or first run is never an
  error" principle that `NET-3` names app-wide; the opt-in trigger and the
  badge-dot rule themselves are consent semantics and lie outside `NET-3`.
  This rule therefore stays `[active]` unchanged.
- **NR-9** [replaced by NR-9a] [gtk] — builds on NR-3: the badge
  from NR-3 shows the **count** of entries with `seen_at IS NULL`,
  from 10 shown as „9+", disappears on opening (all listed entries get
  stamped), and renders no empty element at 0.
- **NR-10** [active] [gtk] — Row hover or focus fades out the status
  chip and fades in the row actions; on leaving, the chip returns.
  Keyboard parity: the row is focusable, focus shows the actions, and
  the buttons are reachable via Tab/Enter.
- **NR-11** [active] [gtk] — „Open announcement" opens a URL by
  priority: MusicBrainz URL relations of the release group (Bandcamp/
  purchase/streaming before official homepage/discography) → fallback
  to the MusicBrainz release-group page. Opened externally (default
  browser).
- **NR-12** [replaced by NR-12a] [gtk] — The history is a persistent
  record of all announcements ever shown as a **popover subpage** (no
  dedicated navigation location), grouped by period, with hidden
  entries individually recoverable. Retention: 6 months **and** at
  most 200 entries (the stricter limit wins), hard deletion, but never
  within the 90-day fetch window. Replaces NR-4.
- **NR-13** [active] [gtk] — Released releases already present in the
  library are marked (not filtered out) and offer the action „Show in
  library" (navigate + focus, **no** direct play path).
- **NR-3a** [active] [gtk] — The header trigger opens „Updates" and is
  visible as soon as at least one active feed has entries or a
  first-run state per NR-8. Its badge counts exclusively unseen
  entries of all active, fetch-ready feeds.
- **NR-5b** [active] [gtk] — The popover is transient; opening/closing
  never changes the navigation stack. Explicit row actions and the
  jump rows „Show all releases/concerts →" navigate normally and close
  the popover. The popover has no internal subpages; the history
  lives in the full releases view (NR-12a).
- **NR-9a** [active] [gtk] — The badge shows the sum of unseen releases
  and concerts, from 10 shown as „9+", and renders nothing at 0.
  Opening stamps the entire delta set of both sections in the current
  scope. Releases fully present in the library are listed and
  stamped, but never count toward the unseen badge.
- **NR-12a** [replaced by NR-16] [gtk] — The persistent history of
  all announcements ever shown lives in the full releases view as its
  own sidebar location. Hidden entries there are individually
  recoverable via the hidden filter with „Show again". Retention
  remains: six months AND at most 200 entries, hard deletion, never
  within the 90-day fetch window.
- **NR-14** [replaced by NR-17] [gtk] — The full releases view is a
  table `Date · Title · Artist · Type · Status`, sorted by date
  descending by default. Status is `In library`, otherwise `upcoming`
  or `released`. Activation always runs the three-way primary action:
  hidden → Show again; fully present and released → Show in library;
  otherwise Open announcement. The permanent filter row offers sticky
  chips for Not in library, Type, and Hidden, along with „X of Y
  releases", „Clear all", and exactly one „Show all" step at zero
  results.
- **NR-15** [replaced by NR-18] [gtk] — „Releases" is a sidebar
  location in SMART, before Concerts and only with the `new_releases`
  module active. Its badge equals exactly the number of rows visible
  on opening after persistent filters; 0 renders no badge.
- **NR-16** [active] [core] [gtk] — The full releases view is a
  discography-gap catalog for artists currently represented in the
  library. It contains regular albums and EPs regardless of age, but
  never singles or releases already fully present. Individual
  pre-release singles or incomplete album titles do not count as
  ownership; a released release only counts as complete once its
  distinct local track identities cover at least the smallest
  official MusicBrainz edition. Hidden gaps remain recoverable via the
  hidden filter; album and EP catalog rows are not subject to any
  time-based retention.
- **NR-17** [active] [gtk] — The gap view remains the table `Date ·
  Title · Artist · Type · Status`, sorted by date descending by
  default. Status is `upcoming`, `Missing`, `Incomplete`, or — when
  the length is known — `X of Y tracks`. The permanent filter row now
  offers only sticky Type and Hidden chips; activation opens the
  external release URL, Hidden activates `Show again`. An empty
  default filter confirms „No missing albums or EPs"; the footer
  contains no six-month retention.
- **NR-18** [active] [core] [gtk] — „Releases" remains a sidebar
  location in SMART, before Concerts, visible only with the
  `new_releases` module active. Its badge equals exactly the number of
  discography gaps visible with the persistent Type/Hidden filters; 0
  renders no badge.
- **NR-19** [planned] [gtk] — A releases gap may additionally offer a
  purchase path clearly marked as an affiliate link, but only for a
  partner contractually approved for installable Linux desktop apps.
  The disclosure sits directly at the purchase link; without approval
  or a genuine purchase relation, the unchanged external MusicBrainz
  relation remains commission-free. Library data and secret keys never
  end up in the URL. <!-- REVIEW: rule proposal -->
- **NR-20** [active] [core] [gtk] — The releases table extends NR-17
  with the `Buy` column. Only when MusicBrainz supplies a genuine
  HTTP(S) relation for the release group to a `/album/…` page on
  `bandcamp.com` or a subdomain does the row show `Bandcamp` there and
  open exactly that URL in the default browser. Lookalike domains,
  artist homepages, guessed search URLs, and all other targets produce
  no purchase button. The direct link is commission-free, contains no
  tracking parameters, and is not labeled as an affiliate link; NR-19
  remains reserved for a later contractually approved monetization.
## S. Surfaces & Geometry

<!-- Section letter: R (New Releases) is the last one assigned; S follows
     on directly. Prompted by four cases in one day (2026-07-18) that all
     passed with a green test and only showed up in the screenshot — ledger:
     docs/superpowers/plans/2026-07-18-style-explicit-rule.md. -->

Anything meant to be visible must be set explicitly. Inherited or
framework defaults do not count as set: they are the most common reason a
property is set and yet nothing happens.

- **STYLE-1** [active] [gtk] — **Effect explicit, not inherited.** Every
  surface meant to set itself apart from content (headerbar, revealed bars,
  sidebar edges, panels) carries background **and** separator line
  explicitly; every binding geometry (fixed widths, minimum heights) is
  checked against its actual allocation. `flat` stays exactly where **no**
  separation is deliberately wanted. Known traps this rule addresses:
  `AdwToolbarView` with `ToolbarStyle::Flat` suppresses bar backgrounds
  (including `@headerbar_bg_color`); an `AdwHeaderBar` without a title
  widget renders the window title as a fallback (`show-title` must
  additionally be off); a `GtkLabel` without `ellipsize` reports its full
  text as **minimum** width and defeats any `max-width` on the container;
  `AdwOverlaySplitView` computes in `sp` without `sidebar-width-unit = Px`.
  **Test rule:** intent may be checked, but for surfaces and geometry the
  **result** must be proven — not "property X is set", but "the surface has
  a visible background" or "the column stays at its width in a narrow
  window". What the framework guarantees is tested for existence; what can
  fail to appear is tested for effect (like TIP-1a/2a and SEARCH-2). If an
  interface is hidden in the test build (e.g. `SectionModel` via `cfg`),
  only the E2E evidence counts — "green" is structurally meaningless there.
## T. Accessibility & Keyboard

<!-- Section letter: S is the last section assigned on main; T follows
     seamlessly. The automatable rules are activated through isolated
     GTK/CUA runs; ACC-7 additionally requires real visual inspection. -->

- **ACC-1** [active] [e2e] — Full input parity: every action reachable by
  mouse or touch is, in the same context, also executable with the keyboard
  alone and ends in the same action/callback path. A gesture on `Label`,
  `Image`, `Box`, `DrawingArea`, or a drag surface without an equivalent
  keyboard path is a bug. A context menu or global shortcut only counts if
  it is available at the focused target and discoverable via Help, a label,
  or accessible help text.
- **ACC-2** [active] [gtk] — Semantics are part of operation: every
  interactive element exposes a short translated name, the matching role,
  its current state (`selected`, `checked`, `expanded`, `disabled`, `busy`)
  and — where needed — relationships, shortcut, and help text. Decoration
  carries `Presentation`. Native GTK/libadwaita controls are the standard;
  a custom role is a promise to fully deliver the matching native keyboard
  semantics.
- **ACC-3** [active] [e2e] — Focus order follows visible meaning: Tab
  forward and Shift+Tab backward traverse the interface logically, without
  jumps into hidden/inactive controls and without duplicate stops for the
  same command. Sidebar, list, and grid are each **one** tab stop; arrows
  move the active entry within them. Merely focusing/selecting triggers no
  navigation, playback, or other action — only activation does.
- **ACC-4** [replaced by ACC-4a] — Standard keys apply consistently
  everywhere; the global Space exception for the left sidebar toggle is now
  made explicit in ACC-4a.
- **ACC-4a** [active] [e2e] — Standard keys apply consistently
  everywhere: arrows navigate spatially or row-wise, Home/End jump to the
  start/end in long collections, Page Up/Down move page-wise, Enter
  activates the focused entry. Space remains global Play/Pause in passive
  collections and on an already-selected, passive view tab. The same
  applies to the focused left sidebar toggle; it collapses and expands the
  sidebar only via pointer or Enter. Other focused buttons/toggles with a
  genuine local action keep Space, text fields type a space character.
  Menu key/Shift+F10 opens the context menu, F10 the primary menu, and Esc
  closes the topmost transient container. A global shortcut must never
  steal text input or the local semantics of a focused control.
- **ACC-5** [active] [e2e] — Focus has a traceable lifecycle: start and
  navigation set it into the active target view; Ctrl+F sets it into the
  search field, whose Esc cascade returns it to the **current** content
  view. Dialogs/popovers start on their first meaningful control, keep
  focus within the topmost layer, Esc closes exactly that layer and returns
  focus to the trigger. Back/Forward restores the last meaningful focus of
  the target view instead of focusing the header or invisible children.
- **ACC-6** [active] [gtk] — Dynamic updates never steal or lose focus: if
  the logical element persists, its focus persists too; if it is removed,
  focus falls to the next, otherwise the previous operable entry, and
  finally to the stable container. Filtering, re-sorting, view rebuild,
  track changes, scan/sync/mount, and asynchronous card updates never move
  focus to another target unasked.
- **ACC-7** [planned] [manual] — Focus is always visible and unambiguous:
  every keyboard-reachable element shows a persistent focus indicator in
  both the normal and high-contrast theme, one that cannot be confused with
  hover, selection, or "currently playing". `outline: none` is only allowed
  with an at-least-equally-clear `:focus-visible` replacement. Actions
  revealed on hover also appear on keyboard focus or are reachable via the
  context menu of the focused container.
  <!-- REVIEW: rule proposal -->
- **ACC-8** [active] [e2e] — Direct manipulation has an alternative: every
  drag-and-drop/reorder target offers the same permitted move also via
  button, menu, or documented keyboard action; the same guards and
  persistence paths apply. Custom value controls (e.g. waveform seek) are
  focusable ranges: arrows adjust finely, Page Up/Down coarsely, Home/End
  set minimum/maximum; name, current value, and bounds are accessible.
- **ACC-9** [active] [gtk] — Shortcuts and access keys follow GNOME:
  existing standard actions use the standard bindings (among others Ctrl+F,
  Ctrl+W, Ctrl+Q, Ctrl+,, Ctrl+?, F1, F10, Alt+←/→); frequent labeled
  actions and primary dialog actions get collision-free mnemonics, as far
  as translations allow. The shortcuts view lists only actually wired
  actions and stays in sync with them within the same commit.

## T. Network features opt-in

- **NET-1** [replaced by NET-1a] — Automatic and bulk network fetches are
  opt-in. Cover downloads, artist portraits, and New Releases only start
  when their module is switched on; Online Lyrics also has a switch, so
  fully network-free use remains possible. Switching off takes effect
  immediately and does not hide images already cached locally.
- **NET-1a** [active] [core] [gtk] — Extends `NET-1` by a global gate
  `online-sources-enabled` (Preferences page "Online sources", `SET-9`): an AND
  condition in front of **every** network fetch in the app, sitting on top of
  the respective module or source switch — cover downloads, artist portraits,
  New Releases, online lyrics **and** the three online sources YouTube,
  Podcasts and Radio. The single authority for it is
  `reprise_core::online_sources::network_allowed` in core, right next to the
  module registry; every network entry point hangs off that instead of checking
  only its own module. Off means: no requests, no downloads, nothing hidden
  except the three sidebar entries — subscriptions, favorites and images
  already cached locally are left untouched. Per source and module, YouTube
  additionally has its own module flag, independent of Podcasts (issue #96):
  "Podcasts off + YouTube on" is a valid state.
- **NET-2** [active] [core] — Updates protect demonstrable prior use:
  existing downloaded covers or portraits activate their module, existing
  library databases keep Online Lyrics, and a previously active
  `artist_news` is carried over as an active New Releases module. Negative
  cache markers do not count as use; fresh installations start with all
  four network modules off.
- **NET-3** [planned] — Offline is a state, not an error: no network-backed
  place in the app may treat a missing network connection like an error
  message. The contract covers seven states every network-backed view (feed,
  search, refresh) must know: **cached** (the last successful state stays
  visible along with its age, never replaced by an error), **empty** (never
  fetched successfully yet — a loading/first-run state rather than silent
  emptiness), **queued** (an online action was accepted and is waiting for the
  network), **interrupted** (a running fetch aborts — the cache stays, only a
  discreet inline note, never a banner), **authentication** (401 or a missing
  credential — neutral, without prompting for credentials in the flow itself),
  **rate limit** (429 — treated like "interrupted", no special error picture)
  and **provider failure** (5xx/other — likewise). For the three online sources
  (Podcasts, YouTube, Radio) and phone sync, six concrete behaviours from turn
  6 apply in addition (6e, issue #107):
  1. Downloaded episodes and tracks stay fully playable, seekable and
     resumable — local playback never touches the network.
  2. Entries that are not downloaded stay listed from the cache but read
     "Needs network"; the row is dimmed, never hidden.
  3. Download and sync actions are accepted and carried as "Queued offline"
     rather than being greyed out; they run automatically, in order, as soon as
     the connection returns.
  4. Add dialogs disable their search field with a one-line reason; pasting a
     URL still works, and the subscription comes into being on the next fetch.
  5. Radio is the one exception: a live stream cannot be deferred. Stations
     stay listed, and "Play" reports "No connection · Retry" instead of
     queueing.
  6. Phone sync: MTP is local — syncing files that are already downloaded runs
     offline too; only entries without a local file wait.

  A critical distinction from `NET-1a`: "switched off" (the global gate
  `online-sources-enabled`, or a module switch being off) and "offline" (this
  contract) are **different states** and must not be conflated. Switched off is
  a privacy promise — it refuses search **and** the URL path alike, without
  exception; the only check for it remains `online_sources::network_allowed`
  and its callers (`podcasts::add_dialog_input::submit_refusal`,
  `radio::add_dialog::submit`). Offline refuses nothing — it marks things
  pending or offers a retry. This contract never checks a module switch, only
  connectivity.

  This rule is the contract prose and only turns `[active]` itself once all of
  its lettered sub-rules are. It consolidates the shared "a cache is never an
  error" principle that `NR-6`, `NR-8` and `CONC-4b` each already state in
  their own words for their own surface — those three rules stay `[active]`
  unchanged and refer here from now on instead of duplicating the principle;
  none of them changes behaviour or tests. `INST-12` stated the same principle
  for the instrumental model download and belonged in this list when it was
  written, but that surface has since been removed from the GTK frontend and
  the rule is replaced, so it is deliberately not counted here. `LYR-3` does **not**
  belong here: it governs the switched-off state of the lyrics module
  (`NET-1a` family), not offline — see the note there.
- **NET-3a** [active] [core] — The pure, display-free projection
  `reprise_core::connectivity`: `row_presentation(Connectivity,
  LocalAvailability)` yields `Playable` as soon as a file is present locally
  (the same online or offline), otherwise `NeedsNetwork` only while `Offline`.
  `deferrable_action_outcome` yields `RunsNow` when online, or when the file is
  already present locally (phone sync of an already downloaded file — MTP is
  local), otherwise `QueuedOffline`.
  `podcasts::download_state::DownloadState::local_availability` is the bridge
  from Podcasts' and YouTube's richer download state into this simple local
  signal. `Connectivity` is an explicitly set state, not an inference from a
  failed request — what it does not know: the reachability of an individual
  provider, authentication, or rate limits; those are request outcomes, not a
  connectivity state. This projection is a wiring foundation, not a finished
  display: automatically running pending actions when the network returns, and
  the display in the podcast and YouTube rows, follow in a later commit
  (`NET-3c`, F2).
- **NET-3b** [active] [gtk] — The radio exception: stations always stay listed.
  The context menu shows "Play" (`radio_context_menu::play_menu_label`) when
  `Connectivity::Online` holds or the station is already playing; under
  `Offline` the entry reads "No connection · Retry", and a fresh play attempt
  (`radio_view::try_play_station`) opens no connection while connectivity stays
  offline — no marking pending, no automatic run, because a live stream cannot
  be deferred. Connectivity is an injectable state
  (`RadioView::set_connectivity`), `Online` by default and not yet attached to
  a real operating-system signal in this rule.
- **LYR-1** [planned] [core] — Local embedded lyrics and `.lrc` sidecars
  are shown independently of the Online Lyrics module. Reprise does not
  yet read these local formats today; the rule stays planned until this
  dedicated format feature exists.
- **LYR-2** [active] [gtk] — LRCLIB is contacted only when the Lyrics tab
  is open, local text is missing, and the Online Lyrics module is switched
  on. There is neither prefetch nor batch fetch for upcoming queue entries.
  What matters is the loaded track, not the playback state: a track
  restored from the session that sits in the player bar shows its lyrics
  without a prior start. The empty state "Play a track to see its lyrics"
  applies only as long as no track is loaded at all.
- **LYR-3** [active] [gtk] — With the Lyrics tab open, text missing, and
  the module switched off, a centered StatusPage shows an icon, the title
  "Online lyrics are disabled", the subtitle "Enable them to load missing
  lyrics automatically", and "Enable in Settings" as a deep link to the
  briefly highlighted Plugins row. As long as LYR-1 stays planned, this
  state promises no local embedded lyrics. A switched-on module with no
  match shows "No lyrics found" instead.
  A distinction from `NET-3`: this rule handles the **switched-off** module
  (the `NET-1a` family — a deliberate user decision, not connectivity) and
  stays `[active]` unchanged for it. The case "module on, but offline" is not
  specified for lyrics today; were it to arise, `NET-3` would govern it, not
  this rule — the two states must not be confused.
- **DISCOVER-1** [replaced by BROWSE-1] — Network features without a
  permanently visible surface of their own get exactly one subtle,
  dismissible inline hint at the location of the visible gap: covers from
  three simultaneously visible fallback tiles, portraits from three
  simultaneously visible initials avatars, and New Releases at the top of
  the Artists view. Visible evidence latches the hint in; once shown or
  dismissed, it never returns permanently. The hint is neither a badge nor
  a toast.
- **DISCOVER-2** [replaced by BROWSE-1] — At most one activation row is
  visible per view. If the portrait hint and the New Releases hint meet in
  the Artists view, they are combined into one row "Enable network
  features for artists (images & new releases) →" with a deep link to the
  Plugins page; two stacked activation rows are forbidden.
## U. UI polish, contrast & cross-view context

<!-- Decisions and the boundary with Batch B are in
     docs/superpowers/plans/2026-07-18-ui-polish-beschluesse.md. -->

- **SEARCH-6** [active] [gtk] — The magnifier and Ctrl+F toggle the
  search bar both ways (show ↔ hide). Hiding never clears the query: with
  a non-empty query, its chip stays visible and the magnifier stays in the
  `:checked` accent style (FIL-1, SEARCH-3/5).
- **SEARCH-7** [active] [gtk] — If the search field along with its
  internal controls loses keyboard focus, the open search bar collapses
  after the current pointer activation completes. A non-empty query
  remains, per SEARCH-3/5, as an active filter along with its chip and
  accent magnifier; a click on the magnifier must not accidentally reopen
  the bar that was closed by that same focus change.
- **LYR-4** [active] [gtk] — Centering of the active lyrics line is
  clamped to the top at the start of the song. As long as there aren't
  enough context lines above the active line, the text block sits at the
  top; only once there is enough lead-in does the active line move to the
  middle.
- **STYLE-2** [active] [gtk] — Content and the track table use the
  `.view` level; the left sidebar and the right Now Playing panel jointly
  use the one-level-higher `sidebar_bg` surface of the active theme. Both
  flanks carry a 1px hairline on their inner edge. There is no pane-specific
  retinting and no hardcoded pane surface.
- **STYLE-3** [planned] [gtk] — Two accent roles stay separate: the fixed
  app accent (`@accent_color`) denotes durable UI meaning such as
  selection, ratings, active toggles, links, chips, and focus; the dynamic
  playback accent (`@reprise_player_accent`) denotes exclusively the
  running track, such as Play/Pause, waveform, playing row, EQ, glow, and
  the GRID-1 inner ring. An element never mixes the roles.
- **STYLE-4** [replaced by STYLE-1] — Chrome glass is neutral and
  theme-dependent, never tinted by the cover accent. GL/NGL/Vulkan use 24px
  backdrop blur over a neutral tint floor of at least 80%; Cairo, unknown
  renderers, High Contrast, and disabled animations degrade fail-closed to
  a neutral, at least 94% opaque tint.
- **STYLE-5** [active] [gtk] — **Shrinking never cuts off essential
  controls.** On horizontal, vertical, or combined window shrinking, the
  primary controls and status information remain reachable within the
  window area. In particular, the structural player bar (PLAY-7b) keeps
  its full height; cover, Play/Pause, position time, waveform, duration,
  and volume lie entirely within their allocation. Long titles and artists
  ellipsize within the left metadata zone and never push transport or
  waveform out of the window center. Scrollable content gives up space,
  not the player bar.
- **STYLE-6** [active] [gtk] — On strong horizontal shrinking, the track
  table temporarily collapses secondary visible columns; cover, title, and
  artist stay visible. This collapsing changes neither stored visibility,
  order, or widths nor the sort. "Show columns" restores the user's
  configuration in the narrow window; additional width is then scrolled
  exclusively horizontally within the table.
- **STYLE-7** [active] [gtk] — If the library window is shrunk or snapped
  to a width where both flanks visibly displace the main content, the left
  library sidebar and the right Now Playing panel close together in the
  same responsive transition. A 10s undo toast restores exactly the state
  of both flanks before the shrinking; later widening also restores this
  state, provided the user did not change the flanks themselves in the
  narrow window. Responsive changes never overwrite a stored sidebar or
  panel preference, and both header toggles remain reachable for manual
  reopening.
- **CONTRAST-1** [active] [gtk] — There are three central text levels:
  primary approximately 0.95 for titles and values, secondary approximately
  0.7 for artist, status, metadata, and column headers, hint approximately
  0.5 for placeholders, hints, and disabled secondary text. Matching
  Adwaita named colors take precedence over custom alphas; there is no
  per-element retinting.
- **CONTRAST-2** [replaced by CONTRAST-2a] — Every "N tracks ·
  duration" status line is a genuine bottom bar with a defined surface and
  a top hairline. It reserves its own space and never covers a track row;
  only against this fixed surface is its secondary-text contrast
  determined.
- **CONTRAST-2a** [active] [gtk] — The "N tracks · duration" status line
  is a compact pill overlay with a defined surface, rounding, and hairline
  at the bottom right of the track table. It reserves no full-width row.
  If the right info column opens, the pill stays at the same distance from
  its left edge; only against its fixed surface is the secondary-text
  contrast determined.
- **CONTRAST-3** [active] [gtk] — Status lines, column headers, sidebar
  section labels, and card meta lines reach at least 4.5:1 against their
  respective surface. `.caption` plus secondary level counts as small type
  here and needs the same check as hint at normal size.
- **CONTRAST-4** [replaced by CONTRAST-1] — Every active text and every
  active icon in the glass reaches at least 4.5:1 against the worst case
  of its zone: the tint floor composited over the lightest or darkest
  translucent content respectively. Artist, time, search field, and header
  actions are active content; only disabled or purely decorative elements
  are allowed to fall below that.
- **NAV-10** [replaced by NAV-10a] — The running context stays visible in
  all views with a shared playback-accent marker; on first entry into a
  view it is revealed once, later switches restore NAV-5's remembered
  ID-plus-offset anchor. Explicit "Go to album/artist" always jumps
  deterministically; selection never follows playback.
- **NAV-10a** [active] [gtk] — **Marking and scrolling are separate.**
  Every visible instance of the loaded track carries the same playback
  marker. Double-click/Enter on an already-visible row does not change the
  viewport. Play from Stopped as well as explicit Previous/Next center the
  new track without stealing focus or selection. Auto-advance centers only
  if no scroll movement has occurred for 1.5 seconds; explicit
  metadata/reveal navigation always selects, focuses, and centers.
- **QUE-7** [active] [gtk] — Up Next consists of the manual queue plus a
  virtual, named context tail with a count. The tail is not materialized as
  individual rows but only rendered within the visible window; the
  sidebar row "Queue" counts exclusively the manual queue and shows no
  counter at zero.
- **QUE-8** [active] [gtk] — Drag reorder exists exclusively in "Next in
  Queue". The manual section is reorderable; a drag out of "Continuing"
  upward materializes exactly that entry in the manual section.
  Multi-select, Clear, Save as Playlist, and the full context menu remain
  in the queue ColumnView.
- **NPP-11** [active] [gtk] — The panel views use a centered
  `AdwViewSwitcher` as the title widget and adaptively degrade in a narrow
  window to a bottom `AdwViewSwitcherBar` or an icons-only
  `AdwInlineViewSwitcher` via `AdwBreakpoint`. Implementation in Batch B;
  see the decision document.
- **NPP-12** [active] [gtk] — Without a stored preference, the right Now
  Playing panel starts closed. As soon as the user opens or closes it via
  the header toggle, this persisted state wins on all following starts
  (NPP-4); the new default never overwrites an existing preference.
- **NPP-13** [active] [gtk] — A track change does not visibly rebuild the
  right column: tabs, queue or active tab, footer, and panel surface
  remain standing throughout. Only the album cover changes with the
  standard token; the old cover lies on top of the fully resolved new
  cover or placeholder for this and only fades out afterward. The queue
  updates its rows independently of this, so the played title moves up out
  of the list. The playback accent derived from the cover continues to
  follow the ambient transition from MOT-1 separately; interruptions
  follow MOT-6. Newly loaded synchronized lyrics start at line 0 and
  position it per LYR-4. Without animations, cover and content switch hard
  (MOT-7).

## V. My Stats

- **STATS-0** [active] [core] — A "play" is the same thing everywhere: at
  least 50% of the track or at least four minutes listened. Exactly these
  events live in `listen_events`, and the My Stats view computes
  exclusively from them — hero time, plays, top lists, spotlight, genres,
  clock, and highlights are projections of the same row set. The running
  counter `tracks.play_count` never feeds the view; time and count
  therefore cannot drift apart. Day and hour boundaries do not arise in
  SQL: the core functions take a timezone as a parameter and bucket every
  event individually through it, so that daylight-saving transitions never
  shift a boundary. Everything is local: no network, no cloud, no
  third-party source is mixed in.
- **STATS-1** [replaced by STATS-11/STATS-12] [core] — The header shows
  total listening time large in whole hours ("68 hours"; under an hour in
  minutes, never "0 hours"), a comparison pill "▲ N % vs <previous period>"
  in the teal app accent (never the cover accent), and the subline "N
  plays · Ø X min/day · N artists" in secondary tone. Given enough width,
  the period dropdown sits at the right ("<year> so far / <previous year> /
  All time / Last 30 days"). Before total time or pill ellipsize, dropdown
  and Customize menu wrap below the hero; at even narrower width the pill
  sits below the hour anchor. Below that runs a slim area ribbon of
  listening time, whose axis **follows exactly the chosen period** —
  "2026 so far" shows Jan–Jul, never a rolling 12-month window. The
  running bucket is marked open (dashed, hollow point), the peak is set;
  hover names the exact value. If a previous period with listening time is
  missing, the pill is dropped. The pill **names** the compared span
  instead of saying "previous period". The previous period is equal in
  length **and** seasonally matched: "2026 so far" is computed against
  Jan–Jul 2025 and called "vs same period 2025", never against the
  equal-length stretch immediately before it (Jun–Dec 2025) — listening
  time is seasonal, otherwise summer would stand against winter. A full
  calendar year is computed against the whole previous year ("vs 2025"),
  the rolling window against the 30 days directly before it ("vs previous
  30 days"), because there is no recognizable calendar equivalent one year
  back for that. February 29 clamps to the 28th in the previous year.
  "All time" has no previous period and never carries a pill.
- **STATS-1a** [replaced by STATS-11a] [core] — The comparison pill stays
  readable at any ratio: increases below +1000% keep appearing as a whole
  percentage, from +1000% as a rounded factor ("▲ ×11 vs 2025"). A
  meaningful decimal place is kept ("×11.5"), a meaningless zero is
  dropped ("×11", never "×11.0"). Strong declines from 50% use the same
  form with a downward marker ("▼ ×0.3"); a non-zero factor under 0.1
  stays honest as "▼ ×<0.1" and never rounds up to "×0". If the compared
  time was under a minute, it is effectively zero for the visible minute
  granularity; instead of percent or factor, a period-appropriate
  qualitative statement stands, such as "New this year".
  The pill names only the short reference ("vs 2025") and never
  ellipsizes; the tooltip carries the full semantics ("vs same period
  2025"). `×` and the decimal separator remain translatable. The seasonal
  span and comparison calculation from STATS-1 do not change as a result.
- **STATS-2** [replaced by STATS-13] [core] — The Artist Spotlight is the
  centerpiece: #1 artist with a large cover and rank badge, eyebrow "YOUR
  #1 ARTIST", name, line "N plays · N h · N % of your artist listening" —
  the share refers to listening time with artist attribution, the same
  population that forms the ranking, not to every play —, three top-track
  chips, and the actions Play (container play over the artist's track
  list) and "Go to artist" (regular NAV push with back history). Behind
  the cover sits a subtle cover-accent glow — the cover accent stays
  reserved for playback elements. Below it, a ghost row names ranks 2–5.
- **STATS-3** [replaced by STATS-15] [core] — The Genre Spectrum is
  **one** horizontal segment bar in teal gradations with a legend (dot ·
  name · %), fed from the library's genre tags. The five strongest genres
  form their own segments, the rest is bundled into "Other"; tracks
  without a genre count neither as a segment nor as "Other". The bar is
  pure display and not navigation: segments and legend are not clickable.
- **STATS-4** [replaced by STATS-10] [core] — Below the spectrum sits an
  asymmetric row (1.35fr / 1fr): on the left the Listening Clock as a
  24-hour histogram from the timestamps with teal-highlighted peak hours
  and a caption ("Peak 11 PM–1 AM · night owl"), on the right four
  highlight tiles — Streak (longest run of consecutive local days with ≥ 1
  play), Discovered (tracks first played in the period), Busiest day, On
  repeat (highest play count) — plus the CTA "Mix from <top genre> ·
  Create". It mixes exactly the track group of the displayed genre, never
  the tracks that happen to be spelled exactly that way (STATS-9). If the
  group is expressible as a rule — i.e. a single spelling —, a genuine
  Smart Playlist is created; if it combines several spellings, an ordinary
  playlist with exactly the tracks of the group is created instead,
  because the rule engine only links its rules with AND and knows no
  alternative. Always **one** genre is mixed; without a genre in the
  period the CTA is dropped. Day and hour boundaries follow the user's
  local time, not UTC. In a narrow window the row collapses to a single
  column via AdwBreakpoint, without the order changing. The row is sized
  so that its two minimum widths together stay below the breakpoint —
  otherwise there would be window widths where it still stands
  side-by-side but is narrower than it needs.
- **STATS-5** [replaced by STATS-14] [core] — Top Tracks spans the full
  width: numbered list with cover, title, and artist, a relative play bar,
  and play count, with a sort toggle "by plays / by time". The bar is
  relative to the list's frontrunner, never to an absolute maximum.
- **STATS-6** [active] [core] — Empty and sparse data situations are
  never shown as empty charts. Without listening history in the period, a
  friendly empty state appears ("Start listening to see your stats")
  instead of axes with a single lonely bar. With sparse data the
  granularity becomes finer (days or weeks instead of mostly empty
  months).
- **STATS-6a** [active] [gtk] — An error is not an empty state: if the
  query fails, a dedicated error page appears ("Your stats could not be
  read"), never the invitation "Start listening to see your stats".
  Visibility arises here through the page switch, not through additional
  showing/hiding of individual sections underneath.
- **STATS-6b** [replaced by STATS-6c] [gtk] — Imported listening history
  used to generate its own status page, even though its counters cannot be
  assigned to any stats period.
- **STATS-6c** [active] [gtk] — The period list follows exclusively the
  detailed listening history: the current year always stays available,
  older calendar years appear only if they contain at least one
  `listen_event`. Imported `tracks.play_count` counters generate neither a
  year nor a special notice. If an available period is empty, the regular
  empty state stays visible; hero and period dropdown stay operable above
  it, so the selection never becomes a dead end.
- **STATS-7** [replaced by STATS-10] [gtk] — My Stats is curated, not
  freely editable: no drag-and-drop widget board. A ⋮ menu "Customize"
  shows and hides the Clock, Genres, and Highlights sections via
  CheckButton; the selection persists across sessions. The menu contains
  nothing more — the spotlight is fixed as the Artist Spotlight. The order
  of sections is fixed, sizes are not manually adjustable — adaptation to
  window width happens exclusively via AdwBreakpoint.
- **STATS-8** [active] [gtk] — In My Stats there is no filter row and no
  search of the track list — that is a different view. The right Now
  Playing column behaves as everywhere. The period dropdown is this
  view's only view control.
- **STATS-9** [active] [core] — **Dedup:** Unclean tags must not
  splinter numbers. Top Artists, Top Genres, album-artist aggregates, the
  spotlight, and every track selection that starts from one of these rows
  use **one single** key resolution — never a second formula per caller.
  **The name first:** trim, Unicode lowercasing (`str::to_lowercase`, so
  beyond ASCII, but no full casefold — "Straße" stays separate from
  "STRASSE"), whitespace collapse, and diacritics folding (NFKD without
  combining marks). "Lorna Shore", "lorna shore", and "Lorna Shore " are
  thus one entry with one sum. **Only then the MBID, and only within the
  name group:** it is the stable identity of this group and merges further
  name groups with the same identity; but it must **never split** a name
  group, because MBIDs are sparsely populated and typically attach to
  exactly one spelling ("Sigur Rós" with, "Sigur Ros" without). If several
  MBIDs carry a name group, the most-played one wins, on a tie the
  alphabetically first — the key never depends on row order.
  `tracks.artist_mbid` describes the raw `artist` column: if the row's
  album artist names a different act, the MBID does **not** apply there,
  otherwise a guest contribution would pull two unrelated bands into one
  row, one "play", and a tag-editor invitation. Because period and whole
  catalog are different populations, their resolutions may differ: if a
  track selection finds nothing for the key, it falls back to the name
  group, and an empty result is logged, never swallowed silently.
  The key exists only at runtime: no stored column, and the view
  **never** writes tags back — stats are read-only.
  What is displayed is always a genuine original spelling of the group
  (the most frequent one; on a tie the most recently played, then
  alphabetical), never the normalized form. **Never guessed:** what is
  merged is exclusively what is exactly equal after normalization — no
  fuzzy matching, no Levenshtein distance, no prefix merge, so "Lorna
  Shore Band" stays separate from "Lorna Shore". If a group combines at
  least two spellings, a subtle hint at the list entry points this out and
  leads into the multi-tag editor of the affected tracks; unifying remains
  an invitation, never an automatic write.
- **STATS-10** [active] [gtk] — My Stats tells its story in a fixed order
  from top to bottom: header row (title, optional "New this year" badge,
  period selection) · hero (total figure, subline, KPI row) · weekly chart
  · two-column row of band card and songs card · optionally the expanded
  top-track list as its own full-width section · genre card. There are no
  more sections: no Listening Clock, no highlight tiles, no Customize menu
  — the page is curated and not configurable.
  In a narrow window the two-column row stacks, without changing the
  order. Period selection remains, per STATS-8, this view's only control.
- **STATS-11** [active] [core] — The hero shows total listening time
  huge, in the page-wide unified compact format: from one hour "N h M",
  at whole hours "N h", under one hour "N min". Below that sits the
  subline "N plays · N artists", on the right along the baseline four KPI
  pairs: "Per day" (avg. listening time/day) · Trend (absolute
  listening-time delta to the comparison span with a direction icon in
  accent color) · "Pace for <year>" (linear yearly projection, only in the
  current year) · "Best week" (start date and listening time of the
  strongest local calendar week). All KPI durations use the same compact
  format. The comparison span remains, unchanged, the seasonally matched
  previous period: "<year> so far" against the same span of the previous
  year, a full year against the previous year, the 30-day window against
  the 30 days before it; "All time" has no trend KPI. KPIs without a value
  are dropped without replacement instead of showing a placeholder.
- **STATS-11a** [active] [core] — The trend stays honestly readable at
  any ratio: the KPI names the absolute delta and the short reference
  ("vs 2025"); the tooltip carries the full semantics including the
  percentage value, from ×11 ratios on as a rounded factor per the
  existing form rules. If the compared time was effectively zero (under a
  minute), the badge "New this year" appears in the header row instead of
  the KPI — never "∞ %" and never "×0". The KPI does not ellipsize.
- **STATS-12** [active] [core] — The chart shows listening time per local
  calendar week. From eight weeks with plays on, the area chart applies
  over the exactly chosen period. With fewer weeks, the axis begins at the
  first play week and every week gets an equally wide slot across the full
  card width; zero weeks stay visible as a 2-pixel tick on a continuous
  1-pixel baseline. Under ten weeks, every slot carries a week label,
  longer axes carry month labels. The compact variant is approximately 160
  pixels tall. Both variants leave 10–15% of headroom above the maximum.
  The best week gets a lighter accent shade instead of a marker line; its
  measured label sits above it with edge spacing ("best week · 4 h 12").
  The current week ends in an open point. Hover names the week and the
  exact value. Markers and points are pure display. Only if the period is
  too short for weeks does the axis fall back to days (STATS-6); very long
  "All time" spans may show months and then drop the week marker — the
  best-week KPI stays.
- **STATS-13** [active] [gtk] — The band card shows the most-listened
  artist as an image hero: the album cover of their most-played track
  fills the card and fades out downward into the card background; if a
  cover is missing, an initials tile stands in its place — never an empty
  surface. Above it the kicker "MOST PLAYED BAND", name, and the line "N
  plays · <duration> · N % of your artist listening"; the duration follows
  the compact format from STATS-11. Below it, ranks 2–5 with a thin bar
  relative to rank 1. Clicking the card or a rank row opens the library
  filtered to the artist (regular history push). If a group combines
  several spellings, the unification hint from STATS-9 is retained.
- **STATS-14** [active] [gtk] — The songs card shows the six leading
  tracks: cover, title, and artist in two lines, a horizontal bar relative
  to rank 1 in an accent gradient, the play count on the right. Next to
  the kicker, the toggle "by plays / by time" sorts both these six rows
  and the full ranking. Clicking a row opens the library filtered to the
  artist with the track focused; hover or focus shows a play button on the
  cover that immediately plays exactly that track; the context menu offers
  "Play next", "Add to queue", and "Go to album". The ghost button "Show
  all top tracks" expands the numbered top-10 list below the two-column
  row as its own full-width section; the genre card follows below that,
  and the bar stays relative to the frontrunner of the respective sort.
  The list shows durations in the compact format from STATS-11; its
  titles and artists get link color and underline only on hover, the
  focus ring stays visible.
- **STATS-15** [active] [core] — The genre card consists of a stacked bar
  (segment width = share, accent gradations by rank, last segment
  neutral, tooltip "<genre> · N % · <duration>") and up to four tiles of
  the strongest genres: cover of the most-played track in the genre,
  "<genre> · N %", below it "<duration> · top: <artist>". Both durations
  follow the compact format from STATS-11. Top artist and cover per genre
  arise from the same key resolution as all groupings (STATS-9). Clicking
  the tile cover opens the library filtered to the album; clicking a
  segment or the remaining tile area opens the library scoped to the
  respective genre.
  Tracks without a genre continue to count neither as a segment nor as
  "Other".
- **STATS-16** [active] [gtk] — Under ten plays in the chosen period, the
  data situation is too thin for a trend: instead of the chart, the hint
  "Keep listening — stats grow with you" appears; hero numbers stay real,
  and only cards with data are rendered — never placeholder cards. Without
  any play at all, the empty state from STATS-6/STATS-6c still applies,
  including operable period selection.
- **STATS-17** [active] [gtk] — My Stats stands fully in place from the
  first frame on: cards, hero number, KPIs, texts, cover, and images do
  not fade, do not slide, and do not count up. Only bars move, together
  after a calm start frame of approximately 100 ms and with ease-out
  `cubic-bezier(0.16, 1, 0.3, 1)`: in sparse-week mode the chart bars grow
  in 500 ms from the baseline with 80 ms stagger; the best-week label
  fades in only after its own bar ends, over 150 ms. The alternative
  area/line mode is already fully drawn in the first frame and has no
  entrance animation. Horizontal bars — band ranks 2–5, song bars, and
  genre segments — grow within their respective card in 450 ms from the
  left with 40 ms stagger; genre segments run in reading direction. Bars
  below the visible viewport also follow the same start, there is no
  special fold handling. A period change never restarts the entrance
  choreography and only interpolates bar values over 250 ms; all other
  content switches immediately to its new final state. With
  `gtk-enable-animations=false`, without exception all bars and the
  best-week label stand immediately in their final state.
## W. Buttons & interaction states

<!-- Section letter: V (My Stats) is the last section assigned on main;
     W follows on without a gap. The letter position was verified against
     the main state when inserting. Careful on merge: feature/tag-rework
     also claims a „W" (Library Doctor) on an older base, but there the
     whole layout from T onward is shifted — the letter needs to be
     reassigned during that branch's rebase, not here. -->

A button that doesn't respond to pointing and pressing is broken for the
user, even if it works. Reprise didn't have this problem because Adwaita
delivers too little, but because the app's own CSS runs at
`STYLE_PROVIDER_PRIORITY_APPLICATION` and beats the theme rules regardless
of specificity: a *stateless* `background-color: transparent` on a button
selector wipes out Adwaita's `:hover` and `:active` right along with it.
So the rule here isn't "more effects", but: **one state vocabulary,
centrally defined, applied everywhere** (BTN-4, the button reading of
STYLE-1).

- **BTN-1** [active] [gtk] — Every clickable button has four distinguishable
  states, and each one is visible. **Rest** is idle. **Hover** lifts the
  surface: icon buttons get a visible background (white ~8%), cursor
  `pointer`, transition on the micro token (150 ms) — not just a shadow.
  **Active/Pressed** sinks in immediately: surface white ~14% plus
  `scale(0.94)`, so the click lands. **Focus-visible** is an accent ring
  for the keyboard and never the hover state alone. The cursor here is a
  widget matter, not CSS: GTK4 CSS has no `cursor` property, so
  `style::buttons::arm` sets it — and only on the app's own surfaces, so
  dialogs and Preferences stay native.
  <!-- Deliberate HIG deviation: Adwaita buttons don't change the cursor. -->
- **BTN-2** [active] [gtk] — Toggle buttons show their state persistently,
  not just in the moment of the click. Shuffle and Repeat are both
  `GtkToggleButton` and speak the same `:checked`: accent surface in the
  app accent (never the cover accent) plus a small dot under the icon as a
  second, **non-color** signal — color alone doesn't carry for color
  blindness. The state survives hover and unhover; hover only modulates
  the surface's brightness and never flips the state indicator. Repeat-One
  additionally switches to the icon with the „1".
  <!-- The dot is a second background layer (radial-gradient), not an
       extra widget: that way it also works on round buttons. The
       „filled icon" from the template can't be implemented — the
       Adwaita symbolic set has no filled variant for shuffle/repeat; the
       accent surface delivers the fill signal instead. -->
- **BTN-3** [active] [gtk] — Not all buttons are equally loud, and
  loudness is a tier, not a case-by-case decision. **Primary** (Play,
  Create Mix, Apply): accent surface, strongest hover and press.
  **Standard** (icon transport, header actions): flat, hover background,
  subdued press. **Tertiary** (menu entries, list rows): background hover
  only, no scale — a row in a list must not jump under the cursor. The
  big Play/Pause button is the primary action and may respond more
  visibly on press than its neighbors: an additional ring in the playback
  accent.
- **BTN-4** [active] [gtk] — Hover, Active and Focus are defined **once**
  (`ui/style/buttons.rs`) and applied everywhere, via class or — where
  Adwaita builds the buttons internally — via selector from the same
  list. No per-button re-tinting. A surface may keep its own *resting
  look* (fill, radius, a designed hover like in the context menu), but
  must never define `:active` or `:focus-visible` locally. Hover and
  press are alphas over `currentColor` — not over the accent, which sinks
  into the glass of the player bar and the Now Playing tab bar, and not
  over a fixed white, which would be invisible in the light palettes.
  `currentColor` is the surface's own foreground color, so it's always
  measured against the tint and never against a zero background. With
  `gtk-enable-animations = false`, scale and transition drop out,
  **the state change remains** and switches hard — feedback must never
  disappear entirely.
  <!-- CSS `transition` and `@keyframes` follow the setting on their own
       (MOT-7, probe `mot_7_css_honours_enable_animations_setting`).
       Ungated, only `transform` in `:active` would remain — a static
       state style, not a transition. The provider in
       `style/reduced_motion.rs` neutralizes that. -->

## X. Song Visuals

<!-- History: This section used to be called „Local Sound Profile" and
     carried the rules for Song Analysis (Audio Character), Create Similar
     Mix, and Related Artist Discovery. These features were removed (chore
     eda0edaebb); their rules AC-1..AC-6, AC-9, and AC-12..AC-18 are deleted
     here (git preserves the history). What remains are the still-active
     Song Visuals rules. The AC prefix remains as the stable rule ID of the
     visuals rules. -->

- **AC-7** [replaced by AC-10]
- **AC-8** [replaced by AC-11]
- **AC-10** [replaced by AC-19]
- **AC-11** [active] [gtk] — Continuous motion exists only with the
  Visual tab visible and only as long as the player holds a track at
  all. Ongoing playback shows the audio-reactive bars. Pause and Stop
  let the live bars decay and hand over to a resting breathing motion: a
  flat wave, tapering off at both edges, of at most 10% bar height, that
  travels across the width once every six seconds and is redrawn at rest
  at roughly 30 Hz instead of the full render rate. Without a loaded
  track, the surface stays empty and without a tick callback;
  `gtk-enable-animations=false` shows the resting wave as a static image.
  This is the audio-functional exception for continuous motion permitted
  in MOT-2.
- **AC-19** [replaced by AC-20]
- **AC-20** [replaced by AC-21]
- **AC-21** [replaced by AC-22]
- **AC-22** [replaced by AC-23]
- **AC-23** [active] [core] [gtk] — „Song Visuals" is a plugin, switched
  off by default and applicable live. When switched on, the Linux
  pipeline branches off locally normalized mono PCM before ReplayGain;
  CAVA math generates 64 logarithmic display bands from it, clamped to
  0–1. The portable core uses CAVA's double FFT resolution below 100 Hz,
  quantized cutoff frequencies, and a fixed frequency EQ, as well as
  noise-floor gate, auto-sensitivity, integral, and gravity. Digital
  silence does not increase sensitivity; non-finite inputs and outputs
  are neutralized, and all internal feedback loops stay bounded.
  The scene engine takes over every CAVA band in the same frame without
  a second loudness mapping, normalization, or live envelope. It draws
  64 frequency-dependent, finely segmented neon columns one to one, with
  the existing cyan-to-magenta gradient, reflections, glow, and slowly
  falling peak caps. Under render load, "latest wins" applies strictly;
  old impulses are not carried over into newer CAVA frames. Pause and
  Stop may only apply a visual dampening for the decay required by
  AC-11; the resting wave defined there then only fills in what the live
  bars leave free, and changes neither CAVA values nor peak caps.
  The glow layer behind the columns is never derived from the CAVA
  bands. Auto-sensitivity keeps re-normalizing those, so a quiet sung
  passage climbs to the same band values as a drop and the glow would
  fire on both. Instead a second path measures the same PCM without any
  gain of its own: a 30–150 Hz band, its RMS in true dBFS, and a slow
  baseline of the track's own recent bass. Absolute level and the swell
  above that baseline together produce two presentation-only values. A
  rhythmic kick lifts two broad neon glows softly and in proportion to
  the measured pressure; only pressure sustained across a breakdown adds
  the two brighter inner auras. A bass band that stays quiet in absolute
  terms never glows, however tall the bars grow, and high-frequency
  energy alone never triggers either layer. Both release after the
  impulse instead of flickering, and neither changes CAVA values, peak
  caps, nor bar heights. With animations switched off, the layer holds
  the current frame's value without decay.
  Below the canvas the visual names the analysis it reacts to —
  absolute bass level, baseline, kick glow, and breakdown aura. The
  numbers refresh at most ten times per second so they stay readable,
  and a band without measurable signal reads as a dash instead of a
  bottomed-out level.
  Track and album ReplayGain normalize the audible output only after the
  PCM branch-off; the same input waveform therefore produces the same
  visual deflection regardless of the stored gain value. A mode
  selection and „Grid" do not exist. Fullscreen only constrains the
  internal scene raster area and scales it to the unchanged canvas
  size. When panel height is tight, the visual content stays below the
  tab switcher and scrolls within its own tab, instead of covering the
  switcher. The labeled canvas takes on the current cover accent via the
  same global ambient crossfade as the player bar; only without a usable
  cover color does the theme accent apply.

## Y. Library Doctor / Tag Cleanup

Library Doctor strictly separates detecting, deciding, and writing: a
scan reads and collects suggestions, the review table decides
field-by-field, and only its Apply starts a journaled write job. „Safe"
means deterministic and high-confidence, never „without review".

- **DOC-1a** [active] [core] — **Local is read-only and invents
  nothing.** The scan reads the actual tags of the frozen files; the DB
  only supplies scope, track ID, path, and file identity for this. It
  writes neither tags nor existing track metadata and starts neither
  scanner nor reconcile. As local suggestions, only the following are
  allowed: mechanical Unicode trim at field edges; a missing Album Artist
  from the non-empty Artist of the same file; and unifying Artist, Album,
  Album Artist, and Genre via exactly `normalize_group_key` from
  STATS-DEDUP. Without remote evidence, the most frequent exact spelling
  actually present within the frozen scope wins, after edge trim. A tie
  produces only a manual candidate group made of actually present
  values. Title receives only edge trim. Internal whitespace serves
  grouping only; what gets written is always a real winner. Title case,
  genre alias lists, fuzzy matching, and invented substitute values are
  forbidden.

- **DOC-1b** [active] [core] — **Remote remains a separate source.**
  With MusicBrainz/AcoustID suggestions switched on, the sparing cascade
  applies: valid embedded MBIDs first, MusicBrainz only for metadata
  still unresolved after that, AcoustID fingerprint only for tracks still
  unresolved. A name that is uniquely canonical via MBID beats local
  frequency, but remains a remote suggestion with source and confidence
  and is never relabeled as Local. At most one competing suggestion
  exists per track and field. Remote may only suggest Title, Artist,
  Album, Album Artist, Year, and MusicBrainz Recording ID; in version 1
  no other MBID type is written. Year comes from a unique release's own
  year, otherwise, for a unique release group, explicitly from its
  „original release"; ambiguous editions produce no year suggestion.
  Remote genre, track/disc number, rating, path, and cover remain
  forbidden.

- **DOC-1c** [active] [core] — **Network calls are minimal, limited, and
  cancellable.** MusicBrainz receives only the existing Title/Artist/
  Album/Album-Artist values needed for resolution, MBIDs, and duration
  where applicable; AcoustID receives only fingerprint and duration.
  Path, filename, library root, internal track ID, rating, listening
  history, playlist and device state never leave the app; filename-based
  placeholders are never sent. AcoustID uses HTTPS and POST. Positive,
  complete cache entries are valid for 30 days, negative or ambiguous
  ones for seven days, and are strongly identified via MBID or
  Chromaprint version + fingerprint + duration; a cache hit retains its
  remote provenance. Incomplete responses are not cached. MusicBrainz
  runs jointly limited to at most one request per second, AcoustID under
  its public limit. `429` honors `Retry-After`, timeout/5xx get at most
  two backoff retries, auth/key errors open the circuit for the rest of
  the job. Cancel prevents the next request and also takes effect during
  backoff; local scan and complete individual results remain valid in
  that case.

- **DOC-1d** [replaced by DOC-7a] [gtk] — **Local activation is not a
  network release.** Library Doctor is off by default. Its main switch
  activates only local checks and shows no network prompt. The separate
  switch „MusicBrainz/AcoustID suggestions", off by default, shows a
  short, versioned confirmation with the data allowlist from DOC-1c on
  first activation; Cancel leaves it off. The plugin row and the results
  view bind the same persistent switch. Switching off stops future
  remote requests, hides remote rows, and removes their selection;
  switching on again shows existing or newly loaded remote suggestions
  unreviewed. A missing fingerprint capability is visibly explained as
  „AcoustID unavailable", while Local and pure MusicBrainz resolution
  keep working.

- **DOC-2a** [active] [core] — **Scope and scan result are snapshots.**
  Whole Library contains only locally present tracks currently `PRESENT`;
  Current View contains all matches of the current source, search,
  filtering, and sorting, not just loaded rows; Selection contains
  exactly the selected present track IDs and cannot be started while
  empty. Only „Run scan now" freezes the IDs. Later view or selection
  changes do not alter the run. An invalid or emptied invocation context
  visibly falls back to Whole Library. „Tracks checked" counts only
  successfully read files, skipped files separately. The last fully
  completed result survives navigation and restart with scope,
  timestamp, options, and provenance; a new or cancelled run replaces it
  only after full completion. Cheap staleness checking flags changed
  rows on reopening, exact revalidation follows before writing. Newly
  added tracks are not retroactively taken into the snapshot.

- **DOC-2b** [active] [gtk] — **26a is a summary, never a write
  surface.** After the scan, Library Doctor separately shows
  „N safe · local, preselected" and „N suggestions · review" as well as
  problem classes for casing/whitespace, missing Album Artist, genre
  variants, missing/wrong Year, and missing Recording MBID; each class
  counts concrete track/field changes separately by safe/review. Ties
  additionally appear as „N unresolved groups", not as safe.
  „Review N changes" opens the full review table; „Review N safe fixes"
  opens the same table locally filtered. No control on this page writes
  tags. With the remote switch off, remote classes, rows, and counts
  disappear completely, while the local result remains in place.

- **DOC-2c** [active] [gtk] — **A running scan shows honest
  intermediate results.** 26a replaces the empty starting state during
  the job with „Results found so far" and updates checked/skipped
  tracks, safe, review, problem, and unresolved counters after every
  completed track. The intermediate state is read-only only: review
  actions stay hidden until full completion, it is neither persisted nor
  applicable. Cancel or an error discard it and show the last fully
  completed result from DOC-2a again.

- **DOC-3a** [active] [core] — **Review decides per field.** Every
  concrete track/field change has its own selection. „All safe" is a
  reset preset to exactly all currently allowed unambiguous local fixes
  and removes remote, manual, stale, and unresolved selection; „None"
  removes everything. A tie shows „N spellings, no clear winner — pick
  one" with only real candidates and their frequencies, with no
  default. Picking a candidate materializes the affected track/field
  diffs; individual rows remain deselectable. Changing the candidate
  recalculates them and preserves manual deselections, as long as the
  same row remains affected. The review order stays stable during the
  session: selected local safe, tie groups, remote at 85%+, remote
  50–84%, remote under 50%, stale/conflict; within that, scope order and
  the fixed field sequence Title, Artist, Album, Album Artist, Year,
  Genre, Recording MBID. Apply receives an immutable plan made of
  exactly the current selection.

- **DOC-3b** [active] [gtk] — **26b shows the same diff wide and
  narrow.** Wide, Checkbox · Track + Field · Current · Proposed · Source
  sit in a virtualized table; empty appears as „— empty —", a replaced
  Current value struck through. At the narrow breakpoint, the same row
  stacks Current → Proposed with no horizontal page scroll. Both
  presentations bind the same selection and preserve row focus and
  stable order when switching. Ellipsized values have a full-text
  tooltip and an accessible description. „Edit track tags…" opens the
  existing Tag Editor; its Save marks affected Doctor rows stale and
  deselects them. The footer treats tracks as the unit of action:
  „Apply N tracks"; next to it „X tag changes · M files · undo available
  after".

- **DOC-4a** [active] [core] — **Confidence never chooses for the
  user.** Unambiguous local fixes are preselected; remote suggestions,
  ties, stale rows, and conflicts never are. A validly, directly
  resolved MBID carries 100%, otherwise the native MusicBrainz or
  AcoustID score is retained. If multiple sources agree, both are shown
  and the lower score applies; scores are never averaged. Conflicting
  sources produce a manual candidate group. With multiple remote hits, a
  single suggestion may only arise if the top value leads the second by
  at least ten percentage points and neither duration nor entity
  conflict; otherwise the user must choose. Below 50%, a suggestion
  remains explicitly low-confidence and unreviewed. There is no fuzzy
  auto-merge.

- **DOC-4b** [active] [gtk] — **Confidence is redundantly visible.**
  Local appears with source „Local" in the app accent. Remote at 85%+
  appears normal with source and percentage value, 50–84% yellow, under
  50% red with a warning icon and an unreviewed checkbox. Color is never
  the only information: source, percentage value, warning icon, or
  accessible help text carry the same state. Candidate details name
  source, score, artist, title, album, year, and duration deviation,
  where available.

- **DOC-5a** [active] [core] — **Every Library Doctor write goes
  through review.** Neither scan, nor 26a, nor the plugin row has a
  direct write path. Only Apply in 26b may start the immutable review
  plan, and only its reviewed fields are written. Immediately before
  each file, track/path identity and every expected current value are
  read again from the file. A field changed in the meantime is skipped
  as a conflict, without blocking other still-valid selected fields of
  the same file. Vanished or moved tracks drop out of the run as
  unavailable/skipped, not as a write error. Library Doctor, Tag Editor,
  and Revert use the same Lofty write primitive and the same error
  classification.

- **DOC-5b** [active] [core] — **Apply and Revert are journaled file
  jobs.** Before every write, a persistent journal stores the immediate
  before value and the planned after value per field; only fields
  written successfully are applied. A crash reconstructs prepared
  entries by reading the file: current = after means applied, current =
  before not applied, any other value a conflict. Already successful
  writes remain in place on Cancel; Cancel takes effect cooperatively
  between files, lets the running container write end cleanly, and
  starts no further file. There is neither auto-rollback nor
  auto-retry. The last fully completed Doctor cleanup remains
  revertible across restarts and is only replaced after a newer one
  completes safely. Revert only writes a field if current still matches
  the journaled after value, itself runs cancellable between files, and
  reports partial successes, errors, and conflicts. A full revert
  consumes the cleanup; Tag Editor jobs never replace its visible
  pointer.

- **DOC-5c** [active] [gtk] — **Write jobs don't freeze the UI.** Apply
  and Revert run in the shared progress card with a visible Cancel and
  the same geometry as Scan/Sync. Button, progress, completion, and
  errors count tracks primarily: „Apply 128 tracks",
  „Updating tags… 42/128 tracks", „Tags updated · 128 tracks" or
  „42 tracks updated · 86 cancelled". Tag changes and files are only
  supplementary. A successful or partial Doctor write shows exactly one
  undismissable undo-class toast with „Revert"; collected errors appear
  once as „N updated, M failed · Details", never as a per-file toast.
  The remote toggle and the Apply selection are locked during the write
  job.

- **DOC-5d** [active] [gtk] — **Result and app stay honestly current
  after writes.** After Apply or Revert, track list, Browse Bar,
  cover/player metadata, sidebar, Albums, Artists, and Stats are renewed
  via a shared tag-mutation invalidation; a restart is never needed.
  Result rows stay visible as Applied, Remaining, Failed, Stale,
  Conflict, or Reverted. Cancelled/unstarted and Failed remain
  reconstructably selected for a new review run, stale/conflict
  unreviewed. 26a summarizes partial results as „N applied · M
  remaining". Reverted rows can be reviewed again; a new full scan
  replaces the scan result independent of the still-valid undo
  journal.

- **DOC-6a** [replaced by DOC-7b] [gtk] — **Library Doctor is a
  main-window navigation.** 26a lives as a root page in the existing
  `content_nav`, 26b is pushed onto it; Back returns to 26a with the
  in-session selection unchanged. There is no Doctor dialog and no
  additional Apply confirmation dialog. Entry points are the Plugins
  page with the privacy subtitle „contacts MusicBrainz / AcoustID", the
  library's ⋮ menu, and the STATS-DEDUP hint. With the module disabled,
  an entry leads to the highlighted plugin row, with it active to the
  Doctor page; Preferences closes before the main-window navigation. The
  scope is not a persistent plugin setting: default Whole Library,
  suggested as Current View from a filtered view, Selection from a
  selection context. The expanded plugin row shows scope, remote switch,
  the note „local fixes always included · no network", „Run scan now",
  and „Revert last cleanup", but no „Local fixes only" switch. Revert
  remains available even with the plugin disabled via a minimal Doctor
  job page and activates neither the plugin nor the network.

- **DOC-6b** [active] [gtk] — **A running job has exactly one place.**
  Scan, Apply, and Revert survive navigating away; the one sidebar
  progress card leads back to the matching Doctor page. Concurrent
  Doctor jobs are forbidden. Library scan and Doctor scan/apply/revert
  do not run in parallel, and all tag writes are globally serialized;
  playback, navigation, and read-only device sync remain usable. A
  repeated Doctor entry during a running job navigates to that job
  instead of starting a second one. Scope, remote toggle, and scan
  action are locked during the job and explain the running job; Cancel
  lives exclusively on its progress surface.

- **DOC-7a** [active] [gtk] — **Local checks are an available tool;
  network stays opt-in.** Library Doctor has no main switch and its
  local, purely read-only checks can be started manually at any time.
  This is not a network release. The separate switch „MusicBrainz/
  AcoustID suggestions", off by default, shows a short, versioned
  confirmation with the data allowlist from DOC-1c on first activation;
  Cancel leaves it off. The plugin row and the results view bind the
  same persistent switch. Switching off stops future remote requests,
  hides remote rows, and removes their selection; switching on again
  shows existing or newly loaded remote suggestions unreviewed. A
  missing fingerprint capability is visibly explained as „AcoustID
  unavailable", while Local and pure MusicBrainz resolution keep
  working.

- **DOC-7b** [active] [gtk] — **Library Doctor is a directly available
  main-window navigation.** 26a lives as a root page in the existing
  `content_nav`, 26b is pushed onto it; Back returns to 26a with the
  in-session selection unchanged. There is no Doctor dialog and no
  additional Apply confirmation dialog. Entry points are the Plugins
  page with the privacy subtitle „contacts MusicBrainz / AcoustID", the
  library's ⋮ menu, and the STATS-DEDUP hint; every entry leads directly
  to the Doctor page. The scope is not a persistent plugin setting:
  default Whole Library, suggested as Current View from a filtered view,
  Selection from a selection context. The expanded plugin row shows,
  without a main switch, scope, remote switch, the note „local fixes
  always included · no network", „Run scan now", and „Revert last
  cleanup". Revert remains available via a minimal Doctor job page and
  activates no network.

- **DOC-6c** [planned] [manual] — **The visible sign-off matches
  frames 26a, 26b, and 27.** On a real GNOME display, wide and narrow
  review geometry, row virtualization while scrolling, strikethrough and
  empty display, teal/yellow/red source states, the 41% warning, focus
  indicators in the normal and high-contrast theme, plugin expansion
  including the one-time network confirmation, and the shared
  scan/apply/revert progress card are checked. No text is truncated, no
  column forces horizontal page scrolling, and the interface remains
  operable during real file jobs.
## Z. Single-pane track browser

- **BROWSE-1** [active] [e2e] — **Music has exactly one track list.**
  Album and Artist are navigable library scopes derived from track
  metadata within the same virtualized track list, not tabs, modes, or
  persistent database entities. My Stats remains its own dashboard
  location.

- **BROWSE-2** [active] [core] — **Every browser location owns its
  state.** Source, scope, text search, facets, sorting, ID-plus-offset
  anchor, selection, and stable content focus are held together in the
  history entry. A fresh album/artist navigation starts unrefined;
  Back/Forward restores exactly. A scope that has become empty remains
  navigable within the session as an honest empty state.

- **BROWSE-3** [active] [gtk] — **Sidebar entries are absolute
  destinations.** Every activation also leaves utility pages and routes
  into the active target view; Music leads from a sub-scope back to the
  remembered library root. An already-active root target is a no-op.
  Running jobs remain globally visible and never block navigation.

- **BROWSE-4** [active] [gtk] — **Metadata navigates identically
  app-wide.** Track, Album, and Artist trigger exactly the central
  intents RevealTrack, OpenAlbum, and OpenArtist, regardless of whether
  they originate from the player bar, Now Playing panel, track list,
  queue, cover, or My Stats. The destination selects, focuses, and
  centers the anchor track; Back restores the point of origin.

- **BROWSE-5** [active] [core] — **Session restore is limited.** The
  current browser location, the remembered library root, and the
  structured playback origin are restored. History, open search
  surfaces, utilities, and raw widget focus do not survive a restart.
  Destinations that can no longer be resolved fall back to the library
  root.

- **BROWSE-6** [active] [core] — **Listening events are historical
  facts.** Every qualified play stores the title, album, artist, genre,
  duration, path, and MBID snapshot frozen at the moment playback
  started. Removing, auto-cleaning, or trashing a current library entry
  does not delete these events; My Stats therefore remains stable over
  time. A later tag edit does not change old events, while track
  rankings show the most recent metadata when several snapshots share
  the same track ID. Dialogs explicitly distinguish catalog consequences
  from preserved listening history.

- **BROWSE-7** [active] [core] — **Remove, trash, and list actions are
  different commands.** "Remove from library" leaves files untouched,
  removes current catalog, rating, playlist, and device-sync data, and
  atomically creates a persistent scan exception for the file identity;
  renaming the same file does not lift it. Preferences > Library shows
  the count, and "Restore All" deletes the exceptions and starts a
  rescan. "Move to Trash" moves only successfully confirmed files,
  removes only their current catalog data, and creates no exception — a
  file restored later may return. "Remove from playlist/queue" changes
  only that list. The long-lived listening history always follows
  BROWSE-6.

- **BROWSE-8** [active] [gtk] — **Catalog deletion does not interrupt
  the loaded track.** If the currently loaded track is removed, trashed,
  or hard-deleted by maintenance, its player-owned metadata snapshot and
  the already-open audio file keep running until the natural or
  explicit transport change. All future occurrences of deleted IDs
  disappear immediately from Queue and Up Next; Repeat One cannot
  restart a deleted track. After the change, the loaded queue tombstone
  is also removed. A track link to an ID that no longer exists stays at
  its point of origin and explains this via toast; album and artist
  links continue to open the snapshot scope, but without a phantom
  anchor. After a deletion series, surviving selected rows remain
  focused; otherwise selection and focus fall to the next row, to the
  previous row at the end of the list, and to the stable content
  container when the list is empty.

- **BROWSE-9** [active] [gtk] — **The date added is a normal library
  column.** "Added" is selectable in the column editor, movable,
  width-persistable, and sortable by `added_at`. The ISO-formatted time
  is hidden by default; existing layouts also receive the new column
  hidden when normalized, without losing their stored order or
  visibility.

- **BROWSE-10** [active] [core] — **Conflicting embedded album covers
  are canonicalized.** When cover download is enabled, the library scan
  detects different embedded images for the same normalized album
  artist and album name and fetches exactly one shared cache cover. This
  then wins for all tracks of the album identity; the music files remain
  unchanged. With the module disabled or the network unavailable, purely
  local resolution remains in effect.

## AA. External changes (live refresh from CLI/MCP)

<!-- Section letter: A–Z are already assigned on main (T duplicated);
     the next free mark beyond Z is AA. The letter placement was
     verified against the main state when inserting. This section
     anchors decision 6 of the multi-frontend-core plan (live visibility
     of externally generated changes) and serializes ahead of package F,
     which later extends it with the instrumental/filter rules
     (track 2). -->

A second process (CLI, MCP; further surfaces in the future) writes into
the same database over the same core path. The running app makes such
external changes visible — without a restart, as a **background event**
and thus per P-1/P-4/MOT-2: quiet, without stealing layout, without its
own announcement. The app continues to refresh its *own* write actions
itself (writer-token filter); this section governs exclusively the
external write.

- **EXT-1a** [active] [gtk] — Externally created content appears
  without a restart: a playlist created by another process over the
  same database — generally any external change to playlists, smart
  playlists, or catalog — becomes visible in the running app; the
  affected views (sidebar, current track list) update themselves. The
  visibility budget is generous and degrades deliberately (notifier
  wake-up, polling when file-watch cannot be armed); what is checked is
  the *what* (the playlist appears), not the *how-fast*.
- **EXT-1b** [planned] [manual] — The external refresh is silent: no
  toast, no badge, no indicator, no focus travel as an announcement. A
  background event never serves the announcement role (P-1); the
  update happens quietly in place.
- **EXT-2** [planned] [gtk] — Selection and scroll position survive the
  external refresh: an externally triggered reload resets neither
  selection nor scroll position (navigation-neutral reload per TAG-1).
  An untouched list pays nothing — no anchor, no jump.
- **EXT-3** [planned] [gtk] — No focus theft: a background refresh
  takes nothing away from the current input, grabs no focus, and pulls
  no view to the foreground. The user notices the update only through
  new content, never through jumping focus (P-3/P-4 in the
  live-refresh reading).
- **EXT-4** [planned] [core] — Running playback and queue remain
  untouched: external changes update views exclusively. The playback
  queue is a snapshot (`queue::snapshot`); an external write to the
  library changes neither the running playback nor the order of the
  already-queued tracks.
- **EXT-5** [planned] [gtk] — Authorized external live-queue commands
  update a visible queue quietly in place: no toast, no loss of focus,
  selection, or scroll position. Missing or unknown tracks are not
  queued.
  <!-- REVIEW: rule proposal -->

## AB. Instrumental versions (experimental)

<!-- Section letter: A–Z are assigned (T duplicated), AA is External
     changes; the next free mark is AB. The letter placement was
     verified against the main state when inserting (AA announces this
     section in its header comment). This section anchors the GTK UX of
     the instrumental versions of the multi-frontend-core plan (section
     2.4/3.2, decisions 11/13–19). All progress numbers come
     exclusively from the `ai_jobs` rows/events — the same numbers as
     CLI/MCP (plan 2.2). -->

An instrumental version is an **explicitly commissioned, permanent
title, clearly marked as AI-manipulated** (CONTEXT.md), not a transient
playback effect. The feature is **experimental** (decision 11): its
entire UI appears only behind the "Experimental features" toggle; rough
edges are deliberately accepted. The player plays only finished files.

- **INST-1** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — Triggered via track context menu: with
  the Experimental toggle active, the track context menu carries the
  entry "Create instrumental"; it acts on the **entire selection**
  (multi-selection → one batch with a shared `batch_id` for aggregate
  progress) and is inactive for a selection consisting purely of
  missing items (a missing file cannot be separated). Without the
  toggle, the entry does not appear (INST-11). (Plan 2.4/1)
- **INST-2** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — Conversion playlist = staging area with
  **exactly one aggregate progress bar** (done/total + percent, fed from
  the job rows/events, not from backend-internal numbers). **There is
  no further progress UI**: no sidebar/status-bar slot (the
  android-sync-V2 bottom slot is not touched), **no toast**.
  (Decision 18)
- **INST-3** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — One visible state per row: queued /
  processing (with row progress) / done — unsaved / saved / failed. The
  view is technically a special view over `ai_jobs` + staging store
  (playback via file path), not a playlist row source — even though it
  feels like a playlist. (Plan 2.4/7)
- **INST-4** [replaced by INST-4a and INST-4b] — The original rule
  bundled the view-side marking and the actual playback; it is split
  into the view-side marking (INST-4a) and the real staging playback
  (INST-4b, P3b).
- **INST-4a** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — In the conversion view, a finished
  render present in staging is marked as **playable** (Play active),
  while a still-processing entry is not (it shows progress). The
  staging render is a real file prior to any save decision. (Decision
  15, plan 2.4/7)
- **INST-4b** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — Activating a playable entry **actually
  plays** the staging render (or the promoted title) — playback via
  file path. Until the player can do this, the action is a marked
  placeholder (P3b).
- **INST-5** [replaced by INST-5a and INST-5b] — The original rule
  bundled the click decision and the ongoing wait interaction; it is
  split into the view-model decision (INST-5a) and the app interaction
  (INST-5b, P3b).
- **INST-5a** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — Wait rule (decision): a click on a
  **still-processing** entry triggers "wait with progress" — **never
  Play** (no original fallback), **never auto-skip**. The pure
  view-model decision is thereby enforceable, independent of playback.
- **INST-5b** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — In the running app, clicking a
  processing entry blocks the start with visible render progress and
  begins after completion (no fallback/skip). Progressive early start
  is a later optimization, not v1 (P3b).
- **INST-6** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — Save decision per row (Save / Discard)
  plus "Save all" in the header row. Saving **promotes** via the core
  facade (move into the dedicated folder, final tags incl. AI
  provenance, registration — atomic, no re-render); afterward **the row
  switches to the promoted library title and stays** there until the
  user cleans up. Discarding deletes the staging render; undecided items
  never appear in the library. (Decision 15/16)
- **INST-7** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — "Clear playlist" **warns** when undecided
  (done-unsaved) entries exist — hours of compute time do not evaporate
  unconfirmed. (Decision 15)
- **INST-8** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — Undecided renders **persist across
  restarts**; their **disk cost is visible in the view** (size per row /
  total). There is **no silent reaper** — only the explicit discard
  action (or saving) removes a render. (Decision 15)
- **INST-9** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — Dragging an **already converted** track
  into the conversion playlist produces a **notice referencing the
  existing one**, not a duplicate job (dedup skip in the core facade).
  (Decision 16)
- **INST-10** [active] [gtk] — Promoted versions carry a visible **AI
  badge** ("Instrumental · AI-manipulated") with a **source reference**,
  where linked. Provenance is DB-primary (`track_provenance`), the tag
  reference secondary; the badge keys off the DB flag, never off the
  storage folder. (Decision 13/14)
- **INST-11** [active] [gtk] — **Master gate:** the entire instrumental
  UI — context menu entry, conversion view, AI badges, "Hide AI music"
  filter (FIL-7) — is **hidden as long as the "Experimental features"
  toggle is off**. The toggle is a persisted setting; its state alone
  decides visibility. (Decision 11)
- **INST-12** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — Model provisioning: behind the toggle
  lies the first-use download flow for the ML runtime weights via the
  core facade `ensure_weights` (background thread with progress,
  SHA-256 checksum, license note next to the file, clear error paths
  incl. offline — pattern from the cover-download module). Weights are
  **not** bundled into the default build/Flatpak. In a build **without**
  the `stem-backend` feature, the view shows an honest, disabled
  placeholder with a hint instead of a non-functional button.
  (Decision 11)
- **INST-13** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — Reachability: the conversion/staging
  view is reachable via its own **sidebar entry**
  (`ViewSource::Conversions`, title "Instrumental conversions"). The
  entry appears **only as long as the "Experimental features" toggle is
  on** (INST-11) — the same gating that also creates the content page,
  so the entry never selects a missing page. (Plan 2.4/7, package F)
- **INST-14** [replaced by nothing — the instrumental surface was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — The sidebar entry "Instrumental
  conversions" is a drop target for tracks from the library. A
  multi-selection is queued as one batch; missing or removed tracks are
  skipped, existing work is referenced per INST-9 instead of duplicated.
  If the verified model/runtime assets are missing, the action opens the
  Experimental settings and does not create a job that is guaranteed to
  fail.
  <!-- REVIEW: rule proposal -->
## AD. Compact mode / mini player

<!-- Section letter: Z (single-pane track browser) is the last
     single-letter section; A–Z are assigned (T doubly occupied —
     legacy), AA (External changes) and AB (Instrumental) are assigned;
     AC is the rule prefix of the "Local sound profile" (section X), so
     compact mode continues with AD. The rules describe an already
     implemented and tested feature: they start directly at [active]
     with existing mini_* tests as evidence. Reference frames from the
     redesign mockup: 1e (idle), 9b (hover), 9c (context menu). -->

- **MINI-1** [active] [gtk] — **The mini player is the window.** Ctrl+M
  toggles between full and compact view (also via the ⋮ entry "Compact
  Mode") — both directions, the same playback session, nothing is
  rebuilt; the full state remains untouched (BROWSE-2) and Ctrl+M back
  lands exactly there. It is the same, undecorated window; the card IS
  the surface: 430×76, radius 16, tint rgba(34,34,34,0.92), 1 px
  hairline, opaque — no live blur (STYLE-1); the window itself is
  transparent (CSS), so only the card floats — the window size IS the
  card size (430×76), the card fills the transparent window edge to
  edge (any positive margin would let the opaque Adwaita surface shine
  through as a "backing plate"). Layout per frame 1e: cover 52/radius 10
  with inset hairline; title 13 px bold and artist 11.5 px on one
  ellipsizing baseline row (title prioritized, artist contrast ≥ 4.5:1
  on the tint); below it the mini waveform (46 equal-width bars, played
  portion in the playback accent, remainder white ~18%, click = seek,
  drag = scrub); play/pause 38 px in the accent. No volume, prev, or
  next button visible — deliberate reduction. The compact geometry is
  isolated from the full-window size.

- **MINI-2** [active] [gtk] — **Chromeless card — no hover buttons.**
  The card visibly carries only the play/pause button; there is no
  fade-in ⤢/✕ chrome. Restore and Quit are deliberately reachable only
  via the right-click menu (MINI-3), the keyboard (Ctrl+M back to the
  full window, Ctrl+Q quits), and a double-click on cover/title (=
  Restore). This way the card floats undisturbed and a Play click can
  never accidentally hit a ✕ (Quit). The entire card is a drag surface
  (GtkWindowHandle), except for the play button and waveform.

- **MINI-3** [active] [gtk] — **Right-click menu with fixed order.**
  Right-click, menu key, or Shift+F10 opens: Restore Full Window
  (Ctrl+M) · separator · Pause/Play (Space; label follows the state) ·
  Next (Ctrl+→) · Previous (Ctrl+←) · separator · Always on Top
  (toggle) · separator · Preferences (Ctrl+,) · Quit (Ctrl+Q). "Always
  on Top" is X11-only (GTK4 has no keep-above); where it is not
  supported — Wayland — the entry disappears entirely instead of
  standing there dead as a disabled row.

- **MINI-4** [active] [gtk] — **Keyboard identical to the full
  window.** Space = Play/Pause, Ctrl+←/→ = Previous/Next, Ctrl+M =
  Restore, Ctrl+Q = Quit — no mini-specific bindings. Ctrl+←/→ act as
  real keys on the card (capture phase, so that the waveform's arrow
  seek does not swallow the modified arrows) and match the accelerators
  shown in the context menu.
- **MINI-5** [active] [gtk] — If the library window becomes so small
  that the full view becomes uncomfortable, Reprise offers "Use Compact
  Mode" non-blockingly, at most once per session. Only the explicit
  activation of this offer switches to the compact view via the same
  path as Ctrl+M; Reprise never switches on its own. If no player is
  available or the compact view is already active, the offer does not
  appear.

## AE. Concerts

<!-- Section letter: AD (Compact mode) is the last assigned section;
     Concerts continues with AE. The rules start as drafts and are each
     activated together with behavior and a rule-named test. -->

- **CONC-1** [active] [gtk] — Concerts is a sidebar location in SMART
  and visible only when the module is active. Its badge corresponds
  exactly to the upcoming concerts visible on open after persistent
  filters; 0 renders no badge.
- **CONC-2** [active] [gtk] — The filter row is a permanent header.
  Idle, it quietly shows the total count and "+ Add filter"; every
  active restriction is a chip with its own ×-target of at least 20 px.
  Active, it shows "X of Y concerts" and "Clear all". Without a
  location, Radius is disabled and carries the tooltip "Set a location
  in Preferences".
- **CONC-3** [active] [gtk] — Double-click/Enter on a row and the
  ticket cell open the same external target: offer URL, otherwise the
  event page. Without either, the cell is empty and activation is a
  no-op with a tooltip. There is no play path.
- **CONC-4** [replaced by CONC-4a] — Original state contract without
  explicit live re-evaluation after changes to Concerts settings.
- **CONC-4a** [replaced by CONC-4b] — Original state contract with
  credential input hint and Preferences deep link.
- **CONC-4b** [active] [gtk] — Without a credential, Concerts neutrally
  shows "No concert data yet" with no action; the Concerts section in
  the Updates popover is not visible. There is no credential input hint
  and no Preferences deep link. Changes to credentials, location,
  default radius, time range, and similar settings immediately
  re-evaluate the already-open view, its sidebar count, and the Updates
  popover. Never fetched offers exactly "Fetch now"; zero hits with
  filters offers exactly "Show all". Offline or error leaves the cache
  and "Updated X ago" visible and reports the error exclusively inline
  in the footer — the same `cached`/`interrupted` reading that `NET-3` has
  since named app-wide; credential and filter behaviour stay Concerts' own
  and remain `[active]` unchanged.
- **CONC-5** [replaced by CONC-5a] — Original worker contract with
  view-open staleness, due check, and "Fetch now" as the only network
  triggers.
- **CONC-5a** [active] [core] — Network runs exclusively in the worker
  or `one_shot_task`. Triggers are view-open staleness (24 h plus
  jitter), the hourly due check, "Fetch now", and an explicitly
  confirmed credential check. All Concerts requests share the 1-req/s
  limiter. Track changes, navigation, and individual credential
  keystrokes only read or write locally; fetch results are applied per
  MOT-2 without a fade-in animation.
- **CONC-6** [active] [gtk] — Similar rows carry a dimmed "similar to
  {seed}" and disappear with "Library artists only". The source pill is
  visible as soon as Similar is enabled or similar rows exist.
- **CONC-7** [active] [gtk] — The Updates popover shows the Concerts
  section only when the module is active, at most three unseen entries
  of the persistent filter scope, and "Show all concerts (N) →".
  Opening stamps the entire delta set of both sections. The header
  badge sums unseen entries across all active, fetch-ready feeds per
  the `badge_presentation` idiom.
- **CONC-8** [active] [core] [gtk] — Apply or Enter on a credential row
  checks the stored value exactly once, off-thread, via the shared
  Concerts limiter. Valid, rejected, and unverifiable appear inline;
  empty resets the state without a request. The check never writes
  credential values into logs or error messages.
- **CONC-9** [active] [core] [gtk] — Ticketmaster credentials are
  neither visible nor editable in the UI. The core prefers a stored
  legacy value over the runtime environment and the embedded build
  value; empty values do not count. Bandsintown remains available as an
  optional credential row independently of this.
- **CONC-10** [active] [gtk] — Every Concerts row shares a common
  vertical center. The artist stands as a single-line group on the same
  baseline as date, location, venue, distance, and ticket; an optional
  "similar to …" caption expands and centers the artist group as a
  unit, instead of pinning the artist to the top edge of the row.
## AF. Podcasts & Radio

<!-- Section letter: AE is the last assigned section after Concerts
     landed; this branch therefore claims AF. The rules start
     planned and are each activated in the implementation commit together
     with their rule-named test. REVIEW: rule proposal -->

Podcasts and radio are independent library sources, but share a UX
grammar for location, filtering, adding, and reversible removal.
External media remains structurally outside the track queue and the
listening statistics.

- **SRC-1** [active] [gtk] — Podcasts and radio sit in the LIBRARY
  section between Music and Queue and appear only when the module is
  active. The podcast counter shows unplayed episodes, the radio
  counter shows favorites; zero stays invisible. Radio is active by
  default because it only transmits on user action; the binding
  condition is a radio empty state with exactly one directly reachable
  "Add station" action.
- **SRC-2** [active] [gtk] — Adding uses a tinted rectangular button
  with plus, label, and radius 8 in both sources, never the chip shape.
  The shared toolbar grammar reads Add button · "Add filter" · active,
  deletable filter pills · count on the right; filter rows keep their
  height across state changes.
- **SRC-3** [replaced by SRC-3a] [gtk] — Each source has exactly one add dialog
  with exactly one input field for search terms or a URL. Search
  returns grouped results with row actions; a recognized URL leads
  through preview and options to a confirmation. Network and
  subprocess work starts only on submit and never runs on the GTK main
  loop.
- **SRC-4** [active] [gtk] — Removal takes effect immediately, stays
  tombstoned for ten seconds, and is reversible via a high-priority
  undo toast. Context menu and hover star offer the same destructive
  action; "Play Next" and "Add to Queue" are entirely absent. Podcast
  downloads are never silently deleted on unsubscribe: the commit
  toast reports the files that were kept and offers only moving them
  to trash; multiple unsubscribes are aggregated.
- **SRC-5** [active] [gtk] — RSS podcasts and YouTube are separate library
  places. Both start with source rows grouped by channel or show which expand
  to their episodes; radio stays a station list. The add dialogs show real
  source images, group YouTube hits by channel, and hide podcasts, channels and
  stations that are already subscribed.
- **SRC-3a** [active] [gtk] — Every source has exactly one add dialog with
  exactly one input field for search terms or a URL. Search yields results with
  row actions; a recognized URL **of the dialog's own source** leads through
  preview and options to a confirmation. Network and subprocess work starts
  only on submit and never runs on the GTK main loop. New compared to SRC-3:
  the URL path is bound to the dialog's own source (see SRC-6).
- **SRC-6** [active] [gtk] — Podcasts, YouTube and Radio each have their own
  add dialog with its own identity (title and placeholder). A dialog queries
  **exclusively** the provider of its own library place; there is neither a
  mixed result nor a shared search. A URL belonging to another source is
  rejected with a one-line reason — it is neither evaluated nor silently handed
  to another dialog, and the primary button stays inactive meanwhile. The note
  appears while typing, not only on submit.
- **SRC-7** [active] [gtk] — All result rows in all three add dialogs carry the
  same compact action: a plus icon plus the short label "Add". After adding,
  that exact same surface becomes an inactive "Added" with a check icon — the
  row does **not** disappear immediately, so that the success stays visible;
  only the next submitted search hides the source (SRC-5). Because the visible
  label cannot name the source, the accessible name and the tooltip always
  carry the full sentence ("Subscribe to {source}", "Add {source}", "{source}
  is already in your library"). Offered and added never differ by colour alone,
  or by two nearly identical theme glyphs. Each dialog explains once in its
  footer that subscribed sources drop out of later searches.
- **SRC-8** [active] [gtk] — In all three add dialogs, only the result list
  grows. It sits in a scroller of its own that scrolls **vertically only**;
  there is never a horizontal scrollbar — titles and subtitles ellipsize
  instead. The input field, status line, footnote and the fixed footer bar with
  Cancel and the primary action stay visible and reachable regardless of the
  number of hits. Rows keep their distance from the overlay scrollbar so that
  no row action ends up beneath it, and the last row scrolls clear of the
  footer bar. Artwork and row action keep their size throughout.
- **SRC-9** [active] [core] [gtk] — Channel search results show the subscriber
  count as a compact addition ("62.4k subscribers", "1.2M subscribers") as soon
  as the channel publishes it. It is an optional addition and never replaces
  the existing hit count. Missing, hidden or malformed values are **omitted** —
  never rendered as zero and never as "unknown". The number comes from the
  search subprocess that already runs; there is no additional query per
  channel.
- **SRC-10** [active] [gtk] — The genuine "nothing added yet" empty state
  carries the same geometry for Podcasts, YouTube and Radio: the glyph of its
  own sidebar entry in a muted rounded tile, a title, a paragraph with one
  sentence each on *what* lands here and *where it comes from*, exactly one
  primary button with a plus icon, and beneath it, as a quiet second line, the
  URL path — where the source has one of its own; radio has none, because the
  paragraph already names the stream URL. Neither toolbar nor filter row nor
  counter appears in this state, and never "0 of 0": the surface looks unused,
  not broken. Never a generic placeholder graphic, never a spinner with nothing
  to do. As soon as the first subscription lands, this state disappears
  entirely; "Nothing matches these filters" and the remaining empty-state
  classifications (`NoEpisodes`/`NoResults`) keep their own, unchanged surface.
- **SRC-11** [active] [core] [gtk] — Channel, show and station images (YouTube
  `thumbnails`, iTunes `artworkUrl600`, radio-browser `favicon` — `C1`) run
  through a module of their own (`module.source_images.enabled`) and are
  subject to `NET-1a`: a cache hit is always shown, regardless of the gate — a
  cache miss triggers a fetch only when the global gate **and** the module are
  both active, otherwise the source glyph stays, never an error image. The pure
  fetch and cache policy lives, testable without a display, in
  `reprise_core::remote_image` (no gtk4/libadwaita/gstreamer/zbus); decoding
  and display stay in the GNOME crate. The on-disk cache is limited to
  `MAX_CACHE_ENTRIES` (300) entries and, when exceeded, deterministically
  clears the files untouched for longest first — unlike the unbounded,
  permanent cover-art cache. Every caller (podcast library view, YouTube
  channel detail, all three add dialogs) computes the gate itself at its own
  connection rather than relying on an upstream checkpoint — the lesson from
  `T6-G1-gap`: a privacy promise in UI copy needs a test per call path, not per
  feature.
- **POD-1** [active] [core] — Episode status is a pure derivation:
  Played exactly when `played_at` is set, otherwise Resume when
  `position_ms > 0`, otherwise New. An episode ending sets Played and
  clears the position.
- **POD-2** [active] [core] — RSS is the data API:
  enclosure/guid/pubDate/itunes:duration; the GUID — or, failing that,
  the enclosure URL, and for YouTube the video ID — is the sole
  episode identity for dedupe, resume, played, and download.
  Conditional refresh runs on a worker with interval and deterministic
  jitter; upserts preserve seen and position state. Automatic refresh
  requires an active module, at least one subscription, a due TTL, and
  an unmetered connection.
- **POD-3** [active] [core] — YouTube sits exclusively behind the
  yt-dlp provider boundary: flat playlist for listing, audio
  resolution only at playback time and never persisted. Errors are
  classified legibly and never crash. If the binary is missing, the
  setting stays unchanged and the degradation is made visible on the
  YouTube toggle, which is active by default.
- **POD-4** [active] [gtk] — Episodes start at the saved position;
  this is persisted throttled as well as on pause, stop, switch, and
  quit. After the end, the app offers the next unplayed episode of the
  same show by date via toast and a persistent player-bar button, but
  never plays it automatically. Podcast sessions produce neither
  scrobbles nor `listen_events` nor play counts.
- **POD-5** [active] [gtk] — Downloads are opt-in per subscription,
  live in the app's XDG data path under a GUID-stable path, follow the
  chosen cleanup policy, and are preferentially played back locally
  offline.
- **POD-6** [active] [core] [gtk] — Individual RSS and YouTube
  episodes can be removed from the context menu, disappear
  immediately, and stay reversible via undo for ten seconds. The
  commit deletes only the database entry and permanently blocks its
  source-stable GUID against renewed feed import; a downloaded file
  remains in place and can only be removed via the offered trash
  action.
- **POD-7** [active] [core] [gtk] — An episode's download state is visible in
  the row context: not downloaded, running with bytes and a progress bar, local
  with file size, failed, or locally vanished. Progress stays transient;
  completed paths and sizes are persisted together and deleted together.
- **POD-8** [replaced by POD-12] — Former rule: only downloaded episodes from
  RSS subscriptions explicitly selected per stable device identity are eligible
  for Android sync; YouTube sources are never synchronized to a device
  regardless of their download state. Both halves became wrong with the three
  named sync targets (`MTP-38`, `MTP-23`) — YouTube audio gets a target of its
  own and now synchronizes on equal terms.
- **POD-9** [active] [core] [gtk] — Within each show grouped by stable
  subscription ID, and within each channel, episodes are ordered by date
  descending with the status semantics from POD-1; the group row shows the
  total and unplayed counts, the newest episode, and the local data volume.
- **POD-10** [active] [core] [gtk] — The YouTube channel page starts with at
  most the ten newest long-form entries from the official keyless UULF feed and
  keeps Shorts hidden by default. "Load more" extends that same channel once,
  past the yt-dlp provider boundary, up to entry 40. Selection and bulk
  download or removal stay bound to the channel; every row shows the download
  state from POD-7.
- **POD-11** [active] [core] [gtk] — On the YouTube channel page every row
  carries a download column of its own with the state from POD-7 and, as soon
  as a file actually exists, its compactly formatted size (e.g. "148 MB",
  "1.2 GB"); a size is never invented for episodes that are not downloaded. A
  header line summarizes the channel currently loaded — the window size of the
  listed set, the number of downloaded episodes and their total size on disk
  (e.g. "10 of 487 · 3 downloaded · 1.2 GB") — and stays correct after "Load
  more", when showing or hiding Shorts, and after a download completes or is
  deleted. **Addendum (block H, MCP parity):** the window, the Shorts filter
  and this header total are pure projections in
  `reprise_core::podcasts::channel_window` (`visible_window`,
  `available_count`, `channel_download_summary`); GTK (`YoutubeChannelState`)
  and the MCP tool `music_get_channel_detail` call the same function instead of
  computing separately.
- **POD-12** [active] [core] [gtk] — Downloaded episodes from RSS **and**
  YouTube subscriptions explicitly selected per stable device identity are
  eligible for Android sync on equal terms — the selection is decided per
  subscription and device, not by source kind. The selection control appears
  only when at least one MTP device is currently connected: a single device is
  addressed directly, and with several devices the targets can be chosen
  independently. RSS episodes land under their PodcastEpisodes target (default
  `Podcasts/Reprise/<Show>/`), YouTube audio under its own YoutubeAudio target
  (default `Music/Reprise-YouTube/<Channel>/`, `MTP-38`); both are copied 1:1,
  never re-encoded (`MTP-24`). Music playlists stay unchanged under their
  playlists target (default `Music/Reprise`). A change of subscription kind at
  the same feed URL (e.g. a re-import as a different channel type) clears the
  previous device selection — that is not a source-kind special case but
  applies symmetrically to any change of kind.
- **RAD-1** [active] [gtk] — Only the currently connected station is
  accented in the table; its state icon, name, now-playing, and row
  tint change together. All others, as well as a presented but
  disconnected paused station, show "—". Only the player bar may keep
  the last ICY title dimmed as session memory.
- **RAD-2** [active] [gtk] — Live playback has neither seek nor
  duration: the player bar and mini-player show elapsed time and a
  geometry-matched waveform placeholder, MPRIS reports `CanSeek=false`
  and no length. Pause disconnects the stream but stays presented as
  Paused/CanPause with station and dimmed last title; play reconnects
  live. A reconnect error leaves the paused state standing with an
  inline error and retry. Radio produces no listening statistics;
  reactivating the running row stops it.
- **RAD-3** [active] [core] — Radio-browser servers are chosen via
  the discovery endpoint and rotated on failure. Every start of a
  UUID station reports the click tag; a dead stream is re-resolved
  exactly once via its UUID before the error is shown.
- **RAD-4** [active] [core] — Pasted radio URLs are resolved at most
  one level through PLS or M3U down to the stream URL; HLS manifests
  remain the stream URL themselves. The preview reads name, bitrate,
  genre, and content type exclusively from ICY/HTTP headers and never
  streams the body.

## AG. Runtime service (headless control)

<!-- Section letter: AF was the last assigned mark, AG is the next
     free one. This section anchors the runtime design from section 9
     of the multi-frontend-core plan (thin-core plan, stage 1 task
     1.1). All rules start [planned] and switch individually to
     [active] once the respective slice from stage 3 is implemented. -->

Playback, queue, background jobs, and device runs will belong to a
runtime, not the GTK process. Surfaces and agents are clients of the
same state. This section governs exclusively what a user sees of
this; ownership, lease, and error categories are in the architecture
plan.

- **RUN-1** [active] [core] — A single owner: playback, queue, jobs,
  and device runs belong to exactly one runtime at any point in time.
  A second surface never starts a competing runtime; it connects or
  fails, named. Two simultaneously visible, diverging playback states
  are a bug, not a display problem.
- **RUN-2** [planned] [gtk] — No guessed state: as long as there is
  no connection to the runtime, the surface visibly shows transport,
  queue actions, and device actions as unavailable instead of a dummy
  built from the last known state. A control that cannot trigger
  anything also does not look like it could (FB vocabulary, G).
- **RUN-3** [planned] [gtk] — Reconnection is a background event: a
  brief disconnection produces no toast per attempt, no dialog, and no
  focus movement. Only a persistently unsuccessful state is named
  once, in place. After reconnecting, a complete snapshot replaces the
  runtime-bound state without sacrificing selection and scroll
  position (like EXT-2).
- **RUN-4** [active] [core] — Idle shutdown never interrupts work:
  the runtime only terminates when no client is connected, nothing is
  playing or loaded paused, no device run and no job is active. A
  service that aborts running work for the sake of resources is a
  data-loss feature.
- **RUN-5** [planned] [gtk] — External control is silent but honest:
  if an agent or a second surface changes playback or queue, the
  visible surface follows immediately and quietly — no toast, no
  focus theft, no view is pulled to the foreground (P-1/P-4 in the
  live-refresh reading, like EXT-5). The user notices it by the
  changed state, never by an announcement.
  <!-- REVIEW: rule proposal — open and deliberately not decided
       along with the rest is whether closing the window ends
       playback. The runtime lifecycle allows both: idle shutdown
       (RUN-4) keeps the service alive as long as something is
       playing, so music would keep running after closing until it
       ends. That is a product decision, not an architectural
       consequence, and needs its own rule before stage 3.3 migrates
       the "Playback/Queue" slice. -->

---

If a case comes up during testing that no rule covers: add a rule
(process rules above), do not decide locally.
