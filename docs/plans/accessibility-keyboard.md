# Accessibility & keyboard — implementation plan

As of: 2026-07-18
Starting base: `main` at `e0493d0`
Working branch: `feat/accessibility-keyboard`

## Goal

Every Reprise surface that ships today must be comprehensible, complete and
free of focus dead ends when operated with the keyboard alone.
The plan does not treat keyboard operation as a collection of additional
shortcuts, but as an end-to-end contract made up of:

1. full parity with mouse/touch actions;
2. logical focus order and a stable focus lifecycle;
3. native semantics for GTK, AT-SPI and screen readers;
4. visible focus and focus-equivalent hover affordances;
5. keyboard alternatives for drag-and-drop and custom value controls;
6. automated coverage of every GUI surface plus honest manual
   GNOME acceptance.

The corresponding proposed rules are recorded as `ACC-1` to `ACC-9` in
`docs/ux-rules.md`. They stay `[planned]` until they are fully implemented and
have their rule-named proof.

## Scope boundary

This plan covers all existing surfaces in `reprise-gnome`:
main window, sidebar, Tracks/Albums/Artists, Queue/Playlists, filter,
player bar, Now Playing/Lyrics, Issues, Devices/Sync, Stats, Preferences,
First Run, tag editor, import/confirmation dialogs, popovers, compact/minimal
view and portal dialog invocations.

Not part of this stage are:

- a visual redesign beyond the necessary focus indicators;
- new product features or new roadmap views;
- full WCAG certification;
- GNOME/GTK upstream fixes;
- native Wayland, media key or lock screen verification;
- changes to the core data model, unless a genuine keyboard operation
  absolutely requires them.

Screen reader semantics, High Contrast and Large Text are nevertheless
checked, because an apparently working keyboard path without a name, role or
visible focus is not a dependable accessibility solution.

## Normative basis

