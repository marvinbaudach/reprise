# UX rulebook + acceptance-test foundation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Status: **grilled, ready to implement**
Branch: `feat/ux-rules-acceptance-tests` (worktree `.worktrees/ux-rules-acceptance-tests`)
Date: 2026-07-17

> **Read this as a record, not as instructions (note added 2026-07-31).**
> This plan shipped. What it built has since moved on, so two things in the
> body are historical rather than current:
>
> - **Language.** The plan states that the rulebook is written in German. It
>   was, then; `docs/ux-rules.md` has been English for some time, and as of
>   2026-07-31 every document in this repository is English (`AGENTS.md`,
>   "English everywhere"). The status tokens are `[active]`/`[planned]`/
>   `[replaced by <ID>]`, never their German forms.
> - **The embedded lint.** The Bash script quoted below is the original
>   version. The shipped `scripts/check-ux-traceability.sh` has grown a third
>   direction, level tags, a display-runner marker and a `RELEASING.md` check,
>   and it matches the English tokens. Its German `grep` patterns and its
>   `UX-Traceability ok: N aktive Regeln abgedeckt` output are preserved here
>   as the record of what was written on 2026-07-17 — copying them today would
>   produce a lint that matches nothing.
>
> The reasoning, the rule conventions and the pilot pattern are unchanged and
> still binding; `docs/ux-rules.md` is the authority for both.

**Goal:** establish `docs/ux-rules.md` as a binding, hardened UX rulebook (German, numbered rules with status and level tags), wire it to the tests via a traceability lint, and demonstrate the pattern once in full with a pilot (area C, playback/queue).

**Architecture:** The rulebook is the single UX source of truth; tests reference rule IDs in the test name; a Bash lint in `check-merge-readiness.sh` enforces document↔test consistency in both directions. Behavior changes do NOT happen in this branch — the rules for them sit in the document as `[planned]`.

**Tech Stack:** Markdown, Bash (lint), Rust `cargo test` (reprise-core), the existing cua-e2e harness (AT-SPI/Xvfb).

## Global Constraints

- Document language **German**; code, commits and the AGENTS.md edit in English (repo convention).
- Rule IDs are **append-only**: never renumber, never reuse; replacements as `[replaced by <ID>]`, the target ID always named.
- Status **binary**: `[active]` (enforceable: code conforms + rule-named test green + merge blocker) / `[planned]` (target state). The switch to `[active]` **only in the same commit** that proves the behavior/test. Half implemented → a/b split.
- **Exactly one primary rule ID per test**: Rust `fn play_1a_…` (snake_case), cua-e2e scenarios `play-2-…` (kebab-case). Collective tests across several rules are forbidden.
- `#[ignore = "UX <ID> [planned] — …"]` is only allowed on `[planned]` rules (the lint checks that).
- **No behavior changes in this branch.** In particular: do not touch the queue view, toasts, or the history stack.
- **Coordination:** a parallel agent implements QUE-1–5 + NAV-9 in the same branch. Task 1 (the document) is therefore committed **immediately and first** — the queue agent then flips "his" rules in his implementation commits. The queue agent's files (queue view, player bar) are off limits.
- Commit format `<type>: <description>`, **no attribution footer** (repo rule in AGENTS.md).
- After every task: extend `.superpowers/sdd/progress.md` (append-only ledger).
- Every commit leaves `scripts/check-merge-readiness.sh` green (at minimum: fmt, clippy, workspace tests, and from task 3 on the new lint as well).
- 800-line limit per file (repo lint); `docs/ux-rules.md` is exempt from it as Markdown.

## Decision table (grilling 2026-07-17, all confirmed by the user)

| Question | Decision |
|-------|-------------|
| OS-3 vs PLAY-5 | PLAY-5 restricted to **background events**; user actions change playback naturally |
| FB-1 vs FB-7 | **Two-class toasts:** actionless ones replace each other (max. 1 waiting); action toasts (Undo) are undismissable, 10 s |
| ALB-1 vs PLAY-1 | **PLAY-1a container play:** queue = the container in canonical order; the grid filter only determines reachability |
| NAV-3 vs NAV-2 | **Global history stack** across place boundaries; a sidebar click replaces it; the highlight follows the topmost entry; NAV-2a: the stack is not session-persistent, Back with no entries is disabled |
| Decision references | FB-4/FB-7/SET-4 **inline** as full text (see the document in task 1) |
| P-1 | **Role formulation:** announcement=toast · view state=StatusPage · process=card · request=badge |
| NAV-5 vs START-1 | Mode memory only within the session; a restart restores only the last view |
| Status model | Binary `[active]`/`[planned]`; same-commit activation; a/b split instead of "partially" |
| Language/location | **German**, `docs/ux-rules.md` |
| Change process | Append-only IDs; git history instead of an inline changelog; an AGENTS.md protocol for rule proposals |
| Test levels | Tag `[core]`/`[gtk]`/`[e2e]`/`[manual]`; the lowest falsifying level; the *what* automated, the *how fast* manually (the RELEASING.md checklist speaks the same IDs) |
| Branch scope | Foundation + pilot area C; behavior changes in their own branches |
| Initial status | Conservative: everything `[planned]`; audit **section by section** when touching it (the pilot audits C completely) |
| Gates | `[core]`/`[gtk]` → workspace suite → pre-push; `[e2e]` → cua-e2e → release; lint in `check-merge-readiness.sh` |
| Traceability | The ID in the test name; lint in three directions: every `[active]` rule ≥ 1 test · no unknown/replaced ID · no `#[ignore]` on `[active]` |
| QUE-1–5, NAV-9 | Taken **verbatim** from the queue-fix prompt into the document as `[planned]` (implementation in the parallel queue branch/agent) |

---

### Task 1: create `docs/ux-rules.md` and commit it immediately

**Files:**
- Create: `docs/ux-rules.md`
- Create (already done while planning): `docs/plans/ux-rules-acceptance-tests.md`
- Modify: `.superpowers/sdd/progress.md` (append)

