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
(cua-e2e harness against the real app), `[web]` (showroom suite,
`showroom/tests/`), `[manual]` (RELEASING.md checklist, which references the
same rule IDs). Testing happens at the **lowest level
that can disprove the rule**. Timing numbers (100 ms, 150 ms, …) are design
intent, not assertions: the *what* (feedback exists) is automated, the
*how fast* is checked manually. If a `[manual]` rule later becomes
automatable, only its tag changes, never its ID.

**Traceability.** A test carries **exactly one primary rule ID in its
name** (Rust: `fn play_1a_…`, cua-e2e scenario: `play-1a-…`, showroom:
`test('show-1 …')`). If a scenario
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

**Language.** This rulebook is English. Design docs and specs under
`docs/superpowers/` remain German as the project's working language.
Tests and scripts are code and therefore English (AGENTS.md); rule IDs
and status tokens are quoted verbatim there.

**Changes.** If you encounter a case while implementing or testing that no
rule covers: **add a rule, don't decide locally.** Agents do this by
adding a `[planned]` draft with the next free ID in the affected section,
marked with `<!-- REVIEW: rule proposal -->` — the decision rests with the
human. Rationale for changes lives in the git history.

**Every feature reaches every frontend.** A feature is not finished when
its window works. If a caller without a window could sensibly use it, it
is also exposed over MCP in the same change — not later, not "if someone
asks". The window is one frontend among several; `reprise-core` holds the
feature and each frontend only presents it, so a feature that only the
GTK app can reach is a sign that logic leaked into the window. Sensibly
means: it reads or changes something a caller can name and act on
(library data, playback, playlists, queue, derived analysis). It does not
mean pure presentation — an animation, a hover state or a panel's layout
has nothing to expose. When a feature deliberately stops at the window,
its plan says so and why; silence is not an exemption. The MCP surface
degrades honestly: a feature whose data has not been computed yet reports
what is missing instead of returning an empty answer that reads like a
result.

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
  (session restore retains the visible destination, but not its history —
  see START-3 and BROWSE-12);
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
  position. START-3 restores, across restarts, the last visible browser
  location including scroll position; no history stack is reconstructed.
- **NAV-6** [replaced by NAV-6a] [e2e] — Search (Ctrl+F) filters the current
  view live; Esc clears and closes. Search never navigates on its own.
- **NAV-6a** [active] [gtk] — Search (Ctrl+F) filters the current view live;
  Enter accepts the query, while Esc clears the query and filtering and closes
  the popover in the same key press (SEARCH-4a). Search never navigates on its
  own: an Esc that aborts an open search is consumed there and does not also
  travel back through the navigation history. With the popover closed, Esc
  consumes the key only when it removes a committed search chip; otherwise it
  remains available to local overlays and navigation.
- **NAV-7** [replaced by NAV-15] — Hamburger menu: "Scan Library" → starts the
  scan, stays in the view (card appears). "Preferences" → Preferences
  window. "Keyboard Shortcuts" → shortcuts overlay. "About Reprise" →
  About dialog. No menu item silently switches the content view.
- **NAV-7b** [replaced by NAV-15] — The seek bar's colour arrives the way its
  shape already does: by itself. The analysis starts with the app and is
  resumable, so a library that is already done ends it at once and shows
  nothing — a run appears on the scan card (P-1) only when it has real
  work. The track being played is caught up on its own, and its bar
  crossfades from the plain accent into its colours when the curve
  lands; nothing waits for that. One hamburger item next to the scan
  carries the same two-label shape as it does — "Analyze Library" /
  "Stop Analysis", labelling independently of the scan — and exists to
  stop a run under way or start one again, not to grant permission. A
  first full pass is roughly three quarters of an hour of CPU, so
  stopping it must always be one click away.
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
- **NAV-13** [replaced by NAV-10b] — Starting playback is not
  navigation: Enter or double-click on a track row leaves selection,
  keyboard focus, and viewport unchanged; only the now-playing marker
  changes. The separation of marking and scrolling in the one track
  browser is now governed by NAV-10b.
- **NAV-14** [active] [gtk] — **A section header carries its own create
  action.** The PLAYLISTS header carries a `+` button that creates a playlist
  immediately: a new row appears in place, named "Untitled playlist" with the
  name selected for inline rename, and no dialog opens. Enter or moving focus
  away commits the typed name; an empty name keeps "Untitled playlist";
  Escape discards the row and the playlist with it. "Import playlist…" lives
  in the global ⋮ menu with the other library-wide verbs; neither action
  occupies a sidebar row. *Tests:*
  `nav_14_the_playlists_header_creates_a_playlist_in_place_without_a_dialog`,
  `nav_14_escape_discards_the_new_playlist_row_and_the_playlist`,
  `nav_14_an_empty_name_keeps_the_untitled_playlist`,
  `nav_14_import_playlist_lives_in_the_overflow_menu`.
- **NAV-15** [active] [gtk] — **The primary menu offers decisions, not
  housekeeping.** Its sections contain Compact Mode; Library Doctor and
  Import playlist…; then Preferences, Keyboard Shortcuts, Help, and About
  Reprise. It exposes no scan, scan-cancel, analysis, or analysis-cancel
  action. Rendering-data backfill
  starts after the window's first idle frame and after every completed scan;
  starting it again while it runs is a no-op. When its progress card is
  visible and no scan owns that card, the card's cancel action can stop it.
  *Tests:* `nav_15_library_section_omits_manual_analysis`,
  `nav_15_library_section_omits_header_rescan`,
  `nav_15_a_second_start_never_opens_a_second_run`,
  `nav_15_a_started_run_can_still_be_cancelled_from_its_progress_card`,
  `scan_completion_notifies_cover_and_rendering_follow_ups`.
- **NAV-15b** [active] [manual] — **A manual rescan keeps its own doors.**
  With the header item gone, Preferences → Library and the track list's
  unavailable or empty retry state are the two ways to start a scan by
  hand. Both start a real scan and both raise the scan card, which stays
  visible for its minimum perceivable time even when the scan finishes at
  once. Checked by hand because no automated level drives either entry
  point end to end: the cua-e2e scenario that once proved this path drove
  the header item and retired with NAV-7.
- **NAV-16** [active] [gtk] — **Optional sidebar places carry their own off
  switch and way back.** A secondary click, Menu, or Shift+F10 on Podcasts,
  YouTube, Radio, Releases, or Concerts opens an arrowed menu anchored to the
  highlighted row's trailing edge. The menu offers "Turn Off {name}" and
  "{name} settings…"; Settings opens Plugins with that module highlighted.
  Turn Off changes the same module setting as the Plugins switch, refreshes
  immediately, and keeps subscriptions, favorites, caches, and source data.
  Success removes the row and offers a five-second Undo toast. If the place
  was open, Music becomes selected and the toast names that fallback; Undo
  restores the row and returns there only when this turn-off caused the
  fallback. Otherwise the current place does not change. While any optional
  place is off, a dimmed "{n} turned off" action at the end of Library opens
  Plugins with every disabled module highlighted; it is never a session
  source. Music, Queue, playlists, smart lists, and My Stats never offer the
  menu. *Tests:* `nav_16_only_optional_module_rows_offer_turn_off`,
  `nav_16_turn_off_dispatches_the_clicked_module_once`,
  `nav_16_module_settings_dispatches_the_clicked_module`,
  `nav_16_turned_off_row_tracks_every_disabled_optional_module`,
  `nav_16_secondary_click_turns_off_the_row_and_falls_back_to_music`,
  `nav_16_turn_off_posts_undo_and_restores_the_active_module`,
  `nav_16_turned_off_row_is_not_a_restorable_session_source`.
- **NAV-17** [active] [gtk] — **A Shift selection starts at an anchor, not at
  the beginning of the list.** The user sets the anchor with the last click
  without Shift. If no such anchor exists after a fresh load, sort change, or
  filter change, the playing track's row takes its place when that row is
  present in the current track source. If neither exists, Shift+click selects
  exactly the clicked row instead of stretching from the start of the list. A
  range never moves the anchor; the next input starts from it again. The
  playing track remains passive: it receives neither selection nor keyboard
  focus, and playback still moves nothing, preserving NAV-10b.
- **NAV-18** [active] [gtk] — **The sidebar marks the visible view, and the
  marked entry stays clickable.** Exactly the sidebar entry whose view is
  visible in the content area carries the marking — including Library Doctor
  and the opened device card, neither of which is a `ViewSource`. At most one
  entry is marked at any time across both navigation lists and the device
  cards. When the visible view has no sidebar entry, nothing is marked. A
  sidebar rebuild never changes the marking. While a placeless view is
  visible, activating **any** source entry routes into it — including the
  source that was last visible (BROWSE-3); activating the entry of the already
  visible placeless view does nothing.

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
- **PLAY-5** [replaced by PLAY-5a/PLAY-5b/PLAY-5c] — Original queue-hygiene
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
- **PLAY-5c** [replaced by QUE-12] [core] — Unsubscribed episode hygiene: an episode
  whose show is no longer subscribed leaves the manual queue silently,
  including during session restoration and before queue advance.
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
  start, the automatic context's ordered track IDs, cursor, complete
  browser origin, and its display name are frozen; typed manual queue
  items remain a separate ordered line in front. Later navigation,
  search, facets, or even refining down to zero hits change neither the
  running item nor a snapshot that still has titles ahead of it; the
  exhausted case belongs to PLAY-11. After the last context track, playback
  ends with Repeat off unless an explicit manual entry or PLAY-11's new
  full-library continuation follows; queue hygiene is governed by
  PLAY-5a/5b/5c.
- **PLAY-9** [active] [gtk] — Play/Pause, with playback stopped and no
  loaded title, queue snapshot, or "Play Next", immediately starts a
  randomly chosen existing library title. For this, an immutable
  snapshot is created from all existing library titles in random order;
  Missing and deleted titles are excluded. With an empty library,
  Play/Pause stays disabled and playback stays stopped.
- **PLAY-10** [active] [gtk] — A loaded podcast, YouTube episode, or radio
  station projects its stored source image into the full-width player bar as
  well as the Now Playing panel. Both surfaces use SRC-11's same gated,
  bounded cache and decode path; a missing or refused image keeps the normal
  player placeholder, never a broken-image state.
- **PLAY-11** [active] [gtk] — **Playback remains an immutable snapshot
  while it still has titles ahead of it.** Later navigation, search, facets,
  and clearing a filter never rewrite a snapshot with a future. Exception once
  it is exhausted — nothing left ahead of the cursor: if the snapshot
  originated in a search- or facet-filtered Music library and Music is now
  completely unfiltered, Reprise continues from all existing library titles in
  random order. **While a title is still playing, that continuation is bound in
  at once**, the moment Music becomes unfiltered, so the queue stops reading
  empty for the rest of the title; the running title keeps the cursor, is never
  restarted or re-ordered, and every other library title follows it exactly
  once. If the final title has already ended instead, a new random snapshot
  starts on a title other than the one just finished — which may occur later in
  it, but never starts it. Missing and deleted titles are excluded. If the
  filter is still active, the origin was not the Music library, the visible list
  is not the complete library, or no other title exists, playback ends as
  before. Explicit Play Next entries retain priority and Repeat One/All
  retain their existing queue behavior.
- **PLAY-12** [active] [gtk] — **The player bar and the Now Playing panel have
  no dead surfaces.** The title, channel/artist line, and cover are links in
  every playback mode. What is playing is findable: each of the three surfaces
  leads to the place where the loaded item stands in a list. If a surface has
  no distinct target in a mode, it leads to the nearest target that does exist,
  never nowhere. A surface may be insensitive only when no item at all is
  loaded; it is then visibly inactive rather than silently inert, in the bar
  and in the panel alike — a link that stays clickable with nothing loaded
  swallows the click. Ending playback returns both to that state instead of
  leaving the finished session's labels standing. A surface's label and tooltip
  name the actual target for the current mode; the Now Playing and information
  panels share these links and labels.
- **PLAY-13** [active] [gtk] — The source, never the view, selects exactly one
  player-bar progress language. Local media keeps the ordinary played
  progress and remaining time, without a buffer segment or network copy.
  Finite remote media keeps played progress, adds a paler contiguous-buffer
  segment underneath it, and says “Streaming · X% loaded” from buffered time
  divided by duration only while loading is incomplete. Live media has no
  seekable progress or duration: the full-width bar replaces the waveform with
  an accent point, “LIVE”, and the station name while retaining connected
  elapsed time. The point pulses only during active playback through MOT-7's
  central motion gate; pause and reduced motion leave the same point static.
- **PLAY-14** [active] [gtk] — **Previous follows the actual playback history,
  not queue order.** After the current item has played for more than 3 seconds,
  Previous seeks to its beginning; otherwise it selects the most recently
  played item — under shuffle, the item that actually played rather than a
  neighbour in shuffled order. When history is empty, Previous seeks to the
  beginning so the control always does something meaningful and
  `CanGoPrevious` honestly remains true; this also applies to an episode with
  no predecessor. Rewinding is a seek, not a pipeline restart. After stepping
  back, Next returns to the item the jump left. History exists only at runtime
  and starts empty after launch.
- **SEEK-1** [active] [gtk] — **The seek bar's colour is a reading, not a
  decoration, and it is averaged over time.** The spectral centroid swings
  from beat to beat: taken per bar it puts cyan next to magenta inside two
  seconds of music, which is noise, and noise forms no pattern anyone can
  read. The bar therefore averages the stored curve over a window of **eight
  seconds** centred on each point — about four bars — and paints that. The
  window is defined in **seconds and never in bars**: a window measured in
  display bars would smooth a narrow window differently from a wide one, so
  the same track would read differently after a window resize. It is derived
  once per track, from the curve and the duration, and cached beside the bar
  heights; nothing recomputes it while drawing. At the two ends the window
  shrinks to what is available rather than being padded, because padding
  would run the first and last seconds of every track into one end of the
  axis regardless of what plays there. What the listener gets is contiguous
  fields of eight to thirty seconds — an intro, a verse, a breakdown —
  instead of a rainbow. A track with no structure may legitimately look
  almost uniform; that is the correct answer, not a broken one.
  **The colour lies on both sides of the playhead.** Played is the spectral
  colour at full opacity, coming is the *same* colour at **0.34** — progress
  is a step in opacity, never a change of colour. Ending the colour at the
  playhead put it exactly where it was no longer needed: the point of seeing
  the shape of a track is to see it before hearing it. The 0.34 is measured
  at both ends of the axis: lower and a deep bass intro disappears against
  the bar's background, higher and progress stops being readable at a glance.
  Two colourings are offered under Appearance → Seek Bar — "Frequency" and
  "Single Color" — and the quiet one is a second colouring, never an "off":
  it draws the played side in the accent, the coming side in grey, and
  hairlines where the music changes, so it still says where the structure is.
- **SEEK-2** [active] [gtk] — **A colour scale nobody explains is a
  decorative strip, so it is explained exactly once.** A legend sits under the
  bar at the height of the times — the two ends named, a 150×6 px gradient
  between them, and what it measures — and the gradient is drawn by the same
  function the bar is, never rebuilt, so the two cannot drift apart. It
  appears on the **first three track changes** and then no more; the measure
  is a **count**, not a timestamp, because "seen it three times" says more
  about having understood it than "shown two days ago" does. It leaves after
  six seconds, fading and collapsing its height together so the row settles
  instead of jumping, or at once on the first press anywhere in the bar —
  a press means the user is aiming at the bar rather than reading it. There
  is no close button: it would be larger than what it closes. Afterwards it
  stays reachable from the bar's context menu ("Explain the Color Scale"),
  because a one-off hint that can never be called back is a trap for
  everyone who missed it the first time. In the single-colour bar it neither
  appears nor spends a showing, and the menu entry is inactive: there is no
  scale on screen to explain.

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
  Tracks"). "Show all N tracks ›" → Tracks mode in the artist place; its
  visible place pill, which leaves the place on click, is already active
  via FIL-1c.
- **FX-1** [planned] [manual] — All effects respect
  `gtk-enable-animations=false` (hard switch) and only run GPU-cheap
  (opacity/transform, pre-rendered glows). No live blurs in lists.

## E. MTP / Sync

- **MTP-1** [active] [gtk] — A newly connected Android MTP device
  produces a device-name-specific connected toast and a device card in
  the sidebar. It never automatically navigates away from the current
  view.