- The [GNOME HIG on keyboard operation](https://developer.gnome.org/hig/guidelines/keyboard.html)
  demands parity with pointer actions, a logical tab order as well as the
  standard semantics for Tab, Shift+Tab, Enter, Space, F10,
  Menu key/Shift+F10 and Esc.
- The [GNOME HIG on accessibility](https://developer.gnome.org/hig/guidelines/accessibility.html)
  demands short descriptive accessible names and real checks with
  keyboard, High Contrast, Large Text, screen reader and on-screen keyboard.
- The [GTK4 accessibility documentation](https://docs.gtk.org/gtk4/section-accessibility.html)
  treats a role as a promise of behavior: a custom surface made clickable by
  a gesture needs not only the `Button` role, but also an activatable action
  and the expected keyboard semantics. Non-standard
  interactions need accessible help text.
- The [GNOME reference for standard shortcuts](https://developer.gnome.org/hig/reference/keyboard)
  is binding for existing standard actions; custom bindings must not displace
  the system and access keys.
- WCAG 2.2 serves as a supplementary review heuristic for
  [focus order](https://www.w3.org/WAI/WCAG22/Understanding/focus-order.html)
  and [visible focus](https://www.w3.org/WAI/WCAG22/Understanding/focus-visible).
  GTK/GNOME conventions remain paramount for the concrete desktop
  interaction.

## Working decisions

### Native controls before custom gesture semantics

When an element behaves like a button, toggle, link or range, a corresponding
GTK/libadwaita control is used first and only adjusted visually. A
`GestureClick` on `Label`, `Image`, `Box` or `DrawingArea`
stays only when a native control is technically unsuitable and role,
name, action, focus, keys and tests are supplied in full.

### One tab stop per collection, secondary actions in context

Sidebar, `ColumnView`, `GridView`, `ListBox` and comparable collections
are one tab stop each. Arrows move the active entry. Sub-actions
of a row/card do not automatically produce a long chain of nested
tab stops: frequently used native buttons may stay focusable; further
secondary actions live in the context menu reachable via
Menu key/Shift+F10. The mouse path and the keyboard path invoke the same
action.

### Focus is logical identity, not widget identity

Many Reprise views rebuild GTK widgets on filter, scan, device update or
navigation. Focus restoration therefore stores a stable
domain identity (`track_id`, album key, artist name, device ID,
issue/playlist ID) and not merely a position or an old `Widget`.
When the target disappears, the following applies deterministically: next
operable target, otherwise the previous one, otherwise the stable view
container.

### Global shortcuts are focus-sensitive

Space triggers play/pause on passive content, in passive collections and even
when the left sidebar toggle is focused; the toggle collapses the
sidebar only via pointer or Enter. Other buttons, toggles and value controls
focused via keyboard keep Space locally. The same
focus principle applies to Enter, Escape, arrows and page keys.
Popovers/dialogs and text entries always win over window shortcuts. This
priority is tested with real key events, not derived from controller
order.

### No pointer coordinates as accessibility proof

The final keyboard scenarios use AT-SPI targets, key events and
focus states. Pointer E2E remains useful for hit testing/DnD feel, but proves
no keyboard operability. Every app run stays isolated from user data and
desktop with a private D-Bus, Xvfb, scratch XDG and
`REPRISE_AUDIO_SINK=fakesink`.

## Existing-state analysis at `e0493d0`

### Already good foundations

- Track lists activate via Enter and open their context menu via
  Menu key/Shift+F10.
- The album grid has arrow navigation, Enter activation, a keyboard
  context menu and its own `:focus-visible` ring.
- The sidebar separates focus browsing from activation; mere tabbing/arrowing
  no longer routes into a different view.
- With TAG-8, the tag editor has detailed Enter/Esc/Tab
  semantics and Ctrl+Page-Up/Down navigation.
- The column editor offers Alt+Arrow as an alternative to the internal reorder
  and exposes `KeyShortcuts`.
- Compact view and track/album menus already have keyboard
  context menu paths.
- Standard controls such as Button, Switch, ComboRow, DropDown, Scale,
  SearchEntry and native libadwaita dialogs come with a dependable
  base contract.

### Proven or highly likely gaps

| Surface | Finding | Risk |
|---|---|---|
| Player bar | Cover, title and artist are passive `Image`/`Label` with `GestureClick` | pointer-only actions, no role/name/focus |
| Waveform | `DrawingArea` with `GestureDrag`, without a key or range contract | seek is not reachable by keyboard |
| Artist detail | Top track is a `Box` with a double-click gesture | Enter/Space and focus are missing |
| Lyrics | Synced lines are seekable via gesture | NPP-8 is operable only by pointer |
| Album card | The artist subtitle is a clickable `Label`; Play is a nested hover button | unclear/duplicated focus paths |
| Sidebar activity | Device, scan and relink cards activate via gestures on containers | the card is semantically/passively inert for keyboard/AT-SPI |
| Issues | Row pills appear only on hover; several context menus are wired only for right-click | actions are not discoverable/reachable |
| DnD | Queue/playlist reorder and drop onto sidebar targets are pointer-centric | no equivalent reorder/add operation |
| Search | A second Esc always focuses the track list | wrong target in Albums/Artists/Stats/Issues/Device |
| View rebuilds | several views remove children completely and rebuild them | focus loss possible on filter/sync/refresh |
| View switcher CSS | `outline: none` without a local `:focus-visible` replacement | keyboard focus can be invisible |
| Semantics | explicit names/roles/states are set only in places | AT-SPI/Orca can report controls incompletely |
| Dialogs/popovers | many custom Esc and stack paths, no shared focus contract | focus cannot return to the trigger |

### Still to be verified live

The CUA system probe is blocked in the current sandbox before app start with
`Operation not permitted`. That is a host/socket limit, not a
Reprise finding. For that reason the following statements are explicitly
hypotheses until the isolated run on an AT-SPI-capable host:

- the concrete tab order and visible focus in the full window;
- the actual focus trap/return of AdwDialog, FileDialog and popovers;
- GTK's real priority between global Space and focused controls;
- accessible names/states that GTK derives implicitly from tooltip/label;
- Orca output and High Contrast rendering.

## Target inventory of the keyboard flows

Every line is reached, operated and left again at least once in the final CUA
sweep using the keyboard alone.

| Area | Must be covered |
|---|---|
| App shell | Start focus, header, sidebar toggle, back/forward, view switcher, search, primary menu, shortcuts/help/about |
| Sidebar | Places, playlists, New/Import Playlist, Issues, device card, scan/relink cards, collapse/overlay mode |
| Tracks/Playlist/Queue | Arrow/Home/End/Page, multi-select, Enter, rating, sort, filter, context menu, queue sections |
| Albums | Grid roving focus, open, play/queue, artist target, context menu, back/forward focus |
| Artists | Master list, detail, album cards, top tracks, show all, hero menu |
| Player/Now Playing | Transport, shuffle/repeat, volume, queue, cover/title/artist, panel tabs, Up Next, lyrics, waveform |
| Issues/Import | Groups, collapse, row selection, pills/menus, locate, remove/undo, retry/dismiss/export |
| Device/Sync | Filter chips, track list, sync/cancel, settings, eject, add-to-playlist drop alternative |
| Preferences | Page navigation, all switch/combo/scale/entry surfaces, scrobbler, sync, column editor |
| Modals | First Run, tag editor, confirm/discard/delete/locate, import progress, FileDialog, About/Shortcuts |
| Stats | Year selection, scrolling, non-interactive charts/lists without wrong focus stops |
| Compact/Minimal | Restore, transport, volume, menu, always-on-top, Preferences, quit, Escape/Ctrl+W |

## Implementation — task by task, strictly TDD

Every task starts with a red test, introduces only the smallest necessary
change, runs through all repo gates, is checked adversarially against rules and
inventory and ends in exactly one commit. The ledger line is added after
the commit. No rule is set to `[active]` before its full coverage.

### Task KBD-1 — Extend the keyboard/AT-SPI test foundation

**Goal:** The existing CUA harness can prove real key events, focus states,
tab sequences, window changes and focus return.

**Red:**

- Add contract tests for the still missing `cua_press_key_label`,
  `cua_press_key_focused`, `cua_hotkey`, `assert_focused_label`,
  `assert_focus_within` and `assert_focus_returns_to`.
- A fake driver scenario must fail when no before/after snapshot
  exists, the key lands at the wrong PID or focus after the action does not
  carry the expected semantic node.
- The runner must react fail-closed to `degraded`, `suspected_noop`,
  escalation and missing `focused` states.

**Green:**

- Extend `scripts/cua-e2e/lib.sh` with the key/focus primitives.
- Expand `scripts/tests/cua-e2e.sh` with a deterministic fake tree.
- Create `scripts/cua-e2e/keyboard.sh` as a separate scenario runner,
  independent of the pointer sweep.
- A manifest holds the surface list above and the corresponding scenario;
  missing areas make the contract test fail.

**Verification:** shellcheck/contract test, then the full gates. The real
CUA run may be documented only as a `deferred host check` when the host is
blocked, never as green.

**Commit:** `test(a11y): add keyboard and focus acceptance primitives`

### Task KBD-2 — Harden the shell, focus target and shortcut priority

**Goal:** App shell, sidebar and global actions form a stable
focus graph.

**Red:**

- Display test: a second search Esc returns focus to Tracks, Albums, Artists,
  Stats, Issues and Device, each to their active container.
- Display test: sidebar arrow changes focus/selection, routes only on
  Enter/Space.
- Key delivery test: Space toggles playback on passive content and in
  passive collections, but never on an entry, a button/toggle focused via
  keyboard, a range or an open popover/dialog. Independently of that, the
  left sidebar toggle always remains a global play/pause target.
- Tests for F10, Ctrl+W, Ctrl+Q and the synchronicity of the help list.

**Green:**

- An `ActiveContentFocus` adapter replaces the fixed TrackList dependency
  of the Esc logic; every view exposes exactly one stable focus target.
- Global actions consult a central focus/transient decision instead of
  individual widget exceptions.
- Standard shortcuts are wired as actions; existing
  Alt+Left/Right, Ctrl+F/L/,/? and F1 paths remain unchanged.
- Navigation and back/forward save/restore focus logically per view.

**Affected files:** `ui/shortcuts.rs`, `ui/help.rs`,
`ui/window/window_runtime_wiring.rs`, `ui/window/library_shell.rs`,
`ui/sidebar/sidebar_row_wiring.rs`, the focus adapters of the views.

**Commit:** `fix(a11y): harden shell focus routing and shortcut scope`

### Task KBD-3 — Make library collections keyboard-complete

**Goal:** Tracks, Albums and Artists are operable as native roving
collections, without nested or pointer-only actions.

**Red:**

- Track list: Tab lands once in the `ColumnView`; Arrow/Home/End/Page move,
  Enter activates, Space selects, Menu/Shift+F10 opens on the
  keyboard selection.
- Album grid: card open, play/queue, artist navigation and context menu are
  reachable from the focused card item; no duplicate card/child stop.
- Artist: master selection does not activate on a mere focus change;
  album cards and top tracks have Enter/menu paths.
- Back/forward restores the collection and the logical entry.

**Green:**

- Grid/list-native activation stays the primary path.
- Secondary card/row actions move into the keyboard context menu or into
  real controls; existing mouse actions delegate to the same actions.
- Passive double-click boxes are replaced by native rows/buttons or a
  fully semantic action surface.
- Selection/focus and playing marker stay separate states.

**Affected files:** `ui/track_list/*`, `ui/library_views/album_*`,
`ui/library_views/artist_*`, `ui/nav_history.rs`.

**Commit:** `fix(a11y): make library collections keyboard complete`

### Task KBD-4 — Open up Issues, devices and activity cards

**Goal:** Every card, row and inline action in Issues/Import/Device/Progress
has a comprehensible focus and activation path.

**Red:**

- Scan, relink and device cards must expose name, role, state and activation
  via Enter/Space.
- Missing/import row context menus open via Menu/Shift+F10 on the current
  selection.
- Hover pills are visible on row focus or fully represented in the keyboard
  context menu.
- Rebuild/collapse/retry/dismiss/remove keeps focus logical; removed rows
  use the ACC-6 fallback order.

**Green:**

- Replace container gestures with `Button`/`ActionRow` or make them fully
  semantic via a shared action surface helper.
- Pointer and keyboard menus use the same model/action builder.
- `busy`, `expanded`, `disabled`, progress name/value and cancel action are
  updated dynamically.

**Affected files:** `ui/issues/*`, `ui/import_errors_view.rs`,
`ui/scan/scan_progress.rs`, `ui/sidebar/sidebar_device_card.rs`,
`ui/device_view/device_view.rs`.

**Commit:** `fix(a11y): expose issue device and activity surfaces to keyboards`

### Task KBD-5 — Operate player, waveform and lyrics

**Goal:** All player and Now Playing actions work without a
pointer.

**Red:**

- Cover, title and artist of the player bar have unambiguous focus stops,
  names and Enter/Space activation with the same callbacks as click.
- Transport/volume/queue keep native keys; global Space does not disturb
  them.
- The waveform reports range min/max/now/text and supports Arrow,
  Page-Up/Down, Home/End with a single seek commit per key.
- The lyrics list is a roving focus container; arrow moves, Enter seeks only
  with synced lyrics; unsynced text is not a wrong action stop.
- The Now Playing tabs expose TabList/Tab/Selected/Controls correctly.

**Green:**

- Turn passive player metadata into real flat controls or complete
  action surfaces.
- `WaveformSeek` gets a central `SeekStep` decision, focusable
  range semantics and an accessible time value; drag and keys call the same
  commit path.
- Lyrics lines use list/row activation instead of gesture-only click.

**Affected files:** `ui/player_bar/*`, `ui/now_playing/*`, `ui/lyrics/*`,
`ui/playback/*`, `ui/compact/*`.

**Commit:** `fix(a11y): make player waveform and lyrics keyboard operable`

### Task KBD-6 — Unify the dialog/popover focus contract

**Goal:** Every transient layer starts, contains, closes and restores
focus deterministically.

**Red:**

- Tab/Shift+Tab do not leave an open dialog/popover.
- Esc cascades: autocomplete → tag editor → the triggering library row;
  browse chooser → browse button; context menu → the focused row/card;
  confirmation → the triggering button/row.
- First Run starts on the primary sensible action; Preferences on the
  chosen page; the Rhythmbox import keeps focus across selection → progress →
  complete.
- Ctrl+W closes the topmost closable layer, without triggering an underlying
  app action.

**Green:**

- A shared `TransientFocusGuard` stores the weak trigger, sets
  initial focus after present and restores on close with a stable fallback.
- Custom Esc controllers delegate to a central cascade decision;
  native dialog semantics are not overridden twice.
- Primary actions and frequently used dialog buttons get translatable
  mnemonics; conflicts are tested per surface.

**Affected files:** `ui/tag_edit/*`, `ui/preferences/*`, `ui/first_run.rs`,
`ui/dialogs.rs`, `ui/delete_tracks.rs`, `ui/issues/missing_dialogs.rs`,
`ui/browse/*`, `ui/track_list/column_layout_editor.rs`, `ui/about.rs`,
`ui/help.rs`, `ui/sidebar/sidebar_playlist_creation.rs`.

**Commit:** `fix(a11y): unify dialog and popover focus lifecycle`

### Task KBD-7 — Give DnD and reorder keyboard alternatives

**Goal:** No move/add operation depends exclusively on drag-and-drop.

**Red:**

- Playlist/queue reorder via keyboard produces exactly the same
  `ReorderMove`/`QueueReorderOp` as the drop path and respects sort,
  filter, section and playing guards.
- "Add to playlist/queue" is reachable from the focused track selection via
  the context menu and delegates to the same membership/queue
  functions as the sidebar drop.
- Column reorder via Alt+Arrow stays in sync with DnD; header reorder has
  the same reachable persistence path via the column editor.
- Impermissible moves are disabled and named, never silent no-ops.

**Green:**

- Reorder/add commands are built as shared actions out of the existing
  pure decision functions.
- Context menus offer move up/down/to top or add targets only when they are
  valid for the current context.
- `KeyShortcuts`/help text document non-standard reorder keys.

**Affected files:** `ui/track_list/track_list_dnd.rs`,
`ui/track_list/track_menu.rs`, `ui/track_list/track_list_context_menu.rs`,
`ui/sidebar/sidebar_dnd.rs`, `ui/track_list/column_*`, queue/playlist wiring.

**Commit:** `fix(a11y): add keyboard alternatives for drag and drop`

### Task KBD-8 — Close the semantics, focus and hover audit

**Goal:** The entire GTK tree has honest names/roles/states and
visible focus indicators; new pointer-only surfaces become a gate failure.

**Red:**

- Widget walks over every constructible surface report nameless
  interactive controls, wrong roles, missing `selected/checked/expanded`
  states, decorative duplicate reports and invisible focus stops.
- The CSS gate fails on `outline: none` without an equivalent
  `:focus-visible` rule.
- The input parity gate finds every new `GestureClick`, `GestureDrag`,
  `DragSource`, `DropTarget` and pointer cursor site without a documented,
  tested keyboard partner.
- A display test proves focus indicators at least on shell, switcher,
  track list, grid, sidebar, player, dialog and custom range.

**Green:**

- Add accessible labels/relations/states centrally and update them on state
  changes; take decoration out of the tree.
- Close the view switcher and further CSS focus gaps, without overriding
  theme defaults unnecessarily.
- Hover-only controls get a `:focus-within` rendering or full
  keyboard menu parity.
- A narrow `scripts/check-input-parity.sh` gate demands an explicit
  rule/test reference for every custom pointer/drag surface; no blanket
  file allowlist.

**Commit:** `test(a11y): gate semantics focus visibility and input parity`

### Task KBD-9 — Rule-named end-to-end acceptance and status flips

**Goal:** All automatable ACC rules only become enforceable after a
complete keyboard sweep.

**Red:**

- `acc-1-keyboard-only-surface-sweep` walks the target inventory without a
  pointer action.
- `acc_2_every_interactive_surface_has_name_role_state_and_action` checks the
  widget/AT-SPI contract.
- `acc-3-tab-order-and-roving-collections`,
  `acc-4a-space-routes-global-and-local-controls`,
  `acc-5-transients-and-navigation_restore_focus`,
  `acc_6_dynamic_updates_preserve_logical_focus`,
  `acc-8-direct-manipulation-has-keyboard-equivalence` and
  `acc_9_help_matches_registered_standard_shortcuts` each fail deliberately
  against one reverted implementation.

**Green:**

- Real isolated CUA runs for an empty and a populated profile, narrow and
  wide windows, every inventory target and all transient layers.
- A snapshot after every action; focus, state and visible effect are
  checked together. No coordinates, no silent escalation fallback.
- Only in this commit set `ACC-1/2/3/4/5/6/8/9` from `[planned]` to `[active]`
  and run traceability.
- `ACC-7` stays `[planned]` until the manual visual check.

**Commit:** `test(a11y): activate the automated keyboard accessibility rules`

### Manual GNOME acceptance and stage closeout (not an implementation task)

After KBD-9 comes the real visual and assistive technology check. It is
not another implementation task and produces a commit only when
the check has passed and `ACC-7` together with the release checklist can be
activated honestly.

**Manual matrix:**

1. the complete app with keyboard only, without a pointer;
2. default and High Contrast theme: focus visible at every stop and
   distinguishable from selection/hover/playing;
3. Large Text: no truncated primary controls or unreachable
   actions;
4. Orca: names, roles, states, values and context comprehensible; operation
   possible with the monitor switched off;
5. on-screen keyboard: all entry/autocomplete/save paths usable;
6. real GNOME/Wayland: dialogs, portal FileChooser and shortcut priority;
7. reduced animation: focus/state stays visible (MOT-7).

**Closeout:**

- Record the results with a literal `ACC-7` reference in `RELEASING.md`.
- On a passed visual check set `ACC-7` to `[active]` in the same commit;
  otherwise the rule stays honestly `[planned]` and the concrete
  finding is documented as a manual check.
- Update the full gate battery, file limits, input parity lint, CUA evidence,
  ledger and, if applicable, the coordination board; release the lock.

**Commit on a passed visual check:**
`docs(a11y): activate the visible focus acceptance rule`

## Mandatory gates per task

```sh
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
scripts/check-architecture.sh
scripts/check-ux-traceability.sh
scripts/check-display-tests.sh --rule-named
scripts/check-input-parity.sh          # from KBD-8
git diff --check
```

After changes to `reprise-core`, additionally:

```sh
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'
```

The output must stay empty. Every substantially changed code file ends up
under 800 lines; existing stricter architecture limits continue to apply.

Every real app/CUA run contains in full:

```sh
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  <REPRISE_SMOKE_* hooks> cargo run
```

The existing private AT-SPI harness may build this shell internally;
the evidence must prove scratch XDG, private bus, X11/Xvfb and fake audio.

## Adversarial review per task

Before every commit, the following classes of defect are specifically
looked for:

- pointer and keyboard path run into different callback/guard paths;
- focusable passive controls or active controls without focus;
- duplicate tab stops within the same row/card;
- navigation on focus change instead of only on activation;
- focus is lost or stolen by a rebuild/sort/filter/async update;
- Esc closes several layers or returns focus to a wrong/destroyed
  widget;
- global Space/Enter/Escape/arrow overrides local widget semantics;
- role/state/name does not match the visible function;
- hidden/disabled controls stay in the focus path;
- the DnD alternative bypasses sort/filter/identity/persistence guards;
- the focus indicator is visible only in the default theme or only on hover;
- a test checks only a helper, but not the real signal/action
  wiring;
- a CUA test uses pointer or coordinates and claims keyboard coverage.

## Definition of done for the accessibility stage

- All tasks KBD-1 to KBD-9 are implemented in order and committed.
- The complete GUI inventory has an isolated keyboard CUA flow.
- `ACC-1/2/3/4/5/6/8/9` are `[active]` with rule-named tests.
- All mouse/touch/DnD actions have an equivalent keyboard path
  on the same action/guard/persistence path.
- Focus order, visibility, restoration and dynamic preservation are
  checked on all surfaces.
- Help/shortcuts, accessible properties and actual actions are
  in sync.
- Mandatory gates, core purity and file limits are green.
- Ledger/coordination state are current, the lock is released.
- What remains are exclusively honestly documented manual checks;
  `ACC-7` is not called active without a real visual check.

The next roadmap stage does not begin automatically as a result.