**Interfaces:**
- Produces: the rule-line format `- **<ID>** [<status>] [<level>] — <text>`, which the lint (task 3) and all tests (task 4/5) rely on. `<ID>` = `[A-Z]+-[0-9]+[a-z]?`, `<status>` ∈ {active, planned, replaced by <ID>}, `<level>` ∈ {core, gtk, e2e, manual}.

- [x] **Step 1: write the document** — exactly this content into `docs/ux-rules.md`:

````markdown
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
against replaced rules are re-hung in the same commit.

**Test levels.** Every rule carries a level tag: `[core]` (reprise-core,
workspace suite), `[gtk]` (widget/logic tests in reprise-gnome), `[e2e]`
(cua-e2e harness against the real app), `[manual]` (RELEASING.md checklist,
which references the same rule IDs). Testing happens at the **lowest level
that can disprove the rule**. Timing numbers (100 ms, 150 ms, …) are design
intent, not assertions: the *what* (feedback exists) is automated, the
*how fast* is checked manually. If a `[manual]` rule later becomes
automatable, only its tag changes, never its ID.

**Traceability.** A test carries **exactly one primary rule ID in its name**
(Rust: `fn play_1a_…`, cua-e2e scenario: `play-1a-…`). If a scenario happens
to cover further rules along the way, that does not count — the second rule
needs its own test. `#[ignore = "UX <ID> [planned] — …"]` is only allowed on
`[planned]` rules. `scripts/check-ux-traceability.sh` (part of the merge
gate) enforces: every `[active]` rule has ≥ 1 test · no test references an
unknown or replaced ID · no ignore on `[active]`.

**Changes.** If you encounter a case while implementing or testing that no
rule covers: **add a rule, don't decide locally.** Agents do this by adding
a `[planned]` draft with the next free ID in the affected section, marked
with `<!-- REVIEW: rule proposal -->` — the decision rests with the human.
Rationale for changes lives in the git history.

## A. Core principles

- **P-1** [planned] [manual] — Every feedback role has exactly one
  mechanism: announcing an event = toast · state of a view =
  StatusPage/inline · running process = progress card · open request =
  badge. An event may serve multiple roles at once (disconnect →
  toast + StatusPage), but never two mechanisms in the same role (never two
  toasts, never toast + dialog as a duplicate announcement).
- **P-2** [planned] [gtk] — Click responds instantly: every click produces
  visible feedback (state change, spinner in the button, selection), target
  < 100 ms. Never a click into the void. The *what* is automated (state
  after click ≠ starting state); the 100 ms are a manual checklist item.
- **P-3** [planned] [gtk] — Hover never navigates: hover shows (tooltip,
  pills, +3% area), click acts. No hover-to-open.
- **P-4** [planned] [manual] — Nothing shifts uninvited: layout shifts only
  as a direct consequence of a user action or a process started by them
  (sync removals collapse). Fade-ins (device card, ISSUES) fade in without
  reflowing neighboring content.
- **P-5** [planned] [core] — The app never deletes files. "Remove" always
  means: the library entry. Dialogs name the cascades (ratings, listening
  history) explicitly.
- **P-6** [planned] [core] — Evidence rule: what is provably present is
  shown/healed (mount event, resurrect); what is provably gone is marked
  honestly right away (eject). Assumptions (unmounted) are never grounds
  for deletion.

## B. Navigation model

- **NAV-1** [planned] [gtk] — Sidebar = places, content = mode. The sidebar
  chooses the place (Music, Queue, Playlists, My Stats, Devices, Issues).
  Within "Music" the switcher toggles the mode: Tracks | Albums | Artists.
- **NAV-2** [planned] [core] — One global history stack across the entire
  content area, even across place boundaries (Queue → Artist detail → Back →
  Queue). Content clicks (NAV-3) always push; Alt+← / mouse-back / header-‹
  pop. A sidebar click replaces the stack (places are restarts, not stack
  entries). The sidebar highlight follows the topmost stack entry — if the
  stack shows Artist detail after a Queue click, "Music" is highlighted.
- **NAV-2a** [planned] [core] — The stack does not survive the session
  (session restore only restores the topmost view, START-1 unchanged); Back
  with no stack entries is disabled, never a no-op.
- **NAV-3** [planned] [e2e] — Clickable metadata is the same everywhere: in
  every track list (Library, Playlist, Queue, Album detail, Top Tracks) the
  following applies: click on artist name → artist detail; click on album
  name/cover → album detail; both push the global stack (NAV-2). Hover shows
  an underline as affordance. Also applies in the player bar (there:
  artist/album click per this rule, cover/title per NAV-9).
- **NAV-4** [planned] [gtk] — Double-click on a row = play in the context of
  the visible list (see PLAY-2). Single click = select. Enter = like
  double-click. Exception Queue view: double-click jumps to the track
  (playhead) per QUE-3, instead of rebuilding the queue.
- **NAV-5** [planned] [gtk] — Mode memory (scroll + selection per
  Tracks/Albums/Artists) applies only within the session; sidebar/place
  changes also preserve the scroll + selection of the mode being left.
  START-1 restores, across restarts, only the last active view including
  scroll position; all other modes start at the top, unselected.
- **NAV-6** [planned] [e2e] — Search (Ctrl+F) filters the current view live;
  Esc clears and closes. Search never navigates on its own.
- **NAV-7** [planned] [e2e] — Hamburger menu: "Scan Library" → starts the
  scan, stays in the view (card appears). "Preferences" → Preferences
  window. "Keyboard Shortcuts" → shortcuts overlay. "About Reprise" → About
  dialog. No menu item silently switches the content view.
- **NAV-8** [planned] [gtk] — My Stats is a sidebar place like any other:
  full content area, the header bar with search stays put (search there
  being disabled/hidden is allowed, but the bar remains).
- **NAV-9** [planned] [gtk] — "Jump to Now Playing": clicking the cover or
  the title in the player bar navigates to the home of the playing track
  (library mode Tracks, or the playlist it is playing from), selects the row
  and centers it (scrolled so that the row sits in the middle third — no
  scrollIntoView edge-sticking). Additionally the Ctrl+L shortcut. This is
  the explicit "where am I right now" gesture; it pushes onto the history
  stack (NAV-2 global, Back returns). Artist/album clicks in the bar keep
  their NAV-3 targets — only cover/title jump to the track.