- **MTP-2** [replaced by MTP-13]
- **MTP-3** [replaced by MTP-48] — The device card and an open device page
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
  The planned-growth segment is hatched so it reads as not yet written.
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
- **MTP-13** [replaced by MTP-64] — The entire device card is exactly one
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
- **MTP-15** [replaced by MTP-60/MTP-63] — The playlist workspace and sync overview
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
- **MTP-17** [active] [core] — `Music/Reprise` — the playlists target — is
  fully authoritative for music and playlists. After
  successfully publishing all desired tracks and playlists, all remaining
  safe files are removed there, even if they are not in the Reprise
  inventory; desired track and playlist paths are preserved. Nothing is
  written, moved, or deleted outside this subfolder, and a missing or
  invalid playlist target state schedules no destructive work.
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
  the cable was pulled — is marked interrupted with an end time when Reprise
  next starts rather than left open or dropped, because "it never finished"
  is itself the answer.
  Successful copies are counted, not listed; every file that deviated —
  skipped, failed, removed, or kept in its original format — is recorded
  individually with its device path and reason, removals included, since the
  mirror owns `Music/Reprise` and what it deleted is the question that gets
  asked. The device page shows the recorded runs newest first, one
  expandable row each with its deviations inside. Recording never blocks a
  sync: a log write that fails is dropped, not propagated. Only the most
  recent thirty runs per device are kept, and the whole log is capped at 240
  runs so volatile connection identities cannot grow it without bound.
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
- **MTP-22** [active] [core] — The one playlists sync plan is read as either a
  numbered balance "N new · M removed" or "Unavailable, kept on phone" when
  there is nothing local to compare against and existing device files must be
  left untouched. Copying and removing each keep their own file and byte count
  in the target row and overall balance ("To copy 14 files · 2.6 GiB", "To
  remove 3 files · 148 MiB", "Playlists rewritten 2"). Whether the plan has
  work is decided by file count, never byte value, so a removal-only run moving
  0 B stays distinguishable from "nothing to do" as "3 to remove · frees 0 B".
- **MTP-23** [active] [core] [gtk] — The actual transfer writes and deletes
  through the single playlists target (default `/Music/Reprise`, `MTP-17`).
  Deleting and copying run serially, one transfer at a
  time; progress comes from the transport's send callback. The MTP transport
  layer is an injectable abstraction (`DeviceBackend`); the real GVfs/MTP
  binding and a recording test double share the same contract, so the
  regression gate runs without a phone attached.
- **MTP-24** [active] [core] — Music follows the transfer profile
  (`profile.rs`): lossless source material is re-encoded to Opus 160 kbit/s or
  MP3 256 kbit/s, while lossy material is left untouched.
- **MTP-25** [replaced by MTP-54] — The size cap from `MTP-39` is real: before a
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
  into Music, Other and Free, matching the single playlists target. The bytes
  this sync will write carry their own, clearly
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
  the target folder path in the Content section (`MTP-37`) opens the device
  folder browser. The browser offers storage selection (internal/SD card, from
  the storages found on the device), a folder tree with navigation into a
  folder and one level back, "New folder", and a target preview ("Files will be
  stored at ⟨Storage⟩ → ⟨path⟩") which resolves only once a storage has been
  chosen and otherwise reads "once a storage is chosen", or "no longer
  available" for a storage that has disappeared. "Reset to default" resets
  path and storage to `/Music/Reprise`. If a device refuses to create a folder
  directly in a storage's root, the browser shows that error inline rather than
  swallowing it silently or claiming success. MTP knows no stable paths:
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
  storage change still goes through the existing copy-and-orphan path,
  because a folder cannot cross MTP storage boundaries by moving. The decision
  (Unchanged / MoveFolder / CopyAndOrphanPrevious) is the pure function
  `reprise_core::device_sync::browser::target_relocation_action`, which reuses
  `target_storage_transition` instead of duplicating it; a move that
  fails on the device does not block saving the new folder — the next sync then
  simply copies afresh, but logs a warning.
- **MTP-33** [active] [core] — The switch "Remove from phone when removed from
  a playlist" (`DeviceSettings::remove_deleted`) decides whether a managed
  playlist track that is no longer selected leaves the device on the next sync
  plan. Switched off, the file stays; switched on, the playlists delta removes
  it. The setting is read from that same device, never hardcoded.
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
- **MTP-36** [replaced by MTP-54] — `MTP-45`'s YouTube rule "the latest N episodes
  per enabled channel among those already downloaded or explicitly wanted"
  names N; this is where that value lives. **Decided 2026-07-29:** a global default (default **5**,
  `podcasts::config::PodcastConfig::latest_per_channel_default`, key
  `podcasts.latest_per_channel_default`), overridable per channel
  (`podcast_subscriptions.latest_per_channel`, schema v47) — the same shape as
  `O-5` for "Keep N downloaded", so that both quantity limits share a single
  mental model rather than two different ones. N is **device-independent**:
  since `E-5` there is exactly one device, and storing it per device would be
  effort without meaning — it lives beside the other global podcast config,
  never on `DeviceSettings` or a per-device `SyncTarget`. **Decided 2026-07-29
  (the zero question):** a value of 0 means **unlimited**, here and in every
  other numeric sync setting — the size cap has modelled 0 that way since
  `MTP-38` (`cap_bytes` is an `Option`), and two adjacent numbers on one page
  must not read 0 in opposite directions. "Nothing from this channel" is what
  the channel toggle from 6b says; it is not a quantity, so it is not
  expressed by a quantity.

  The live pipeline (`device_sync_compact::recompute_delta_silent`) resolves
  each enabled channel's effective N via
  `selection::resolve_latest_per_channel` (the channel's override if one is
  persisted, else the global default) before building
  `EpisodeSelectionRule::LatestPerChannel`'s `channel_latest` map, and that
  map — not a single shared `latest` — is what actually bounds each channel's
  selection now. This rule is `[core]` only: design 6b's channel page has no
  control yet for setting a channel's own override, so today it is set
  through `podcasts::store::set_latest_per_channel` alone, with no GTK
  surface. That gap is tracked, not hidden behind `[planned]` — the
  persistence, the default, and the live wiring this rule actually decides
  are real and tested (`mtp_36_…`), which is what `[active]` asserts here.
- **MTP-37** [active] [core] [gtk] — The device view has one Content row for
  the playlists target: its folder path (`MTP-31`'s "Change folder…"), an
  honest live selection summary, size on the device, and "Choose…". There is
  no per-category activation switch or size cap; selecting no playlists is the
  existing blocked state and does not create a second empty-state concept.
- **MTP-38** [replaced by MTP-54] — Reprise knows three named sync targets per
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
- **MTP-39** [replaced by MTP-54] — When a sync target with a size cap exceeds its
  limit, the oldest files leave the device first until the total is at most the
  cap again; removal stops as soon as that is reached and never takes more than
  necessary. The selection is a pure function over size and age per entry,
  independent of target kind or transport.
- **MTP-40** [replaced by MTP-54] — Every episode carries a persistent
  `wanted_on_device` state (7f). "Sync to phone" on an episode without a local
  file sets that state instead of refusing the action; the download follows
  automatically — immediately when online, marked as pending when offline,
  through the existing `NET-3a` contract (`deferrable_action_outcome`), without
  duplicating its online/offline decision. An already downloaded episode needs
  no download step. The downloader that actually works off the pending state is
  not part of this rule (E2/E4).
- **MTP-41** [replaced by MTP-45] — The intended set per sync category is a pure
  projection from the selection rule and the library state (E2). Playlists
  yield "N of M selected · K tracks"; YouTube audio limits each enabled channel
  (channel toggle from 6b) to its latest N episodes regardless of download
  state ("N of M channels · latest K each"); podcast episodes want every
  unplayed, already downloaded episode of enabled shows with no upper bound
  ("Unplayed downloads only"). A wanted episode without a local file
  (`wanted_on_device`, `MTP-40`) counts as waiting, never as ready to copy —
  the intended set keeps the two visibly apart instead of silently filtering a
  waiting episode out of the result.
- **MTP-42** [replaced by MTP-54] — Design 7f's preparation phase
  (`reprise_core::device_sync::preparation`) is a pure projection over
  `MTP-45`'s waiting set, `NET-1a`'s global gate, `NET-3a`'s connectivity, and
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
- **MTP-43** [replaced by MTP-54] — The device page surfaces `MTP-42`'s preparation
  phase without re-deciding it. A preparation overview ("2 files to
  download · 312 MiB", with episode titles) exists only for the `Offered` and
  `Planned` phases — for every other phase, including `Absent`, there is no
  box, no disabled row, nothing. The device's own "Download missing files
  before syncing" switch persists next to `sync_automatically`/
  `remove_deleted`, defaults on, and is never mutated by connectivity or
  metered state — only the stored value feeds `MTP-42`'s `prepare_switch_on`
  fact; offline and metered are decided there, not by silently flipping this
  switch. The primary button reads "Download & sync" exactly when
  `primary_action` answers `DownloadAndSync`, otherwise "Sync now". While a
  `Planned` preparation actually runs, progress reads two phases — "Step 1 of
  2 · Downloading N of M · P%" during the download, then the existing
  transfer progress as step 2 — and a run with no preparation phase stays the
  single-phase reading it always was. Cancelling a preparation in progress
  stops issuing further downloads but never deletes or rolls back an episode
  that already finished downloading. `SkippedOffline` reads "N episodes
  skipped · not downloaded" and leaves every skipped episode's
  `wanted_on_device` flag set for the next attempt.
- **MTP-44** [replaced by MTP-54] — Device-sync preparation (7f, E9) downloads a
  `wanted_on_device` episode (`MTP-40`) by giving the existing podcast
  download manager a priority lane, never a second download path: a
  high-priority request is served ahead of any ordinary request already
  queued in front of it, but both still run through the one
  `PodcastsOperation::Download` and the one worker thread. The priority lane
  is only a queue in front of that single executor, not an alternate one.
  Priority work never permanently starves the ordinary lane — once the
  priority lane runs dry, the worker resumes ordinary requests exactly where
  it left off. The executor itself is singular too, down to the function: the
  GTK worker has no download body of its own — both lanes call
  `reprise_core::podcasts::pipeline::download_episode`, the same function the
  refresh pipeline's auto-download branch and MCP's `music_manage_episodes`
  call, so the episode lookup, `NET-1a` gate, `.part` handling, and progress
  emission cannot drift between a manual click and a preparation download.
- **MTP-45** [active] [core] — The intended set for the playlists target is a
  pure projection from the playlist selection and library state. It yields "N
  of M selected · K tracks", preserves playlist order and duplicate entries,
  and reports unavailable selected tracks separately instead of silently
  treating them as ready to copy.
- **MTP-46** [replaced by MTP-54] — A source whose module is switched off
  contributes nothing to a device sync: no candidates, no counts in the
  content panel, no *new* files on the phone — and, just as importantly, no
  removals of the files already there. YouTube and Podcasts are peers (issue
  #96), so each switch acts only on its own source — switching YouTube off
  leaves podcast episodes syncing untouched, and the reverse. The global "Use
  online sources" gate sits above both and empties the sync on its own, since
  `SET-11` calls that state "a local player only" and a phone still filling
  with feed downloads would not be one. This does **not** delete anything —
  and that clause is the load-bearing one. A switched-off source is not a
  source with nothing selected: with "Remove from phone when deleted or
  unsubscribed here" on, an empty desired set makes *every* resident file of
  that source a removal, so a gate that only emptied the candidate list would
  have turned switching YouTube off into "wipe YouTube off the phone on the
  next sync". `SET-11`'s promise that subscriptions and favorites are *kept*
  stands unchanged, switching the module back on restores exactly the previous
  sync, and `build_plan` returns an empty plan for a disabled source rather
  than a plan full of deletions. The ordinary `remove_deleted` cleanup for an
  *enabled* source is untouched. The gate sits in
  `device_sync::podcasts::query_candidates_for_device` and its selection
  sibling — at the two places the rows are actually read, deliberately not at
  their callers, so a future caller cannot reach the rows around it. It is
  deliberately not `online_sources::network_allowed`: copying an
  already-downloaded file makes no request, so the two share a formula but not
  a meaning, and must stay free to diverge.
- **MTP-47** [replaced by MTP-54] — An episode on the phone is named
  `<Show>/<YYYY-MM-DD> - <Title>.<ext>`, never by its database id. The date is
  `published_at`, or the day the episode was first seen when nothing else is
  known — the same fallback the download cleanup order already uses. It is
  never a constant, because every undated episode of one show would otherwise
  collapse onto one name and overwrite the others. The day is formatted in UTC,
  not local time: the path is the sync's delta key, and a key that moved with
  the machine's timezone would evacuate and re-copy the whole podcast tree
  after a trip. The separator is ASCII because the inventory side of that
  comparison arrives through a lossy UTF-8 conversion, and components use the
  same byte cap as music paths. Two episodes that would land on one name are
  told apart by their stable episode id in brackets, not by a positional index:
  podcast device files are inventoried live and persisted nowhere, so a
  positional index would move whenever another episode was deleted and cost a
  delete plus a full re-transfer. When a collision group shrinks to one member,
  that survivor loses its now-unnecessary suffix once; deleting any namesake
  never renumbers the rest. Uniqueness is judged on the composed path, never on
  the bare name, because the suffix becomes part of that name: an episode whose
  title already ends in `[42]` would otherwise take exactly the path episode 42
  receives the moment a namesake disambiguates it, and two episodes on one path
  are both copied and overwrite each other on the phone rather than colliding
  anywhere Reprise could notice. A file Reprise itself named `.audio` is
  managed like every other audio file, so it is inventoried, removable and
  counted instead of being re-copied on every sync. Renaming makes files from
  an earlier sync look new: with "Remove from phone when deleted or
  unsubscribed here" on, the old names are removed and the new names copied in
  one noisy sync; with it off, both remain until the user deletes one. Sorting
  a show folder by name is now chronological, which the id form never was.
- **MTP-48** [active] [core] [gtk] — Replaces `MTP-3`. Exactly one MTP
  device is active at a time and exactly one session is ever open. The first
  device detected owns it; a second one is detected and listed but never
  opened, its row reading "Plugged in · disconnect {other} to use it" in
  amber with no sync action. There is no queue, no parallel transfer and no
  device chooser.
- **MTP-49** [active] [core] — Device identity is a stable key: the GVfs
  mount UUID, else the USB serial number from udev/sysfs. The `mtp://` root
  URI is never an identity — it carries the USB bus number and changes on
  every replug. A device with no stable key is usable but not remembered, and
  the UI says so rather than pretending. Persisted per identity: target
  folder, last verified state, size on device, local name — nothing else.
- **MTP-50** [active] [gtk] — The sidebar shows the hardware that is here:
  connected devices stand open, the active one first. Remembered devices are
  history and wait, dimmed, behind the section heading, which carries a
  disclosure arrow and opens them on click — keyboard included, closed again
  on every launch, because a phone that is not plugged in is not a place to
  go. With no history behind it the heading is a plain label: no arrow,
  nothing to open. A remembered device shows no
  diff — only "Not connected · synced 3 days ago" or
  "Not connected · never verified" — because a balance for an absent device
  would be a guess. Opening one shows its target folder and last verified
  state; syncing requires connecting it. Right-click → "Forget device" drops
  the persisted row and deletes nothing, on the device or locally. A local
  rename keeps two identical models distinguishable and is never written to
  the device.
- **MTP-51** [active] [core] [gtk] — The device page's one Content row offers
  "Choose…", opening one playlist picker: one rule row, one grouped checkbox
  list, a filter, "Select all", and one live footer. The
  picker and the library's right-click "Include in phone sync" are two ways to
  operate **one** decision — they write the same flag and neither holds a copy
  of it. It offers "Everything" and "Keep smart playlists up to date on each
  sync", because a
  smart playlist not re-evaluated at sync time freezes on the phone. This
  filter is the deliberate dialog exception to SEARCH-4a: Esc with filter text
  clears it and keeps the unfinished picker open; Esc with an empty filter
  retains the dialog's standard close behaviour.
- **MTP-52** [active] [core] [gtk] — After a complete, successful device scan,
  a selected track whose recorded device path is missing from the phone is
  copied again through the ordinary transfer plan, together with its analysis
  sidecar. The desktop inventory records what Reprise remembers writing, not
  proof that the file is still there. A device that was never scanned, or whose
  scan failed, keeps the inventory-only guard and schedules no such recovery;
  a matching file that is present remains untouched.
- **MTP-53** [active] [gtk] — A phone is recognized as the same phone whether
  it was plugged in before Reprise started or after: a listed mount whose root
  URI matches a volume's activation root belongs to that volume immediately,
  so the volume supplies identity, name, and icon before GIO links the two
  objects or shadows its plumbing mount. A volume with no matching listed
  mount remains disconnected, and an unshadowed MTP mount no volume claims
  remains usable as a fallback.
- **MTP-54** [active] [core] [gtk] — Reprise synchronizes exactly one target
  per device — the playlists target under `/Music/Reprise` (`MTP-17`). There
  is no per-category selection, no size cap, and no automatic accumulation
  from subscriptions; what reaches a phone is what the user put in a playlist.
  A device that previously had a podcast or YouTube target gets one dismissible
  notice that those files were left untouched outside `/Music/Reprise`.
- **MTP-60** [active] [gtk] — A pending or running synchronization lives in a
  full-width docked bar at the bottom of the device page. The bar is inside the
  window but outside the scroll view, so it neither scrolls away nor covers the
  player bar. It remains present while ready, running, finishing, or failed and
  is the only place a run failure is shown. Ready offers "Sync now" with the
  pending scope; running shows the file count, current title, transfer rate,
  remaining time, progress, and a primary "Cancel" action. The playlist
  workspace and sync overview keep stable top and bottom card edges independent
  of delta, track, and speed text. Changing status text stays within a bounded
  dock width and never moves the bar's controls or resizes the workspace. During
  copying, the current track and MTP transfer rate occupy separate labeled
  lines.
- **MTP-61** [active] [gtk] — The lower device-page section is named "On this
  device" and is a balance, not a second selection surface. It shows Reprise
  music, hatched growth for this run, Other, and Free; the playlist, track, and
  size balance; folder, smart-list policy, and size-limit controls; and "Rules
  for this phone" switches for removing locally deleted music and syncing
  automatically on connection. Playlist selection exists only in the upper
  card; this section reports its result and links back to it.
- **MTP-63** [active] [gtk] — The sidebar device card carries three distinct
  contrast steps, without dimming the entire card. A running sync has an accent
  edge, tinted ground, accent icon chip, an accent status row with the file
  count "x / y", a separate progress line, and a Cancel button in the card. A
  connected idle device keeps the device name at full strength on a solid
  neutral surface with a neutral edge and dims only its status line. A
  remembered disconnected device has no surface, only a hairline edge, and
  reduces the title and status separately. The contrast between active and
  remembered is the primary information. The Devices heading is one step
  brighter than the other sidebar section headings. During Checking, Sync, and
  Finishing, known free device storage leads the status; while storage is
  unknown, that placeholder is omitted so ellipsizing cannot hide the current
  activity. The same omission applies to the idle "Needs attention" status.
- **MTP-64** [active] [gtk] — The entire device card is one native keyboard and
  pointer entry point into a non-modal full device page in the main window and
  does not start a sync directly; exclusively while a sync of this device is
  running, it carries a second entry point, the Cancel button. The button is
  not a descendant of the card surface but a sibling in an overlay, so the
  card surface itself remains exactly one target. The primary menu item opens
  the same page for one device, and a compact selection first for multiple
  devices. The page contains no song or device file list, and the transfer
  profile as its only setting; it shows every playlist with a visible,
  markup-safe name, selection, last verified sync, and the target size
  projected for the active profile, as well as, during a running sync, a
  progress bar and current smoothed MTP transfer rate.

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
- **SET-6a** [replaced by SET-10] [gtk] — The Plugins page groups by user intent:
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
- **SET-7** [replaced by SET-10] [gtk] — "New Releases" and "Concerts" are peer
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
- **SET-9** [replaced by SET-10/SET-11] [gtk] — Replaces `SET-8`: the same Preferences main page
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
- **SET-10** [active] [gtk] — Plugins is the only settings surface for
  optional capabilities. It has exactly three groups in this order: Local,
  Online content, Connected services. Every capability appears exactly once;
  a capability with settings is an `AdwExpanderRow` whose settings are child
  rows. There are no "Online sources", "New Releases", or "Concerts"
  Preferences main pages. Every Online-content row names the service it
  contacts, so Plugins is also the privacy overview. Phone sync deliberately
  does not appear here: its rules stay on the device page (`MTP-37`). Location
  is the explicit exception: it is app state shared by multiple capabilities,
  not an optional capability, and therefore owns the main page specified by
  `SET-15`.
- **SET-11** [active] [gtk] — The Online-content group's own header is the
  master switch. Off is a kill switch, not a bulk toggle: no request of any
  kind runs, sidebar entries are hidden, and running downloads are cancelled,
  while every per-module key keeps its value so switching the master back on
  restores the exact previous configuration. The group collapses to its
  header, which never disappears, and offers "Show the N sources"; revealing
  it shows every source read-only. Connected services collapses the same way
  and is labelled "Scrobbling · needs online sources".
- **SET-12** [replaced by SET-14] [gtk] — Replaces `SIM-8`, which stated the same thing
  inside a module that no longer exists: plugin provision badges are derived
  from the static registry, never from current enable state. A provision-kind
  set is the unbadged group norm only when it occurs at least twice and
  strictly more often than the runner-up; otherwise every row is badged.
  Panel-tab and sidebar-section badges use the accent, while all other kinds
  are neutral.
- **SET-13** [active] [gtk] — Preferences follows the content-list search
  grammar: its header reveals one centred "Search settings" field, `Ctrl+F`
  reveals the same field, and one `Esc` clears the query and closes the field,
  returning focus through the existing close path. Matching reads each
  preference row's title and subtitle plus its page
  name. Search mode keeps the sidebar's width, order and height stable: "All
  results" is selected above the five pages, every page shows its hit count,
  and a page with no hits remains present at 42% opacity. The result bar uses
  the shared search-chip/count/"Clear all" order, matches use the shared accent
  highlight, and the shared end line reports hidden settings. A hit is the
  actual preference control, temporarily re-parented under a dim page-and-group
  path; its origin parent and index are recorded before any matching control
  moves so clearing restores every control to its exact place. Activating the
  path closes search, opens that page and focuses the restored control.
- **SET-14** [active] [gtk] — Every row on the Plugins page places its enable
  switch on the same right edge. A non-expandable switch row reserves the
  expander arrow's trailing slot even though the row does not open, so its
  switch stays aligned with the switch of a row that exposes child settings.
- **SET-15** [active] [core] [gtk] — Location has one app-wide value and one
  Preferences owner. Its main page sits between Library and Plugins, owns City
  and Default radius, and names every reader under Used by: Concerts, Radio's
  Near you, and Podcasts' Popular in country chart (`SRC-19`). Optional
  capability pages only link to Location; they never duplicate or own its
  controls. Clearing Location removes only latitude, longitude, name, country
  name, and country code: it preserves the default radius, view filters, module
  choices, and online-source choice. On this page, City displays
  `{city}, {country}` and falls back to `{city}` when no country name is stored
  (`set_15_location_name_omits_the_separator_without_a_country`). Disabling
  Concerts or online sources never makes the stored location or radius
  unreadable and never suppresses the app-wide location-change announcement.

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
  same full card contract from FB-8 in the main-window sidebar for every
  run > ~1 s, including visible Cancel and navigation to the associated
  view. Modal dialogs carry their status in chrome according to FB-9
  instead of duplicating that card contract in their content flow.
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
  queue item faults → skip. A track shows one toast "Track unavailable
  — skipped".
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
- **FB-8** [active] [gtk] — In the main-window sidebar, Scanner and
  Relink scans run off-thread in the existing progress cards, stackable
  with Sync/Doctor, in the bottom-anchored area. The card block sits
  **below** the Issues block and the two are visible at the same time: the
  heading "ISSUES" and its rows — Import errors, Missing files, Library
  Doctor — stay where they are while a job runs. Fully inactive progress cards
  occupy no space; only active or still-fading-out cards take part in
  the layout. The bottom edge of the visible card block sits directly
  above the player bar, while all free sidebar height stays above the
  block. Persistent device status remains visible independently of
  this.
  *Amended 2026-08-07.* Until then a visible card **replaced** the whole
  Issues block. That made starting any scan take the `ISSUES` section away,
  including the Library Doctor's own result row — so the entry that says
  "there is something to review here" was hidden by the job that produced it,
  and `Missing files` disappeared for the duration of an unrelated scan. The
  design shows both at once; coexistence is the rule now. Do not restore the
  replacement.
  Card: spinner + title + % on the right (tabular) + 3px bar +
  ellipsized detail line. Clicking the card → Missing files; the visible
  Cancel button checks for abort before each audio file. Modal dialogs
  carry the same work in their chrome according to FB-9; they do not add
  this sidebar card to their content flow.
- **FB-9** [active] [gtk] — Transient status indicators do not displace
  existing layout. Use the first implementation that applies, in this
  order: (1) **chrome** — the header, footer or edge region of the window
  or dialog, as an overlay with no layout height of its own; this is the
  first choice for global states; (2) **reserved space** — a line of fixed
  height that is always present and names the resting state at rest
  ("Library up to date"), so it never sits empty; (3) **state change of an
  existing element** — the row that triggers the action shows the progress
  itself, at unchanged height. Three prohibitions: never insert a banner
  above the content and remove it again; never let an indicator's own
  height change with its state (error text, a second line, a growing
  detail list); never leave an empty placeholder that occupies area without
  saying anything. One task's status is never duplicated within the same
  window. The rule is per window, not app-wide: the main window may retain
  its one sidebar card while a modal dialog uses its one chrome location,
  regardless of which page is open. The short status is icon, percentage
  and Cancel; details are spelled out only where the user can act on them,
  otherwise they belong in the tooltip. Progress bars are ≤ 2–3 px high and
  sit on an edge, never between two elements. Background status fades in
  place with the Micro token (150 ms), never with a height animation.
  Continuous gear rotation and indeterminate pulsing obey the central
  reduced-motion gate and remain statically legible when animations are
  disabled.
  Named exception: the shared source-error banner of NR-21, CONC-11,
  POD-19, RAD-2 and NET-3 may still be inserted above a populated view and
  removed again when the next refresh succeeds. This is a documented
  deviation, not a second sanctioned pattern — it is precisely what the
  first prohibition names, and it is carried only because those five
  surfaces shipped before this rule became enforceable. It expires when
  those views are next reworked: the failure notice then moves into chrome
  or a reserved line, and this paragraph goes with the last in-flow
  banner. No new surface may cite it.

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
- **OS-6** [active] [core] [gtk] — A release that reaches its release
  date announces itself once. The desktop notification fires only for a
  release whose row already existed before the current run began, so a
  first fetch announces nothing, and a stamp on the row prevents the hourly
  due check from repeating it the same day. Up to three releases send one
  notification each, carrying the release title, `{artist} · {type} · out
  today` and the cover when it is available; four or more collapse into a
  single collected notification. Activating a per-release notification
  opens exactly the URL its popover row would open, through the shared
  external-link guard; the collected notification has no single release to
  point at and opens the Releases view instead.
  Test: `os_6_the_first_fetch_announces_nothing`
  (`crates/reprise-core/src/artist_news_notify.rs`, `#[cfg(test)]`).
- **OS-7** [active] [gtk] — Update notifications are a three-step
  setting on the New Releases plugin row — `Off`, `Releases only`,
  `All updates` — stored as `updates.notifications` and defaulting to
  `Releases only`. `All updates` adds one collected notification per run
  for newly found concerts of library artists, and behaves like
  `Releases only` while the Concerts module is off. Nothing else notifies.
  Test: `os_7_all_updates_adds_the_concerts_delta`
  (`ui/preferences/preference_new_releases.rs`, `#[cfg(test)]`).

## I. Start state

- **START-1** [replaced by START-3] — Normal start previously forced the
  library root and left selection untouched.
- **START-2** [planned] [gtk] — Start with an unavailable library root:
  StatusPage per Root-Guard, no mass Missing marking; library views
  show the last known holdings normally (Root-Guard hasn't marked
  anything), only the StatusPage/card reports the state. No blank
  screen.
- **START-3** [active] [gtk] — Normal start restores the last valid browser
  destination, including its local refinements, but never reconstructs the
  Back/Forward stack and never autoplays. The last loaded track or episode is
  presented paused; the first Play starts that exact item through a fresh
  playable source, applying an episode's existing resume position, and leaves
  the viewport exactly as the start placed it (NAV-10b). If its stable ID belongs to the
  restored destination, that row becomes the sole selection and is centered
  without taking keyboard focus; grouped podcast and YouTube sources expand
  the required group and preview window first. An unavailable item leaves the
  destination's own selection and viewport untouched.

## J. Queue view

- **QUE-1** [active] [gtk] — A shared queue model feeds two surfaces
  with different depths: the sidebar row "Queue" opens the ColumnView
  as a management surface with sections, DnD reorder, right-click,
  Clear and StatusPage. The panel toggle opens "Up Next" as a viewing
  surface of the same queue with sections, jump and remove. The player
  bar has no redundant queue icon. No surface maintains its own second
  list. Every ColumnView section header shares one uniform height; the
  plain title row grows to the authored button-row floor rather than
  shrinking the real Clear button's target.
- **QUE-2** [active] [gtk] — The panel divides the future into exactly
  two conditional sections: **Next in Queue** for manually enqueued
  tracks and **Continuing from "<Album/Playlist>"** for the automatic
  context from `play_origin`. A header appears only if its section has
  entries; an empty manual section leaves only "Continuing …" standing.
  Their visible order is also the playback order; as long as something
  is playing, the queue never shows two empty sections. QUE-10 owns the
  direct-episode variant of the named context section.
- **QUE-3** [active] [core] — Played manual entries silently disappear
  from "Next in Queue" on queue-item change: no strikethrough and no
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
  Metadata comes from one batched query per item kind present in the
  visible window, never a per-row query; row recycling and loading only
  the visible window bound widgets and work independent of queue length.
  With the panel closed or another tab active, item changes and reorders
  only update the model and render no panel rows.
- **QUE-9** [replaced by QUE-12] [core] — The manual queue stores typed track and
  episode entries and preserves their identity even when their numeric
  IDs collide. RSS and YouTube episodes advance in manual queue order,
  never enter the container queue's automatic `QueueSnapshot` context,
  never earn a listen, and are never gaplessly pre-fed. POD-20 owns their
  shared playing marker, POD-21 owns their frozen queue-neighbour transport,
  and QUE-10 owns the direct-episode rendering projection.
  Radio remains excluded. Outward queue snapshots add typed item lists
  alongside the legacy `*_track_ids` projections; those legacy fields
  remain track-only and omit episodes. MPRIS identifies an episode under
  `/org/reprise/Reprise/episode/{id}` and exposes title, show-as-artist
  and length, but no album or rating.
- **QUE-10** [active] [gtk] — While a podcast or YouTube episode plays
  directly from its source view, both queue surfaces show that episode as
  Now Playing and render its frozen POD-21 neighbours as the named context
  section, labelled with the show or channel. The manual queue and container
  `QueueSnapshot` remain unchanged underneath and reappear unchanged when
  queue playback resumes.
- **QUE-11** [active] [core] [gtk] — Session persistence keeps the track-only
  manual queue, its current entry, and the stable identity of a loaded podcast
  or YouTube episode. Direct playback additionally stores its bounded frozen
  episode-neighbour order. Signed or resolved stream URLs never persist. On cold
  start the identity is validated against the current episode catalog and is
  reconstructed as paused metadata only.
- **QUE-12** [active] [core] [gtk] — Replaces `QUE-9`. Podcast and YouTube
  episodes never enter the manual queue. Core rejects episode items at every
  manual-queue insertion point and session restore removes any pending or
  current episode left by an older build. GTK queue actions and drop targets
  accept and report only the surviving tracks; an episode-only action or drop
  is disabled or refused without a success toast. `QueueItem::Episode` remains
  available for direct episode rendering and outward typed projections.
- **QUE-13** [active] [gtk] — Removing the Now Playing row of a directly loaded
  podcast or YouTube episode ends that external session and reports one removed
  row. Its frozen show-context rows remain read-only and are never removed from
  the music queue.
- **QUE-14** [active] [gtk] — The Up Next panel's remove control follows its
  bound row's live position. When a model shift moves that row into a read-only
  podcast or YouTube show context, the control disappears immediately.

## K. Filter & search visibility

- **FIL-1a** [active] [gtk] — One truth about restrictions (track
  lists): anything that restricts the visible track list appears as a
  chip in the filter row directly above the list — including the
  header-bar search (chip ⌕ "falling" in any field, own ×-click target
  ≥ 20 px; the × removes only the search, Esc per NAV-6). Applies in
  every track source (Library, Playlist, Smart, Queue, Missing). The
  search remains only while the same track list stays current; its chip
  appears everywhere it actually restricts — in sources without search effect
  (Import Errors: own panel rows) no chip appears. Facet chips and
  "+ Add filter" stay library-only. An invisible active filter is a bug.
  (Revision history: the 2026-08-05 text made search section-local and
  restorable under SEARCH-8. SEARCH-8a supersedes that state: choosing another
  sidebar destination drops only the query, while drilling into an Artist,
  Album or Genre place carries it into that narrower context; Back restores
  the complete history-owned list state. Every list view still carries its
  active query as its first chip, worded for the fields that view actually
  reads — FIL-1d.)
- **FIL-1b** [planned] [gtk] — Albums/Artists mode: the global search
  already works there (grid filtering); the same chip row incl.
  counting and "Clear all" will follow there per the pattern of
  FIL-1a/FIL-2. Until then, the gap is named here instead of silently
  broken.
- **FIL-1c** [active] [gtk] — Artist, Album and Genre pages are
  **places**, not filters, and are marked as such: a place pill sits in
  the filter row's own left zone, outlined rather than filled, prefixed
  with "‹", and without a × — its whole surface is the click target
  (≥ 20 px), and its tooltip and accessible name say leaving
  ("Leave <place>"), never removing. Leaving happens through the regular
  NAV-2 history push to the Library; there, its remembered search and
  facets are restored. A place carries a pill exactly when no sidebar
  row already names it: Artist, Album and Genre pages qualify; Library,
  Recently added, Playlist, Smart, Queue, Missing and standalone panels
  do not. The "FILTER" heading, the chips and "Clear all" describe the
  filter zone only and never appear for a place alone; "Clear all" still
  clears search and filters and never changes location. Counting follows
  FIL-2. (Revised 2026-07-31: the original rule rendered places as
  removable scope chips under the FILTER heading — one shape for two
  meanings, which measurably read as a filter that turned out to be a
  navigation.)
- **FIL-1d** [active] [gtk] — The search chip names its scope. Every
  list view accepts the section's query as its **first** chip, ahead of
  the facet chips and with the same removable ×-affordance FIL-1a gave
  the Library one — whether or not the list is playable: Music,
  Podcasts, YouTube, Radio, Queue, playlists, Releases, Concerts,
  Missing files. The wording is not decoration but a promise about what
  was matched, so it names the fields that view actually reads: Music
  and its sibling track sources say "⌕ "{query}" in track, artist and album";
  Podcasts says "in episode titles", YouTube "in video titles", Radio
  "in station names", Releases "in title and artist", Concerts "in
  artist and venue", Missing files "in file paths". A view may never
  claim a field it does not search, and may never quietly search one it
  does not name. Matching is case-insensitive substring matching,
  mid-word included ("wer" matches "Antwerpen"). The chip's accessible
  remove name stays "Remove search: {query}" everywhere. The chip's ×
  and "Clear all" clear the query and the facets of the **current**
  view only (FIL-2a, SEARCH-8a).
- **FIL-2** [replaced by FIL-2a] [gtk] — Counting is state: the filter row is the
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
  is the only counting (decided 2026-07-17). The counting base is always
  the current place: inside an Artist, Album or Genre page "X of Y"
  relates to that place's own total, never to the whole library — the
  same way a playlist reports its own length. The row is visible when a
  filter is active, when a place pill is due (FIL-1c), or when the
  preference asks for it. (Counting base revised 2026-07-31 together
  with FIL-1c.)
- **FIL-2a** [active] [gtk] — One filter row grammar binds Music, Releases,
  Concerts, Podcasts, YouTube and Radio. Its slots are normative and always
  read left to right as: the Music-only place pill; the removable search chip;
  active facet chips; the "+ Add filter" control where that view offers
  extensible facets; an expanding spacer; the view's count; and "Clear all"
  at the right edge. The spacer is the only expanding slot: every slot before
  it stays content-sized even when its child requests expansion, so "+ Add
  filter" remains adjacent to the preceding search or facet chip. A
  selection-only action follows "Clear all" rather than entering the filter
  slots. No row renders a `FILTER` caption: the chips
  already state what restricts the list. With an active restriction, the
  count keeps the view's own unit and accents its bold shown number (for
  example "15 of 1,664 tracks" or "168 of 629 gaps"); "Clear all" clears the
  current view's query and facets together. Without a restriction, the count
  remains a neutral dim caption and "Clear all" is absent. Music retains
  FIL-1c's place semantics and existing preference-driven idle visibility;
  every counting base remains the current place, never the whole library.
- **FIL-3** [replaced by FIL-3a] [gtk] — End-of-results row: below the last
  row of a restricted track list (≥ 1 hit) sits the hidden-track count and a
  Show all pill.
- **FIL-3a** [active] [gtk] — The end-of-results row binds Music, Podcasts,
  YouTube, Releases, Radio and Concerts. It appears only when at least one row
  is shown and at least one row is hidden; zero hits remain FIL-6's empty
  state. Centered below the final result, it names the restriction and counts
  in that view's own unit — tracks, episodes, videos, gaps, stations or
  concerts — for example "End of results — 41 episodes hidden by search
  “afd”" with the pill "Show all 44 episodes". Search plus active facets is
  named as both; facets alone are named as active filters. The pill fires the
  same clear-all behavior as the filter row (FIL-6). The row visually belongs
  to the end of the list: directly below the last row when the list is shorter
  than the viewport; for longer lists it only appears once the end scrolls
  into the viewport; it never floats over rows (not sticky). A positioned
  overlay leaves each list's native row rendering or virtualization untouched,
  is input-transparent except for the pill, and recalculates on scroll,
  model/filter and resize changes.
- **FIL-4** [replaced by SEARCH-3] [gtk] — The search field carries its
  state: as soon as the field contains text, it gets an accent border +
  tinted background — even unfocused.
- **FIL-5** [replaced by FIL-5a] [gtk] — Hit highlighting: the search term is
  highlighted in all searched, visible text columns (Title, Artist,
  Album, Genre; accent bold, Pango-escaped). If the only matching
  column is hidden, the row stays unmarked — an accepted remaining gap.
  Chip wording stays "in any field".
- **FIL-5a** [active] [gtk] — Every visible field that participates in the
  current view's query marks every matching occurrence with the same
  Pango-escaped accent-bold foreground and translucent accent tint (18%
  background alpha). The foreground is mixed toward that label's text color,
  so the mark survives both titles and dim subtitles. This binds the fields
  FIL-1d names in Music and every sibling track source, Podcasts, YouTube,
  Radio, Releases (title and artist explicitly), Concerts and Missing files;
  a visible field the view does not search stays unmarked. Title and Artist
  are always reachable. Album is foldable, so an album-only hit can be
  off-screen at narrow widths; "Show columns" restores it per STYLE-6.
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
  are not refilled. The filter is always available in the library; the
  Experimental switch that used to gate it is gone (INST-11).
  (Decision 17)
- **FIL-8** [active] [core] [gtk] — "Recently added" is its own library
  scope over all currently existing tracks whose `added_at` is at most
  seven days ago; there is no 50-track limit. The source initially
  sorts by `added_at` descending and carries no place pill: it is a
  sidebar place, and the sidebar row already names it (FIL-1c, revised
  2026-07-31). Selecting another sidebar row leaves it, like any other
  sidebar place.
- **FIL-9** [active] [gtk] — When a **facet** filter is set, changed or
  removed and the loaded track belongs to the new result set, its marked
  row is vertically centered instead of anchored to the top table edge.
  Selection and keyboard focus remain unchanged. Without a loaded track
  visible in the target, the existing ID-plus-offset anchor is retained.
  The header-bar search is no longer covered: SEARCH-9 governs it, because
  a query changes with every keystroke and paid for the centering far more
  often than a facet click does. “Clear all” with an active query follows
  SEARCH-16; without a query, clearing facets remains governed by FIL-9.

## L. Tag editor

- **TAG-1** [active] [gtk] — Save is navigation-neutral: saving changes
  neither scroll nor the library's view (NAV-5 holds through the
  dialog); there is no "jump to next song". After closing, focus sits
  on the library, selection = the **written** tracks (on partial
  failures, the successful ones; unchanged after Cancel/Discard) —
  feedback about the user's own action is allowed, jumping to
  uninvolved tracks is not. When a written field participates in the
  active sort, the first written row keeps the same place in the frame
  while the scroll value follows its new sorted position; otherwise the
  existing viewport anchor remains unchanged. Mechanics at the root: the
  reload secures selection via track IDs and scroll via an anchor (track ID + offset,
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
  the track menu (no „Rescan library" — that lives in Preferences →
  Library, NAV-15b). Right-clicking an unselected row selects it first;
  the menu always applies to the visible selection. Shift+F10 / menu key
  open on the keyboard selection.
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
- **CTX-11** [planned] [gtk] — A Queue selection containing only tracks
  keeps using the CTX-1 track-menu builder. As soon as the selection
  contains an episode, the Queue routes to a typed common-action menu:
  „Move to top", positional reorder, and „Remove from queue". Track- or
  podcast-specific actions never apply to a heterogeneous selection.
  <!-- REVIEW: rule proposal -->
- **CTX-12** [active] [gtk] — If a podcast episode cannot currently be
  resolved, "Play next" and "Add to queue" stay visible but disabled.
  Activating either route revalidates every selected episode against the
  current subscription and tombstone state; a stale or mixed-validity
  selection is refused as a whole and is never guessed from a numeric ID.
- **CTX-13** [active] [gtk] — Podcast and YouTube episodes. A single episode
  with a present downloaded file offers "Show in Files" (opens its folder and
  selects the file). A multi-selection where every episode has a file and all
  files share one folder offers "Open Folder" instead and opens only that
  folder. In every other case — nothing downloaded, a recorded path missing
  on disk, or a selection spanning folders — the entry is absent. The
  selection decides, not the rendered window: an episode a collapsed group
  or a Shorts filter takes off screen still counts. Radio never offers it.

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
  ease-out-cubic for atmospheric, non-interactive transitions (waveform
  build and crossfade, plugin settle) · **Spatial** = AdwSpringAnimation with Adw
  default spring parameters for directed navigation, added in code
  starting with the first directed navigation case. Ease-in only for
  what is leaving (toast out, Micro duration); linear only for genuine
  progress bars. Adw-internal widget animations without a duration API
  (OverlaySplitView, NavigationSplitView, ToastOverlay, Banner, Dialog,
  Popover — e.g. the push/pop slides of the settings subpages) count
  as system-given and are exempt from the token requirement.
  A continuous activity indicator that Reprise draws itself (the scan
  chip's gear) is a loop, not a transition: none of the four tokens
  describes it. Its period is named in `ui/motion.rs` alongside them
  (`INDICATOR_SPIN_MS`, 1,200 ms, linear, matching `gtk::Spinner`'s
  pace) and is never written into a CSS string by hand. Linear easing
  is permitted here for the same reason it is permitted for progress
  bars: a rotation has no start and no end to ease.
  **The accent is not its own animation.** Changing its source reloads one
  named color; the carrying widget's existing transition applies. The play
  button, for example, transitions `background-color` and `box-shadow` on
  Micro. A central accent duration would override that widget's interaction
  transition because a CSS property has exactly one transition.
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
  desaturates the waveform fill with draw-local color math, play reverses
  it. The effective accent itself stays untouched. The EQ indicators
  (track list, mini-player) and the radio LIVE point run only during active
  playback; pause freezes them and the idle bar is static — no permanent loop
  without playback.
- **MOT-6** [active] [gtk] — Nothing blocks: the model changes at frame
  0, the animation only illustrates. A second action during a running
  animation jumps to the end state via `AdwAnimation::skip()` and then
  starts the new one; animation slots (track crossfade and icon crossfade)
  call `skip()` instead of silently dropping the old handle.
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
  control** (P-1). The colour named here follows the appearance per
  NPP-17; the strengths stay.
- **NPP-3** [active] [gtk] — Glow instead of full tint: a radial
  gradient of the effective accent color (`@accent_color`) sits in the upper third behind
  the cover and fades down into neutral panel-dark. The reason is
  legibility — the base surface stays neutral so the lyrics contrast
  stays constant over the whole height. Idle shows no glow. Rendered as a gradient, never
  live-blurred.
- **NPP-4** [active] [gtk] — Tab memory only for the session (NAV-5); a
  restart lands on Up Next. Panel *visibility* continues to persist
  across restarts — tab and visibility are separate states.
- **NPP-5** [active] [gtk] — Line hierarchy in the lyrics tab: active
  line 15 px bold white with accent underline (26 × 2.5 px, centered,
  color = `@accent_color`), neighbors stepped white 45% (±1) / 32% (±2) /
  28% (further). All lines centered, 13 px, generous spacing. Whole
  LRC lines, no karaoke word highlighting. The colour named here follows
  the appearance per NPP-17; the strengths stay.
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
  NPP-7. The colour named here follows the appearance per NPP-17; the
  strengths stay.
- **NPP-9** [active] [gtk] — Fallbacks without a dead end: unsynced →
  static scrollable text (white 65%), no highlight, no auto-scroll,
  footer „lyrics · tags"; no lyrics → subtle empty state without a
  search CTA; error → inline retry in the tab. Instrumental gap (> 10 s
  without a line) holds the active line and dims it to 60% instead of
  losing the highlight. The colour named here follows the appearance per
  NPP-17; the strengths stay.
- **NPP-10** [replaced by NPP-13] — A track change is not a place
  change: cover, title block, glow, and tab content crossfade
  **together** in one transition (Standard token, MOT-5), never as a
  slide; the lyrics then start at line 0 and position it per LYR-4.
  `gtk-enable-animations=false` switches hard here too (MOT-7).

## Q. Search

- **SEARCH-1** [replaced by SEARCH-1a] [gtk] — At rest, search occupies only a
  magnifying-glass icon in the header bar. The search field lives in a
  second, collapsed-by-default top bar and is never shown as a
  permanent wide field.
- **SEARCH-1a** [active] [gtk] — At rest, search is only the header lens. The
  field lives in a popover attached to that lens, not in a second top bar.
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
- **SEARCH-2b** [replaced by SEARCH-2c] [gtk] — Clicking the magnifying glass, Ctrl+F,
  or typing directly opens the search bar and focuses the field. It is
  a full-width, opaque strip flush under the header bar with its own
  surface and a bottom divider line; on reveal it structurally
  reserves its own height. The search field is clamp-centered at
  approximately 450 px. The bar slides with the central Standard
  duration (MOT-1/3); for GTK-native revealers their default applies,
  provided it matches the Standard token.
- **SEARCH-2c** [active] [gtk] — The lens and Ctrl+F open the search popover;
  typing into the list no longer opens it. Opening focuses the entry and puts
  the caret at the end of any existing query; closing returns focus to the
  list. The panel is bottom-end under the lens, without an arrow, on the chrome
  surface, and carries the entry plus one muted caption line naming the
  searched scope and the "Esc to close" hint. It reflows nothing (SEARCH-10).
- **SEARCH-3** [active] [gtk] — The magnifying glass is a ToggleButton
  and carries the `:checked` accent style when the search popover is
  open **or** an active non-empty query exists. A query remains visible
  even when the popover is closed: its search chip persists. The
  magnifying glass gets no badge dot; dots remain reserved exclusively
  for the request role (FB-4, P-1).
- **SEARCH-4** [replaced by SEARCH-4a] [gtk] — Esc is two-stage and applies to the
  whole search bar: with text present, the first Esc clears the query,
  leaves the bar open, and keeps the field focused; with an empty
  field, Esc collapses the bar. A query is never made invisible by
  collapsing without its chip carrying it.
- **SEARCH-4a** [active] [gtk] — Escape is one stage: it closes the popover and
  discards the query through the active section's same clear path as the search
  chip's ×, removes the chip and filtering, and returns focus to the list in
  the same key press. The capture-phase handler consumes that key before the
  entry or navigation can. With the popover already closed, an unmodified Esc
  removes a committed search chip; with no query or chip it proceeds unchanged
  so local overlays and navigation keep precedence. Enter still accepts the
  query and only closes the popover.
- **SEARCH-5** [active] [gtk] — Collapsing ends only the input, not the
  filter when the collapse comes from Enter, the lens, Ctrl+F, or autohide.
  Query, results, and search chip are preserved until the user explicitly
  removes them via Esc, chip, or „Clear all".
- **SEARCH-10** [active] [gtk] — Opening and closing search changes no layout.
  The search surface is a popover over the content; the header keeps its
  height, the content area keeps its allocated height, and the player bar stays
  flush with the window's bottom edge in both states. Nothing is inserted into
  the window's vertical layout.
- **SEARCH-11** [active] [gtk] — While the search popover is open, the entry is
  the only place the query is shown: the filter bar renders no search chip,
  even though results and the "N of TOTAL {unit}" count already reflect the
  query. Facet chips stay visible throughout.
- **SEARCH-12** [active] [gtk] — Closing with a non-empty query renders exactly
  one search chip, in the filter bar's search slot ahead of the facet chips. It
  is built once, on close, not from the entry's `changed` signal.
- **SEARCH-13** [active] [gtk] — Closing with an empty or whitespace-only query
  renders no chip and changes nothing.
- **SEARCH-14** [active] [gtk] — Enter accepts: it closes the popover and keeps
  both query and filtering. Escape discards: it clears query, chip and
  filtering through the same section clear path and closes in one press. A
  click outside, Ctrl+F and the lens still dismiss without undoing
  (SEARCH-5/6). The query stays session- and section-scoped: it
  is never written to `podcasts::config::save_filter` or the radio settings
  keys, is dropped on restart, and is never carried between sections
  (SEARCH-8a).
- **SEARCH-15** [active] [gtk] — Reopening the popover while a search chip
  exists hides that chip and pre-fills the entry with its query, with the caret
  at the end. The chip is never duplicated.

## R. New releases

- **NR-1** [replaced by NR-1a] [core] — A library-wide MusicBrainz
  pipeline is the sole source of truth for new releases and later
  artist-news views. Artist MBIDs come first from tags, otherwise from
  a persisted name resolution including negative results; artists are
  prioritized by play count. Per artist, at most five regular albums
  or EPs from the last 90 days remain, plus exclusively future
  singles; incomplete data is never treated as future, secondary types
  stay out.
- **NR-1a** [replaced by NR-27] [core] — A library-wide MusicBrainz pipeline is
  the sole source of truth for new releases and later artist-news
  views. Artist MBIDs come first from tags, otherwise from a persisted
  name resolution including negative results; artists are prioritized
  by play count. Per artist, at most twenty regular albums or EPs from
  the last 90 days remain, plus exclusively future singles; incomplete
  data is never treated as future, secondary types stay out.
- **NR-2** [active] [gtk] — Release covers load lazily via Cover Art
  Archive (`/release-group/{mbid}/front-250`). A missing cover is the
  normal state and immediately shows an equally sized tile made of the
  effective accent color from STYLE-8 plus initials — never a hole or a
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
- **NR-6** [replaced by NR-22] [gtk] — „Fetch now" replaces its refresh icon
  with a spinner during the fetch and otherwise shows the age of the last
  update. Offline or error still show the last cache along with its age.
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
- **NR-10** [replaced by NR-10a] [gtk] — Row hover or focus fades out the status
  chip and fades in the row actions; on leaving, the chip returns.
  Keyboard parity: the row is focusable, focus shows the actions, and
  the buttons are reachable via Tab/Enter.
- **NR-10a** [replaced by NR-36] — Row hover or focus fades in the row
  actions without displacing the status chip; the chip remains visible
  in every state. The row stays focusable and its sensitive action
  buttons remain reachable via Tab/Enter.
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
- **NR-13** [replaced by NR-28] [gtk] — In the full Releases overview, released
  releases already present in the library are marked and offer the
  action „Show in library" (navigate + focus, **no** direct play path).
  The delta popover does not list releases already complete in the
  library.
- **NR-3a** [active] [gtk] — The header trigger opens „Updates" and is
  visible as soon as at least one active feed has entries or a
  first-run state per NR-8. Its badge counts exclusively unseen
  entries of all active, fetch-ready feeds.
- **NR-5b** [replaced by NR-34] — The popover is transient; opening/closing
  never changes the navigation stack. Explicit row actions and the
  jump rows „Show all releases/concerts →" navigate normally and close
  the popover. The popover has no internal subpages; the history
  lives in the full releases view (NR-12a).
- **NR-9a** [replaced by NR-9b] [gtk] — The badge shows the sum of unseen releases
  and concerts, from 10 shown as „9+", and renders nothing at 0.
  Opening stamps the entire delta set of both sections in the current
  scope. Releases fully present in the library are listed and
  stamped, but never count toward the unseen badge.
- **NR-9b** [replaced by NR-9c] [core] [gtk] — The popover shows one visit batch:
  every unseen entry, or, when none is unseen, every entry carrying the
  newest `seen_at` stamp. Opening stamps the complete unseen batch in
  scope, including entries below the visible cap. Rows and section
  counts use the pre-stamp state; the badge uses the post-stamp state.
  Releases already complete in the library do not enter the popover.
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
- **NR-16** [replaced by NR-24] [core] [gtk] — The full releases view is a
  discography-gap catalog for artists currently represented in the
  library. It contains regular albums and EPs regardless of age, but
  never singles or releases already fully present. Individual
  pre-release singles or incomplete album titles do not count as
  ownership; a released release only counts as complete once its
  distinct local track identities cover at least the smallest
  official MusicBrainz edition. Hidden gaps remain recoverable via the
  hidden filter; album and EP catalog rows are not subject to any
  time-based retention.
- **NR-17** [replaced by NR-25] [gtk] — The gap view remains the table `Date ·
  Title · Artist · Type · Status`, sorted by date descending by
  default. Status is `upcoming`, `Missing`, `Incomplete`, or — when
  the length is known — `X of Y tracks`. The permanent filter row now
  offers only sticky Type and Hidden chips; activation opens the
  external release URL, Hidden activates `Show again`. An empty
  default filter confirms „No missing albums or EPs"; the footer
  contains no six-month retention.
- **NR-18** [replaced by NR-26] [core] [gtk] — „Releases" remains a sidebar
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
  end up in the URL. Two independent gates keep this planned, checked
  2026-08-11. First, channel approval: Amazon PartnerNet forbids
  affiliate links „durch oder in Verbindung mit einer kundenseitigen
  Software-Anwendung" without express prior written approval
  (participation requirements, clause 6a), which covers an installable
  desktop app; only a separately approved mobile application is a
  regulated channel. Second, publisher qualification: every ticketing
  network (Impact/Ticketmaster, Awin/Eventim, Ticketcorner,
  Reservix/ADticket) admits publishers only with a self-controlled
  promotional space carrying traffic — an application to
  Ticketmaster/Impact was already rejected on that ground. Reopen only
  with a written channel approval in hand, and prefer the mechanic where
  the provider itself emits the tracked URL (Ticketmaster returns
  affiliate-tagged event URLs once an Impact publisher id sits in the
  developer account) over appending parameters ourselves.
  <!-- REVIEW: rule proposal -->
- **NR-20** [replaced by NR-30] [core] [gtk] — The releases table extends NR-17
  with the `Buy` column. Only when MusicBrainz supplies a genuine
  HTTP(S) relation for the release group to a `/album/…` page on
  `bandcamp.com` or a subdomain does the row show `Bandcamp` there and
  open exactly that URL in the default browser. Lookalike domains,
  artist homepages, guessed search URLs, and all other targets produce
  no purchase button. The direct link is commission-free, contains no
  tracking parameters, and is not labeled as an affiliate link; NR-19
  remains reserved for a later contractually approved monetization.
- **NR-21** [replaced by NR-21a] [gtk] — A failed New Releases fetch leaves every
  cached release and the existing update age untouched. A neutral shared
  banner above a populated view names the failed refresh, what remains
  available, and „Try again"; only a genuinely empty cache uses the shared
  full-area failure state. Both surfaces carry the same collapsed `Details`
  block with Copy, and technical status, host, and exception text appears
  only there. Offline is written from the window's explicit connectivity
  value, dims the remote-action rows, and never overwrites a provider
  failure already on screen; reconnect removes only an offline-authored
  notice. A successful fetch removes the notice silently. NR-22's spinner
  and NR-8's consent and first-fetch loop remain unchanged.
- **NR-22** [replaced by NR-37] [core] [gtk] — „Fetch now" replaces its refresh icon with
  a spinner during the fetch. The Releases footer replaces the stale update
  age with determinate checked/total artist progress for that run, then shows
  the age measured from the completed update again, including a successful run
  with no queued artists. Offline or error still show the last cache and its
  previous age. The shared failure surface remains specified by NR-21.
- **NR-23** [replaced by NR-34] — The delta popover shows at most five
  releases and three concerts without an internal scroller. A section's
  count chip names the full batch size, but appears **only while that
  batch is genuinely unseen**: a batch held over from the last visit
  still renders — looking twice must not empty the popover — yet carries
  no count, because the badge has cleared by then and a header still
  claiming "1 new" would contradict it. A section without a batch loses
  both header and rows; when both are empty, exactly one quiet empty row
  appears. Jump rows remain visible while their module is active, even
  when its delta section is absent. The fetch trigger keeps the update
  age inside its own button, so the header carries one labelled target
  rather than a bare symbolic glyph.
  *Reason:* the count-without-unseen case was found in a screenshot on
  2026-08-04, after the rule's own display tests had passed — the two
  halves of the surface were each self-consistent and only disagreed
  with each other (see also STYLE-1).
- **NR-24** [active] [core] [gtk] — The catalog contains albums, EPs,
  and singles for artists currently in the library; secondary types
  never enter. A release counts as owned, and therefore does not
  appear, when its distinct local track identities cover at least the
  smallest official MusicBrainz edition, or more than half the
  official track count of an already-released release, or, for a
  single, when the library holds any track by that artist under that
  title. Unknown official counts and not-yet-released titles never
  count as owned. Entries sharing artist, normalized title, and
  release date collapse to one row: album ahead of EP ahead of single.
  Catalog rows of all three types are durable and exempt from
  time-based cache retention.
- **NR-25** [replaced by NR-31] [gtk] — The gap view remains the table `Date ·
  Title · Artist · Type · Status`, sorted by date descending by
  default. A fixed cover column leads them, and the `Buy` column of NR-20
  trails them; both follow STYLE-10's fixed-column rule. The named text
  columns are otherwise unchanged in name and order. Its permanent filter
  row carries independent Album, EP,
  and Single toggles — album and EP on by default, single off — a
  persistent window `1 year · 5 years · 10 years · All` defaulting to
  five years, and the Hidden chip. An empty type selection shows every
  type; a release without a parsable date survives every window. The
  count line always names shown and total, the total being the widest
  scope. Activation opens the external release URL, and Hidden
  activates `Show again`. Zero results offer exactly one "Show all"
  step clearing type, window, and hidden together.
- **NR-26** [active] [core] [gtk] — "Releases" remains a sidebar
  location in SMART, before Concerts, visible only with the
  `new_releases` module active. Its badge equals the number of gaps
  visible under the persistent type, window, and hidden filters; 0
  renders no badge.
- **NR-9c** [replaced by NR-29] [core] [gtk] — NR-9b's batch and stamping
  semantics remain unchanged. The delta popover and its badge draw
  from the same persisted filter as the full view. Releases owned
  under NR-24, filtered out by type or window, or already hidden do
  not enter the popover and do not badge. Duplicates collapse there
  the same way. Singles therefore announce themselves exactly when
  their chip is on — there is no separate preference.
- **NR-27** [active] [core] — replaces NR-1a. A library-wide MusicBrainz
  pipeline remains the sole source of truth for the releases catalog and
  the artist-news views. Artist MBIDs come first from tags, otherwise
  from a persisted name resolution including negative results; artists
  are prioritized by play count. What the pipeline *stores* is the
  artist's regular albums, EPs and singles as durable catalog rows
  regardless of age — NR-24 owns that scope, and secondary types stay
  out. The cap of twenty entries per artist belongs to the *news* path
  alone: it bounds the delta candidates the popover and badge read, not
  the catalog. Incomplete data is never treated as future.
  *Reason:* NR-1a described a pipeline that kept only ninety days of
  albums and exclusively future singles. NR-16 had already voided the
  first half, and NR-24 voids the second. The rule survived both as
  `[active]` while being false about persistence — found in review on
  2026-08-07, not by a test, because every individual test agreed with
  the code.
- **NR-28** [active] [gtk] — replaces NR-13. The gap catalog never lists
  a release that counts as owned under NR-24, so it carries no
  „Show in library" action and never renders an `In library` status: a
  row that could offer it is a row the filter has already removed. The
  Updates popover's own row actions are unaffected and stay with NR-29.
  *Reason:* NR-13 promised an action the overview has not had since
  NR-16 excluded complete releases. The status value stays in the model
  because the presence it names is real, and a test pins that the
  filtered view never yields it — a filter change that let owned rows
  through would otherwise reintroduce the dead branch silently.
- **NR-29** [active] [core] [gtk] — The Updates popover and its badge show
  announcements, not the full discography-gap catalog: eligible releases
  have a parsable date in the future or no more than 90 days in the past.
  The full view's persistent age window never widens this announcement
  scope. Its type selection still applies, hidden and NR-24-owned releases
  stay out, and duplicates still collapse. An upcoming single requires an
  exact date; a recent single announces itself only while the Single chip is
  on. NR-9c's visit-batch, cap, stamping, and badge-consistency semantics
  remain unchanged. Consequently, "new" means newly discovered within this
  bounded announcement scope, never merely a newly fetched historical gap.
- **NR-30** [active] [gtk] — replaces NR-20. The trailing release action is a
  visible external-link button for every row with a launchable target. The
  column is hideable and movable; `trailing` describes its default position,
  not a guarantee. A
  genuine tracking-free HTTP(S) Bandcamp `/album/…` relation wins and reads
  `Bandcamp`; otherwise a launchable stored announcement reads `Open`; if that
  value is missing or unsafe, the launchable MusicBrainz release-group fallback
  reads `MusicBrainz`. Every candidate passes the shared external-link guard,
  so non-web schemes never reach the desktop launcher. Bandcamp lookalike
  hosts, homepages, search URLs and tracked URLs remain ordinary announcement
  links rather than receiving the `Bandcamp` label. The cell tooltip exposes
  the exact selected target, and the button's accessible label matches its
  visible label.
- **NR-31** [replaced by NR-33] [gtk] — replaces NR-25. The gap view's fixed trailing
  action column is named `Link` and follows NR-30; its fixed cover column,
  named text columns, sorting, filters, counts, activation semantics and zero
  result recovery remain exactly as specified by NR-25.
- **NR-32** [active] [core] [gtk] — A release the listener deliberately deleted
  does not return as a gap. When "move to trash" or "remove from library"
  removes the last track of an album, album-scope memory hides every matching
  album/EP row; deleting a song writes track-scope memory that hides a matching
  `single` even when another song keeps the album row owned. The catalog also
  applies memory learned before a release was fetched. Missing-files cleanup,
  including tombstone purge, never writes memory. "Show again" restores every
  row hidden by the selected memory scope, while re-acquisition forgets only
  the scope that returned.
- **NR-33** [active] [gtk] — replaces NR-31. The gap view's default columns are
  `Cover · Date · Release · Artist · Type · Status · Link`; this is the default,
  not a fixed order, and every unpinned column is hideable and movable. The
  `Cover` column remains pinned at the leading edge. The second text column is
  named `Release` because its rows are albums, EPs and singles, not songs.
  Sorting, filters, counts, activation semantics and zero-result recovery
  remain exactly as NR-31 specified.
- **NR-34** [active] [gtk] — replaces NR-5b and NR-23. The Updates
  popover shows at most five releases and three concerts without an
  internal scroller, and both feeds use one identical row shape. Each
  section header is the only bridge into its full view: activating it — by
  pointer or by keyboard — closes the popover and navigates, exactly as the
  removed jump rows did. A header stays visible while its module is active
  even when its section is empty, and then shows a quiet empty line. The
  header's count chip names the full batch size and appears only while that
  batch is genuinely unseen. The popover remains transient and has no
  internal subpages.
  Test: `nr_34_an_empty_section_keeps_its_header_and_its_bridge`
  (`ui/updates/popover_tests.rs`).
- **NR-35** [active] [gtk] — replaces CONC-7. The popover's Concerts
  section appears only while the Concerts module is active, shows at most
  three unseen entries of the persistent filter scope, and reaches the full
  view through its header per NR-34. Opening still stamps the entire delta
  set of both sections, and the header badge still sums unseen entries
  across all active, fetch-ready feeds.
  Test: `nr_35_the_concerts_section_header_carries_the_unseen_count`
  (`ui/updates/popover_tests.rs`).
- **NR-36** [active] [gtk] — replaces NR-10a. The row's trailing slot
  holds the status tag and the dismiss button side by side, permanently:
  the button rests at reduced contrast and reaches full contrast on hover
  or focus, and it never displaces the tag. The button is a sibling of the
  row's activation surface, not a child of it, so dismissing a row can
  never open its link. Both are reachable with Tab and activate with Enter
  or Space.
  Test: `nr_36_dismissing_a_row_never_opens_its_link`
  (`ui/updates/popover_tests.rs`).
- **NR-37** [active] [core] [gtk] — replaces NR-22. The Releases view and the
  Updates popover use CONC-15's live-state footer with `releases` and
  `updates` as their unit. There is no "Fetch now" button and no update
  age; the reload icon button carries the manual trigger, and the
  determinate checked/total artist progress appears in the footer's
  progress bar. The popover's footer aggregates both feeds: any running
  fetch makes it "updating", otherwise it reports the older of the two
  timestamps.
  Test: `nr_37_the_popover_footer_reports_the_older_of_both_feeds`
  (`ui/updates/popover_tests.rs`).
- **NR-38** [active] [gtk] — A popover row opens its link on a single
  click anywhere on its activation surface — cover, title, meta or tag —
  and on Enter or Space when focused. Releases follow NR-11's URL
  priority, concerts prefer the offer URL over the event page. The
  provider name appears as the row's tooltip and, hover-free, in CONC-16's
  Source column. A concert row without a launchable target is insensitive
  and says why.
  Test: `nr_38_a_row_opens_the_same_url_its_tooltip_names`
  (`ui/updates/popover_tests.rs`).
- **NR-39** [active] [gtk] — The Releases table's `Status` and `Link`
  columns are ordinary columns in the free band: hideable, movable, visible
  in the column editor, and visible by default. Only the `Cover` column stays
  fixed. Hiding both removes the visible routes for hiding a release and for
  opening its purchase link; the header popover restores either column. A
  layout saved before this change keeps both columns visible, while a saved
  layout that never mentioned them starts without them.
  Test: `nr_39_the_column_editor_lists_status_and_link_and_hides_them`
  (`ui/releases/releases_view_tests.rs`).
- **NR-21a** [active] [gtk] — replaces NR-21. A failed New Releases fetch
  leaves every cached release untouched and reports the failure through
  CONC-15's footer state. A neutral shared banner above a populated view
  names the failed refresh, what remains available, and „Try again"; only
  a genuinely empty cache uses the shared full-area failure state. Both
  surfaces carry the same collapsed `Details` block with Copy, and
  technical status, host, and exception text appears only there. Offline
  is written from the window's explicit connectivity value, dims the
  remote-action rows, and never overwrites a provider failure already on
  screen; reconnect removes only an offline-authored notice. A successful
  fetch removes the notice silently. NR-37's live-state footer and NR-8's
  consent and first-fetch loop remain unchanged.
  Tests: `nr_21a_cached_and_empty_failures_choose_the_shared_surfaces` and
  `nr_21a_going_offline_writing_path_preserves_a_provider_failure`
  (`ui/releases/releases_failure_ui.rs`).

- **NR-39** [active] [gtk] — In the popover a status tag marks the exception,
  not the state. A concert row carries a ticket tag only when its availability
  is `Off sale` or `Unknown`; `On sale` is the expectation for a freshly
  announced show and carries none. The Concerts table is the comparison
  surface and keeps naming all three values in its Tickets cell (CONC-13), so
  no column reads as a missing value. Where both surfaces show a tag they show
  the same word and the same pill: the popover's ticket tones are declared
  exactly as the table's `on-sale`, `off-sale` and `unknown` classes are. The
  Releases chip keeps its own outlined tone and is untouched by this. CONC-12
  remains the only source of the values, and the tag carries no tooltip of its
  own, so the row keeps naming its source per NR-38.
  Test: `nr_39_the_feed_tags_only_the_exception`
  (`ui/updates/concerts_section.rs`, `#[cfg(test)]`).

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
  `online-sources-enabled` (Plugins, `SET-11`): an AND
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
- **NET-2** [replaced by NET-2a] [core] — Updates protect demonstrable prior use:
  existing downloaded covers or portraits activate their module, existing
  library databases keep Online Lyrics, and a previously active
  `artist_news` is carried over as an active New Releases module. Negative
  cache markers do not count as use; fresh installations start with all
  four network modules off.
- **NET-2a** [active] [core] — Replaces `NET-2`. A fresh installation starts
  with the global gate `online-sources-enabled` off: no online sidebar
  entries, no requests, and no startup dialog. An update keeps the gate on
  where prior use is demonstrable: a podcast or YouTube subscription, a radio
  favourite, a downloaded episode, or a populated cover or portrait cache.
  An existing database without any such use starts with the gate off.
  Negative cache markers do not count, and an explicitly stored gate value
  is never overwritten. The per-module grandfathering described by `NET-2`
  remains in force unchanged.
- **NET-4** [active] [gtk] — Discovery without nagging: exactly one
  dismissible banner appears in the Library on the first launch after the
  update: "Reprise can now follow podcasts, YouTube channels, radio and
  concerts — all off by default." with "Review in Preferences" and "Not now".
  Once dismissed or acted on it never appears again; it is never shown when
  the global gate is already on, and is never a modal or a toast. The
  permanent path is Preferences → Plugins. On the first enable the master
  turns on and every source remains off except Radio. This banner is the path
  for an *existing* installation; a fresh install is asked once by the
  first-run wizard of `NET-4a` and never sees the banner. "Never a modal"
  constrains this banner, not that wizard.
- **NET-4a** [active] [gtk] — On a fresh install the first-run wizard asks
  the online-sources question once, in the same dialog as the music folder
  and the Rhythmbox import: with the gate still shut all three sources open
  off, and with the gate already open they open on the stored module states
  instead, so a choice made in Preferences is displayed, never overwritten.
  Both exits — "Skip for Now" and "Set Up Library" — persist the visible
  source selection and close the discovery banner of `NET-4`, so the question
  is never asked twice. No source chosen on a fresh database leaves the gate
  shut and writes no module; clearing every source behind an open gate closes
  it and writes those three modules off. An existing library never sees the
  wizard and keeps the banner.
- **NET-5** [active] [gtk] — Enabling Artwork while the global online-sources
  gate is open and the device is online immediately starts exactly one fresh
  cover pass through the same Preferences transition used by Plugins and the
  sidebar. Repeating the enabled state or completing a library scan while that
  pass is active never replaces it; disabling Artwork stops the pass. An
  offline Artwork enable waits without writing a permanent miss. Online
  Lyrics keeps the immediate start promised by `LYR-6` independently of the
  live-connectivity projection, while its persisted module and global gates
  still apply.
- **NET-6** [active] [gtk] — When an Artwork enable opens the effective gate,
  every already mapped artwork surface immediately rebinds only its retained
  image cells: My Stats uses its current snapshot, and Podcasts, YouTube, and
  Radio keep their rows, selection, expansion, and viewport intact. Hidden
  surfaces stay cold. The refresh is part of the same Preferences transition
  that starts the cover pass; it does not rerun a statistics or source query
  and does not rebuild an entire source page.
- **NET-3** [active] [core] [gtk] — Offline is a state, not an error: no network-backed
  place in the app may treat a missing network connection like an error
  message. The contract covers seven states every network-backed view (feed,
  search, refresh) must know: **cached** (the last successful state stays
  visible along with its age, never replaced by an error), **empty** (never
  fetched successfully yet — a loading/first-run state rather than silent
  emptiness), **queued** (an online action was accepted and is waiting for the
  network), **interrupted** (a running fetch aborts — the cache stays and one
  neutral source banner explains what failed, what still works and how to
  retry), **authentication** (401 or a missing
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
     For Podcasts and YouTube this means a placeholder subscription (the URL
     itself as its title) that the next successful refresh — already
     scheduled independently of this dialog — fills in for real. Radio has no
     such background refresh to defer to, so its URL path instead reaches its
     normal preview step immediately, using only locally detectable facts (a
     `.m3u`/`.pls` suffix, an ICY-probe fallback name) instead of the ICY
     probe itself; the user can re-add the station once online for the real
     probed metadata.
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

  This rule is active because all lettered sub-rules and their GTK
  presentation are wired. It consolidates the shared "a cache is never an
  error" principle that `NR-22`, `NR-8` and `CONC-4b` each already state in
  their own words for their own surface. NR-21 and CONC-11 now specify their
  shared banner, Details, and explicit-connectivity rendering without changing
  the existing fetch, consent, credential, or filter mechanics. `INST-12`
  stated the same principle
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
  already present locally, otherwise `QueuedOffline`.
  `podcasts::download_state::DownloadState::local_availability` is the bridge
  from Podcasts' and YouTube's richer download state into this simple local
  signal. `Connectivity` is an explicitly set state, not an inference from a
  failed request — what it does not know: the reachability of an individual
  provider, authentication, or rate limits; those are request outcomes, not a
  connectivity state. This projection is a wiring foundation: automatically
  running pending actions when the network returns is now built on top of it
  (`NET-3c`, F2). Podcast and YouTube rows consume this projection: missing
  rows remain listed, dimmed, and read "Needs network"; downloaded rows remain
  fully playable.
- **NET-3b** [active] [gtk] — The radio exception: stations always stay listed.
  The context menu shows "Play" (`radio_context_menu::play_menu_label`) when
  `Connectivity::Online` holds or the station is already playing; under
  `Offline` the entry reads "No connection · Retry", and a fresh play attempt
  (`radio_view::try_play_station`) opens no connection while connectivity stays
  offline — no marking pending, no automatic run, because a live stream cannot
  be deferred. Connectivity is an injectable state
  (`RadioView::set_connectivity`), `Online` by default. The main-window
  composition root initializes it, both podcast views, Concerts, New Releases,
  and radio from one `gio::NetworkMonitor` and updates every seam from
  `network-changed`. Returning online removes an offline-only notice; it never
  dismisses a dead-stream or other provider failure, which remains until
  playback succeeds or the user acts on it.
- **NET-3c** [active] [gtk] — Manually requested podcast/YouTube downloads and
  Load more actions made while offline retain their click order in a transient,
  de-duplicated action list. `PodcastsView::set_connectivity` replays that list
  through the ordinary worker on the `Offline` to `Online` transition. An
  action leaves the list only after the worker accepts it; if dispatch is
  refused, that action and everything behind it retain their order for a later
  replay. The reconnect transition also removes an offline-only notice without
  dismissing a provider-specific failure. No persistent download queue or
  phone-sync authority is involved.
  The window-level `gio::NetworkMonitor` is the sole production caller of the
  injectable seams.
- **NET-3d** [active] [core] — One translation layer: every provider and
  transport error is mapped, in core, onto exactly one of five user-facing
  states — *unreachable*, *rate-limited*, *source gone*, *helper outdated*,
  *offline* — and only those are ever rendered. The raw text (HTTP status,
  host, tool output, exception) is reachable only through an explicit detail
  accessor, for the copyable `Details` block and the log, and never through
  `Display`. `404`/`410` on a feed is *source gone* and is the only failure in
  the app that asks the user to act; everything else offers "try again".
  `429` and provider bot-checks are *rate-limited* and retry in the background
  with the shared exponential backoff. Feed and search requests time out at
  10 s from one shared constant, so no view can sit on a spinner longer than
  that without resolving; downloads keep their own longer budget.

  The core projection is rendered by one reusable neutral
  `SourceErrorBanner` and one shared full-area failure state. Both embed the
  same collapsed-by-default monospace `SourceErrorDetails` widget with Copy;
  technical text is reachable there and in logs, never in ordinary labels or
  toasts. Concerts and New Releases use the same two widgets and retain typed
  refresh failures in their Core reports; missing Concerts credentials remain
  a configuration outcome whose action opens its Plugins row, never a network
  retry. The podcast and YouTube refresh loop records a failure immediately,
  then consults
  `retry_delay` before another provider attempt; its bounded per-source attempt
  count resets on success, and exhaustion returns the source to its fixed
  refresh interval. A successful fetch removes the banner silently; cached
  content is never replaced, and three or more failures become one collected
  notice.
- **LYR-1** [active] [core] — Local embedded lyrics and `.lrc` sidecars
  are shown independently of the Online Lyrics module. Sidecars take
  precedence over embedded tags; synchronized text takes precedence over
  plain text. Local lookup itself never changes files; only LYR-7 may create a
  previously absent sidecar after a network result.
- **LYR-2** [active] [gtk] — An interactive online lyrics lookup starts only
  when the Lyrics tab is open, local synchronized text is missing, and the
  Online Lyrics module is switched on. Local plain text is shown immediately
  while the provider chain continues looking for synchronized text.
  What matters is the loaded track, not the playback state: a track
  restored from the session that sits in the player bar shows its lyrics
  without a prior start. The empty state "Play a track to see its lyrics"
  applies only as long as no track is loaded at all.
- **LYR-3** [active] [gtk] — With the Lyrics tab open, text missing, and
  the module switched off, a centered StatusPage shows an icon, the title
  "Online lyrics are disabled", the subtitle "Enable them to load missing
  lyrics automatically", and "Enable in Settings" as a deep link to the
  briefly highlighted Plugins row. This state appears only after the always-on
  local lookup found nothing. A switched-on module with no match shows "No
  lyrics found" instead.
  A distinction from `NET-3`: this rule handles the **switched-off** module
  (the `NET-1a` family — a deliberate user decision, not connectivity) and
  stays `[active]` unchanged for it. The case "module on, but offline" is not
  specified for lyrics today; were it to arise, `NET-3` would govern it, not
  this rule — the two states must not be confused.
- **LYR-5** [active] [core] — Lyrics providers run in this order: embedded
  tags and `.lrc` sidecar locally, LRCLIB exact lookup, conservative LRCLIB
  search, then NetEase. LRCLIB search runs only after a clean exact miss or to
  upgrade an exact plain result. It requires normalized exact title and artist
  plus duration within two seconds; album equality ranks otherwise valid
  candidates, synchronized lyrics outrank plain lyrics, and an equally ranked
  tie is rejected. An exact plain result remains the fallback unless search
  finds one valid synchronized result. Across providers, the first
  synchronized result wins and the first plain result remains the fallback.
  Instrumental stops the chain unless local text was already found. Transport
  and 5xx failures open a per-host circuit breaker after three failures for
  five minutes. LRCLIB `429 Retry-After` blocks requests until its deadline; a
  user retry bypasses cache and the ordinary breaker, but not that server
  deadline. The Lyrics footer names the source and whether the result is
  synchronized.
- **LYR-6** [active] [core] [gtk] — With the Online Lyrics module enabled, a
  cancellable serial background run fills the lyrics cache for the present
  library after the cover batch, after completed library scans, and the moment
  the module is switched on — switching it on starts the run once; a further
  settings change while it is already on never restarts a run in progress, and
  switching it off only stops one. Tracks
  with local lyrics, complete positive cache entries, or fresh negative
  entries are skipped; a cached plain result is retried for synchronized text
  at most once per seven-day negative-TTL window. Provider requests keep at
  least 250 ms between calls to the same host. The shared ScanControls card
  reports checked, cached, and unavailable counts; if every provider breaker
  is open, the run fails immediately while already cached entries remain.
- **LYR-7** [active] [core] — A synchronized lyrics result obtained from a
  network provider is written as standard `.lrc` beside the existing track,
  at the path derived exclusively by replacing the track extension. Plain,
  instrumental, cached, tag, and existing-sidecar results never trigger a
  write. An existing sidecar is never overwritten — on filesystems without
  hard links (FAT, exFAT, NTFS, MTP) the file is created exclusively instead
  of published atomically, which still cannot replace one. Every filesystem
  failure is logged but otherwise silent, so the cache
  result and displayed lyrics remain available. The write is invisible to the
  folder watcher — neither the sidecar nor its temporary file triggers a
  library rescan — and sweeps up temporary files an interrupted earlier write
  abandoned in that directory, matching Reprise's own name pattern and
  nothing else. Device sync copies this
  sidecar under the transferred audio's basename, and removes it with that
  audio only when the library still holds the sidecar it was mirrored from —
  a `.lrc` on the device with no library counterpart, or one beside a file
  the run cannot trace back to the library at all (an unrecorded orphan, a
  podcast or YouTube download), is the user's own and stays. The attachment
  never counts as another transfer, and its failure
  never fails the track transfer.
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
  search popover both ways (open ↔ close). Closing never clears the query: with
  a non-empty query, its chip stays visible and the magnifier stays in the
  `:checked` accent style (FIL-1, SEARCH-3/5).
- **SEARCH-7** [replaced by SEARCH-7a] [gtk] — If the search field along with its
  internal controls loses keyboard focus, the open search bar collapses
  after the current pointer activation completes. A non-empty query
  remains, per SEARCH-3/5, as an active filter along with its chip and
  accent magnifier; a click on the magnifier must not accidentally reopen
  the bar that was closed by that same focus change.
- **SEARCH-7a** [active] [gtk] — The popover autohides. A click outside closes
  it and keeps the query, chip, and accent lens per SEARCH-3/5. The held-pointer
  machinery SEARCH-7 needed is gone with the strip: a popover close inserts and
  removes nothing, so nothing below it can move out from under a click.
- **SEARCH-8** [replaced by SEARCH-8a] — The query belongs to the section it was
  typed in, not to the window. Switching sections swaps the header
  entry's text to that section's own query; it never carries over, and a
  query typed in Podcasts leaves the Music query untouched and vice
  versa. Per section, SEARCH-5 and SEARCH-6 hold unchanged: collapsing
  the bar preserves that section's query, and a query the user
  explicitly removed is never resurrected. The query is **transient**:
  it is not persisted with the section's facet filters (podcast filter
  config, radio settings keys) — a launch never starts inside somebody's
  old search. Where there is no list there is no search: in My Stats and
  device Sync the header lens is insensitive with the tooltip "Nothing
  to filter in {section}", Ctrl+F is a no-op, and the search bar cannot
  be revealed by typing either. The scoped chip each section shows for
  its own query is FIL-1d.
- **SEARCH-8a** [active] [gtk] — A query belongs only to the sidebar
  destination where it was typed. Choosing another sidebar destination drops
  the outgoing query, starts the destination empty and closes the search
  popover, because a person who switches destinations is looking for something
  else. This binds both top-level switches such as Music ↔ Podcasts and track
  destinations such as Library ↔ Recently added ↔ Playlist ↔ Smart ↔ Queue ↔
  Missing. Drilling into an Artist, Album or Genre place by activating a row is
  not a switch: it carries the current query and its chip into that narrower
  context. `RevealTrack` is not such a metadata drill and follows BROWSE-14.
  Back out of such a place restores the complete remembered list
  state, including its query and facets, from the existing navigation history;
  search owns no parallel origin or history state. Closing the popover without
  navigating still preserves the current query per SEARCH-5/SEARCH-6, and an
  explicitly removed query is never resurrected. Facet filters chosen through
  + Filter — including type, window, hidden, unplayed and downloaded — are not
  search and survive sidebar switches untouched. The query itself remains
  transient and is never persisted with those facets. Where there is no list
  there is no search: in My Stats and device Sync the lens is insensitive with
  the tooltip "Nothing to filter in {section}", and Ctrl+F is a no-op — with
  the lens and Ctrl+F the only two ways in since SEARCH-2c, there is no third
  route left to close off. The active query's scoped chip is FIL-1d. This
  replaces the per-section restoration described by FIL-1a's 2026-08-05
  revision; FIL-1a records the corrected boundary.
- **SEARCH-9** [active] [gtk] — **Searching answers at once, and clearing
  answers immediately.** Exactly one wait sits between a keystroke and the
  result — the application's own debounce of 150 ms; the entry's built-in
  `search-delay` is switched off so the two never stack. Emptying the query
  waits not at all: Esc, the chip's ×, "Show all N tracks" and a
  hand-cleared field reload straight away. A query that is set or refined
  places the viewport at the top of its results and moves it no further
  after the model swap — it centers nothing (superseding FIL-9 for search).
  Emptying the query's viewport follows SEARCH-16. (Revised 2026-08-14:
  SEARCH-16 distinguishes an ordinary clear from one after deliberate
  playback during the query.)
- **SEARCH-16** [active] [gtk] — Emptying the query — the chip's ×, Escape,
  clearing the entry by hand, “Show all N tracks”, and “Clear all” alike —
  restores the pre-search anchor, unless the user started playback during that
  query (a deliberate start or an explicit transport, not an automatic
  advance), in which case the loaded track is centred; if that track is absent
  from the cleared list, the pre-search anchor applies again, and if that row
  is gone too, the top. The rule needs a pre-search anchor to have been taken,
  which happens only on the transition from an empty to a non-empty query:
  clearing facets alone, with no query ever typed, stays with FIL-9.
  (Revised 2026-08-19: whichever of those places the viewport takes, it is
  reached in one move — the restoration is never visible as an intermediate
  position first. The eye lands on the destination, it does not follow the
  list there.)
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
- **STYLE-3** [replaced by STYLE-8] — Two accent roles stay separate: the fixed
  app accent (`@accent_color`) denotes durable UI meaning such as
  selection, ratings, active toggles, links, chips, and focus; the dynamic
  playback accent (`@reprise_player_accent`) denotes exclusively the
  running track, such as Play/Pause, waveform, playing row, EQ, glow, and
  the GRID-1 inner ring. An element never mixes the roles.
- **STYLE-4** [replaced by STYLE-1] — Chrome glass is neutral and
  theme-dependent, never tinted by the effective accent. GL/NGL/Vulkan use 24px
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
- **STYLE-8** [active] [gtk] — Reprise has one effective accent color for
  every accent role. Appearance offers exactly two sources between Theme and
  Color Scheme: the Reprise app accent `#4FDBD4`, which is the default, and
  the system accent provided by libadwaita. Changing the source applies
  immediately without restarting the app.
- **STYLE-9** [active] [gtk] — **A column never takes its width from the
  rows that happen to be on screen.** Every column of every table carries an
  explicitly set width; exactly one visible free column per table additionally
  expands and absorbs the leftover width. Hiding that column transfers the
  filler role to the first visible free column in the user's order. A column
  left at the framework default
  (`fixed-width = -1`) measures itself against the cells realized at that
  moment, and `GtkColumnView` recycles its row widgets while scrolling — so
  the whole table shifts sideways as the user scrolls, and the column that
  visibly jumps is usually not the one at fault but the one absorbing what
  its unset neighbors leave over. The width stays the user's: columns remain
  resizable, and a header drag simply writes a new width. Where the set
  widths exceed the window, the table scrolls horizontally instead of
  squeezing its columns (STYLE-6). **Test rule:** measured, not asserted —
  the rule-named test exchanges the rows on screen for markedly wider ones
  and compares the columns' realized widths.
- **STYLE-10** [replaced by STYLE-13] [gtk] — **Columns belong to the user, in every
  table.** A right-click anywhere on a table's header band opens the column
  editor popover: toggle visibility, drag to reorder, reset. Behaviour
  learned in the music library is the same behaviour in Releases, Concerts
  and Radio — a user does not experience a missing editor there as an absent
  feature but as the app forgetting what it taught them. The same editor is
  reachable without a pointer through the primary menu's "Edit column
  layout…", which addresses the table of the active view and is insensitive
  where no table is shown. Order, visibility and header-dragged widths are
  stored per table and survive a restart. A table may declare fixed columns
  — a leading artwork column, a trailing action column on a surface without
  a row context menu — which stay visible, keep their position and never
  appear in the editor; every other column belongs to the user. Exactly one
  visible column is the filler (STYLE-9); when the user hides it, the filler
  role moves to the first visible free column in the table's order. Hiding
  the sorting column changes the sort to the first visible sortable free
  column, ascending, or clears sorting when no such column remains, so an
  active sort and its header indicator never become invisible. **Test
  rule:** one rule-named display test per table, plus a measured filler
  test. Design:
  `docs/superpowers/specs/2026-08-09-table-columns-and-system-dates-design.md`.
- **STYLE-11** [active] [core] [gtk] — **A date looks the same everywhere.**
  Every displayed calendar date follows the system locale's date pattern, with
  a numeric month and an always four-digit year; a locale pattern the app
  cannot render numerically falls back to ISO. Incomplete dates shorten within
  that same pattern instead of switching to a different one. Times show
  minutes and never seconds, on the system's twelve- or twenty-four-hour dial.
  No call site formats dates itself and no surface keeps a month name. A label
  may show fewer fields than the pattern holds — a chart axis whose period is
  already named on screen omits the year — but never a different pattern.
  Machine-readable strings (API query keys, stored timestamps, filenames) and
  relative phrasings that name an interval rather than a day are not dates in
  this sense and are unaffected. **Test rule:** the pattern renderer is
  unit-tested against the day-first, month-first, year-first and suffixed
  conventions; one display test renders the affected surfaces under a pinned
  pattern. Design:
  `docs/superpowers/specs/2026-08-09-table-columns-and-system-dates-design.md`.
- **STYLE-12** [active] [gtk] — **The title bar only carries what is always
  true.** The window header holds actions whose meaning does not change with
  the visible page: the primary menu, search, the panel toggles, global
  status. Anything that belongs to one page — selection presets, bulk
  actions, page-local filters — lives inside that page, near what it acts
  on. A control that appears and disappears with navigation is
  indistinguishable from a permanent one while it is there, and its label
  competes with the page's own vocabulary (the case that prompted this: the
  Library Doctor's `All` preset sat in the title bar directly above the
  review's own `All` filter segment). Views do not push widgets into the
  shared header; if a view seems to need it, the action is in the wrong
  place.
- **STYLE-13** [active] [gtk] — **Columns belong to the user, in every
  table.** A right-click anywhere on a table's header band opens the column
  editor popover: toggle visibility, drag to reorder, reset. Behaviour
  learned in the music library is the same behaviour in Releases, Concerts
  and Radio — a user does not experience a missing editor there as an absent
  feature but as the app forgetting what it taught them. The same editor is
  reachable without a pointer through the primary menu's "Edit column
  layout…", which addresses the table of the active view and is insensitive
  where no table is shown. Order, visibility and header-dragged widths are
  stored per table and survive a restart. A table may declare a fixed leading
  artwork column which stays visible, keeps its position and never appears in
  the editor; every other column belongs to the user. Exactly one visible
  column is the filler (STYLE-9); when the user hides it, the filler role moves
  to the first visible free column in the table's order. Hiding the sorting
  column changes the sort to the first visible sortable free column,
  ascending, or clears sorting when no such column remains, so an active sort
  and its header indicator never become invisible. A table shows exactly one
  sort indicator: the indicator for its current primary column. Every other
  indicator is invisible while its width stays reserved, so headers do not
  jump. A header without a sort field carries no sorter and therefore does not
  appear clickable; a sortable header orders its own column. **Test rule:**
  one rule-named display test per table, plus a measured filler test. Tests:
  `style_13_hiding_the_sorted_column_keeps_a_visible_sort_indicator`
  (`ui/table_columns/registry.rs`),
  `nr_39_the_column_editor_lists_status_and_link_and_hides_them` and
  `two_release_sorts_leave_one_indicator`
  (`ui/releases/releases_view_tests.rs`),
  `conc_16_the_source_column_is_available_but_off_by_default` and
  `only_the_ticket_header_carries_no_sorter`
  (`ui/concerts/concerts_view_tests.rs`),
  `two_concert_sorts_leave_one_indicator`
  (`ui/concerts/concerts_sort_indicator_tests.rs`),
  `the_cover_status_and_link_headers_carry_no_sorter`
  (`ui/releases/releases_view_tests.rs`), and
  `hiding_venue_by_default_moves_the_filler_to_the_artist_column`
  (`ui/concerts/concerts_column_layout.rs`). Design:
  `docs/superpowers/specs/2026-08-09-table-columns-and-system-dates-design.md`.
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
  respective table, sidebar, card, popover, or dialog surface. `.caption`
  plus secondary level counts as small type here and needs the same check as
  hint at normal size.
- **CONTRAST-4** [replaced by CONTRAST-1] — Every active text and every
  active icon in the glass reaches at least 4.5:1 against the worst case
  of its zone: the tint floor composited over the lightest or darkest
  translucent content respectively. Artist, time, search field, and header
  actions are active content; only disabled or purely decorative elements
  are allowed to fall below that.
- **CONTRAST-5** [active] [gtk] — An accent used as a text or glyph foreground
  reaches at least 4.5:1 against the critical surface of the current light or
  dark appearance. The foreground is derived from the effective app or system
  accent by adjusting OKLab lightness; themes and feature CSS never type their
  own accent foreground. Accent-colored surfaces are outside this rule and
  continue to pair with `accent_fg_color`.
- **NAV-10** [replaced by NAV-10a] — The running context stays visible in
  all views with a shared playback-accent marker; on first entry into a
  view it is revealed once, later switches restore NAV-5's remembered
  ID-plus-offset anchor. Explicit "Go to album/artist" always jumps
  deterministically; selection never follows playback.
- **NAV-10a** [replaced by NAV-10b] [gtk] — **Marking and scrolling are separate.**
  Every visible instance of the loaded track carries the same playback
  marker, from one implementation (`ui/playing_marker.rs`) serving every
  surface that lists tracks: the track table, the podcast groups, and the
  YouTube channel detail. The player bar is not such a surface — it shows
  the running track's state through the play/pause button, not through a
  second copy of the list marker. Double-click/Enter on an already-visible
  row does not change the viewport. Play from Stopped as well as explicit
  Previous/Next center the new track without stealing focus or selection —
  except for the one Play that starts a restored session, whose track
  START-3 already selected and centered at startup: that row is placed, so
  starting it only starts the audio. Centring it again would be a second
  visible scroll on a viewport that is already the target.
  Centring moves the viewport over the Standard token rather than
  teleporting it, and yields immediately to anything else that writes the
  scroll position — the user's own scrolling, a model replacement, or
  GTK's own reset. A distance of more than three viewport heights is
  still applied at once, so the first placement after launch stays
  instant (START-1).
  Auto-advance centers only if no scroll movement has occurred for 1.5
  seconds; explicit metadata/reveal navigation always selects, focuses, and
  centers.
- **NAV-10b** [active] [gtk] — **Marking and scrolling are separate.**
  Every visible instance of the loaded track carries the same playback
  marker, from one implementation (`ui/playing_marker.rs`) serving every
  surface that lists playable items: the track table, the podcast groups,
  the YouTube channel detail, and the radio table. Its order is the same in
  every surface: artwork, marker, then the title in the playback-accent
  colour. The signal depends exclusively on playback state and never on
  selection. The player bar is not such a surface — it shows the running
  track's state through the play/pause button, not through a second copy of
  the list marker. Double-click/Enter on an already-visible row does not
  change the viewport. Play from Stopped as well as explicit Previous/Next
  center the new track without stealing focus or selection — except for the
  one Play that starts a restored session, whose track START-3 already
  selected and centered at startup: that row is placed, so starting it only
  starts the audio. Centring it again would be a second visible scroll on a
  viewport that is already the target.
  Centring moves the viewport over the Standard token rather than
  teleporting it, and yields immediately to anything else that writes the
  scroll position — the user's own scrolling, a model replacement, or GTK's
  own reset. A distance of more than three viewport heights is still applied
  at once, so the first placement after launch stays instant (START-1).
  Auto-advance centers only if no scroll movement has occurred for 1.5
  seconds; explicit metadata/reveal navigation always selects, focuses, and
  centers.
- **NAV-19** [active] [gtk] — **Switching source in the sidebar centers the
  running track.** Choosing a different place in the sidebar puts the loaded
  track in the middle of the track table it opens, if that table lists it —
  the same promise SRC-13 already makes for the source lists ("revealed …
  row centered — on entering the view"). It arrives there in one move, not
  through an intermediate position (SEARCH-16). If the new view does not list
  the loaded track, the view's remembered position stands unchanged, and the
  centering never switches view or tab to find a track to show. It changes
  neither focus nor selection: the view keeps the selection it remembers.
  Back and Forward are not source switches and are not covered — BROWSE-2
  keeps restoring exactly what was left behind.
- **QUE-7** [active] [gtk] — Up Next consists of the manual queue plus a
  virtual, named context tail with a count. The tail is not materialized as
  individual rows but only rendered within the visible window; the
  sidebar row "Queue" counts exclusively the manual queue and shows no
  counter at zero. QUE-10 applies the same virtual tail to a direct episode's
  frozen show or channel context without changing the container queue.
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
  of the list. The effective accent is independent of the cover and does
  not change with the track. Newly loaded synchronized lyrics start at line 0 and
  position it per LYR-4. Without animations, cover and content switch hard
  (MOT-7).
- **NPP-14** [active] [gtk] — The switcher carries the three built-in tabs
  **Up Next · Lyrics · Visuals**, in that order, as installed symbolic icons
  only at every panel width and never as painted text. Each icon retains its
  title as its accessible name and tooltip. Built-in tabs always precede any
  extension tabs, which follow in activation order.
- **NPP-15** [active] [gtk] — Disabling an extension never leaves its selected
  page empty. If an extension's tab is open, selection falls back to **Up
  Next** before the page is hidden; disabling an extension while another tab
  is selected leaves that selection unchanged.
- **NPP-16** [planned] [gtk] — Once a fifth reachable panel tab exists, the
  switcher moves extension tabs beyond the fourth into an overflow menu whose
  button carries their count. No overflow control exists while four tabs are
  the only reachable registry state.
- **NPP-17** [active] [gtk] — **The panel's text follows the appearance.**
  No CSS the Now Playing panel installs — the stage, the head, the pill tabs,
  the footer, the Up Next list, the lyrics view and the Song Visuals canvas —
  paints a foreground with a fixed colour literal. Every text role takes an
  appearance-aware colour: `@sidebar_fg_color` where the role is "the panel's
  text at full strength", `@reprise_primary_fg_color` for titles and the active
  state, `@reprise_secondary_fg_color` for artist lines, section headings, the
  footer, tab labels at rest and unsynchronized lyrics. Every one of those
  roles reaches at least 4.5:1 against `@sidebar_bg_color` in **both**
  appearances and in every theme. Surface washes (the pill fill, the canvas
  tint, the cover's inset hairline) take the same foreground so that they
  lighten on a dark panel and darken on a light one. `@reprise_hint_fg_color`
  is not available here: on the sidebar surface it reaches only 3.2:1 light /
  4.3:1 dark.

  This supersedes the word "white" in NPP-2, NPP-5, NPP-8 and NPP-9. Their
  strength ladders, sizes and geometry stay in force exactly as written; only
  the colour those percentages are taken *of* now follows the appearance. The
  dimmed neighbour steps of NPP-5 are a distance cue, not readable content, and
  are exempt from the ratio floor.
- **NPP-18** [active] [gtk] — Body text never lies over artwork. On any surface
  that places cover art near type, every reacting cover-derived layer ends
  above the text block and fades to zero before that boundary. The text remains
  on the flat surface whose contrast is owned by the stylesheet.

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
  in the effective accent, and the subline "N
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
  the cover sits a subtle effective-accent glow. Below it, a ghost row names
  ranks 2–5.
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
- **STATS-13** [replaced by STATS-23] [gtk] — The band card shows the most-listened
  artist as an image hero: the album cover of their most-played track
  fills the card and fades out downward into the card background; if a
  cover is missing, an initials tile stands in its place — never an empty
  surface. Above it the kicker "MOST PLAYED BAND", name, and the line "N
  plays · <duration> · N % of your artist listening"; the duration follows
  the compact format from STATS-11. Below it, ranks 2–5 with a thin bar
  relative to rank 1. Clicking the card or a rank row opens the library
  filtered to the artist (regular history push). If a group combines
  several spellings, the unification hint from STATS-9 is retained.
- **STATS-14** [replaced by STATS-22] [gtk] — The songs card shows the six leading
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
- **STATS-16** [replaced by STATS-20] [gtk] — Under ten plays in the chosen
  period, the data situation is too thin for a trend: instead of the chart, the
  hint "Keep listening — stats grow with you" appears; hero numbers stay real,
  and only cards with data are rendered — never placeholder cards. Without
  any play at all, the empty state from STATS-6/STATS-6c still applies,
  including operable period selection.
- **STATS-20** [active] [gtk] — Replaces STATS-16, whose thin-history hint
  existed only to stand in for the chart STATS-19 removed. A thin period is no
  longer a special case: hero numbers stay real at any play count, and a
  section renders exactly when it has data — a period without bands, without
  songs, or without genres hides that section rather than showing an empty
  frame or a placeholder card. Without any play at all the empty state from
  STATS-6/STATS-6c still applies, including operable period selection.
- **STATS-17** [replaced by STATS-19] [gtk] — My Stats stands fully in place from the
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
- **STATS-18** [active] [gtk] — The loaded track carries the shared playback
  marker in every My Stats song list it appears in — the top-ten card and the
  expanded ranking alike — and that marker tells running from paused. It takes
  the row's rank slot: where every other row prints its number, the loaded one
  shows the equaliser, so the marking costs no width and the row never shifts
  when playback moves. Title and bar take the playback accent with it, and the
  row keeps a tint that hover does not remove. Marking never re-renders either
  list: the expanded state and the scroll position survive a track change,
  exactly as NAV-10b requires of the track table. Pausing is the transport's
  job, not the row's — the marker reports the state, the player bar and Space
  change it.
- **STATS-19** [active] [gtk] — Replaces STATS-17. **The page reads hours →
  bands → songs → genres, and every section owns the full width.** The weekly
  chart is gone: it held the largest area and said the least, and the one
  reading worth keeping is a number in the hero's KPI row ("This week"),
  alongside "Per day" and the period comparison. Pace and best week went with
  the chart. Bands come first as a 2 : 1 : 1 : 1 : 1 row — the leader double
  width with its image bleeding into the card ground, four runners-up beside
  it with a bar relative to the leader; a band without artwork shows its
  initials on a tinted ground, never an empty frame. Songs follow as a full
  top ten in two columns, with an expander that *continues* the ranking from
  rank 11 rather than restating the rows already on screen — it is offered only
  when there is something past the visible ten. Genres close as a strip of
  roughly 90 px: one
  stacked bar plus a single-line legend, with duration and leading artist in
  the segment's tooltip rather than on screen. Activating a band, a segment or
  a legend entry scopes the library to it. My Stats still stands fully in
  place from the first frame: cards, hero number, KPIs, texts and images do
  not fade, slide, or count up. Only horizontal bars move — band ranks 2–5,
  song bars, genre segments — growing in 450 ms from the left with 40 ms
  stagger after a calm start frame of approximately 100 ms, ease-out
  `cubic-bezier(0.16, 1, 0.3, 1)`, genre segments in reading direction. Bars
  below the fold follow the same start. A period change never restarts the
  choreography and only interpolates bar values over 250 ms; everything else
  switches straight to its new final state. With `gtk-enable-animations=false`
  every bar stands immediately at its target.
- **STATS-21** [active] [gtk] — My Stats behaves like the rest of the app under
  the pointer and after a click. **Hover:** every activatable surface of the
  page answers with the same wash — the shared button hover alpha over
  `currentColor` (BTN-1/BTN-4) — and with the pointer cursor. Song rows paint
  it on their own ground; the band cards cannot, because their artwork covers
  that ground, so they wear the identical wash as an overlay above the image
  and below their text, never targetable and never a second, hand-picked tint.
  **Playback:** starting a song from the ranking seeds the queue with that
  ranking — the visible top ten in the sort currently selected — beginning at
  the activated row, exactly as a row activated in the track table plays inside
  the visible view. Previous, Next and Shuffle therefore have a context, and
  the Queue shows what follows instead of one orphaned track. The context's
  origin is My Stats itself: the queue's context tail carries that name and
  jumps back to this page, rather than borrowing the name of one artist out of
  a ranking that spans many.
- **STATS-22** [active] [gtk] — Replaces STATS-14, which still described the
  six-row card from before STATS-19 and let the expander open a second,
  full-width section underneath it. **The ranking is one card.** The songs card
  carries the top ten in two columns (STATS-19): cover, title and artist on two
  lines, a horizontal bar relative to rank 1 in an accent gradient, and the
  metric on the right, which follows the "by plays / by time" toggle beside the
  kicker — that toggle sorts the visible rows and the continuation alike. A row
  plays its track inside the visible ranking (STATS-21), its two labels lead
  into the library, and its context menu offers "Play next", "Add to queue" and
  "Go to album". **The expander grows this card and never opens a second one:**
  "Show more top tracks" reveals ranks 11 and up inside the same card, directly
  below the button that opened them, and the page keeps exactly the sections it
  had — bands, songs, genres. Collapsing returns the card to its ten rows. The
  continuation continues the ranking rather than restating it and is offered
  only when there is something past the ten (STATS-19); its durations use the
  compact format from STATS-11, its titles and artists take link color and
  underline on hover, and the focus ring stays visible. **Its rows answer like
  the ten above them:** rank 11 is a focusable row that carries the pointer
  cursor and the shared hover wash (BTN-1/BTN-4), plays its track on click and
  on Enter or Space, and offers the same "Play next", "Add to queue" and "Go to
  album" on right-click and Shift+F10. A row that sits in the ranking and stays
  inert reads as broken, and in one card it reads that way twice over.
  **The ranking a play hands over is what is on screen:** the visible ten while
  the card is collapsed, those ten plus the revealed ranks while it is open —
  which refines STATS-21's "visible top ten" to follow the card rather than the
  render, so the queue never starts from rows nobody was shown. The clause in
  STATS-10 that let the expanded list stand "as its own full-width section" is
  void with this rule; everything else STATS-10 says about the page — its order,
  its curation, its narrow-window stacking — stands.
- **STATS-23** [active] [gtk] — Replaces STATS-13, which pinned the band card to
  "the album cover of their most-played track" while the code shipped the
  alphabetically first path, and which knew no ranking past rank 5. **The bands
  row is one card and answers like the songs card.** Every band surface — the
  leader's hero, the four runner-up tiles and the continuation rows — resolves
  its image the same way while the Artist portraits module is enabled: the
  cached artist portrait first, then the cover of the artist's most-played album
  in the period, then the next most-played album that actually carries artwork
  (at most three tried), then an initials tile; never an empty surface. A missing
  portrait is fetched only for the ranks on screen. With the module off nothing
  is read from the portrait cache or requested, and the album cover stands. The
  album cover paints as soon as it resolves, and a portrait arriving later
  replaces it. **The "by
  plays / by time" toggle beside the row sorts the whole row** — leader, tiles
  and continuation alike — and the leader's "N % of your artist listening" is
  recomputed for whoever leads under the chosen metric, against the same artist
  population STATS-13 divided by. **"Show more top artists" grows this card and
  never opens a second one:** it reveals ranks 6 to 20 in two columns directly
  below the button, each row carrying its rank, a round portrait, the name, a bar
  relative to rank 1 and the metric the toggle selects. It is offered only when
  there is something past the five on screen, and collapsing returns the card to
  its row. **Its rows answer like the tiles above them:** a focusable target with
  the pointer cursor and the shared hover wash (BTN-1/BTN-4) that opens the
  library filtered to the artist on click and on Enter or Space (regular history
  push). Where a group combines several spellings the unification hint from
  STATS-9 is retained; durations follow the compact format from STATS-11.
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
  `GtkToggleButton` and speak the same `:checked`: a surface in the effective
  accent from STYLE-8 plus a small dot under the icon as a
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

<!-- History: This section previously also carried the rules for Song
     Analysis (Audio Character), Create Similar
     Mix, and Related Artist Discovery. These features were removed (chore
     eda0edaebb); their rules AC-1..AC-6, AC-9, and AC-12..AC-18 are deleted
     here (git preserves the history). What remains are the still-active
     Song Visuals rules. The AC prefix remains as the stable rule ID of the
     visuals rules. -->

- **AC-7** [replaced by AC-10]
- **AC-8** [replaced by AC-11]
- **AC-10** [replaced by AC-19]
- **AC-11** [replaced by AC-27]
- **AC-19** [replaced by AC-20]
- **AC-20** [replaced by AC-21]
- **AC-21** [replaced by AC-22]
- **AC-22** [replaced by AC-23]
- **AC-23** [active] [core] [gtk] — „Song Visuals" is a plugin, switched
  on by default and applicable live. When switched on, the Linux
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
  old impulses are not carried over into newer CAVA frames. A loaded,
  paused scene follows AC-27: only its display projection breathes, while
  the last CAVA values remain intact and the peak caps keep their normal
  independent decay.
  The glow layer behind the columns is never derived from the CAVA
  bands. Auto-sensitivity keeps re-normalizing those, so a quiet sung
  passage climbs to the same band values as a drop and the glow would
  fire on both. Instead a second path measures the same PCM without any
  gain of its own: a 30–150 Hz band, its RMS in true dBFS, and a slow
  baseline of the track's own recent bass. That path produces four
  presentation-only values: the swell over the running baseline
  (`impact`), the same held across a breakdown (`aura`), the attack
  against the band's own recent floor (`kick`), and the absolute held
  level (`pressure`).
  **The two broad neon glows are a stage light driven by `kick`.** A hit
  throws them to full in the same frame and they fall at the render
  clock; they are deliberately not driven by `impact`, which measures
  the swell over a two-second baseline and therefore cannot reach full
  on a limited master — measured across three real tracks it tops out at
  0.85 and clears 0.6 for one percent of a blast-beat track, while
  `kick` reaches 1.00 on all three. The fall belongs to the render clock
  and not to the detector: `kick`'s own release is 70 ms, and at the
  twelve hits per second a blast beat produces, passing it straight
  through would be a strobe. Only pressure sustained across a breakdown
  adds the two brighter inner auras. A bass band that stays quiet in absolute
  terms never glows, however tall the bars grow, and high-frequency
  energy alone never triggers either layer. Both release after the
  impulse instead of flickering, and neither changes CAVA values, peak
  caps, nor bar heights. With animations switched off, the layer holds
  the current frame's value without decay.
  Below the canvas the visual names the analysis it reacts to —
  absolute bass level, baseline, breakdown aura, the attack `kick` the
  stage light runs on, the held `pressure`, and the slow `swell` the
  cover and the panel light breathe on. `impact` is produced but not
  shown: since the glow became a stage light nothing reads it, and this
  strip names what the visual reacts to, not everything the detector
  computes. The
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
  switcher. The labeled canvas takes on the effective accent color from
  STYLE-8; its secondary hue is always a fixed 42-degree shift of that same
  color. Changing the app/system source or the live system accent updates the
  canvas without reading or sampling the cover.

- **AC-24** [active] [gtk] — The reactive light lives on the cover and the
  playhead, nowhere else. The now-playing backdrop, the cover in the panel
  and the cover in the player bar read the `BassPressure.pressure` that
  already reaches the UI and its UI-side slow envelope, `swell` — never
  the CAVA bars, whose auto-sensitivity
  makes a quiet vocal reach the same value as a drop, and never
  `impact`, which answers how loud a whole track is rather than what its
  beat is doing: on a limited master it never leaves its resting value.
  `pressure` carries the backdrop's base brightness and `swell` the slow
  movement of every large surface. **Outside the Visualizer's own canvas,
  nothing reads `kick` at all.** The panel cover used to take the beat
  while that tab was open — round 5's one exception — and it read as the
  cover twitching under its own shadow.
  **The transport controls stay still.** The waveform's **bar heights** never
  move: three attempts to swell them around the playhead were rejected,
  because neighbouring bars cross their pixel boundary at different moments
  and the eye reads that as noise rather than as life. **The seek bar now
  reacts to nothing at all.** Its played part used to take a floor plus what
  the bass added, to keep the progress boundary legible while that boundary
  was a change of *colour*; since both sides carry the same colour and
  progress reads as a step from full opacity to a third of it (SEEK-1), a
  bass term on the played side would eat that very step — at the deep end of
  the axis it would drop the played/coming luminance ratio to about 2.1:1.
  The 3:1 requirement is unchanged and now has to hold at every point of the
  axis, which is what the fixed opacities deliver. The playhead stays a
  one-pixel line with a slim glow beside it, and that glow follows `pressure`,
  not the beat. **Four** attempts put the beat on this surface — a lens twice,
  a radial glow, then a pulsing dot — and all four were rejected on sight, for
  one reason: at five to seven kicks a second, on the surface the user has to
  *aim* at, anything answering per beat reads as flicker rather than as life.
  Reducing its amplitude only makes the flicker quieter, because the rate is
  what does the damage. The glow rests during build-up and during a
  crossfade; the mini player has none. It stays put while the user drags the
  playhead — the playhead is what the hand is holding on to, and it now
  carries the only hard edge in the picture. The afterglow that used to trail
  the played side is gone with the boundary it emphasised.
  Every large surface that does move breathes over seconds on `swell`, a
  UI-side slow envelope of `pressure` crossed with a free-running 5.5 s cycle
  — deliberately not locked to the tempo, because a swell that locks to the
  beat is a tick again, only slower.
  Outside the Visualizer view, `kick` therefore drives only the playhead dot.
  **In the track list only the marker's tempo follows**, in steps: the
  three-bar loop that says "this one is playing" runs slower where the
  track rests and faster where it pushes, driven by `swell` and never by
  `kick`. Nothing else there moves — no light, no wash, no change to a
  bar's height, because that view is a surface for reading and for
  hitting. The steps carry hysteresis and sit on the `ColumnView` as
  ancestor classes, exactly like the paused state, so no cell is touched
  and the viewport cannot move; they are steps rather than a tracked
  rate because GTK restarts a keyframe cycle whenever its duration
  changes.
  The play/pause button is what a
  pointer aims at and, once the running track scrolls out of the list,
  the only place the playback state is read from — a control that
  answers the music moves under the cursor and competes with the state
  it reports. The cover itself never changes brightness either: the
  eye reads luminance change in peripheral vision, so a brightening
  cover pulls attention off the list; it lifts on its shadow, carries a
  one-pixel light seam along its edge, and has a soft disc of the blurred
  artwork turning behind it — one turn a minute. The seam sits
  one pixel outside the artwork, so the cover's footprint grows by exactly
  one pixel on each side; nothing crosses the picture itself. The seam
  uses the effective app or system accent (`@accent_color`), exactly like
  the other accent-bearing UI; it never extracts a separate color from the
  cover. **The turning disc is
  the artwork itself, not colours extracted from it.** A palette sweep was
  built first and measured against a real library: half the covers are
  greyscale or near-black and yield no usable colour at all, and most of
  the rest are monochrome artwork, so the sweep came out as one flat tone
  lying on a backdrop of the same tone. The blurred cover always has
  structure, even in black and white. The lift is
  two cached shadow layers
  whose opacities cross-fade with the composite coverage held constant —
  a linear `1 - swell` pair sums to one and still dips 14 %, which reads
  as a flicker during the cross-fade. Every large effect rests at its
  value for `swell = pressure = 0`; outside playback the slow signal
  decays instead of freezing at the last reading. The "Song Visuals"
  plugin is the deliberate off-switch for the whole layer. With
  `gtk-enable-animations=false` (MOT-7), the brightness remains at the
  bare slow base while the free-running breath stops. **The head of the
  panel looks the same whichever tab is open.** The Visual tab used to hold
  the backdrop at rest and darken the turning disc, on the theory that two
  light languages in one panel fight each other; in use the plain treatment
  was simply better there too. The backdrop and the disc rest when the
  "Song Visuals" plugin is off, when the panel is closed, or when what plays
  is not music (AC-26) — the second because a pinned backdrop runs no tick,
  and without it the paused breath would keep redrawing a widget nobody can
  see.

- **AC-25** [replaced by AC-26]
- **AC-26** [active] [core] [gtk] — **Song Visuals follow the music, not the source.**
  A radio station gets the same treatment as a local track: the Visual tab
  with its audio-reactive bars, and the cover
  bloom and shimmer driven by the session's own artwork — one load, shared
  with the cover it already shows, never a second request for the same image.
  A YouTube episode follows YouTube's own stored category: `Music` receives
  that same treatment, while `News & Politics`, `Education` and the other
  unambiguously spoken categories do not. A category that is absent or
  ambiguous, including `Entertainment` and `Film & Animation`, keeps the
  existing YouTube default and receives Song Visuals; unknown is not guessed
  into speech. Reprise learns and stores the raw category only from the full
  extraction that playback or download already performs — it never makes a
  request solely to classify an episode, and existing unclassified episodes
  remain unchanged until one of those operations naturally extracts them.
  An RSS podcast is speech, not music: speech has no spectrum worth drawing,
  so the bars would flicker around a voice instead of answering it. While an
  episode plays, the whole audio-reactive chain behaves as though the "Song
  Visuals" plugin (AC-23) were off: **the spectrum stops at the source**, the
  Visual tab disappears from the panel, the reactive light of AC-24 rests
  without a cover, and **the bar's bass layers settle instead of freezing at
  their last reading**. The episode's own surfaces are untouched — the seek
  bar, the source image and the playing marker are status, not visualization.
  A user who was on the Visual tab lands on Up Next and stays there, the same
  way an external session displaces the Lyrics tab (`POD-21`). When the
  episode ends, the plugin's own setting decides again. One predicate decides
  the category, `ExternalPlaybackSnapshot::carries_music`; no surface
  re-derives it from the media variant.

- **AC-27** [active] [core] [gtk] — **A loaded live scene breathes through pause and stop.**
  Ongoing playback shows the audio-reactive bars. A real user pause or stop
  while the track remains loaded retains the last live band distribution and
  smoothly blends its display, without a frame-zero jump, into a low resting
  shape: a 10% floor plus 20% of each retained band and an
  eight-percentage-point travelling-wave amplitude. For
  the 0.2-to-0.9 acceptance distribution, every band spans 16 percentage
  points over a full period, and the resulting display range is 6% to 36%.
  Three crests carry different phases across the field, so the motion travels
  instead of pulsing in unison while each field third averages nearly one full
  wave. Throughout the period, the mean of the lower field third stays below
  the mean of the upper field third by at least nine percentage points for that
  acceptance distribution. The wave repeatedly returns near its starting
  values instead of drifting toward zero. The unchanged Bars renderer keeps
  the cyan-to-magenta band gradient. Resume makes the retained live values
  authoritative immediately, before another audio frame arrives.
  Buffering with play intent remains live, and a track boundary still clears
  the previous distribution. A loaded scene without a live distribution uses
  the existing generic resting wave; without a loaded track the surface
  releases to empty and its tick callback stops. The portable engine advances
  from monotonic elapsed time and completes one cycle in six seconds regardless
  of redraw cadence. Android's reduced paused redraw and the desktop's roughly
  30 Hz idle redraw therefore change only sampling smoothness, never wave
  speed. With animations disabled, the resting scene is a static image. This
  is the audio-functional exception for continuous motion permitted in MOT-2.

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

- **DOC-1f** [active] [core] — **Matcher output passes two guard rails before
  it becomes a proposal.** First, `Various Artists` is a structural
  placeholder, not a name: it never replaces a non-empty Artist or Album
  Artist, and it is only a valid proposal for an empty local field when the
  matched release demonstrably contains more than one distinct track artist.
  Without that evidence, no proposal is created. The placeholder is recognized
  exactly through a short curated name list and through the MusicBrainz special
  entity `89ad4ac3-39f7-470e-963a-56509c546377`; localized spellings are
  deliberately excluded because a fuzzy list would eventually match a real
  band. Second, a proposal that reduces specificity — a named value to a
  placeholder, a full title or album name to a truncated one, or a track-tag
  year to an earlier release-group year — receives a confidence cap and never
  starts selected. *Tests:*
  `doc_1f_various_artists_never_overwrites_a_named_album_artist`,
  `doc_1f_a_placeholder_needs_evidence_of_several_track_artists`,
  `doc_1f_the_placeholder_is_recognised_by_name_and_by_mbid`,
  `doc_1f_a_localised_placeholder_spelling_is_not_recognised`,
  `doc_1f_a_single_artist_album_on_a_compilation_produces_no_album_artist_proposal`.
  *Amended 2026-08-08: from DOC-1e onward, evidence that the matched release
  carries several distinct track artists is available; with that evidence the
  placeholder may be proposed for an **empty** field. Without the evidence it
  remains forbidden.*

- **DOC-1e** [active] [core] — **An album is matched as a release, not one
  track at a time.** There is one request per (Album Artist, Album). Candidates
  are scored for equal track count, title overlap, artist-credit similarity,
  and year proximity. Release groups whose secondary type is Compilation,
  DJ-mix, Live, Mixtape, or Remix are demoted unless the local album artist is
  a placeholder or the local album title names that same kind of release — all
  five kinds, not just Live and Remix. A track's Album, Album Artist, and Year proposals
  demonstrably come from the same selected release. *Tests:*
  `doc_1e_a_release_parses_its_secondary_types_and_track_count`,
  `doc_1e_a_release_without_secondary_types_is_not_marked_as_one`,
  `doc_1e_a_release_reports_its_distinct_track_artists`,
  `doc_1e_the_release_search_sends_artist_album_and_track_count`,
  `doc_1e_a_single_artist_release_beats_a_compilation_containing_one_track`,
  `doc_1e_a_locally_tagged_compilation_is_not_demoted`,
  `doc_1e_track_count_equality_outweighs_a_single_title_hit`,
  `doc_1e_the_best_release_is_deterministic_for_equal_scores`,
  `doc_1e_a_single_artist_album_whose_tracks_are_on_a_compilation_produces_no_album_artist_proposal`,
  `doc_1e_an_albums_album_fields_all_carry_the_same_resolved_release_mbid`,
  `doc_1e_the_network_is_asked_once_per_album_not_once_per_track`,
  `doc_1e_an_empty_release_search_keeps_the_directly_resolved_album_fields`,
  `doc_1e_every_demoted_release_kind_can_be_named_by_the_local_album_title`,
  `doc_1e_a_release_nobody_can_recognise_is_no_match_at_all`,
  `doc_1e_a_matching_track_count_with_title_overlap_stays_a_match`.
  *Amended 2026-08-08: a candidate below the minimum score is no match. Being
  the best of a bad field does not select a release; the album fields then have
  nothing to come from and no proposal is made.*
  *Amended 2026-08-08: only a **selected** release speaks for the album fields.
  A release search that comes back empty — no candidate, or a failed request —
  leaves the fields the track resolved on its own untouched.*

- **DOC-1g** [active] [core] — **The complete local pass runs first, followed
  by the network pass.** The phases are reported separately, and a track being
  fingerprinted says so while it runs — that step decodes the audio and can
  hold one track for a minute, which under the remote phase's own wording
  reads as a stall. The network
  makes one request per release rather than per track, caches searches, and
  skips unchanged files. A fingerprint is created only for a track without a
  Recording MBID and without a confidently matched release. *Tests:*
  `doc_1g_the_local_pass_completes_before_the_first_network_request`,
  `doc_1g_a_cached_album_search_makes_no_request_on_the_second_scan`,
  `doc_1g_the_album_search_is_cached_by_normalised_artist_album_and_track_count`,
  `doc_1g_a_track_with_a_recording_mbid_is_never_fingerprinted`,
  `doc_1g_a_confidently_matched_album_is_never_fingerprinted`,
  `doc_1g_a_second_scan_of_an_unchanged_library_reads_no_file`,
  `doc_1g_a_changed_file_is_read_again`,
  `doc_1g_a_skipped_track_keeps_its_previous_proposals`,
  `doc_1g_the_reading_pass_stops_for_a_cancelled_scan`,
  `doc_1g_a_new_track_does_not_send_the_unchanged_ones_back_to_the_reader`,
  `doc_1g_a_multi_disc_album_is_one_release_lookup`,
  `doc_1g_a_title_that_only_looks_like_a_disc_marker_is_left_alone`,
  `doc_1g_a_fingerprinted_track_says_so_while_it_runs`,
  `doc_1g_the_flag_stands_only_for_the_duration_of_the_fingerprint`.
  *Amended 2026-08-08: unchanged is decided per track, so a library that grew
  by one file keeps every other skip. The release decision stays whole: an
  album is reused entirely or resolved entirely.*
  *Amended 2026-08-09: one request per release includes a multi-disc set whose
  discs are tagged with different album titles — a trailing disc marker
  („Album (Disc 1)", „Album [CD2]", „Album, Disc 3") is dropped from the
  grouping key and from the search, so the set is looked up once and compared
  whole. The marker is dropped only at the end of the title, only with a
  number, and never down to an empty title.*

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

- **DOC-2b** [active] [gtk] — **The result page is a summary of three
  meanings, never a write surface.** After a scan the Doctor shows at most
  three blocks: what was already applied without asking, what needs a
  decision, and what has no clear winner. A block whose count is zero is not
  rendered. The applied block names spacing/casing and MusicBrainz IDs
  separately and carries Undo. The decision block names each remaining
  category plus the album count and carries "Review N changes". The conflict
  block says that conflicts are skippable at the end of review. Every number
  counts tag changes that were or will be written; checked and skipped tracks
  remain muted scan facts. Remote categories disappear completely while the
  remote switch is off.

- **DOC-2c** [active] [gtk] — **Running and finished are two different
  pages.** While a job runs the Doctor page shows the job title, the
  „N/M tracks" line, a progress bar, at most the two live counters, and Cancel
  as its only button. No „Scan again", no „Review", no „Undo", no
  checked/skipped footer, no „results are kept" — every one of those describes
  a scan that has ended. The counters are stated as forecasts
  („N will be fixed quietly", „N waiting for you"), because the quiet write
  starts when the scan completes, and a counter that would read zero is not
  shown at all. The result page appears only once the quiet write has finished,
  so the applied block never has to promise a past tense it has not earned.
  Intermediate state is never persisted or applicable; cancel or error restores
  the last completed result and discards the partial one. The locked-controls
  reason is a tooltip on the control it disables, never a line of content.
  *Amended 2026-08-07: this replaces "the same two blocks, counting up" — a
  page that said "Results Found So Far", "27 checked · 0 skipped", "Scan again"
  and "2 fixes to apply" at 1 % progress, i.e. in-progress and final vocabulary
  at once.* *Tests:*
  `doc_2c_the_running_page_offers_cancel_and_nothing_else`,
  `doc_2c_a_zero_counter_is_not_rendered`,
  `doc_2c_the_quiet_write_forecasts_nothing`,
  `doc_2c_a_running_scan_shows_progress_and_no_result_vocabulary`,
  `doc_2c_the_running_page_counters_are_forecasts_from_the_live_summary`,
  `doc_2c_progress_fraction_survives_an_unknown_total`,
  `doc_2c_the_running_page_names_every_scan_phase`,
  `doc_2c_the_sidebar_card_names_every_scan_phase`.

- **DOC-3a** [active] [core] — **Review decides per field, and everything
  reviewable starts selected.** Every concrete track/field change has its own
  selection and arrives preselected. The master checkbox in the column header
  selects every ready row when it is on and clears every row when it is off;
  neither touches a stale or conflicting row. A tie shows
  „N spellings, no clear winner — pick one" with only real candidates and
  their frequencies, with no default. Picking a candidate materializes the
  affected diffs; individual rows stay deselectable; changing the candidate
  recomputes them and preserves manual deselections while the same row remains
  affected. Review order stays stable during the session, in scope order and
  the fixed field sequence Title, Artist, Album, Album Artist, Year, Genre.
  Apply receives an immutable plan of exactly the current selection.
  *Amended 2026-08-08: `All`, `None`, and the immutable Apply plan operate on
  the filtered set described by DOC-9d.*
  *Amended 2026-08-11: the `All`/`None` title-bar buttons became one
  tri-state master checkbox in the review's own column header — see DOC-3c
  and STYLE-12.*

- **DOC-3b** [active] [gtk] — **One column header serves the whole page,
  wide and narrow.** The review page carries exactly one header row — Track,
  Field, Current, Proposed, Source — bound to every row through shared size
  groups; no row repeats a caption. Empty appears as „— empty —" and a
  replaced Current value is struck through. One page-level breakpoint at 640
  px stacks Current → Proposed and hides the shared header; there is no
  per-row breakpoint or horizontal page scroll. Both presentations bind the
  same selection and preserve row focus and stable order. Ellipsized values
  retain full-text tooltips and an accessible description naming track,
  field, current, proposed, and source. „Edit track tags…" opens the existing
  Tag Editor; Save marks affected rows stale and deselects them. *Tests:*
  `doc_3b_breakpoint_changes_layout_without_changing_row_identity`,
  `doc_3b_review_page_virtualizes_rows_without_horizontal_scroll`.

- **DOC-3c** [active] [gtk] — **Every set checkbox says what it covers.**
  The review's column header carries one master checkbox above the row
  checkboxes, and every album header carries a group checkbox. Each is checked
  when every selectable row in its set is selected, mixed when only some are,
  and unchecked when none are; it is insensitive when its set contains nothing
  selectable. An insensitive set checkbox names its reason in visible text,
  not only on hover. Toggling affects exactly the represented rows and never
  touches a stale or conflicting row. The master stays reachable in the narrow
  layout, where the column titles are hidden and it is labelled instead.
  *Tests:* `doc_3c_the_master_check_mirrors_the_visible_selection`,
  `doc_3c_album_header_state_names_the_reason_at_zero`,
  `doc_3c_an_album_with_nothing_selectable_binds_an_insensitive_header_check`.
  *Amended 2026-08-14: the sensitivity and reason contract now covers album
  group checkboxes as well as the page master.*

- **DOC-4a** [replaced by DOC-4c] [core] — **Confidence never chooses for the
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

- **DOC-4c** [active] [core] — **Confidence does not select, and exactly one
  predicate decides initial selection.** A validly and directly resolved MBID
  carries 100%; otherwise the native MusicBrainz or AcoustID value applies.
  Values are never averaged; when multiple sources agree, the lower value
  applies; a lead below ten points is no lead; and anything below 50% starts
  unreviewed. Every reviewable row in the Ready state starts selected except
  when it reduces specificity under DOC-1f or is below 50%. This condition
  exists exactly once in the code; a second copy is a defect.
  `DoctorProposal.preselected` remains the tier marker for the quiet write and
  is never set for remote proposals. An explicit user action — `All`, or a
  selection through the agent adapter — may select any reviewable row because
  it is not initial selection. *Tests:*
  `doc_4c_remote_is_never_preselected`,
  `doc_4c_a_specificity_reducing_proposal_is_capped_and_never_preselected`,
  `doc_4c_a_truncated_title_is_a_specificity_loss`,
  `doc_4c_a_truncated_album_is_a_specificity_loss`,
  `doc_4c_an_earlier_release_group_year_on_a_track_tag_is_a_specificity_loss`,
  `doc_4c_an_earlier_year_of_the_matched_release_is_a_correction`,
  `doc_4c_a_release_group_year_against_a_track_tag_is_capped`,
  `doc_4c_a_tie_choice_that_reduces_specificity_is_capped_and_never_preselected`,
  `doc_4c_a_capped_proposal_does_not_start_selected`,
  `doc_4c_a_row_below_fifty_percent_does_not_start_selected`,
  `doc_4c_never_preselect_survives_a_store_round_trip`,
  `doc_4c_a_capped_row_reaches_the_review_unselected`,
  `doc_4c_confidence_is_release_match_times_field_agreement`,
  `doc_4c_a_partial_release_match_can_never_report_one_hundred`,
  `doc_4c_a_directly_resolved_mbid_still_reports_one_hundred`,
  `doc_4c_the_specificity_cap_still_wins_over_the_joint_score`.
  *Amended 2026-08-08: confidence is additionally a **joint value**: release
  match score multiplied by field agreement. A field from a partially
  matching release can never report 100%. Directly resolved MBIDs remain at
  100%.*

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

- **DOC-5c** [active] [gtk] — **Write jobs don't freeze the UI.** Apply and
  Revert run in the shared sidebar progress card with visible Cancel and the
  same geometry as Scan/Sync. Progress counts tracks, while the Apply button
  counts changes because that is what review decides. Completion names
  updated tracks and collected errors once, never per file. The remote toggle
  and selection remain locked during a write.
  One job is one card, and its label stays readable: the title owns a row with
  the spinner, while percentage and Cancel sit in a second row below it. The
  title and detail both ellipsize without imposing a minimum width on the
  fixed sidebar, rather than making two different jobs look like duplicates
  of each other. *Tests:*
  `doc_5c_the_card_label_stays_whole_at_sidebar_width`,
  `doc_5c_progress_uses_tracks_as_the_primary_currency`,
  `doc_5c_every_count_on_the_write_progress_inflects`.

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

- **DOC-5e** [active] [gtk] — **Every job card uses one dock at the bottom of
  the sidebar.** The dock is outside the scrolling list and gives every job
  type the same height and position; it never overlaps a navigation row. The
  flat card body is one button leading to that job's page. `Cancel` is inside
  the body but never activates navigation. The title receives the remaining
  full first row after the spinner, while percentage and Cancel share the
  second row and the detail line owns any remaining ellipsis. *Tests:*
  `doc_5c_a_visible_job_card_never_overlaps_a_navigation_row`,
  `doc_5e_every_job_card_docks_at_the_same_place_and_height`,
  `doc_5e_the_card_body_activates_and_cancel_does_not`,
  `doc_5e_each_job_card_opens_its_own_job_page`,
  `doc_5e_the_relink_card_label_stays_whole_at_sidebar_width`.

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

- **DOC-7a** [replaced by DOC-7c] [gtk] — **Local checks are an available tool;
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

- **DOC-7b** [replaced by DOC-7c] [gtk] — **The Library Doctor has exactly one
  entry point.** The global ⋮ menu carries one flat "Library Doctor" item with no
  badge or submenu; there is no Preferences surface. Its start page owns
  scope, the remote switch, "Run Scan Now" and the only "Revert Last Cleanup"
  in the app. The summary is a root page in `content_nav`, review is pushed
  onto it, and Back preserves the in-session selection. There is no Doctor
  dialog or Apply confirmation. Scope is not persistent: Whole Library by
  default, Current View suggested from a filtered view, Selection from a
  selection context.

- **DOC-7c** [active] [gtk] — **The Library Doctor opens in the content slot,
  not as a push over `content_nav`.** The right Now Playing pane stays open if
  it was open. The Doctor stack child owns an `adw::NavigationView`: Start or
  Result is its root and Review is pushed inside it, preserving the back
  gesture, title animation, and in-session selection. The shared outer header
  remains the single draggable window-chrome row while the Doctor is visible:
  its Library-only source title, search action, and scan action are hidden, its
  title reads "Library Doctor", and Review places only its "All" and "None"
  actions there. Doctor pages carry no nested header. The provider switch
  exists only on Start, never on Review.

  Library Doctor has no main switch and its local, purely read-only checks can
  be started manually at any time. This is not a network release. The separate
  switch „MusicBrainz/AcoustID suggestions", off by default, shows a short,
  versioned confirmation with the data allowlist from DOC-1c on first
  activation; Cancel leaves it off. The plugin row and the results view bind
  the same persistent switch. Switching off stops future remote requests,
  hides remote rows, and removes their selection; switching on again shows
  existing or newly loaded remote suggestions unreviewed. A missing fingerprint
  capability is visibly explained as „AcoustID unavailable", while Local and
  pure MusicBrainz resolution keep working.

  The global ⋮ menu carries one flat "Library Doctor" item with no badge or
  submenu; there is no Preferences surface. Its start page owns scope, the
  remote switch, "Run Scan Now" and the only "Revert Last Cleanup" in the app.
  There is no Doctor dialog or Apply confirmation. Scope is not persistent:
  Whole Library by default, Current View suggested from a filtered view,
  Selection from a selection context. *Tests:*
  `doc_7c_the_doctor_is_a_content_stack_child_not_a_content_nav_push`,
  `doc_7c_opening_the_doctor_keeps_the_now_playing_pane_open`,
  `doc_7c_the_review_page_is_pushed_inside_the_doctors_own_navigation_view`,
  `doc_7c_the_doctor_uses_the_shared_window_chrome`,
  `doc_7c_the_review_page_carries_no_provider_toggle`,
  `doc_7c_a_second_review_session_replaces_the_first`.
  *Amended 2026-08-15: the search action is the single exception to both
  sentences above. It stays hidden on Start and Result and is revealed on
  Review, which is searchable per DOC-12a, so the clause "Review places only
  its 'All' and 'None' actions there" reads "only its selection actions and
  its search action". The Library-only source title and the scan action stay
  hidden on every Doctor page.*

- **DOC-8a** [active] [gtk] — **The menu holds the verb, the sidebar holds
  the noun.** The global ⋮ menu is the only way to start a scan. While a
  completed scan has unreviewed findings, and only then, a "Library Doctor"
  row appears under ISSUES next to "Missing files", carrying the count of tag
  changes still waiting; it disappears when the scan is acknowledged —
  "Done" on the post-apply page, or "Skip all" in the conflicts section —
  even if not every row was applied. A finished scan never interrupts: on the
  Doctor page it resolves in place, elsewhere it is that sidebar row. Exactly
  one toast fires, "N tags fixed" with Undo, and only for the set that was
  applied without asking. Findings that need review never toast.
  **A change that is already on disk is not a finding.** What the scan's own
  tag-write job wrote leaves the finding set — until an Undo puts the journal
  row back to `reverted`, which makes it a finding again. There is exactly one
  predicate for this (`library_doctor::finding_kind`), asked with the track's
  real staleness, and the sidebar count and the result page both read it. The
  sidebar badge counts only Ready fixes the review page can apply now. Stale
  findings never inflate that number; the review page names their count once in
  an out-of-date notice and offers the same-scope scan path. They
  used to disagree: the count asked with `stale: false` while the pages asked
  with the real value, so a restart turned every fix the quiet job had just
  written into a review row — our own write moves the file's mtime, a moved
  mtime reads as "changed under us", and a stale row falls out of the quiet
  tier. One scan then reported 85 findings in the sidebar and 200 on its own
  page, and offered rows whose current and proposed values were identical.
  *Tests:*
  `doc_8a_the_menu_carries_exactly_one_library_doctor_item_and_no_sync_device`,
  `doc_8a_the_issues_entry_appears_only_with_unreviewed_findings`,
  `doc_8a_quiet_fixes_produce_one_undo_toast_and_review_findings_produce_none`,
  `doc_8a_pending_review_count_excludes_everything_already_written_for_that_scan`,
  `doc_8a_pending_review_count_is_zero_once_the_scan_is_marked_reviewed`,
  `doc_8a_pending_review_count_splits_ready_and_stale_findings`,
  `doc_8a_the_badge_and_unfiltered_review_header_count_the_same_ready_fixes`,
  `doc_8a_conflicts_alone_do_not_produce_a_pending_count`,
  `doc_8a_auto_tier_write_conflict_does_not_produce_a_pending_count`,
  `doc_8a_done_marks_the_scan_reviewed_and_clears_the_sidebar_entry`,
  `doc_8a_skip_all_marks_the_scan_reviewed`.

- **DOC-8b** [active] [core] — **Two tiers, and exactly one predicate
  decides.** A proposal is applied without asking when it is a MusicBrainz
  recording ID, or when it is local and preselected; never when its track is
  stale. Everything else is shown for review, preselected. Recording IDs never
  appear in the review list. The applied set is enqueued as a tag-write job the
  moment the scan completes, before the summary is presented, and is reported
  as done; nothing is written while the scan is still running. There is no
  surface that lists the applied set — it is represented by two counted lines
  and an Undo. The tier is computed by one function used by the core, the GTK
  surface and the agent adapter alike; a second copy of the condition is a
  defect. *Tests:*
  `doc_8b_auto_applied_tier_is_local_preselected_plus_every_recording_mbid`,
  `doc_8b_stale_rows_are_never_auto_applied`,
  `doc_8b_review_tier_preselects_every_ready_row`,
  `doc_8b_recording_mbid_never_reaches_the_review_tier`,
  `doc_8b_all_preset_selects_every_ready_row_and_none_clears_them`,
  `doc_8b_the_tie_path_runs_the_same_selection_predicate`,
  `doc_8b_scan_completion_enqueues_the_auto_applied_job_before_the_summary`,
  `doc_8b_a_scan_with_no_auto_rows_creates_no_job`.
  *Amended 2026-08-08: “Everything else is shown for review, preselected” has
  the two DOC-4c exceptions: specificity-reducing rows and rows below 50% start
  unselected. The single predicate that decides initial selection is
  `starts_selected`.*

- **DOC-8c** [active] [gtk] — **The start page owns the run.** Scope is a
  segmented control with three always-visible options. The remote toggle
  carries its privacy sentence verbatim and retains the versioned consent
  sheet. "Run Scan Now" is the single primary action, with a track count and
  rough duration beside it. Below a separator, and only while a revertible
  cleanup exists, are the last-scan line and the only "Revert Last Cleanup"
  action. *Tests:* `doc_8c_start_page_carries_scope_remote_run_and_the_only_revert`,
  `doc_8c_last_scan_block_is_hidden_without_a_revertible_cleanup`,
  `doc_8c_every_count_on_the_start_page_inflects`.

- **DOC-8d** [active] [gtk] — **The start page is flush left, capped at 620
  pixels, and weighted toward the top.** A small accent icon begins the block;
  nothing is centered. The duration estimate comes from the measured rate of
  the last scan. Before any measurement exists, the estimate names the track
  count and no duration. *Tests:*
  `doc_8d_the_start_column_is_flush_left_and_capped`,
  `doc_8d_the_start_page_icon_falls_back_when_the_theme_lacks_it`,
  `doc_8d_the_estimate_comes_from_the_last_measured_rate`,
  `doc_8d_without_a_measurement_the_estimate_names_no_duration`,
  `doc_8d_the_estimate_accounts_for_the_remote_switch`.

- **DOC-9a** [active] [gtk] — **Three cards, no zero anywhere, and a
  fixed order of emphasis.** The applied, review, and conflict blocks follow
  DOC-2b's order and use written tag changes as their shared unit, including
  album-level proposals expanded over every affected track.
  **Nothing that counts zero is rendered:** not a detail line, not a category
  line, not a block; all three empty is the "Nothing to fix" page, not a page
  of zeros, and the empty state's own sentence drops its skipped clause at
  zero.
  **Each block is a card, not a paragraph:** a 20px leading icon aligned to the
  first line, the heading, the muted detail lines, and the action inline at the
  trailing edge, sized to its content — never a full-width button under the
  text. The review card is the only one that carries emphasis (accent hairline,
  accent icon, primary button); the applied card is a plain surface with `Undo`
  as its only control; the conflicts card is the quietest thing on the page — a
  dashed outline, no fill, a muted icon and no button at all, because it is a
  pointer to the end of the review list.
  **The column is flush left**, capped at 700px, and starts near the top of the
  content area. Nothing on this page is centred.
  **`Undo` is live exactly when there is something to undo:** after the quiet
  write has run and while its cleanup is still on record. A quiet write that
  failed or never ran renders no applied card at all, and a completed revert
  takes the card away again.
  **Every number describes this scan:** counts come from the stored scan, and
  the muted facts line under the title takes the scope and the network flag from
  that scan's own options rather than from the current controls. *Tests:*
  `doc_9a_summary_renders_three_blocks_and_never_a_zero_row`,
  `doc_9a_summary_omits_the_conflicts_block_without_conflicts`,
  `doc_9a_every_visible_count_is_a_written_change_count`,
  `doc_9a_a_scan_with_nothing_to_show_is_the_empty_state`,
  `doc_9a_a_detail_line_that_would_read_zero_is_not_emitted`,
  `doc_9a_review_lines_only_exist_for_classes_with_findings`,
  `doc_9a_a_failed_quiet_write_claims_nothing`,
  `doc_9a_the_applied_block_reports_the_write_not_the_plan`,
  `doc_9a_scan_facts_describe_the_scan_not_the_controls`,
  `doc_9a_scan_facts_stay_silent_about_zero_skipped_tracks`,
  `doc_9a_singular_forms_go_through_ngettext`,
  `doc_9a_every_count_on_the_result_cards_inflects`,
  `doc_9a_the_review_card_leads_with_the_doctors_own_glyph`,
  `doc_9a_the_action_sits_inline_at_the_trailing_edge_top_aligned`,
  `doc_9a_the_conflicts_card_is_the_quietest_and_carries_no_button`,
  `doc_9a_only_the_review_card_carries_the_accent_emphasis`,
  `doc_9a_the_result_page_shows_three_cards_with_the_conflicts_card_quietest`,
  `doc_9a_the_result_column_is_flush_left_and_capped`,
  `doc_9a_a_clean_library_gets_the_empty_state_not_three_empty_blocks`,
  `doc_9a_undo_is_dead_until_there_is_something_to_undo`,
  `doc_9a_the_conflict_count_is_a_property_of_the_scanned_scope`,
  `doc_9a_a_written_fix_does_not_come_back_as_a_finding_after_a_reload`,
  `doc_9a_an_undone_fix_is_a_finding_again`.
  Two defects this rule's own screenshot pass caught, both older than it:
  `emblem-ok-symbolic` is not in the installed Adwaita symbolic set and drew the
  missing-image box — every call site now reads `ui::icons::DONE`, which is one
  name in one place; and the `ISSUES` action row only answered the keyboard,
  because `GtkListBoxRow::activate` does not fire for a single click on a row the
  list box has no source for — it carries a click gesture as well now.

- **DOC-9b** [active] [gtk] — **The review list is grouped by album.** Rows
  appear in scope order under one header per album carrying a group checkbox,
  cover, title, artist and track count, and a change count. An identical
  whole-album change collapses into „All N tracks"; a partial album does not.
  Tracks without an album form one trailing group. The filter bar offers only
  categories the scan produced. Spelling conflicts sit last in an explicitly
  optional container with „Skip all". The virtualized list scrolls under one
  sticky page header and above one sticky footer, without pagination or a
  collapsed remainder row. Album pills, toolbar, and footer count tag changes
  that will be written rather than display rows. *Tests:*
  `doc_9b_rows_group_by_album_in_scope_order`,
  `doc_9b_album_level_change_collapses_into_one_row_over_all_tracks`,
  `doc_9b_tracks_without_an_album_form_one_trailing_group`,
  `doc_9b_group_counts_report_written_changes_not_display_rows`,
  `doc_9b_one_column_header_serves_the_whole_page`,
  `doc_9b_rows_carry_no_caption_labels`,
  `doc_9b_review_groups_render_one_header_per_album`,
  `doc_9b_every_reviewable_row_starts_selected`,
  `doc_9b_the_filter_bar_offers_only_categories_present_in_the_scan`,
  `doc_9b_conflicts_sit_at_the_end_and_skip_all_clears_them`,
  `doc_9b_footer_counts_the_changes_that_will_be_written`,
  `doc_9b_the_album_pill_counts_written_changes_not_display_rows`,
  `doc_9b_the_conflicts_panel_is_the_last_row_of_the_scrolled_list`,
  `doc_9b_the_conflicts_panel_covers_no_row`,
  `doc_9b_the_first_row_carries_its_album_header`,
  `doc_9b_a_fully_deselected_album_says_none_selected`,
  `doc_3c_album_header_state_names_the_reason_at_zero`,
  `doc_3c_an_album_with_nothing_selectable_binds_an_insensitive_header_check`,
  `doc_9b_a_stale_row_names_its_reason_where_the_click_happens`,
  `doc_9b_activating_an_unselectable_row_selects_nothing`,
  `doc_9b_every_section_boundary_binds_a_non_empty_header`,
  `doc_9b_an_album_wide_change_renders_all_n_tracks_italic_and_muted`,
  `doc_9b_a_recycled_row_loses_the_italic_style_again`,
  `doc_9b_stale_notice_is_unfiltered_and_hidden_at_zero`,
  `doc_9b_the_unfiltered_footer_names_selection_and_ready_inventory`,
  `doc_9b_every_count_on_the_review_page_inflects`.
  *Amended 2026-08-08: “every reviewable row starts selected” means every Ready
  row except the two cases excluded by DOC-4c.*
  *Amended 2026-08-08: the conflict panel is the final element **inside** the
  scrolling list, not a block above it; an album-wide change renders "All N
  tracks" in italic muted text; and every row, including the first, appears
  beneath an album header.*
  *Amended 2026-08-14: an album header's change count is its written-change
  inventory; when none is selectable, the reason appears next to that count.
  A refused row names its reason in the Source cell and accessible description;
  activating it changes nothing and performs no refresh, while the page banner
  remains the aggregate explanation.*

- **DOC-9d** [active] [gtk] — **An active filter limits everything.** Apply
  writes only the filtered set and counts that set in its label. `All` and
  `None` operate only on that set. The footer states the scope, for example
  "27 of 390 · filtered by Year".
  **Two references, split cleanly:** the header states what is on screen — the
  filtered inventory and the albums it covers — and does not move when a row is
  unchecked; the footer and the Apply button state the selection inside that
  inventory, and both come out of one selection state, so they can never
  disagree with each other. Filtered and everything selected, all three name the
  same number; unchecking makes the footer and the button fall below the header,
  which is what unchecking means. If an active search or category filter leaves
  no visible rows, the page uses a neutral nothing-matches state that names the
  active filters and offers to clear them; the success state appears only when
  no filter is active. *Tests:*
  `doc_9d_the_filter_scope_line_names_shown_total_and_filter`,
  `doc_9d_a_filtered_apply_writes_only_the_filtered_set`,
  `doc_9d_all_and_none_operate_on_the_filtered_set`,
  `doc_9d_the_footer_states_the_scope_of_the_filter`,
  `doc_9d_the_header_counts_the_inventory_while_the_footer_counts_the_selection`,
  `doc_9d_a_filtered_header_counts_only_the_filtered_rows`,
  `doc_9d_the_selection_counts_recompute_from_one_selection_state`.
  *Amended 2026-08-09: the header used to be described as one more number off
  the selection state, and rendered as one — "1 changes · 2 albums" after
  unchecking, where the changes followed the checkbox and the albums did not.*

- **DOC-9c** [active] [gtk] — **After the write, and after a clean scan, the
  Doctor says so on its own page.** Post-apply names updated tracks, written
  changes, albums and conflicts left open, offers "Undo everything from this
  scan" beside "Done", and names the quiet fixes included by Undo. Its counts
  come from the write report, never the frozen plan. Done acknowledges the
  whole scan. A clean scan is a distinct "Nothing to fix" page with checked
  and skipped counts and "Scan again", never the pre-scan state. *Tests:*
  `doc_9c_post_apply_names_the_quiet_fixes_and_the_unresolved_conflicts`,
  `doc_9c_post_apply_reports_the_write_report_not_the_plan`,
  `doc_9c_nothing_to_fix_is_distinct_from_the_pre_scan_state`.

- **DOC-10a** [active] [core] — **Undo is one bracket per scan.** The job
  applied without asking and the reviewed job of the same scan revert together
  as one operation with one progress count. A failing field does not stop the
  remaining fields or the remaining job; a cancel does stop the next job. A
  partially reverted cleanup stays offered, so a second Undo retries exactly
  the remainder, and a fully reverted scan is no longer offered. *Tests:*
  `doc_10a_undo_reverts_the_quiet_and_the_reviewed_job_of_one_scan`,
  `doc_10a_undo_works_when_only_the_quiet_job_exists`,
  `doc_10a_partial_revert_leaves_the_cleanup_available_for_a_second_attempt`,
  `doc_10a_prepare_failure_returns_the_completed_partial_report`,
  `doc_10a_cancel_between_jobs_does_not_start_the_remaining_job`,
  `doc_10a_a_fully_reverted_scan_is_no_longer_offered`.

- **DOC-10b** [active] [core] — **One tag-write slot, enforced in the
  database.** A tag-write job of any kind may only be created while no other
  job is prepared or running; the check and the insert share one transaction.
  The refusal is caller-visible on both surfaces — a toast in the app, a
  retryable tool error for an agent — and never an internal error. A job left
  behind by a crashed process is finalized by the existing recovery path and
  holds no slot. *Tests:*
  `doc_10b_a_second_tag_write_job_is_refused_while_one_is_prepared_or_running`,
  `doc_10b_a_finalized_interrupted_job_does_not_hold_the_lock`,
  `doc_10b_tag_editor_and_doctor_share_one_lock`,
  `doc_10b_gui_sees_the_same_refusal_while_an_mcp_job_runs`,
  `doc_10b_mcp_refuses_while_a_gui_job_holds_the_lock`.

- **DOC-10c** [active] [core] — **An upgrade never inherits a decision.** A
  scan stored under the previous rules is not reinterpreted and nothing from
  it is applied; the stored result pointer is cleared on upgrade and the
  Doctor opens on its start page. The undo journal is untouched, so a cleanup
  applied before the upgrade stays revertible. *Test:*
  `doc_10c_upgrade_clears_the_stored_scan_pointer_and_keeps_the_cleanup_revertible`.

- **DOC-11a** [active] [core] — **The agent adapter finds and reports; it
  writes only when asked.** `music_scan_tags` is read-only by default: the
  automatic application of unambiguous changes happens only with an explicit
  `apply_safe`. Every mutation — `apply_safe`, and every
  `music_apply_tags` action — requires the `tags:write` capability, which is
  off by default, granted at startup and revocable live. Responses carry no
  file paths, library roots or credentials, and every reported change count
  uses the same per-track-and-field unit as the app. Both surfaces use the
  same job queue and scan id, so an agent scan produces the app's sidebar
  entry and app Undo reverts an agent apply. *Tests:*
  `doc_11a_scan_tags_does_not_write_without_apply_safe`,
  `doc_11a_apply_safe_requires_the_tags_write_capability`,
  `doc_11a_apply_tags_requires_the_tags_write_capability`,
  `doc_11a_review_tags_groups_by_album_and_filters_by_category`,
  `doc_11a_review_tags_counts_written_changes_per_album`,
  `doc_11a_doctor_responses_carry_no_file_paths`.

- **DOC-6c** [planned] [manual] — **The visible sign-off covers every Doctor
  state.** On a real GNOME display, the start page, sidebar entry, grouped
  review with one header, post-apply and nothing-to-fix pages, wide and narrow
  geometry, virtualization, strikethrough, source states, focus indicators,
  one-time network confirmation and the shared scan/apply/revert progress card
  are checked. No text is truncated, no column forces horizontal scrolling,
  and the interface remains operable during real file jobs.

- **DOC-12a** [active] [gtk] — **The review list is searchable, and the search
  is a filter like any other.** Ctrl+F and the header lens open the shared
  search popover on the Doctor's Review page and nowhere else in the Doctor;
  the query matches track, album and artist, case-insensitively and mid-word,
  and never the normalized album key, the field caption, or the current and
  proposed values. Search and the category tabs compose: a row must satisfy
  both. The result is an active filter in the sense of DOC-9d — Apply writes
  only the matching set, `All` and `None` operate only on it, the header counts
  it, and the footer states the scope. A row the query hides keeps its
  selection and stays out of the plan until the query is removed, exactly as
  under a category filter; there is no extra confirmation and no search-only
  label. A query, category, or both with no matches shows a neutral filtered
  state, naming every active filter and the number of fixes it is hiding and
  offering to clear the filters — never the nothing-to-review success page.
  Leaving Review drops the query, entry and chip in one step, as a section
  switch does (SEARCH-8a).
  *Tests:* `doc_12a_the_review_search_matches_track_album_and_artist`,
  `doc_12a_search_and_category_compose_as_an_intersection`,
  `doc_12a_apply_writes_only_the_searched_set`,
  `doc_12a_a_query_with_no_matches_shows_its_own_state`,
  `doc_12a_a_category_with_no_matches_shows_its_own_state`,
  `doc_12a_leaving_review_drops_the_scope_and_the_query`.
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
  centers the anchor track; Back restores the point of origin. For a playing
  podcast or YouTube item, the title and Ctrl+L reveal the episode while the
  channel line and cover reveal its channel. For radio, all three surfaces
  reveal the station row. These jumps always land in the source list, never a
  detail page; an open channel detail page closes for the jump.

- **BROWSE-5** [replaced by BROWSE-12] — Session restore previously retained
  sorting and playback origin but always opened the library root.

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

- **BROWSE-8** [replaced by BROWSE-11] — Catalog deletion originally kept
  the loaded track playing until a later transport change.

- **BROWSE-9** [active] [gtk] — **The date added is a normal library
  column.** "Added" is selectable in the column editor, movable,
  width-persistable, and sortable by `added_at`. The time is rendered per
  STYLE-11 and the column is hidden by default; existing layouts also receive the new column
  hidden when normalized, without losing their stored order or
  visibility.

- **BROWSE-10** [active] [core] — **Conflicting embedded album covers
  are canonicalized.** When cover download is enabled, the library scan
  detects different embedded images for the same normalized album
  artist and album name and fetches exactly one shared cache cover. This
  then wins for all tracks of the album identity; the music files remain
  unchanged. With the module disabled or the network unavailable, purely
  local resolution remains in effect.

- **BROWSE-11** [active] [gtk] — **A user deletion advances the loaded
  track.** When an explicit "Remove from library" or "Move to Trash"
  action successfully deletes the loaded track, playback immediately uses
  the normal automatic queue advance: Play Next entries win, unavailable
  entries are skipped, then the frozen playback snapshot continues. Repeat
  One cannot restart the deleted track; if nothing survives, playback stops.
  Only IDs reported as successfully deleted can trigger this. Background
  watcher, scanner, startup, and maintenance purges continue to follow
  PLAY-5a/PLAY-5b and never interrupt the open audio stream, while all future
  occurrences of deleted IDs disappear from Queue and Up Next. A track link
  to an ID that no longer exists stays at its point of origin and explains
  this via toast; album and artist links continue to open the snapshot scope,
  but without a phantom anchor. After a deletion series, surviving selected
  rows remain focused; otherwise selection and focus fall to the next row, to
  the previous row at the end of the list, and to the stable content
  container when the list is empty.

- **BROWSE-12** [active] [core] [gtk] — **The last browser destination is a
  session value.** Its structured place owns source, scope, search, facets,
  sorting, stable anchor, selection, and content focus and survives a normal
  restart. Stable source roots such as Podcasts, YouTube, Radio, Releases,
  Concerts, and My Stats remain resolvable without a track collection; stale
  database-backed places fall back to the remembered Music root. Back/Forward
  history, utility overlays, and raw widget focus remain process-local.

- **BROWSE-13** [active] [gtk] — **A track-list cover is its album link.**
  When the currently bound track has a nonblank album, its cover exposes the
  shared link hover, pointer cursor, Link role, album-named accessible label,
  keyboard focus, and Enter activation; a plain primary click opens that
  track's album through the same central navigation intent as every other
  album link. Control-click and Shift-click remain row-selection gestures and
  are neither claimed nor activated by the link. A row without an unambiguous
  album target exposes its cover as an image, with none of the link affordance
  or activation. Rebinding or unbinding a recycled cell clears the previous
  target. The unchanged context-menu route continues to follow CTX-4.

- **BROWSE-14** [active] [core] — **Revealing a track removes restrictions,
  not context.** `RevealTrack` keeps the origin's collection and sorting while
  dropping its text query and browse facets, even when the track would have
  survived them, then selects, focuses, and anchors that track. When anything
  was dropped, the narrowed origin enters Back history unchanged; an already
  unrestricted reveal remains an in-place replacement and adds no duplicate
  history entry. Album, Artist, and Genre drills continue carrying the query
  under SEARCH-8a.

- **COVER-1** [active] [core] — After a downloaded album cover has been
  published in the XDG cache, Reprise also writes `cover.<ext>` into every
  existing directory represented by the live track paths of that album, but
  only into a directory that holds no other album: in a flat library or a
  compilation dump, where one folder answers for several albums, nothing is
  written. The extension comes from the validated image bytes. Reprise only
  fills gaps, so an album that already has artwork gets no file: if any
  canonical folder image (`cover`, `folder`, `front`, or `album` with a
  supported image extension) exists there, or any track in that directory
  carries an embedded picture — including the differing-embedded-art case of
  `BROWSE-10`, which is a cache canonicalization, not a missing cover —
  Reprise writes nothing and never overwrites it. On filesystems without
  hard links (FAT, exFAT, NTFS, MTP)
  the file is created exclusively instead of published atomically, which
  still cannot replace one. Every filesystem failure is logged
  but otherwise silent, so the cached download remains successful. The write
  is invisible to the folder watcher — neither the cover nor its temporary
  file triggers a library rescan — and sweeps up temporary files an
  interrupted earlier write abandoned in that directory, matching Reprise's
  own name pattern and nothing else. Covers for release groups without a
  local album remain cache-only.

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
playback effect. The GTK frontend no longer commissions one — the
conversion surface and the "Experimental features" toggle that gated it
are gone; the CLI and MCP frontends still create instrumentals. What
remains here is how the GTK frontend *marks* and *filters* the results:
the AI badge (INST-10) and "Hide AI music" (FIL-7), both always
available. The player plays only finished files.

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
  storage folder. The flag is the badge's only input — no settings gate
  sits in front of it, so a track another frontend produced is marked
  the moment it appears. (Decision 13/14)
- **INST-11** [replaced by nothing — the gate lost its subject when the instrumental surface left the GTK frontend; the ID stays as a signpost per the append-only contract] — **Master gate:** the entire instrumental
  UI — context menu entry, conversion view, AI badges, "Hide AI music"
  filter (FIL-7) — is **hidden as long as the "Experimental features"
  toggle is off**. The toggle is a persisted setting; its state alone
  decides visibility. (Decision 11) The surface the gate protected was
  removed; the two survivors it still covered — the AI badge (INST-10)
  and "Hide AI music" (FIL-7) — mark and filter tracks the CLI/MCP
  frontends produce and are worth showing unconditionally, so the
  toggle and its Experimental preferences page were removed with it.
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
     AC is the stable rule prefix of Song Visuals (section X), so
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
  on the tint); below it the mini waveform (46 equal-width bars, coloured
  exactly as the full bar is under SEEK-1 — the track's own averaged
  colour on both sides of the playhead, or the playback accent where no
  curve exists yet; click = seek, drag = scrub); play/pause 38 px in the
  accent. The mini bar carries no legend and no playhead glow: it is small
  enough that a second explanation would be larger than the thing explained. No volume, prev, or
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
- **MINI-6** [active] [gtk] — **A file-open cold start goes straight to
  the song.** When a desktop audio-file association starts Reprise and
  at least one requested file is a playable library track, the first
  window is the mini player. A request with no playable library track,
  any playlist, first-run setup, or unavailable playback opens the
  Library instead. This automatic transition never writes the saved
  window mode, and a request forwarded to an already-running Reprise
  never changes its mode.

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
  Active, it shows "X of Y concerts" and "Clear all". Radius is active only
  when a location makes it meaningful. Without one, the header shows a
  dashed `{radius} km · off` chip that opens Preferences Location but neither
  filters nor counts as active; the count is the plain total. A banner above
  the independent failure banner says that all concerts worldwide are shown,
  Distance is absent from the table, column editor, and sorting, and Venue
  absorbs its width. Automatic hiding never changes the user's stored column
  choice; setting a location restores the exact prior visibility and sort.
  With location, the active chip reads `{city} · {radius} km` (or just the
  radius for a blank city name), Distance returns according to that stored
  choice, and the banner disappears. The city comes from Nominatim's `address`
  object in `city` → `town` → `village` → `municipality` order, falling back to
  the first comma-delimited `display_name` segment
  (`conc_2_geocode_uses_the_localized_city_and_country`,
  `conc_2_geocode_falls_back_to_the_first_display_name_segment`). Reprise asks
  Nominatim for that address in the active UI language
  (`conc_2_geocode_url_uses_accept_language_http_syntax`).
- **CONC-3** [replaced by CONC-13] — Double-click/Enter on a row and the
  ticket cell open the same external target: offer URL, otherwise the
  event page. Without either, the cell is empty and activation is a
  no-op with a tooltip. There is no play path.
- **CONC-4** [replaced by CONC-4a] — Original state contract without
  explicit live re-evaluation after changes to Concerts settings.
- **CONC-4a** [replaced by CONC-4b] — Original state contract with
  credential input hint and Preferences deep link.
- **CONC-4b** [replaced by CONC-4c] — Without a credential, Concerts neutrally
  shows "No concert data yet" with no action; the Concerts section in
  the Updates popover is not visible. There is no credential input hint
  and no Preferences deep link. Changes to credentials, location,
  default radius, time range, and similar settings immediately
  re-evaluate the already-open view, its sidebar count, and the Updates
  popover. Never fetched offers exactly "Fetch now"; zero hits with
  filters offers exactly "Show all". Offline or error leaves the cache
  and "Updated X ago" visible. CONC-11 specifies the shared failure surface;
  credential and filter behaviour stay Concerts' own and remain `[active]`
  unchanged.
- **CONC-5** [replaced by CONC-5a] — Original worker contract with
  view-open staleness, due check, and "Fetch now" as the only network
  triggers.
- **CONC-5a** [replaced by CONC-5b] — Network runs exclusively in the worker
  or `one_shot_task`. Triggers are view-open staleness (24 h plus
  jitter), the hourly due check, "Fetch now", and an explicitly
  confirmed credential check. All Concerts requests share the 1-req/s
  limiter. Track changes, navigation, and individual credential
  keystrokes only read or write locally; fetch results are applied per
  MOT-2 without a fade-in animation.
- **CONC-6** [active] [gtk] — Similar rows carry a dimmed "similar to
  {seed}" and disappear with "Library artists only". The source pill is
  visible as soon as Similar is enabled or similar rows exist.
- **CONC-7** [replaced by NR-35] — The Updates popover shows the Concerts
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
- **CONC-10** [replaced by CONC-14] — Every Concerts row shares a common
  vertical center. The artist stands as a single-line group on the same
  baseline as date, location, venue, distance, and ticket; an optional
  "similar to …" caption expands and centers the artist group as a
  unit, instead of pinning the artist to the top edge of the row.
- **CONC-11** [replaced by CONC-11a] — A failed Concerts fetch leaves every cached
  event and „Updated X ago" untouched. A neutral shared banner above a
  populated view names the failed refresh, what remains available, and the
  next action; only a genuinely empty cache uses the shared full-area failure
  state. Both surfaces carry the same collapsed `Details` block with Copy, and
  technical status, host, and exception text appears only there. Offline is
  written from the window's explicit connectivity value, dims the remote-action
  rows, and never overwrites a provider or configuration failure already on
  screen; reconnect removes only an offline-authored notice. A successful
  fetch removes the notice silently. Missing credentials are a configuration
  outcome with „Open Preferences" targeting the Concerts Plugins row, never
  „Try again"; CONC-4b's ordinary no-credential empty state remains neutral
  with no action.

- **CONC-12** [active] [core] — Ticket availability is what the source
  says, never an inference. Ticketmaster's `dates.status.code` maps
  `onsale` → On sale, `offsale` → Off sale, and everything else
  (`cancelled`, `postponed`, `rescheduled`, missing) → Unknown; Bandsintown
  maps an `available` offer → On sale, offers without an available one →
  Off sale, and a missing or empty offers list → Unknown. The app never
  renders "Sold out": no provider distinguishes a sold-out show from a
  pre-sale that has not opened.
  Test: `conc_12_offsale_never_becomes_sold_out`
  (`crates/reprise-core/src/concerts/availability.rs`, `#[cfg(test)]`).

- **CONC-13** [active] [gtk] — replaces CONC-3. Double-click, Enter or
  Space on a concert row opens its external target: the offer URL,
  otherwise the event page. The Tickets cell is a status label and is never
  an activation surface. A row without a launchable target does not
  activate, keeps its ordinary appearance, and carries the same sentence in
  its tooltip and its accessible description. There is no play path.
  Test: `conc_13_a_row_without_a_target_does_not_activate`
  (`ui/concerts/concerts_view_tests.rs`).

- **CONC-14** [active] [gtk] — replaces CONC-10. Every concert row is a
  single line and never wraps. Every cell ellipsises at its end. The
  optional dimmed "similar to {seed}" sits on the artist's own line,
  directly after the name, and ellipsises before the name does — losing the
  provenance is acceptable, losing the artist is not. Rows keep a common
  vertical center.
  Test: `conc_14_the_similar_caption_shrinks_before_the_artist`
  (`ui/concerts/concerts_view_tests.rs`).

- **CONC-15** [active] [gtk] — The feed footer states the live state, not
  an age, and it is the only place any of these views shows a timestamp.
  Its nine states are: loaded in this visit, served from cache, updating
  (with determinate progress), failed, offline, never loaded, no
  credentials, online sources off, module off (footer hidden). A loaded or
  cached state carries the accent dot and a reload button; an updating
  state replaces the button with the progress bar; the two configuration
  states offer no button. "Up to date" never appears while a fetch is
  running or has failed.
  Test: `conc_15_the_footer_never_claims_up_to_date_while_fetching`
  (`ui/feed_footer.rs`, `#[cfg(test)]`).

- **CONC-16** [active] [gtk] — The provider name has a hover-free home:
  an optional `Source` column, hidden by default, switchable in the column
  header menu, its visibility persisted like every other column. The row
  tooltip "Opens {source}" is a comfort duplicate of it (TIP-3), never the
  only place the name appears.
  Test: `conc_16_the_source_column_is_available_but_off_by_default`
  (`ui/concerts/concerts_view_tests.rs`).

- **CONC-17** [active] [gtk] — The Concerts table shows `Artist · Date ·
  City · Distance · Tickets` by default, with `Venue` and `Source` hidden.
  Date, Artist, City, Venue, Distance, and Source are sortable; `Tickets`
  carries no sorter because its cell is a button. Migration v75 discards a
  stored Concerts column layout once while preserving stored column widths.
  Tests: `the_default_concert_layout_leads_with_the_artist_and_hides_venue_and_source`
  (`reprise-view/src/columns/concert.rs`),
  `conc_17_every_sortable_concerts_header_orders_its_own_column` and
  `only_the_ticket_header_carries_no_sorter`
  (`ui/concerts/concerts_view_tests.rs`), and
  `v75_drops_the_stored_concerts_column_layout_and_keeps_the_widths`
  (`reprise-core/src/db_concerts_migration_tests.rs`).

- **CONC-4c** [active] [gtk] — replaces CONC-4b. Without a credential,
  Concerts neutrally shows "No concert data yet" with no action; the Concerts
  section in the Updates popover is not visible. There is no credential input
  hint and no Preferences deep link. Changes to credentials, location,
  default radius, time range, and similar settings immediately re-evaluate
  the already-open view, its sidebar count, and the Updates popover. Never
  fetched shows exactly "Not loaded yet" and offers the reload button; zero
  hits with filters offers exactly "Show all". Offline or error leaves the
  cache visible and states so per CONC-15. CONC-11a specifies the shared
  failure surface; credential and filter behaviour stay Concerts' own and
  remain `[active]` unchanged.
  Test: `conc_4c_settings_changes_re_evaluate_credentials_and_refresh_dependents`
  (`ui/concerts/concerts_view_tests.rs`).

- **CONC-5b** [active] [core] — replaces CONC-5a. Network runs exclusively
  in the worker or `one_shot_task`. Triggers are view-open staleness (24 h
  plus jitter), the hourly due check, the footer's reload button, and an
  explicitly confirmed credential check. All Concerts requests share the
  1-req/s limiter. Track changes, navigation, and individual credential
  keystrokes only read or write locally; fetch results are applied per MOT-2
  without a fade-in animation.
  Test: `conc_5b_only_enabled_due_idle_workers_fetch`
  (`ui/concerts/concerts_worker.rs`).

- **CONC-11a** [active] [gtk] — replaces CONC-11. A failed Concerts fetch
  leaves every cached event untouched and reports the failure through
  CONC-15's footer state. A neutral shared banner above a populated view names
  the failed refresh, what remains available, and the next action; only a
  genuinely empty cache uses the shared full-area failure state. Both surfaces
  carry the same collapsed `Details` block with Copy, and technical status,
  host, and exception text appears only there. Offline is written from the
  window's explicit connectivity value, dims the remote-action rows, and never
  overwrites a provider or configuration failure already on screen; reconnect
  removes only an offline-authored notice. A successful fetch removes the
  notice silently. Missing credentials are a configuration outcome with
  "Open Preferences" targeting the Concerts Plugins row, never "Try again";
  CONC-4b's ordinary no-credential empty state remains neutral with no action.
  Test: `conc_11a_cached_and_empty_failures_choose_the_shared_surfaces`
  (`ui/concerts/concerts_failure_ui.rs`).
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
  with a label and radius 8 in both sources, never the chip shape. The plus was
  never rendered on the podcast side because setting its label replaced the
  icon child, and is now absent from radio too, so both buttons describe the
  behavior they actually share.
  The add action is the leftmost child of the source footer, outside the
  filter row; Podcasts and YouTube retain the same footer's spinner, status
  and right-aligned "Refresh now" action, while Radio uses its equivalent
  footer strip. Action names stay unchanged, so every shortcut, empty state
  and context route reaches the same dialog. The filter row follows FIL-2a
  and keeps its height across state changes. On Podcasts and YouTube, the popover offers
  only Unplayed and Downloaded; the existing show/channel groups provide
  per-source narrowing through expansion and collapse.
- **SRC-3** [replaced by SRC-3a] [gtk] — Each source has exactly one add dialog
  with exactly one input field for search terms or a URL. Search
  returns grouped results with row actions; a recognized URL leads
  through preview and options to a confirmation. Network and
  subprocess work starts only on submit and never runs on the GTK main
  loop.
- **SRC-4** [replaced by SRC-4a/SRC-4b] [gtk] — Removal takes effect immediately, stays
  tombstoned for ten seconds, and is reversible via a high-priority
  undo toast. The context menu is the single place to unsubscribe or remove a
  favorite; there is no hover star. "Play Next" and "Add to Queue" are
  entirely absent. Podcast downloads are never silently deleted on
  unsubscribe: the commit toast reports the files that were kept and offers
  only moving them to trash; multiple unsubscribes are aggregated.
- **SRC-5** [active] [gtk] — RSS podcasts and YouTube are separate library
  places. Both start with source rows grouped by channel or show which expand
  to their episodes; radio stays a station list. A YouTube source is named by
  its channel and has no author subtitle; an RSS source keeps its show title
  plus a distinct author subtitle when present. The source identity stays
  vertically centered beside its artwork instead of sticking to the top of the
  group header. The existing episode, new, latest and download facts line stays
  unchanged. The add dialogs show real source images, group YouTube hits by
  channel, and hide podcasts, channels and stations that are already
  subscribed.
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
  entirely. **Addendum (Block B2):** two siblings extend this geometry rather
  than replacing it. When a source's own module is switched off (`G1`/
  `NET-1a`) and nothing is subscribed yet, the same
  tile/title/body/one-button shape appears as "{Source} is turned off" with
  an "Enable in Preferences" button that opens the Online sources page
  directly — existing subscriptions are named as kept, and the button is
  never a plus icon here, since there is nothing to add while the source is
  off (`PodcastsEmptyState::ModuleOff`). Existing subscriptions outrank the
  module gate: it only ever replaces the empty case, never an
  already-populated view. The filter-mismatch state ("Nothing matches these
  filters", `PodcastsEmptyState::NoResults` / `RadioEmptyState::NoResults`)
  and the downloads-only state ("Nothing downloaded yet",
  `PodcastsEmptyState::NoDownloads`) are the opposite of the genuine empty
  state: the toolbar and filter row stay visible, with a "Clear filters"
  action, because clearing the filter — not adding a source — is the way
  out. `NoEpisodes` (subscribed, the feed genuinely has nothing yet) is
  unchanged and keeps the filter row hidden. A fetch failure with an existing
  subscription but no cached or downloaded episode uses the same geometry as
  `PodcastsEmptyState::FetchFailed`, with Retry and the collapsed Details
  block; it never masquerades as "nothing subscribed yet".
- **SRC-11** [active] [core] [gtk] — Channel, show and station images (YouTube
  `thumbnails`, iTunes `artworkUrl600`, radio-browser `favicon` — `C1`) run
  through the shared Artwork module (`module.artwork.enabled`, which also
  covers album covers and artist portraits). On upgrade, an absent unified
  setting inherits consent once when any retired artwork setting
  (`module.cover_download.enabled`, `module.artist_portraits.enabled`, or
  `module.source_images.enabled`) is explicitly on; all-off or absent legacy
  settings stay off, and an existing unified setting always wins. Schema v72
  applies that same rule to databases already stamped by the faulty v71: only
  an absent unified row plus at least one enabled retired row is repaired;
  every existing `0` or `1` remains untouched. Whenever either migration
  actually enables Artwork, Reprise records that inherited consent and shows
  one dismissible Library banner: "Reprise merged the separate image modules
  into Artwork. It now loads album covers, artist portraits, and images for
  podcasts, YouTube, and radio." "Review Artwork Settings" opens Plugins and
  highlights Artwork; "Dismiss" closes it. The notice is consumed before
  either action hides it, never returns after either action, and never appears
  when the unified gate was untouched. Artwork is
  subject to `NET-1a`: a cache hit is always shown, regardless of the gate — a
  cache miss triggers a fetch only when the global gate **and** the module are
  both active, otherwise the surface's source fallback stays, never an error
  image (`RAD-7` defines the radio list's initials fallback). The pure fetch
  and cache policy lives, testable without a display, in
  `reprise_core::remote_image` (no gtk4/libadwaita/gstreamer/zbus); decoding
  and display stay in the GNOME crate. The caller selects one of two bounded
  on-disk stores: persistent artwork for subscriptions, favourites, library
  rows and playback holds at most 1,000 files; transient search results and
  pre-add previews hold at most 200. Each store independently and
  deterministically clears the files untouched for longest first. The legacy
  shared `remote-images` store is removed on first access and is never
  recreated. In-memory textures retain the same scope boundary, so a transient
  hit cannot bypass persistent storage. Every valid display request is admitted
  to an unbounded worker queue rather than dropped for lack of capacity;
  matching in-flight URLs share one network fetch across cache scopes, populate
  every waiter's selected store from the resolved local file, and fan the result
  out to every waiter. The current gate is still read immediately before that
  resolve, not when the waiter joined. Episode images follow the
  same rule: a stored provider URL wins, and when a YouTube episode has none,
  the read projection derives
  `https://i.ytimg.com/vi/<video-id>/hqdefault.jpg` from its durable video id
  without persisting a second value, while RSS never receives a derived YouTube
  URL. An episode row publishes an already-cached show/channel image first,
  replaces it when its episode image arrives, and keeps the show/channel image
  when the episode image is absent or fails; only a source with neither usable
  image stays on its glyph. Both stages use the same row generation, so a
  recycled row cannot accept either image from its predecessor. The same chain
  applies to YouTube channel detail and playback artwork, but not to MPRIS,
  which continues to project one URL. A channel's own image comes from
  its yt-dlp channel dump, and that dump needs its own selection rule: the
  largest square `thumbnails` entry, else `avatar_uncropped`, never a banner
  crop (the video-level rule would hand out a 6:1 strip). No group header ever
  borrows an episode image — neither YouTube nor RSS: a channel without an
  avatar stays on its glyph, so the library group header and the channel detail
  header always agree, and a missing avatar stays visible instead of being
  masked by the newest episode's cover. A YouTube subscription is therefore
  created without an image at all; the search hit's video thumbnail stays a
  preview, and the first refresh fills in the avatar. Every caller (podcast library view,
  YouTube channel detail, all three add dialogs) computes the gate and selects
  the cache scope itself at its own connection rather than relying on an
  upstream checkpoint or URL heuristic — the lesson from `T6-G1-gap`: a
  privacy promise in UI copy needs a test per call path, not per feature.
- **SRC-12** [replaced by SRC-12a] [gtk] — Episodes can be selected in bulk in both the
  grouped library view and the channel detail view, with one shared set of
  batch actions offered only by the context menu for the current selection;
  there are no episode checkboxes or separate selection toolbar. Actions that
  are meaningless for more than one episode are hidden rather than applied to
  an arbitrary member, and a batch reports itself with a single aggregated
  toast and a single undo. Escape clears the current episode selection in
  whichever of the two surfaces is showing, and is passed on untouched when
  nothing is selected.
- **SRC-12a** [replaced by SRC-12b] [gtk] — Episodes can be selected in bulk in both the
  grouped library view and the channel detail view, with one shared set of
  batch actions offered only by the context menu for the current selection.
  Selection is carried by a checkbox over the left media slot in grouped rows,
  never by a permanent extra column or a separate selection toolbar; the
  channel page keeps its existing tint-only selection because it has no media
  column. A checkbox appears only while a selection exists and only on a row
  that is selected, hovered, or focused. Applying a selection never rebuilds
  the list. Ctrl+A selects every rendered episode of the focused source; with
  no focused row it selects the whole rendered list, while the channel page
  selects its current rendered window. Collapsed groups, episodes past a
  preview window, and filtered-out rows are never swept up. Escape and the
  visible Clear action beside the selection count both clear the current
  surface's selection; Escape propagates unchanged when nothing was selected.
  Actions meaningless for more than one episode stay hidden, and a batch
  reports one aggregated toast and one undo. Covered by the `src_12a_…` tests
  in `podcasts_selection`, `podcasts_view_shortcuts` (including
  `src_12a_ctrl_a_survives_caps_lock`, because a shortcut that a lock key
  disarms is not a shortcut), `podcasts_view_tests`, `podcasts_groups_tests`,
  `podcasts_context_menu`, `podcasts_context_menu_browser_tests`,
  `podcasts_view_actions` for the aggregated toast and undo,
  `youtube_channel_detail_tests` — where
  `src_12a_channel_page_select_all_stops_at_the_rendered_window` is what
  actually holds the channel-page clause above — `strings_podcasts`, and
  `source_row::media_column`.
- **SRC-12b** [active] [gtk] — Episodes can be selected in bulk in both the
  grouped library view and the channel detail view, with one shared set of
  batch actions offered only by the context menu for the current selection.
  Selection is shown in both surfaces solely by a neutral row tint; the left
  media slot contains artwork only, never a checkbox, playback marker,
  permanent extra column, or separate selection toolbar. Growing a selection
  with the mouse takes a modifier — Ctrl+click toggles one row, Shift+click
  extends a range — exactly as the music track list has always worked; a plain
  click replaces the selection. The checkbox SRC-12a placed over the artwork
  also let a modifier-free click add a row, and that gesture is deliberately
  gone with it: one selection idiom across every list beats a second one that
  exists only where there happens to be artwork to cover. Applying a selection
  never rebuilds the list. Ctrl+A selects every rendered episode of the
  focused source; with no focused row it selects the whole rendered list,
  while the channel page selects its current rendered window. Collapsed
  groups, episodes past a preview window, and filtered-out rows are never
  swept up. Escape and the visible Clear action beside the selection count
  both clear the current surface's selection; Escape propagates unchanged
  when nothing was selected. Actions meaningless for more than one episode
  stay hidden, and a batch reports one aggregated toast and one undo. Covered
  by the `src_12b_…` tests in `podcasts_selection`,
  `podcasts_view_shortcuts` (including
  `src_12b_ctrl_a_survives_caps_lock`, because a shortcut that a lock key
  disarms is not a shortcut), `podcasts_view_tests`, `podcasts_groups_tests`,
  `podcasts_context_menu`, `podcasts_context_menu_browser_tests`,
  `podcasts_view_actions` for the aggregated toast and undo,
  `youtube_channel_detail_tests` — where
  `src_12b_channel_page_select_all_stops_at_the_rendered_window` is what
  actually holds the channel-page clause above — `strings_podcasts`, and
  `source_row::media_column`.
- **SRC-4a** [active] [gtk] — Radio keeps SRC-4's removal and undo
  behavior, and its station menus continue to omit "Play Next" and "Add
  to Queue". A live stream is deliberately not a citizen of an ordered
  queue. Removing a favorite is operated from the context menu alone;
  there is no hover star.
- **SRC-4b** [active] [gtk] — Podcasts and YouTube keep SRC-4's removal,
  download-preservation, and undo behavior. Their episode menus additionally
  expose "Play Next" and "Add to Queue" for the current episode selection;
  the same actions are the keyboard-accessible partner of the typed episode
  drag source. Beside "Copy episode URL", the menu exposes the single-episode
  action "Open in browser" only when the episode has a launchable web page:
  YouTube uses the durable watch URL from `audio_url`, while RSS uses
  `page_url` when present and never treats its media enclosure in `audio_url`
  as an episode page. As a single-episode action it is absent from a
  multi-selection menu instead of targeting an arbitrary member, as required
  by SRC-12b. This asymmetry with radio is deliberate. Unsubscribing is operated
  from the context menu alone; there is no hover star.
- **SRC-13** [active] [gtk] — **Marking and scrolling are separate in the
  source lists.** The loaded item carries the shared playback marker in every
  source list it appears in; setting the marker never moves the viewport. It is
  revealed — group expanded, row centered — on entering the view and when the
  loaded item changed outside the view, the latter only if no scroll movement
  has occurred for 1.5 seconds. Activating a row never reveals, because the row
  was visible. A reveal changes neither focus nor selection, except START-3's
  one cold-start restoration, which makes the restored episode the sole
  selection without taking focus. A collapsed group's ten-episode preview
  window opens when the loaded episode sits past it; an item hidden by the
  active filter is not revealed and the filter is never cleared to reach it. A
  jump the user asked for always reveals, also in the already visible view and
  regardless of the 1.5-second grace period; it drops exactly those filter
  facets that would otherwise keep the target hidden, and nothing else.
- **SRC-14** [active] [gtk] — **Episode rows select like track rows.** A click
  selects the row alone, Ctrl-click toggles it, Shift-click extends the
  selection from the anchor across the rendered order, and playback takes a
  double click or Enter. Space toggles the focused row's selection and
  Shift+Space extends from the anchor. A secondary click on a row outside the
  selection makes that row the selection before the menu opens, so a menu never
  acts on rows the pointer is not on. That same selection-aware menu is reached
  three ways — by secondary click, by the row's ⋮, and by Menu/Shift+F10 — and
  the YouTube channel view carries the same row menu as the grouped list.
  A range covers only rendered rows: a collapsed group, the episodes past a
  preview window and rows hidden by the filter stay out of it. Applying a
  selection never rebuilds the list, so keyboard focus survives it.
- **SRC-15** [replaced by SRC-15a] [core] [gtk] — **The add dialogs suggest from the
  library, never from a hard-coded taste.** Podcasts and YouTube each carry
  one chip above the result list holding the genre this library has spent the
  most listening time on — "Metal podcasts" on the podcast page, "Metal
  channels" on the YouTube page — and radio carries the same fact as its
  first chip (`RAD-5`). All three read one shared derivation, so they never
  disagree about what this library listens to. Activating a chip fills the
  search field with the term it searched for: the run stays visible,
  editable and repeatable, never a hidden query. A library that has played
  nothing carrying a genre shows **no chip at all** — an empty or invented
  suggestion is worse than none, and the dialogs remain fully usable through
  their search field.
- **SRC-15a** [active] [core] [gtk] — **The library chip suggests from the
  library, never from a hard-coded taste — and it belongs to the surfaces a
  genre is a real query for.** The YouTube add dialog carries one chip above
  the result list holding the genre this library has spent the most listening
  time on ("Metalcore channels"), and radio carries the same fact as its first
  chip (`RAD-5`). Both read one shared derivation
  (`library::taste::top_genre`), so they never disagree about what this library
  listens to. Activating a chip fills the search field with the term it
  searched for: the run stays visible, editable and repeatable, never a hidden
  query. A library that has played nothing carrying a genre shows **no chip at
  all** — an empty or invented suggestion is worse than none, and both dialogs
  remain fully usable through their search field. The Apple Podcasts dialog
  does **not** carry this chip: a bare genre word is a weak podcast search
  term, and that dialog's one chip slot is spent on `SRC-19` instead.
- **SRC-16** [active] [gtk] — **Podcast and YouTube episode lists share one
  row grammar.** Group headers use the same skeleton as their episode rows: a
  fixed 64 × 40 media column, one identity box and one trailing box. The
  group's square 40 × 40 artwork is centred in that column, while episode
  artwork remains either 64 × 36 wide or 36 × 36 square. An episode row's
  leading edge is indented by one named media-column width, so its artwork
  begins to the right of its group's artwork, never before it; the expander
  caret stays in the leading space outside the media column and cannot change
  that relation. Both source kinds therefore start their episode title at the
  same x position and keep the same minimum row height. The second line drops
  absent date or duration values instead of leaving separators behind, and
  carries at most one status chip outside that fact chain. Resume states
  include the measured whole percentage when duration is known and fall back
  to Resume without inventing one otherwise. The 110-pixel download-state
  slot stays reserved on the right even when no size is known; selection occupies the
  media overlay on the left. Covered by
  `src_16_the_shared_row_geometry_is_one_set_of_constants`,
  `src_16_episode_media_starts_after_group_media_in_both_source_views`,
  `src_16_the_row_height_is_carried_by_the_shared_style`,
  `src_16_both_shapes_fit_the_same_column`,
  `src_16_the_checkbox_replaces_the_playing_marker_rather_than_covering_it`,
  `src_16_the_title_starts_at_the_same_x_in_both_source_views`,
  `src_16_rows_have_the_same_height_in_both_source_views`,
  `src_16_a_row_renders_exactly_one_status_chip`,
  `src_16_the_detail_line_drops_empty_values` and
  `src_16_resume_reports_a_whole_percent_and_omits_it_without_a_duration` for
  the second line, `src_16_the_channel_page_renders_the_status_as_a_chip`,
  `src_16_the_rss_source_header_types_its_second_line_like_the_shared_grammar`,
  and `src_16_the_style_takes_its_measurements_from_the_shared_constants` —
  the last one because a stylesheet literal outranks the skeleton's size
  request, so the constants only govern the layout for as long as the
  stylesheet keeps deriving from them. **[planned]** RSS author
  subtitles already use the same quiet second-line typography, but a YouTube
  channel handle cannot join it until the source projection carries a durable
  handle field; no subtitle is invented from other channel data meanwhile.
- **SRC-17** [active] [gtk] — **Approach reveals one reserved source-row
  action surface.** A source row's ⋮ keeps its layout space at all times and
  changes only opacity and targeting on hover, keyboard focus, or selection;
  revealing it can therefore never move the title under the pointer. The same
  hover state is shared with the media overlay rather than collected by a
  second controller. **Focus is watched on the revealed control itself, not
  only on the row:** a container's focus does not bubble up from its children,
  so a row-only rule leaves a control that Tab can reach but no one can see.
  Covered by `src_17_revealing_keeps_the_space_and_only_changes_opacity`,
  `src_17_focusing_the_control_itself_reveals_it`,
  `src_17_the_row_menu_button_is_transparent_until_hover_focus_or_selection`,
  `src_17_the_channel_page_hides_its_row_menu_until_hover_focus_or_selection`,
  and `src_17_revealing_the_row_menu_button_does_not_move_the_title`.
- **SRC-18** [active] [core] [gtk] — **An Apple Podcasts search result says
  when the show last published.** Every RSS result row carries the age of its
  newest episode as the second segment of its subtitle, after the author and
  behind the same `·` separator the YouTube rows use: `New today`,
  `New yesterday`, `New 4 days ago`, `New last week`, `New 2 weeks ago`,
  `Last month`, `3 months ago`, and from a year onwards the absolute
  `Last Oct 2019`. "New" carries only while the show is fresh: 35–64 days say
  "Last month", 65 days onwards drop to "… months ago", and a year onwards
  becomes "Last …" — the wording itself signals decay. Counted units round
  **down** with a plain divisor — 7 days to the week and 30 to the plural month,
  never a calendar walk; the singular month is the explicit 35–64-day bucket.
  Thus 20 days is "New 2 weeks ago", and rounding down never claims a show is
  staler than it is. A feed dated in the future — a mis-set timezone, a
  scheduled episode — reads as `New today` rather than producing a negative
  age. A result whose feed carries no usable date **drops the segment
  entirely** rather than printing "unknown", and a result with no author drops
  the leading separator with it. The date is read in core
  (`itunes::SearchResult::last_episode`) as a Unix second, and one malformed
  date costs that row its segment and never the other eleven results. This
  scale is deliberately **not** `podcasts_presentation::relative_date`'s: that
  one orders episodes the listener already subscribes to, this one judges a
  stranger. YouTube channel rows carry no freshness at all — yt-dlp's channel
  search yields only the upload dates of whichever videos the relevance
  ranking surfaced, so a daily channel could read "last March".
- **SRC-19** [active] [core] [gtk] — **The Apple Podcasts dialog opens on what
  a country listens to.** Its one chip reads `Popular in DE`, and activating it
  loads Apple's country chart **directly into the result list** — the search
  entry stays untouched, because a chart has no search term to fill in with.
  The section carries its own heading (`PODCASTS · TOP IN DE`) in the same
  style as `PODCASTS · APPLE PODCASTS`, so a chart is never mistaken for the
  results of a search nobody typed; submitting a text search afterwards
  replaces the section, exactly as a second search replaces the first. The
  country is resolved **once per dialog** from the stored app-level location's
  country code (`O-4`), falling back to the system locale — unlike `RAD-5`,
  where a countryless location gives Near you its own honest empty state, a
  location that carries no country falls through to the locale here, because
  this chip has a working answer either way. The shared Location page names
  this Podcasts reader under Used by (`SET-15`). A stored code that is not a
  storefront — two ASCII letters, the same
  check the locale territory passes — falls through with it rather than being
  handed to Apple.
  That same country drives the text search below it, so the chip and the
  results it sits above can never mean two different catalogs. The label uses
  the country **code**, matching `RAD-5`'s "Metal in DE": real country names
  would need a translated table covering every Apple storefront. Chart rows are
  ordinary search results — same row widget, same already-subscribed filtering
  (`SRC-5`), same freshness segment (`SRC-18`) — assembled from the chart
  feed's ids plus **one** batched lookup, restored to chart order, with ids the
  lookup drops falling out silently rather than leaving a hole; an id the lookup
  could not be asked for — anything that is not a number — is dropped before
  the request, not after it. A chart with nothing left to show, whether Apple
  returned nothing or `SRC-5` filtered every row away because this library
  already follows all of it, says so **in its own sentence**: the search's
  "Nothing found for '…' — try pasting a feed/channel URL instead" would quote
  the chip's label back as a term the user never typed. Offline the
  chip is **absent**, for the same reason search is (`NET-3` point 4): it is a
  network action, and a pill that only reports failure is worse than none. It
  is equally absent when podcast online sources are switched off (`NET-1a`) —
  reachability is not consent, and activating it would issue the chart and
  lookup requests a refused source is promised never to make. Both halves are
  read once, when the dialog is built, and a failed consent lookup counts as
  refused.
- **SRC-20** [active] [gtk] — **Dormant Apple Podcasts search results sink
  without losing relevance order.** Search results whose newest episode is at
  least 365 days old move after every fresher result, using the exact boundary
  at which `SRC-18` switches to `Last <Mon Year>`. The partition is stable:
  fresh shows keep Apple's order among themselves, and dormant shows do too.
  A result with no usable date stays in the fresh group, because absence is
  not evidence that a show is dormant. This applies only to text search;
  country charts keep their chart order untouched.
- **SRC-21** [active] [gtk] — **Add Podcast search results make their match
  visible.** Whenever a text query produced the result list, the query is
  accent-bold inside each matching title and author, case-insensitively and
  mid-word, reusing `FIL-5` and `POD-25`'s Pango-escaped treatment. The RSS
  subtitle is marked from its parts: only the author can be highlighted; the
  separator and `SRC-18` freshness clause are escaped but never marked, even
  when the query occurs there. A country chart has no query and therefore no
  highlighting. The title keeps `EllipsizeMode::End`, so long provider text
  cannot widen the dialog (`SRC-8`).
- **SRC-22** [active] [gtk] — **An Apple search result explains a match its row
  cannot show.** When `SRC-21`'s exact comparison finds the text query in
  neither the displayed title nor publisher, a quiet information marker sits
  immediately after the title. Its tooltip and accessible description say:
  "The search term is not in the title or publisher shown here, but Apple
  returned this podcast as a result." Apple does not reveal which other field
  matched, so the explanation names none. The marker keeps its space while the
  title ellipsizes and cannot widen the dialog (`SRC-8`). Country charts have
  no query and therefore no marker.
- **POD-1** [active] [core] — Episode status is a pure derivation:
  Played exactly when `played_at` is set, otherwise Resume when
  `position_ms > 0`, otherwise unstarted. The visible New pill is a
  separate discovery fact (`first_seen_at > subscription.added_at`), not
  another spelling of unplayed; an unstarted backlog episode therefore has
  no status pill. An episode ending sets Played and clears the position.
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
  resolution only at playback time, with the ephemeral stream URL never
  persisted. The same full playback extraction, and the existing download
  extraction, may persist the raw first media category when yt-dlp supplies
  one; flat listings never trigger an extra extraction for it, and a missing
  or malformed category remains nullable and non-fatal. Errors are
  classified into actionable, provider-safe UI messages and never
  crash; operation, failure category, exit code or timeout are logged
  without URLs, tokens, cookie paths, raw provider text, or local
  paths. If the binary is missing, the setting stays unchanged and
  the degradation is made visible on the YouTube toggle, which is
  active by default.
- **POD-4** [replaced by POD-24] [gtk] — Episodes start at the saved position;
  this is persisted throttled as well as on pause, stop, switch, and
  quit. After the end, the app offers the next unplayed episode of the
  same show by date via toast and a persistent player-bar button, but
  never plays it automatically. Podcast sessions produce neither
  scrobbles nor `listen_events` nor play counts.
- **POD-5** [active] [gtk] — Downloads are opt-in per subscription,
  live in the app's XDG data path under a GUID-stable path, follow the
  chosen cleanup policy, and are preferentially played back locally
  offline. The "keep last N downloaded" cleanup policy's N is a global
  default (`podcasts.keep_downloaded_default`, itself defaulting to 5) that
  any channel's own "Keep N downloaded" override replaces outright for that
  channel — never intersected with the default, never a silent minimum of
  the two (`O-5`). `0`, on either the default or an override, means
  unlimited, not "keep nothing" (`E-9`).
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
  total and new counts, the newest episode, and the local data volume. New
  means `first_seen_at > subscription.added_at`: the first successful fetch
  writes its backlog with `first_seen_at = added_at`, while later discoveries
  use their refresh time. Playback never rewrites this discovery fact.
  **Addendum (`G2`, design 6a):** above the grouped list, a page-level header
  line ("4 shows · 41 episodes · 7 new") restates the same total/new
  definition as a library-wide sum — number of subscribed shows, total
  episode count, and the new count, all computed over the
  unfiltered list so the header reads as a stable overview instead of
  jittering with the active filter chip. The pure projection is
  `podcasts_presentation::library_summary`; while a filter is active the
  filter bar keeps showing "shown of total" instead, unchanged.
- **POD-10** [active] [core] [gtk] — The YouTube channel page starts with at
  most the ten newest long-form entries from the official keyless UULF feed and
  keeps Shorts hidden by default. "Load more" extends that same channel once,
  past the yt-dlp provider boundary, up to entry 40. Selection and bulk
  download or removal stay bound to the channel; every row shows the download
  state from POD-7. The channel page is reached from the source row's existing
  menu; the already-expandable channel header carries no arrow button or
  second competing navigation affordance. Covered by
  `pod_10_the_source_menu_opens_the_channel_page` and
  `pod_10_the_channel_header_has_no_arrow_button`.
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
- **POD-12** [replaced by MTP-54] — Downloaded episodes from RSS **and**
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
  applies symmetrically to any change of kind. This selection is mirrored
  read-only as an "On phone" indicator on both the channel list and the
  channel detail page (`D3`): the indicator has no control of its own, and
  the selection control described above stays the only place that writes
  it — two places claiming the same selection is a defect this project has
  hit before.
- **POD-13** [active] [core] [gtk] — A failed episode download (POD-7's
  "failed" state) shows a classified reason directly in the row, next to the
  fixed "Download failed" heading — never only on hover or another
  pointer-only affordance, so it stays reachable for keyboard and touch
  users too. The reason is one of a fixed, closed set of categories (timed
  out, could not be reached, returned an HTTP error, returned invalid data,
  disabled in preferences, or "YouTube source could not be read with
  yt-dlp") — the same classifier `source_actions::podcast_source_error`
  already used for every other podcast provider failure; the underlying
  provider error text is discarded at the point of failure and never
  reaches the UI or a normal-level log line, so a signed URL, a query
  string, a credential, or a local filesystem path can never leak through
  it (issue #106). The row's action button stays sensitive and offers a
  clean retry: activating it re-enters the same download path a first
  attempt uses, so it runs `queued` → `downloading` → `downloaded` (or fails
  again, freshly classified) rather than a half-state, and always makes a
  fresh provider call — a previous failure is never cached. `.part` files
  and yt-dlp postprocessor leftovers from the failed attempt are removed
  before the row is offered for retry.
- **POD-14** [active] [core] [gtk] — On the YouTube channel page, when every
  currently available entry is a Short and Shorts are hidden, the row list is
  replaced by "Only Shorts here" with a single "Show Shorts anyway" action
  that reveals Shorts for this channel (the existing per-channel override
  from `POD-10`); the header and its "Hide Shorts" control stay visible,
  since toggling that control is the other way out. The decision is a pure
  projection — `reprise_core::podcasts::channel_window::shorts_only_hidden`
  — reused rather than re-derived from the display code, matching
  POD-10/POD-11's existing split between core decision and GTK display. Does
  not fire for a channel that genuinely has no entries at all yet — that
  case is not covered by this rule.
- **POD-15** [active] [gtk] — `POD-9`'s addendum `G2` header names what the
  page actually subscribes to: "4 shows · 41 episodes · 7 new" on Podcasts,
  "4 channels · 41 episodes · 7 new" on YouTube. Only the leading subject
  differs; the episode and new counts stay the same quantities computed the
  same way (`podcasts_presentation::library_summary`), so both pages keep one
  projection and one tail formatter. The leading subject is the vocabulary
  every other surface on that page has to match — a show stays a show and a
  channel stays a channel — so the same subscription is never called by two
  different nouns within one library. Found by the `source-youtube`
  acceptance scenario, which reads the header back out of the running app.
- **POD-16** [replaced by POD-19] [gtk] — The status line under the Podcasts and YouTube
  libraries never renders a raw error. Its two failure states are fixed
  sentences — "Could not read your subscriptions" when the library itself
  cannot be read, and `POD-11`'s "Refresh failed · showing saved episodes"
  when a refresh fails — and the underlying error goes to a warning-level log
  line instead. This is not cosmetic: a `rusqlite` input error renders as the
  message plus the entire failing statement and a byte offset, so a real
  installation showed "no such column: sync_to_phone in SELECT id, kind,
  feed_url, … at offset 150" across five lines of the footer on both pages.
  Beyond being unreadable it is also a leak of the same class `POD-13` closes
  for provider errors, since a statement can carry a path or an identifier.
  The "Refresh now" button beside the line is the offer; the text does not
  repeat it.
- **POD-17** [active] [core] — Every newly downloaded episode carries Title,
  Album (the show), Artist (the feed author, or the show when no author is
  present), Album Artist (the show), and the recording date in the file itself,
  so the library names it from the file rather than from the subscription row
  it was downloaded through — a downloaded episode stays correctly named after
  its feed is unsubscribed, and any other player opening the same file reads
  the same thing. The container is detected from the
  file's bytes, never its name: the tag is written to the `.part` temporary,
  whose name an extension-based reader would reject. Tags go to the temporary
  and never to a published file because the tag library rewrites Ogg and FLAC
  by truncating first, so a rewrite that fails part-way leaves a destroyed
  file; only a `.part` may ever be that file, which the next attempt deletes
  and which `reclaim_existing` refuses to adopt. Which of the two ways tagging
  can fail decides the download's fate, and they are never collapsed into one.
  A container Reprise cannot read or cannot tag is decided before anything is
  written: the file is untouched and is still published untagged, so an
  untaggable container never costs a completed download. A failed *write*
  fails the download: the file may already be truncated, so the temporary is
  deleted and the episode stays not-downloaded and retryable. Publishing it
  instead would record the truncated wreckage as a finished download: the
  recorded path stops the episode from ever being fetched again, so a file
  that cannot play would be kept forever with no way back. Either way only the
  classified reason may reach a log line (`POD-13`). The size is measured after
  the tag write, so `downloaded_bytes` always equals the published file's size;
  reversing that order would record every tagged episode short by the length of
  its own tag, and the channel's "downloaded" storage figure — which sums
  exactly this column — would understate the disk by that much for every
  episode it counts. Every written value is capped at `MAX_TAG_BYTES`, the same
  byte length the file-name sanitizer caps a path component at, because the
  feed owns these strings and nothing else bounds how much of one reaches the
  truncate-and-rewrite. Files downloaded before this rule keep no tags;
  re-downloading is the remedy. The reclaim path deliberately does not tag
  because it is the one path that would rewrite an already-published file in
  place.
- **POD-18** [active] [core] — A YouTube episode carries the upload day yt-dlp
  reports for it. Reprise asks for it explicitly in the channel listing because
  the flat listing otherwise omits dates entirely. The value is day-granular by
  yt-dlp's own description, which is all the device file name and date tag use.
  A listing without a date still yields every episode, and a date that arrives
  later fills an episode stored without one; an exact date already parsed from
  an RSS feed is never overwritten by an approximate one. Without this rule,
  every episode imported from one channel on the same day would receive that
  import day, so the date could not tell otherwise identical channel episodes
  apart.
- **POD-19** [active] [gtk] — Replaces POD-16's refresh-failure footer.
  The footer keeps only neutral refresh progress, last-updated age and library
  read failures. A provider refresh failure appears once in the shared neutral
  source banner above the unchanged cached list, with fixed safe copy, Retry
  and collapsed Details; a successful refresh removes it silently. When no
  cached or downloaded episode exists, the same information and actions use
  the shared full-area failure state instead. Neither surface renders raw
  provider, transport, database or helper text outside Details. The populated
  banner keeps its copy, actions, Details toggle and labelled close control in
  one compact summary row; only the technical Details expand below it. Closing
  the banner clears the current failure notice without claiming that a refresh
  succeeded, and a later provider failure may appear as a new notice.
- **POD-20** [active] [gtk] — The loaded episode carries the shared
  playback marker in every episode surface it appears in, and that marker
  tells running from paused. Activating the loaded row toggles pause and
  resume; it never reopens the session, because a restart costs an audible
  gap and resumes from the throttled saved position rather than the live
  one. Only the context menu restarts an episode. The marker stays put under
  the pointer: an episode row marks itself on hover the way a music row does,
  through the shared hover tint alone, and its content does not change while
  hovered.
- **POD-21** [active] [gtk] — A playing podcast or YouTube episode has
  neighbours: ⏮/⏭ move to the adjacent row of the list it was started from,
  in rendered order, without wrapping. The neighbour list is frozen when
  playback starts. Radio has no neighbours. While any external session is
  active the lyrics tab is hidden; the Visual tab is hidden for whatever
  AC-26 does not count as music — every RSS podcast, and a YouTube episode
  whose own category says it is speech. The panel header shows the episode
  instead of "Nothing playing".
- **POD-22** [active] [core] [gtk] — When yt-dlp classifies a YouTube
  failure as requiring verification, the failed episode row keeps its normal
  retry action and replaces the generic provider reason with the fixed,
  leak-safe guidance "YouTube needs a signed-in browser — choose one in
  Plugins". The YouTube section on the Plugins page offers an explicit browser
  selector and a separate "Open YouTube" sign-in action. The selector defaults
  to "Do not use browser cookies"; Reprise never infers a browser, reads a
  profile, or enables cookie access automatically. After the user selects a
  supported browser and signs in there, every YouTube listing, search,
  playback-resolution and download path passes that browser choice to yt-dlp;
  probing or updating yt-dlp does not. Reprise stores only the browser kind,
  and neither cookie contents nor browser-profile paths reach the database,
  UI, or normal-level logs.
- **POD-23** [active] [core] — YouTube channel listings, extended listings and
  channel searches ask yt-dlp to prefer metadata in the language actually used
  by the Reprise interface. Locale fallback follows the installed gettext
  catalogs, so an unsupported system locale requests English source strings
  rather than an unrelated provider language; Simplified Chinese is normalized
  to YouTube's `zh-CN` code. If YouTube does not supply localized metadata, the
  original title remains unchanged — Reprise never invents a machine
  translation. Stored episode titles adopt an available localized title on the
  next source refresh.
- **POD-24** [active] [core] [gtk] — Episodes start at the saved position; this is
  persisted throttled as well as on pause, stop, switch, and quit. When a
  directly started YouTube episode reaches its natural end and its frozen
  POD-21 context has a next rendered episode, Reprise automatically starts that
  exact no-wrap neighbour — the same target as the enabled Next transport.
  RSS episodes and YouTube episodes without a next frozen neighbour keep the
  manual next-unplayed offer preserved from POD-4; QUE-12 excludes episodes
  from the manual queue. Podcast and YouTube sessions
  produce neither scrobbles nor `listen_events` nor play counts. Covered by
  `pod_24_direct_youtube_completion_uses_the_frozen_next_episode`,
  `pod_24_finish_offers_next_unplayed_of_show`, and
  `pod_24_external_session_never_scrobbles`.
- **POD-25** [active] [gtk] — The Podcasts and YouTube search matches
  **episode titles only**, case-insensitively and mid-word — not show
  names, not authors, not descriptions (FIL-1d: "in episode titles" /
  "in video titles"). A show is rendered when at least one of its
  episodes matches; it is then auto-expanded and renders only the
  matching episodes, and a show without a match drops out of the list
  entirely. Auto-expansion is for the duration of the query only: it
  never overwrites the show's own collapsed/expanded state, which
  returns as soon as the query goes. The per-group facts line and the
  page summary stay **unfiltered** (POD-9 / G2) — they describe the
  library, not the search. Counting follows FIL-2: while a query or a
  facet narrows the list the status line reads "N of TOTAL episodes"
  with the shown number accented, and returns to the unfiltered summary
  ("3 shows · 13 episodes · 1 new") once nothing restricts. Inside every
  rendered episode title the query itself is accented per FIL-5a — the same
  helper, so a hit reads the same wherever the user finds it. A YouTube
  channel tail the row deliberately dims (POD-15) remains part of the stored
  and searched episode title, so a hit inside that tail is accented too while
  the surrounding tail stays dim.
- **RAD-1** [active] [gtk] — Only the currently connected station is
  accented in the table; its state icon, name, now-playing, and row
  tint change together. All others, as well as a presented but
  disconnected paused station, show "—". Only the player bar may keep
  the last ICY title dimmed as session memory.
- **RAD-2** [active] [gtk] — Live playback has neither seek nor
  duration: the full-width player bar shows elapsed time and PLAY-13's LIVE
  badge, while the mini-player keeps elapsed time and its geometry-matched
  waveform placeholder; MPRIS reports `CanSeek=false`
  and no length. Pause disconnects the stream but stays presented as
  Paused/CanPause with station and dimmed last title; play reconnects
  live. A reconnect error leaves the paused state standing and shows the
  neutral shared source banner with "This station isn't broadcasting right
  now", Retry, Find a new URL and collapsed Details; Retry/Find re-resolve a
  UUID station through radio-browser once before surfacing the failure again.
  Radio produces no listening statistics;
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
- **RAD-5** [active] [core] [gtk] — The Add Station dialog always shows the
  one-click radio-browser searches "Top voted" and "Near you", regardless of
  whether a location is stored, and ahead of them a **library chip** that
  suggests what this library actually listens to. That chip's genre is the
  one with the most **listening time** — not the most files, so a large
  unplayed collection cannot out-vote what gets played — folded across
  spelling variants and clamped per listen exactly as the stats screen
  clamps it. A stored country narrows the search and shows in the label
  ("Metal in DE"); without one the chip keeps the genre and searches
  worldwide rather than becoming a second "Near you". A library that has
  played nothing with a genre gets **no chip at all** instead of a
  suggestion it has no evidence for — never a hard-coded genre, which was
  only ever right for one library in one country. Genre and country are both
  read fresh each time the dialog opens, because the dialog outlives both.
  "Near you"
  reuses the one app-level, already-consented location (`O-4`); it never
  queries the XDG Location portal or a geocoder itself, and hoisting that
  location out of the `concerts.` namespace carries its existing consent
  forward rather than asking again. With a country-taggable location
  stored, the chip runs a country-filtered search. Without a location, its
  result area instead shows "No location set" and explains that one shared
  city serves Concerts, Radio, and local podcasts. A location with no country
  gets the distinct "Location has no country" state and explains that the
  portal supplies coordinates only. Both states offer exactly "Open
  Preferences › Location"; the chip itself never forces navigation, and the
  Add Station dialog remains open underneath Preferences. If the app-wide
  location announcement arrives while either state is open, the pending Near
  you intent re-evaluates immediately and starts the search as soon as the
  location is usable. It never fires a silent unfiltered search standing in
  for "near you": a chip that claims to filter by location but does not is
  worse than exposing the missing input. The country code itself is derived
  only from data a call Reprise already makes — Nominatim's
  `addressdetails` enrichment of the existing forward-geocode request
  behind city search — never from a new reverse-geocoding call, so a
  location set via the portal path stays honestly countryless rather than
  guessed.
- **RAD-6** [active] [gtk] — **Add Station text-search results make their
  station-name match visible.** The free-text query is accent-bold inside
  each matching station name, case-insensitively and mid-word, reusing
  `SRC-21` and `FIL-5`'s shared Pango-escaped highlighter. The generated
  details line (genre, bitrate, country and votes) is escaped plain text and
  never highlighted, even when it contains the query. "Top voted", "Near
  you" and the library genre chip search by tag and/or country, carry no text
  query and therefore produce no highlighting. Radio has no `SRC-22`
  unexplained-match marker: radio-browser searches station names, but its
  server-side accent folding can return a visibly matching name that the
  local comparison cannot mark (for example, "metal" and "Métal"). As in
  `FIL-5` and `POD-25`, that listed but unaccented row is an accepted gap,
  never a wrong row.
- **RAD-7** [active] [gtk] — Every station row has cover-shaped identity. A
  stored station image uses `SRC-11`'s gated cache and decode path; without a
  usable image the radio list shows up to two uppercase initials from the
  shared artist-avatar helper (or `?` for a blank name), never the generic
  microphone glyph. The fallback uses a 16 px, weight-700 accent label on the
  shared 155-degree accent/window gradient with the same 8 px radius as source
  covers. This exception belongs only to the radio list; podcast and channel
  fallbacks remain unchanged.
- **RAD-8** [active] [core] [gtk] — Adding a radio station may fill a missing
  favicon from radio-browser only under the Radio online gate and only with
  secure identity: a matching supplied UUID, an exactly equal resolved stream
  URL, or exactly one trimmed case-insensitive exact-name result, in that
  priority order. Empty favicons, ambiguous exact names and approximate names
  are misses. A closed gate performs no lookup, and every lookup miss or
  provider failure is best-effort: the station is still added and `RAD-7`
  supplies its visible fallback. An explicitly supplied HTTP(S) favicon or
  homepage is stored without lookup.

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
- **RUN-6** [planned] [gtk] — Closing the window stops the playback
  that window started, and only that. Music does not outlive the
  surface a user was listening through: leaving music playing behind
  a closed window is a player the user cannot see, cannot pause, and
  did not ask to keep. Playback an agent or a second surface started
  is left running — it is not this window's to end, and a client that
  stops it is stopping someone else's session. The runtime names the
  originator of what is loaded, so this is a question the surface can
  answer rather than guess.

## AH. Sound Similarity

Sound Similarity was removed on 2026-08-07 because its nearest matches were
audibly unrelated while reporting 100% similarity. The rules stay here in full
as append-only history; the separate spectrogram feature is unchanged. SIM-8
was the only one of them that governed behaviour outside this module — plugin
provision badges. Those badges are no longer live: the provision-pill UI was
removed. Its replacement chain now continues through `SET-12` to `SET-14`,
the active Plugins-row alignment guarantee.

- **SIM-1** [replaced by nothing — the Sound Similarity module was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — A sound profile lives in its own versioned cache
  and follows the spectrogram's source-identity invalidation and track
  deletion. A stale format is absent and is derived again from the stored
  spectrogram; the spectrogram schema itself gains no recommendation scalar.
- **SIM-2** [replaced by SIM-9] — The earlier rule compared band means and the
  mastering scalars alone. Measured over a real library that found the same
  production and not the genre, and the weights it fixed were nominal rather
  than effective; SIM-9 adds the temporal half and the scale that makes a
  weight mean what it says.
- **SIM-3** [replaced by nothing — the Sound Similarity module was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — A row's percentage is its rank in the current
  track's distance distribution across the complete eligible library. Same
  album (album title plus album artist) and same artist exclusions are applied
  only after those ranks are formed, so changing a filter never changes the
  meaning of a percentage.
- **SIM-4** [replaced by nothing — the Sound Similarity module was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — The Sound tab remains present while analysis is
  incomplete and shows numeric progress. Results require at least 50 current
  profiles and the playing track's profile. Profile markers are library-wide
  feature percentiles; the tempo axis is disabled while tempo is excluded.
- **SIM-5** [replaced by nothing — the Sound Similarity module was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — The default result limit is seven. **Add to
  queue** appends exactly the currently shown matches in their displayed
  nearest-first order and never shuffles them.
- **SIM-6** [replaced by nothing — the Sound Similarity module was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — Sound Similarity is a live, default-off Local
  module. Its defaults exclude the same album, retain the same artist, omit
  tempo, use Default weighting, and show seven matches. Its static registry
  declaration provides both the Sound panel tab and **Find similar tracks**.
- **SIM-7** [replaced by nothing — the Sound Similarity module was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — **Find similar tracks** appears in a single,
  present-track context menu only while the module is enabled; disabling the
  module removes the route.
- **SIM-8** [replaced by SET-12] — Plugin provision badges are derived from the
  static registry, never current enable state. A provision-kind set is the
  unbadged group norm only when it occurs at least twice and strictly more
  often than the runner-up; otherwise every row is badged. Panel-tab and
  sidebar-section badges use the accent, while all other kinds are neutral.
  The rule was never specific to Sound Similarity and outlived it; `SET-12`
  restated it where the plugin list is governed. When the provision pills were
  removed, `SET-12` was replaced by `SET-14`; this historical chain remains
  intact while the active guarantee now governs Plugins-row switch alignment.
- **SIM-9** [replaced by nothing — the Sound Similarity module was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — The comparison carries how a track is produced
  *and* how it moves, both derived from the stored spectrogram alone. Band
  means and per-band positive flux stay L2-normalized and are compared with
  cosine distance, each divided by the library's own spread so that a nominal
  weight is an effective one. Centroid mean, centroid variance, frame-level
  crest, onset rate, flux mean, flux variation, pulse strength and an enabled
  tempo estimate are standardized against library spread; zero spread
  contributes zero. The default weights are bands 0.30, timbre 0.12,
  dynamics 0.08, rhythm 0.50, tempo 0.
- **SIM-10** [replaced by nothing — the Sound Similarity module was removed from the GTK frontend; the ID stays as a signpost per the append-only contract] — At most two matches carry the same artist, and
  the list fills up with the next nearest track by someone else. The nearest
  match is never displaced by this. Tracks that name no artist are not capped
  against each other, because unnamed is not a shared identity. The cap applies
  whatever **Exclude tracks by the same artist** is set to; that setting is the
  stricter step, not a replacement.

---

If a case comes up during testing that no rule covers: add a rule
(process rules above), do not decide locally.

## AI. GNOME platform conformance

Rules in this section come from published GNOME and Flathub criteria. Each
rule names its source so a reviewer can check the claim. They exist to keep
Reprise submittable to Flathub and, later, to GNOME Circle.

### Platform idioms

- **GP-1** [planned] [gtk] — Icon-only buttons carry an accessible label via
  `GTK_ACCESSIBLE_PROPERTY_LABEL`. When a button has no tooltip, it also
  carries `GTK_ACCESSIBLE_PROPERTY_DESCRIPTION`. Source: developer.gnome.org,
  accessibility coding guidelines.
- **GP-2** [planned] [gtk] — No blocking I/O on the main thread. UI-adjacent
  asynchronous work runs through `glib::spawn_future_local`; CPU-bound work
  runs through `gio::spawn_blocking` and reports back over a channel. Source:
  gtk4-rs book, main event loop.
- **GP-3** [planned] [gtk] — A closure that captures a widget which itself
  stores that closure uses `glib::clone!(#[weak] …)`, never a strong capture.
  The grep gate catches explicit `#[strong]` captures. An unannotated capture
  is implicitly strong and must be checked in review because the gate cannot
  parse Rust macro arguments reliably. Source: gtk4-rs book.
- **GP-4** [planned] [gtk] — No `unwrap()` in UI paths: signal handlers,
  property access, channel receives. Either `expect()` with context or the
  error is propagated.
- **GP-5** [planned] [gtk] — Widgets that hold state, override virtual
  functions, or expose properties or signals are GObject subclasses, not
  ad-hoc structs. Source: gtk4-rs book, GObject subclassing.
- **GP-6** [planned] [core] — User-facing settings live in a GSettings schema,
  not in a hand-rolled configuration file. Source: gtk4-rs book, settings.

### Human interface guidelines

- **GP-7** [planned] [e2e] — The window stays usable at 1024x600, the smallest
  display size GNOME requires every app to support. Source: HIG, adaptive.
- **GP-8** [planned] [e2e] — Light, dark and follow-system are all supported;
  light is the default. Source: HIG, UI styling.
- **GP-9** [planned] [e2e] — Every interactive element is reachable by
  keyboard, tab order follows the widget tree, and a control's label precedes
  it in focus order. Source: HIG, keyboard.
- **GP-10** [planned] [gtk] — Every interface element exposes a descriptive
  accessible name. Source: HIG, accessibility.
- **GP-11** [planned] [gtk] — Styling uses libadwaita style classes and colour
  variables. Bespoke CSS is the exception and carries a stated reason. Source:
  HIG, UI styling.

### Distribution metadata

- **GP-12** [planned] [core] — The metainfo file passes
  `appstreamcli validate --no-net --explain`.
  <!-- Keep validation offline and deterministic. Do not add --pedantic: it
       rejects the conventional uppercase final component in GNOME app IDs. -->
- **GP-13** [planned] [core] — The desktop file passes `desktop-file-validate`.
- **GP-14** [planned] [core] — The Flatpak manifest passes
  `flatpak-builder-lint manifest`.
- **GP-15** [planned] [manual] — Static sandbox permissions stay at the
  absolute minimum. Where an XDG portal exists, the portal is used instead of
  a static permission, and every remaining static permission is justified in
  `flatpak/README.md`. Source: Flathub requirements.
- **GP-16** [planned] [core] — App name is shorter than 15 characters and the
  summary is at most 35 characters, in sentence case, without a trailing
  period, and without repeating the app name. Source: Flathub quality
  guidelines.
- **GP-17** [planned] [manual] — At least one English screenshot. Window
  1000x700 or smaller, 2000x1400 for HiDPI. Native decoration, no desktop
  wallpaper. Every screenshot carries a one-line caption without a trailing
  period. Source: Flathub quality guidelines.
- **GP-18** [planned] [manual] — Every release carries release notes with real
  content, never "bug fixes and performance improvements". Source: Flathub
  quality guidelines.

### Provenance hygiene

These four points are, verbatim, the rejection reasons the GNOME Circle
committee published on 2026-05-29.

- **GP-19** [planned] [core] — No comments that read as instructions to a
  model, no banner comment blocks drawn from repeated `=` or `-`, no emoji in
  comments.
- **GP-20** [planned] [core] — No dead code: no unused items, and no
  `#[allow(dead_code)]` without a stated reason on the same or preceding line.

## AJ. Showroom (public site)

Rules in this section govern `showroom/`, the public site. Their level is
`[web]`: the showroom suite is static analysis over the built page, the built
CSS and the component source — it has no DOM and cannot measure layout, so a
rule here is phrased as something a stylesheet either states or does not.

- **SHOW-1** [active] [web] — A screenshot plate holds its frame still. On
  hover only the picture inside it moves; no hover or focus rule changes a
  layout-affecting property of the plate.
- **SHOW-2** [active] [web] — No pointer-led sheen: no overlay layer with a
  cursor-dependent gradient, and no `pointermove` handler on a plate.
- **SHOW-3** [active] [web] — Pointing and keyboard focus produce the same
  state: the same lift, the same picture zoom, the same zoom cue.
- **SHOW-4** [active] [web] — Under `prefers-reduced-motion: reduce` a plate
  has no transform transitions; the cue may appear at once.
- **SHOW-5** [active] [web] — Without hover capability there is no hover
  state, so none can stick after a tap.
- **SHOW-6** [active] [web] — The gate wall names the checks the merge gate
  script actually runs, in script order; the list and the number shown come from
  the same derivation out of that script.
- **SHOW-7** [active] [web] — No lane of the pipeline figure carries marks in
  both a writing and a judging step, and the human lane carries exactly one
  mark.
- **SHOW-8** [active] [web] — A failed gate cell blocks the readout and names
  how many checks are failing; with every cell cleared it reads ready again.
- **SHOW-9** [active] [web] — Under `prefers-reduced-motion: reduce` marks and
  gate cells stand in their end state, with no sequence.
- **SHOW-10** [active] [web] — The gate count appears nowhere as a literal;
  every place that shows it reads the derivation.
- **SHOW-11** [active] [web] — The tempo timeline names its weeks from a
  checked-in record. Neither a week's name, nor a date span, nor how many weeks
  there are appears as a literal in a `.tsx`.
- **SHOW-12** [active] [web] — No line count and no share appears as a literal
  beside the words that would claim it; the page reads them from the build's own
  count of the tree.
- **SHOW-13** [active] [web] — The performance figures quote a checked-in
  measurement record carrying a commit and a date; none of them appears as a
  literal in a `.tsx`.
- **SHOW-14** [active] [web] — The footer says, for every group of figures,
  whether it is counted, quoted or stated. It no longer claims the measurement
  is still to come.
- **SHOW-15** [active] [web] — Under `prefers-reduced-motion: reduce` the
  timeline's rail stands still.