## C. Playback, queue, shuffle, filter

- **PLAY-1** [planned] [gtk] — Queue source = visible track list. "What you
  see is what plays": double-click/Play all/Shuffle in a track list build
  the queue from the currently visible (filtered, sorted) list. PLAY-1a
  applies for container buttons.
- **PLAY-1a** [planned] [core] — Container play (play button on cover, Play
  all/Shuffle in hero areas) builds the queue exclusively from the container
  in its canonical order (Album: disc/track number; Playlist: position
  order; Artist "Play all": albums by year, then track number within). The
  visible grid filter only determines which containers are reachable, never
  the queue content.
- **PLAY-2** [planned] [core] — Double-click plays the row and appends the
  rest of the visible list from this position into the queue.
- **PLAY-3** [planned] [core] — Filter constrains shuffle — intentionally.
  Filtered playlist + shuffle = shuffle over the hits ("shuffle my 90s
  tracks"). Changing the filter afterward does not touch an already-built
  queue (the queue is a snapshot; visible in "Queue").
- **PLAY-4a** [planned] [core] — Missing in lists: list playback and queue
  advance skip Missing silently.
- **PLAY-4b** [planned] [gtk] — Double-click on a concrete Missing row:
  toast "File missing since …" + button "Show in Missing files". Enqueueing
  (Play next/Add to queue) is disabled for Missing.
- **PLAY-5** [replaced by PLAY-5a/PLAY-5b] — Original queue-hygiene umbrella
  rule; split during hardening into the sub-rules deleted (5a) and unmounted
  (5b).
- **PLAY-5a** [planned] [core] — Deleted hygiene: externally deleted tracks
  leave the queue silently; the playing track is never stopped by this (if
  the playing track itself faults, FB-6 applies: skip + one toast).
- **PLAY-5b** [planned] [core] — Unmounted hygiene: unmounted tracks stay
  gray in the queue, are skipped on advance, and heal on the mount event
  (P-6). No background event (deleted, unmounted, sync removal, watcher)
  stops the playing track — explicit user actions (double-click, Play all,
  OS-open) naturally change playback.
- **PLAY-6** [planned] [gtk] — Shuffle/Repeat are global player states
  (player bar), not view states. Repeat cycles: off → all → one.

## D. Albums & artists view

- **ALB-1** [planned] [gtk] — Album grid: hover = darkening gradient + play
  button bottom right (fade 150 ms). Click cover/title → album detail
  (push). Click play → plays the album immediately per PLAY-1a, without
  navigating. Context menu: Play next / Add to queue / Edit tags / Show
  files.
- **ALB-2** [planned] [gtk] — Album detail: hero with cover + dominant color
  area (accent pipeline), Play all/Shuffle pills (PLAY-1a), track list by
  disc/track number. Playing track: accent row + EQ icon + bold — identical
  in every list in the app (one marking language).
- **ART-1** [planned] [gtk] — Artist list: click selects and shows detail on
  the right; selection NEVER follows playback, the playing artist only shows
  a mini EQ.
- **ART-2** [planned] [gtk] — Artist detail: hero glow (precomputed texture,
  250 ms crossfade on change), album row (hover like ALB-1), Top Tracks
  (double-click plays per PLAY-2 in the context of "Top Tracks"). "Show all
  N tracks ›" → Tracks mode with the artist filter chip set (visible,
  removable via ×).
- **FX-1** [planned] [manual] — All effects respect
  `gtk-enable-animations=false` (hard switch) and only run GPU-cheap
  (opacity/transform, pre-rendered glows). No live blurs in lists.

## E. MTP / Sync

- **MTP-1** [planned] [gtk] — Plugging in: toast "Pixel 8 connected", the
  device card fades into the sidebar. No auto-navigation — the user is never
  torn out of their view.
- **MTP-2** [planned] [gtk] — Card: click on the card → device view (push).
  Click on the "Sync" pill → starts the sync immediately, without navigating
  (stopPropagation). The hover tooltip shows details.
- **MTP-3** [planned] [core] — Sync running: the card and (if open) the
  device view show the same progress (one state). Cancel anywhere = the same
  action: finish the current file cleanly, toast "Sync cancelled · 28
  copied".
- **MTP-4** [planned] [gtk] — Unmount/eject in the device view: eject click →
  the button becomes a spinner → unmount → toast "Pixel 8 can be unplugged"
  → the view pops itself back to the previous view (150 ms crossfade), the
  card disappears. Eject during a sync: disabled + tooltip "Sync in
  progress".
- **MTP-5** [planned] [gtk] — Cable pulled (without eject): toast "Pixel 8
  disconnected" (+ "— sync incomplete (54 of 82)" if in the middle of a
  sync). If the device view is open, it switches to a StatusPage "Device
  disconnected" with a button "Back to Library" — it does not close itself
  (the user should be able to read what happened). The card disappears. The
  next sync resumes via the .part rule.
- **MTP-6** [planned] [gtk] — End of sync: toast "Sync complete · 82 copied,
  14 removed" (+ "· 3 failed" with "Details"). The card morphs to idle
  ("synced ✓"), the delta card shows "Everything in sync ✓". No 100% hold
  state.

## F. Settings & modals

- **SET-1** [planned] [gtk] — Preferences = one window with vertical
  navigation (pages: General, Library, Playback, Audio, Sync, Plugins).
  Clicking a page switches the content on the right; no tab overflow, new
  features = new page or section.
- **SET-2** [planned] [gtk] — Subpages (e.g. scrobbler configuration) are
  navigation pages in the same window with a ‹-Back in the header — no new
  windows.
- **SET-3** [planned] [gtk] — Modal layers: maximum two. Layer 1 = one
  window above the main window (Preferences OR Tag Editor OR Shortcuts —
  never two at once). Layer 2 = exactly one dialog above that (FileChooser,
  confirmation). A dialog never opens another dialog. Esc always closes the
  topmost layer.
- **SET-4** [planned] [gtk] — Settings take effect immediately (no
  Apply/OK). Destructive toggle, concretely: if Auto-clean is enabled while
  the Deleted group already contains rows past the chosen threshold, a
  dialog appears once: "This will remove N tracks now (deleted more than 30
  days ago) — their ratings and listening history go with them. Remove now /
  Start counting from today." The latter stores the activation date as the
  cutoff (`auto_clean_armed_at`); only what breaches BOTH the threshold AND
  the cutoff gets deleted. Both delete dialogs in the app (this one and
  "Remove all N") name the cascade explicitly: ratings + listening history
  go with it (P-5).

## G. Feedback vocabulary

- **FB-1** [planned] [core] — Two-class toasts (pill, bottom-centered, one
  line, max 1 action button, 4 s / 10 s with Undo; only for completed
  actions or events): Actionless event toasts replace one another — at most
  one waits, the newest wins, no backlog noise. Toasts WITH an action (Undo)
  are undismissable and run their full 10 s; event toasts wait that long.
- **FB-2** [planned] [gtk] — Progress card (sidebar bottom slot, stackable
  Scan over Sync): spinner + title + % on the right (tabular) + 3px bar +
  ellipsized detail line. For everything > ~1 s: scan, sync, relink search
  run, playlist import. Clicking the card → the associated view; Cancel on
  the card aborts.
- **FB-3** [planned] [core] — Errors: individual errors during a run are
  collected, never toasted individually. At the end, ONE toast with "N
  failed · Details" → Details opens the relevant view/dialog. Persistent
  problems live as a badge + ISSUES entry, not as recurring toasts.
- **FB-4** [planned] [core] — Badges only count entries newer than the last
  time the respective view was opened (`last_viewed` timestamp per view in
  the settings store): Missing counts `missing_since > last_viewed`, Import
  Errors counts `first_seen > last_viewed` — excluding dismissed rows and
  excluding notice rows ("imported without metadata"), because only what
  asks the user for something is counted. Reactivating a dismissed row (file
  changed) starts a new episode: `first_seen = now`, `seen_count = 1` — so
  it badges again. Opening the view = badge gone, the total count lives in
  the view.
- **FB-5** [planned] [gtk] — StatusPages for empty states with exactly one
  next step ("No missing files ✓", "Library folder unavailable — Retry").
- **FB-6** [planned] [core] — File deleted (externally, watcher): no toast
  per file (noise) — the row turns gray/disappears per the Missing rules,
  the ISSUES badge counts up. Exception: the currently playing track faults
  → skip + one toast "Track unavailable — skipped".
- **FB-7** [planned] [core] — "Remove from library" does not delete but sets
  `removed_at` (tombstone); the row with ratings, play counts and playlist
  positions stays fully intact for 10 s, Undo only resets
  `removed_at = NULL` — that's why the restoration is exact (same id, no
  race with scans running in parallel). The remove toast always carries Undo
  (FB-1, 10 s). After the toast expires, it is hard deleted (cascade:
  playlist entries, listening history, sync state); app exit within the
  window → the deletion is committed on the next launch, never rolled back
  ("7 removed" must stay true). Auto-clean (opt-in, default off, deleted
  tracks only) hard-deletes without a toast and without Undo — it fires no
  earlier than 30/90 days after disappearance (SET-4).

## H. File association & OS integration

- **OS-1** [planned] [e2e] — A file opened (double-click in the file
  manager): the main window opens in the last-used view (session restore),
  playback starts immediately, the player bar shows the track. No special
  view, no mini-player autostart.
- **OS-2** [planned] [core] — File in the library → normal track (with
  history/rating). File outside → transient track: plays, appears in the
  queue/player bar with a subtle "not in library" chip, is NOT imported,
  leaves no DB row beyond the session. The context menu offers "Add to
  library…".
- **OS-3** [planned] [e2e] — Multiple files (selection → "Open with
  Reprise"): replaces the queue with the selection in file-manager order,
  plays the first one, toast "12 files queued". A second invocation while an
  instance is running: same semantics (replace the queue, don't append —
  predictable beats clever). Replacing during ongoing playback is an
  explicit user action and does not violate PLAY-5.
- **OS-4** [planned] [e2e] — Single instance: a second launch passes files
  through to the running instance and focuses its window.
- **OS-5** [planned] [e2e] — MPRIS always mirrors the player state; playback
  from file association is identically visible there.

## I. Start state

- **START-1** [planned] [e2e] — Normal start: last view + scroll position,
  playback paused on the last track (position restored), startup reconcile
  runs silently (card only for actual work).
- **START-2** [planned] [gtk] — Start with an unavailable library root:
  StatusPage per Root-Guard, no mass Missing marking; library views show the
  last known holdings normally (Root-Guard hasn't marked anything), only the
  StatusPage/card reports the state. No blank screen.

## J. Queue view

- **QUE-1** [planned] [gtk] — The queue is never empty as long as something
  is playing. It shows three sections, in this order: **Now Playing** (1
  row, accent + EQ, as everywhere) · **Play Next** — manually enqueued
  tracks ("Play next"/"Add to queue"), only when present, with a section
  title · **Up Next · from <source>** — the rest of the playback snapshot
  (e.g. "Up Next · from Late Night" or "· from Neverbloom"), including
  shuffle order if shuffle is on.
- **QUE-2** [planned] [core] — Playback logic = display order: first the
  Play Next entries (FIFO), then the snapshot from the current position. No
  hidden priority — what the view shows is what happens.
- **QUE-3** [planned] [gtk] — Interaction: DnD reorder within "Play Next";
  Up Next rows can be dragged into "Play Next" via DnD; right-click "Remove
  from queue" everywhere (removes from the snapshot, not from the library);
  double-clicking a queue row jumps there (playhead, no rebuild — an
  exception to NAV-4). The "Clear queue" button clears only "Play Next"; the
  snapshot stays (it only disappears when playback stops or with a new
  context).
- **QUE-4** [planned] [gtk] — An empty state exists only without playback:
  StatusPage "Nothing queued — play something" (FB-5, one next step, no grid
  of suggestions).
- **QUE-5** [planned] [core] — Sidebar counter "Queue · N": N = Play Next +
  the remaining Up Next tracks (not the total snapshot). The counter is an
  inventory display, not a badge (P-1: no "request").

---

If a case comes up while testing that no rule covers: add a rule (process
rules above), don't decide locally.
````

- [x] **Step 2: check format sanity**

Run: `grep -cE '^\- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(aktiv|geplant|ersetzt)' docs/ux-rules.md`
Expected: `60` (rule lines including the replaced PLAY-5; on deviation, fix the line format)

Run: `grep -cE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[aktiv\]' docs/ux-rules.md`
Expected: `0` (all rule lines start `[planned]`; the pattern counts only
rule lines — a bare `grep -c '\[aktiv\]'` also hits the prose of the process
rules and is never `0`)

- [x] **Step 3: extend the ledger** — append to `.superpowers/sdd/progress.md`:

```markdown
## 2026-07-17 — UX rulebook task 1 (docs/ux-rules.md)

- Binding UX rulebook checked in: 60 rule lines (sections A–J, all
  `[planned]`, PLAY-5 as a replaced-signpost), with process rules
  (status, append-only IDs, level tags,
  traceability, change protocol). Hardening per the grilling of 2026-07-17
  (docs/plans/ux-rules-acceptance-tests.md). QUE-1–5/NAV-9 taken verbatim
  from the queue-fix prompt — implementation is running in parallel.
```

- [x] **Step 4: commit (immediately — the parallel queue agent is waiting for it)**

```bash
git add docs/ux-rules.md docs/plans/ux-rules-acceptance-tests.md .superpowers/sdd/progress.md
git commit -m "docs: add binding UX rulebook (all rules [geplant])"
```

---

### Task 2: wiring into AGENTS.md

**Files:**
- Modify: `AGENTS.md` (after the section "## Shared workflow skills (read these)")
- Modify: `.superpowers/sdd/progress.md` (append)

**Interfaces:**
- Consumes: `docs/ux-rules.md` from task 1.
- Produces: a bindingness paragraph that future agent sessions rely on.

- [x] **Step 1: insert the section** — directly after the "Shared workflow skills" block:

```markdown
## UX rules are binding

`docs/ux-rules.md` is the single UX source of truth (German). Before touching
any user-facing behavior, read the sections you work in. The contract:

- `[active]` rules are enforceable: deviation is a bug; every `[active]` rule
  has a rule-named test (`fn play_1a_…` / cua-e2e `play-1a-…`) that gates
  merges via `scripts/check-ux-traceability.sh`.
- A rule flips `[planned]` → `[active]` in the same commit that implements
  the behavior and adds its test — never retroactively.
- Rule IDs are append-only; replaced rules stay as `[replaced by <ID>]`
  and their tests are re-pointed in the same commit.
- If you hit a case no rule covers: do NOT decide locally. Add a
  `[planned]` draft with the next free ID in the affected section, marked
  `<!-- REVIEW: rule proposal -->`, and surface it for human review.
```

- [x] **Step 2: extend the ledger** — append to `.superpowers/sdd/progress.md`:

```markdown
- AGENTS.md: binding-UX-rules section added (contract, flip rule, proposal
  protocol) — UX rulebook task 2.
```

- [x] **Step 3: commit**

```bash
git add AGENTS.md .superpowers/sdd/progress.md
git commit -m "docs: bind agents to the UX rulebook contract"
```

---

### Task 3: traceability lint + gate wiring

**Files:**
- Create: `scripts/check-ux-traceability.sh`
- Modify: `scripts/check-merge-readiness.sh` (after the `check-architecture.sh` call, line ~43)
- Modify: `TESTING.md` (section "Required merge gates", one sentence)
- Modify: `.superpowers/sdd/progress.md` (append)

**Interfaces:**
- Consumes: the rule-line format from task 1 (`- **<ID>** [<status>] …`).
- Produces: `scripts/check-ux-traceability.sh` (exit 0 = consistent), used as a gate by task 4/5 and every future branch. Test-naming convention: Rust `fn <prefix>_<nr><suffix?>_…` with `<prefix>` ∈ {p, nav, play, alb, art, fx, mtp, set, fb, os, start, que}; cua-e2e scenario stems `<prefix>-<nr><suffix?>-…`.

- [x] **Step 1: write the lint script** — exactly this content into `scripts/check-ux-traceability.sh`:

```bash
#!/usr/bin/env bash
# Traceability-Gate: docs/ux-rules.md <-> regelbenannte Tests.
#
# Prüft drei Richtungen:
#   1. Jede [aktiv]-Regel hat >= 1 Test, der ihre ID im Namen trägt
#      (Rust-fn snake_case oder cua-e2e-Szenario kebab-case).
#   2. Kein Test referenziert eine ID, die im Dokument fehlt oder
#      [ersetzt ...] ist.
#   3. Kein #[ignore] auf einem Test, dessen Regel [aktiv] ist.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

doc=docs/ux-rules.md
[[ -f $doc ]] || { echo "check-ux-traceability: $doc fehlt" >&2; exit 1; }

prefixes='p|nav|play|alb|art|fx|mtp|set|fb|os|start|que'
fail=0

# --- Dokument einlesen: ID -> Status (aktiv|geplant|ersetzt) ---
declare -A status_of
while read -r id st; do
  status_of[$id]=$st
done < <(grep -oE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(aktiv|geplant|ersetzt)' "$doc" \
  | sed -E 's/^- \*\*([A-Z]+-[0-9]+[a-z]?)\*\* \[(aktiv|geplant|ersetzt)/\1 \2/')

# --- Test-Referenzen einsammeln (snake aus Rust, kebab aus cua-e2e) ---
snake_refs=$(grep -rhoE "fn (${prefixes})_[0-9]+[a-z]?_" crates --include='*.rs' 2>/dev/null \
  | sed -E 's/^fn //; s/_$//' | sort -u || true)
kebab_refs=$(grep -rhoE "(${prefixes})-[0-9]+[a-z]?-[a-z0-9-]+" scripts/cua-e2e 2>/dev/null \
  | grep -oE "^(${prefixes})-[0-9]+[a-z]?" | sort -u || true)

to_id() { # play_1a bzw. play-1a -> PLAY-1a
  local raw=${1//-/_} prefix nr
  prefix=${raw%%_*}; nr=${raw#*_}
  printf '%s-%s' "${prefix^^}" "$nr"
}

declare -A tested
for ref in $snake_refs $kebab_refs; do
  id=$(to_id "$ref")
  tested[$id]=1
  case "${status_of[$id]:-fehlt}" in
    fehlt)   echo "FEHLER: Test referenziert unbekannte Regel $id" >&2; fail=1 ;;
    ersetzt) echo "FEHLER: Test referenziert ersetzte Regel $id — umhängen" >&2; fail=1 ;;
  esac
done

# --- Richtung 1: jede [aktiv]-Regel hat einen Test ---
for id in "${!status_of[@]}"; do
  if [[ ${status_of[$id]} == aktiv && -z ${tested[$id]:-} ]]; then
    echo "FEHLER: [aktiv]-Regel $id hat keinen regelbenannten Test" >&2; fail=1
  fi
done

# --- Richtung 3: kein #[ignore] auf [aktiv]-Regeln ---
while read -r fn_name; do
  id=$(to_id "$fn_name")
  if [[ ${status_of[$id]:-} == aktiv ]]; then
    echo "FEHLER: Test $fn_name ist ignored, aber Regel $id ist [aktiv]" >&2; fail=1
  fi
done < <(grep -rA3 --include='*.rs' '#\[ignore' crates 2>/dev/null \
  | grep -oE "fn (${prefixes})_[0-9]+[a-z]?_" | sed -E 's/^fn //; s/_$//' | sort -u || true)

if (( fail )); then exit 1; fi
active_count=$(grep -cE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[aktiv\]' "$doc" || true)
echo "UX-Traceability ok: $active_count aktive Regeln abgedeckt"
```

Then: `chmod +x scripts/check-ux-traceability.sh`

- [x] **Step 2: negative test (see red, then green)**

```bash
scripts/check-ux-traceability.sh   # Erwartet: "UX-Traceability ok: 0 aktive Regeln abgedeckt", Exit 0
sed -i 's/- \*\*P-1\*\* \[geplant\]/- **P-1** [aktiv]/' docs/ux-rules.md
scripts/check-ux-traceability.sh   # Erwartet: "FEHLER: [aktiv]-Regel P-1 hat keinen regelbenannten Test", Exit 1
git checkout docs/ux-rules.md      # Fixture zurückrollen
scripts/check-ux-traceability.sh   # Erwartet: wieder ok, Exit 0
```

- [x] **Step 3: hang it into the merge gate** — insert into `scripts/check-merge-readiness.sh` directly after the line `scripts/check-architecture.sh`:

```bash
echo "== UX traceability =="
scripts/check-ux-traceability.sh
```

- [x] **Step 4: extend TESTING.md** — insert in the section "Required merge gates" after the first paragraph:

```markdown
Merge readiness also runs `scripts/check-ux-traceability.sh`: every `[active]`
rule in `docs/ux-rules.md` needs a rule-named test, no test may reference an
unknown or replaced rule ID, and no `[active]` rule test may be `#[ignore]`d.
```

- [x] **Step 5: run the gate in full**

Run: `scripts/check-merge-readiness.sh`
Expected: all sections green incl. `== UX traceability ==`, closing with `Merge-readiness checks passed`

- [x] **Step 6: ledger + commit**

```markdown
- Traceability lint introduced (scripts/check-ux-traceability.sh, 3 directions)
  and wired into check-merge-readiness; TESTING.md documents the gate —
  UX rulebook task 3.
```

```bash
git add scripts/check-ux-traceability.sh scripts/check-merge-readiness.sh TESTING.md .superpowers/sdd/progress.md
git commit -m "test: add UX rulebook traceability gate"
```

---

### Task 4: audit area C + pilot tests `[core]` + status flips (ONE commit)

**Files:**
- Modify: `crates/reprise-core/src/queue_tests.rs` (append rule tests)
- Modify: `docs/ux-rules.md` (flips PLAY-2/PLAY-3/PLAY-5a; further ones depending on the audit result)
- Modify: `.superpowers/sdd/progress.md` (append, incl. audit notes)

**Interfaces:**
- Consumes: the `Queue` API from `crates/reprise-core/src/queue.rs`: `new()`, `set_tracks(Vec<i64>, usize)`, `current() -> Option<i64>`, `advance_auto() -> Option<i64>`, `set_shuffle(bool)`, `ids_in_order() -> Vec<i64>`, `remove_ids(&[i64]) -> bool`, `is_empty() -> bool`. The lint from task 3.
- Produces: rule-named tests `play_2_*`, `play_3_*`, `play_5a_*`, `que_1_*` (ignored) in the workspace suite.

- [x] **Step 1: carry out the area C audit and note the findings** (commands + expected anchors):

```bash
# PLAY-1/PLAY-2-Verdrahtung (sichtbare Liste -> Queue):
grep -n "queue_ids_for_activation" crates/reprise-gnome/src/ui/track_list/track_list_activation.rs
grep -n "play_from_view" crates/reprise-gnome/src/ui/playback/player_controller.rs
# PLAY-3: keine reaktive Queue-Neubau-Verdrahtung bei Filteränderung:
grep -rn "set_tracks" crates/reprise-gnome/src --include='*.rs' | grep -v test
#   Erwartung: Treffer NUR in player_controller.rs (play_from_view) und
#   up_next_transport.rs — kein Aufruf aus Filter-/Suche-Handlern.
# PLAY-4a (Missing-Skip beim Advance): Implementierung suchen:
grep -rn "missing" crates/reprise-gnome/src/ui/playback crates/reprise-core/src --include='*.rs' | grep -iv test | head
# PLAY-5a (deleted -> still raus): Kern-API + bestehende Tests:
grep -n "remove_ids" crates/reprise-core/src/queue.rs crates/reprise-core/src/queue_remove_tests.rs | head
# PLAY-6 (Repeat-Zyklus off->all->one in der Player-Leiste):
grep -rn "Repeat::" crates/reprise-gnome/src --include='*.rs' | grep -v test | head
```

Write the audit verdict per rule into the ledger note (implemented+tested /
implemented+untested / not implemented). **Only what gets a rule-named test in
step 2 is flipped in this task: PLAY-2, PLAY-3, PLAY-5a.** PLAY-1, PLAY-4a/b,
PLAY-5b, PLAY-6, PLAY-1a stay `[planned]` (= not yet enforceable), even if
parts are implemented — their flips come with their tests in follow-up work.
If the PLAY-4a grep shows that the list skip is missing entirely: a ledger note
only, no doc edit needed (it already says `[planned]`).

- [x] **Step 2: write the rule tests** — append to the end of `crates/reprise-core/src/queue_tests.rs`:

```rust
// --- UX-Regelwerk-Tests (docs/ux-rules.md) ---------------------------------
// Charakterisierungs-Tests für Bestandsverhalten: sie sind ab dem ersten
// Lauf grün (das Verhalten existiert schon); der TDD-Rot-Schritt wird durch
// den Assertion-Flip in Step 3 ersetzt, der beweist, dass sie beißen.

// UX PLAY-2: Doppelklick spielt die Row und hängt den Rest der sichtbaren
// Liste ab dieser Position in die Queue (Aktivierungs-Snapshot).
#[test]
fn play_2_activation_snapshot_starts_at_clicked_row() {
    let mut q = Queue::new();
    q.set_tracks(vec![10, 20, 30, 40], 2);
    assert_eq!(q.current(), Some(30));
    assert_eq!(q.advance_auto(), Some(40));
    assert_eq!(
        q.advance_auto(),
        None,
        "Tracks vor der geklickten Row folgen nicht automatisch (Repeat::Off)"
    );
}

// UX PLAY-3: Queue ist Snapshot der gefilterten Treffer; Shuffle permutiert
// genau die Treffer (Queue = Treffermenge, kein Track von außerhalb).
#[test]
fn play_3_shuffle_stays_inside_filtered_snapshot() {
    let mut q = Queue::new();
    let treffer = vec![11, 22, 33, 44, 55];
    q.set_tracks(treffer.clone(), 0);
    q.set_shuffle(true);
    let mut queue_ids = q.ids_in_order();
    queue_ids.sort_unstable();
    assert_eq!(queue_ids, treffer);
    assert_eq!(q.current(), Some(11), "aktueller Track bleibt beim Shuffle stehen");
}

// UX PLAY-5a: Extern gelöschte Tracks verlassen die Queue still; der
// spielende Track bleibt unangetastet.
#[test]
fn play_5a_deleted_tracks_leave_queue_silently() {
    let mut q = Queue::new();
    q.set_tracks(vec![1, 2, 3, 4], 1);
    assert!(q.remove_ids(&[3]));
    assert_eq!(q.ids_in_order(), vec![1, 2, 4]);
    assert_eq!(q.current(), Some(2), "Hintergrund-Removal stoppt den spielenden Track nie");
}

// UX QUE-1 [geplant] — Demo des Aktivierungs-Workflows: Der Queue-Branch
// nimmt das #[ignore] weg und flippt QUE-1 auf [aktiv] im selben Commit.
#[test]
#[ignore = "UX QUE-1 [geplant] — Drei-Sektionen-Queue kommt im Queue-Branch"]
fn que_1_queue_is_never_empty_while_playing() {
    let mut q = Queue::new();
    q.set_tracks(vec![7, 8, 9], 0);
    assert!(!q.is_empty(), "solange etwas spielt, ist die Queue nie leer");
}
```

- [x] **Step 3: prove that the tests bite (the replacement for the red step)**

```bash
cargo test -p reprise-core play_2_ play_3_ play_5a_    # Erwartet: 3 passed
# Assertion-Flip: in play_5a temporär `vec![1, 2, 4]` -> `vec![1, 2, 3, 4]` ändern
cargo test -p reprise-core play_5a_                    # Erwartet: 1 FAILED
# Flip zurücknehmen
cargo test -p reprise-core play_5a_                    # Erwartet: 1 passed
```

- [x] **Step 4: status flips in the document** — in `docs/ux-rules.md`:

```text
- **PLAY-2** [planned]  ->  - **PLAY-2** [active]
- **PLAY-3** [planned]  ->  - **PLAY-3** [active]
- **PLAY-5a** [planned] ->  - **PLAY-5a** [active]
```

- [x] **Step 5: run the lint + suite**

Run: `scripts/check-ux-traceability.sh`
Expected: `UX-Traceability ok: 3 aktive Regeln abgedeckt`

Run: `cargo test -p reprise-core`
Expected: all tests green, `que_1_…` listed as ignored

- [x] **Step 6: ledger + ONE commit (tests + flips together — the same-commit rule)**

```markdown
- Area C audit (verdicts: PLAY-1 implemented/untested via
  queue_ids_for_activation; PLAY-2/3/5a implemented + now rule-named
  tested -> [active]; PLAY-4a/5b/6/1a: <enter the audit result>).
  Pilot rule tests in queue_tests.rs, the QUE-1 demo as ignored —
  UX rulebook task 4.
```

```bash
git add crates/reprise-core/src/queue_tests.rs docs/ux-rules.md .superpowers/sdd/progress.md
git commit -m "test: pilot UX rule tests for queue area, flip PLAY-2/3/5a to aktiv"
```

---

### Task 5: cua-e2e wiring scenario `play-2-…`

**Files:**
- Modify: `scripts/cua-e2e/lib.sh` (helper `cua_double_click_label`, after `cua_click_label`)
- Modify: `scripts/cua-e2e/run.sh` (scenario block in the populated-library workflow)
- Modify: `.superpowers/sdd/progress.md` (append)

**Interfaces:**
- Consumes: helpers from `lib.sh` (`cua_snapshot`, `element_index_for_label`, `assert_action_landed`, `assert_snapshot_contains`), the log marker `queue set from view` from `player_controller.rs::play_from_view`, the fixtures `sine_01.flac`/`sine_02.flac` (copied in `run.sh` at line ~172).
- Produces: the scenario stem `play-2-doubleclick-row`, which the lint counts as a kebab reference to PLAY-2.

- [x] **Step 1: double-click helper in `lib.sh`** — insert after `cua_click_label` (identical to the click helper, only the verb `double_click`):

```bash
cua_double_click_label() {
  local pid=$1 window_id=$2 label=$3 stem=$4
  local before_path action_path index payload

  before_path=$(cua_snapshot "$pid" "$window_id" "$stem-before")
  index=$(element_index_for_label "$before_path" "$label")
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --argjson element_index "$index" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, element_index: $element_index,
      session: $session}')
  "$CUA_DRIVER_BIN" double_click "$payload" >"$action_path"
  assert_action_landed "$action_path"
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null
}
```

- [x] **Step 2: scenario in `run.sh`** — insert in the populated-library workflow, before the search filter (after it the row is deliberately hidden) and before `finish_scenario` (adopt the PID/window variable names of the surrounding block — when in doubt, reuse those of the search workflow):

```bash
# UX PLAY-2 [e2e]-Verdrahtung: Doppelklick auf eine Row baut die Queue aus
# der sichtbaren Liste (Log-Marker aus play_from_view) und startet Playback.
echo "[cua-e2e] play-2-doubleclick-row: activation builds queue from view"
cua_double_click_label "$APP_PID" "$WINDOW_ID" "sine_01" "play-2-doubleclick-row"
assert_app_log_contains \
  "$APP_LOG" "queue set from view" "play-2-doubleclick-row"
```

Note: the row label is the fixture title (`sine_01`); per the ledger, the
exact label form can only be verified in the host run. If
`element_index_for_label` returns no match, inspect the snapshot
(`play-2-doubleclick-row-before.json`) and adjust the label — do NOT delete
the scenario.

- [x] **Step 3: check the lint (the kebab reference counts)**

Run: `scripts/check-ux-traceability.sh`
Expected: still ok (PLAY-2 is `[active]` and now has both a core and an e2e reference)

- [x] **Step 4: attempt a harness run**

Run: `cargo build && scripts/cua-e2e/run.sh`
Expected: all scenarios green incl. `play-2-doubleclick-row` with its marker.
If the environment does not permit an Xvfb/AT-SPI run (sandbox): do NOT claim
green — record it in the ledger as a "deferred host check" (the existing
convention) and leave the run to the host release gate.

- [x] **Step 5: ledger + commit**

```markdown
- cua-e2e: play-2-doubleclick-row scenario + cua_double_click_label helper;
  wiring proof for PLAY-2 (marker "queue set from view").
  <enter the run result or deferred host check> — UX rulebook task 5.
```

```bash
git add scripts/cua-e2e/lib.sh scripts/cua-e2e/run.sh .superpowers/sdd/progress.md
git commit -m "test: cua-e2e wiring scenario for PLAY-2 double-click activation"
```

---

### Task 6: closing gate

**Files:**
- Modify: `.superpowers/sdd/progress.md` (append)

- [x] **Step 1: full merge gate**

Run: `scripts/check-merge-readiness.sh`
Expected: `Merge-readiness checks passed against origin/main` (incl. `== UX traceability ==`)

- [x] **Step 2: closing ledger entry**

```markdown
- UX rulebook foundation complete: the document (60 rule lines, 3 [active],
  1 replaced),
  the AGENTS.md binding, the traceability gate, pilot area C (core + e2e),
  the QUE-1 activation demo. Behavior changes run as [planned] in
  follow-up branches (the queue branch is being worked in parallel) —
  UX rulebook task 6.
```

```bash
git add .superpowers/sdd/progress.md
git commit -m "chore: close out UX rulebook foundation"
```

- [x] **Step 3: leave finishing the branch to the user** — superpowers:finishing-a-development-branch (merge into main vs. waiting for the parallel queue agent in the same branch — the coordination is up to the user).

---

## Addendum: review corrections (2026-07-17)

The two-axis review of this branch confirmed the steps above as carried out,
but triggered five corrections. The task blocks above remain in place as a
record of what was actually carried out; from here on, this addendum is
binding.

- **PLAY-3 → PLAY-3a/PLAY-3b.** Task 4 flipped PLAY-3 entirely to `[active]`,
  although the test covers only the hit-shuffle clause and the second clause
  (changing the filter does not touch the queue) has no assertion. That
  violated the process rule "half implemented → a/b split". Now:
  PLAY-3 `[replaced by PLAY-3a/PLAY-3b]`, **PLAY-3a** `[active] [core]`
  (tested, `play_3a_shuffle_stays_inside_filtered_snapshot`), **PLAY-3b**
  `[planned] [gtk]` — its flip comes with its test.
- **Language.** Tests and scripts are code: English comments, identifiers
  and messages (AGENTS.md "English everywhere"). Only the rulebook itself
  and the design docs are German — deliberately so, that is the project's
  working language. Rule IDs and status tokens (`[active]`, `[planned]`) are
  quoted verbatim in code and thereby keep the rulebook's language.
- **Gate hardened** (`scripts/check-ux-traceability.sh`): the prefixes are
  derived from the document instead of hardcoded (a new section is thereby
  gated automatically); only `#[test]` functions count as coverage, not
  every helper fn of the same name; comment lines in `scripts/cua-e2e` no
  longer count as a scenario reference; the ignore format
  `UX <ID> [planned] — …` is enforced instead of merely documented.
- **Duplicate removed:** `cua_click_label` / `cua_double_click_label` share
  `cua_pointer_action_label <verb>` in `scripts/cua-e2e/lib.sh`.
- **Wrong sanity check** in task 1 step 2 corrected: `grep -c '[aktiv]'` was
  never `0` (the process-rule prose contains the token); the check now
  counts only rule lines.

**Open (not a blocker, deliberately deferred):**

- PLAY-2 is `[active] [core]`; the gating core test proves the `set_tracks`
  semantics, not the double-click wiring. The proof for that lies in the
  cua-e2e scenario `play-2-doubleclick-row`, which is not part of the merge
  gate — so a wiring regression does not break the gate. The e2e run is
  deferred to the host gate check (task 5 step 4).
