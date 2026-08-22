# Releases: multi-selection and a context menu

## Problem

The releases table is the only large table in the app whose rows cannot be
selected in bulk and cannot be right-clicked. Hiding a release is possible, but
only one row at a time, through an inline button that appears in the status
column on hover — and hiding is silent: no confirmation, no way back except
switching the filter scope to the hidden releases and restoring the row there.

Two consequences follow. A user who wants to clear ten uninteresting releases
clicks ten times. A user who hides the wrong row gets no signal that anything
happened, and no offer to undo it.

## Goal

Bring the releases table to the same selection and context-menu grammar the
track list already uses, and give hiding a visible, reversible outcome:

- `Ctrl`/`Shift` multi-selection, anchored the same way the track list anchors it.
- A right-click context menu on rows, for one row and for a selection.
- Hide (and, in the hidden scope, Restore) as menu actions that work on the
  whole selection at once.
- A toast with an `Undo` button after every hide or restore.
- For a single selected row: navigation into the library.

## Non-goals

- Hide stays scoped to the releases view. A hidden release remains fully present
  in the library, in search, and in playback. This is not a blacklist and not a
  deletion.
- No keyboard shortcut for hiding. `Delete` and `Ctrl+H` stay unassigned here.
- No new actions beyond hide/restore and navigation. In particular no queue or
  playback entries in this menu.

## Current state

| Concern | Track list | Releases |
| --- | --- | --- |
| Selection model | `MultiSelection` (`track_list_builder.rs:43`) | `SingleSelection` (`releases_model.rs:57,63,78`) |
| Anchor / range input | `track_list_selection_input.rs`, `track_list_selection_anchor.rs` | none |
| Row context menu | `track_list_context_menu.rs`, model in `track_menu.rs:155` | none — the only secondary-button gesture is the column-header popover (`releases_columns.rs:762`) |
| Hide | — | inline button in the status column, `releases_columns.rs:149-309`, calls `on_set_hidden(mbid, !hidden)` at line 172 |

The data already supports everything this design needs. `new_releases` carries
`hidden INTEGER NOT NULL DEFAULT 0` and `hidden_at`
(`db_new_releases_accent.rs:72,75`). `set_release_hidden`
(`artist_news_query.rs:400`) writes one row. `artist_news_view.rs:110` filters
the loaded list by the scope's `hidden` flag, so a visible list never mixes
hidden and visible rows. `HistoryEntry` (`artist_news_history.rs:32-46`) carries
`release_group_mbid`, `artist_name`, `title`, `presence` and `local_track_count`.

Reprise has no artist or album IDs. Navigation is name-based throughout:
`NavigationIntent::OpenArtist { artist: ArtistKey::new(name), .. }` and
`OpenAlbum { album: AlbumKey::new(album, album_artist), .. }`
(`browser/navigation.rs:43-73`, `browser.rs:16-19,41-43`), routed by
`MetadataNavigator::navigate` (`window/metadata_navigation.rs:97`). The releases
row therefore navigates by `artist_name` and `title`, exactly as the track list
navigates by `album_artist` and `album`.

## Design

### 1. Shared selection logic

A new widget-free module `ui/table_selection/` owns two things and nothing else:

- **Anchor and range resolution.** Shift claims a range from the stored anchor
  rather than from the current selection; `Ctrl+Shift` extends additively
  (NAV-17).
- **The context-click rule.** Given a clicked position and whether that position
  is currently selected, it returns whether the selection is replaced by the
  clicked row or left as it is. Right-clicking an unselected row replaces the
  selection; right-clicking inside an existing selection preserves it.
The context-key predicate stays where it is. `track_list_context_keys.rs:11-14`
already owns it and `source_context_surface.rs:91` already re-exports it to the
source tables; releases calls the same function rather than adding a third copy.

Both functions take and return plain values — positions, lengths, a small intent
enum. No `SelectionModel`, no widget, no GTK types beyond `u32`.

`track_list_selection_input.rs` and `track_list_selection_anchor.rs` are rewired
to call these functions; their existing tests move with the logic, so NAV-17 and
the context-click rule keep exactly one home. The releases view gets its own
thin wiring onto the same functions.

This is the deliberate middle path. Duplicating the anchor mathematics into the
releases view would let the two tables drift apart the first time either rule is
touched; extracting the gesture and popover machinery as well would mean
reworking the most heavily tested part of the app for no behavioural gain.

### 2. Selection model

`releases_model.rs` replaces `SingleSelection` with `MultiSelection`. Every
reader that treats `selection.selected()` as "the one row" moves to a position
list. The two places that need care:

- Row activation (double-click / Enter) keeps its single-row meaning.
- Restoring the selection after a reload works from `release_group_mbid`, not
  from positions — the list can be reordered or refiltered between the write and
  the restore.

### 3. The row surface

Releases joins the interaction layer the source tables already share.
`ui/source_context_surface.rs` exists for exactly this: `wrap()` gives a cell the
full-row hit area (Adwaita owns `columnview > listview > row > cell` padding, so
a gesture on the factory's own child leaves roughly half the row inert),
`secondary_click()` and `context_keys()` hand out capture-phase controllers, and
`TABLE_CSS_CLASS` moves the padding onto the surface. Radio already does this;
releases opts in the same way, which means every cell factory in
`releases_columns.rs` builds its child through `wrap`.

This replaces the bespoke gesture plumbing an earlier draft of this design
assumed. `radio_context_menu.rs` — 266 lines, a pure `build(row, …) -> gio::Menu`
plus `wire_gesture`/`wire_keyboard` — is the template, not the much larger track
list.

### 4. Context menu

New `releases_context_menu.rs` holds the `GestureClick`
(`gdk::BUTTON_SECONDARY`), a key controller for the Menu key / `Shift+F10` path,
the `PopoverMenu`, and a `gio::SimpleActionGroup` named `releases`, inserted on
the `ColumnView`. A pure menu-model builder, shaped after
`track_menu.rs::build_track_menu` — which takes a summary struct and returns a
`gio::Menu`, and which `track_list_context_menu.rs::build_context_menu_model`
feeds — produces the model on each open. Opened by keyboard, the popover points at the focused row; opened
by mouse, at the pointer.

The model is built from a summary of the *selected entries* — count, hidden
state, artist name, `local_track_count` — never from the filter-bar scope. The
scope and the rows agree today because `artist_news_view.rs:110` filters on it,
but deriving the label from the rows means the menu cannot lie if that ever
changes.

Sections:

| Section | Entry | Shown when |
| --- | --- | --- |
| primary | `Hide` / `Hide N releases` | selected entries are visible |
| primary | `Restore` / `Restore N releases` | selected entries are hidden |
| navigation | `Go to artist` | exactly one row selected |
| navigation | `Go to album` | exactly one row selected **and** `local_track_count > 0` |

Counts follow CTX-6: singular for one row, `Hide 5 releases` for five.
Navigation disappears for multi-selection (CTX-4: navigation needs an
unambiguous target). `Go to album` is gated on local presence because the
MusicBrainz release title is matched against the free-text `tracks.album` field;
without a local track there is nothing to open and the entry would lead into an
empty view.

The existing column-header popover (`releases_columns.rs:762`) is untouched. The
row gesture lives on the cell widgets, the header gesture on the header — as in
the track list.

### 5. Hide, restore, and undo

`reprise-core` gains a batch sibling of `set_release_hidden` that writes a slice
of `release_group_mbid` values in **one** transaction. This is not tidiness: undo
has to take back exactly the set that was written. A loop of single writes can
stop halfway and leave a state that undo can no longer address precisely.

After the write, the view raises a toast through the existing
`toasts::show_with_action` helper:

```
5 releases hidden                                    [ Undo ]
```

The undo callback holds the MBID list and the previous hidden state — not row
positions, since the list may have reloaded in the meantime. It calls the same
batch writer with the state inverted.

Selection after a hide: the hidden rows leave the current scope, so the
selection moves to the row that took the place of the first hidden row; failing
that, to the last row; if the list is now empty, the empty state shows. A stale
selection pointing at departed rows is not acceptable.

Undo brings the rows back **and** re-selects them, so the user can see what
returned.

The toast runs ten seconds, the value FB-7 cites for every undo toast in the
app, not the short 4 s informational default in `toasts.rs`.

Two seams do not exist yet and have to be laid: `ReleasesView` reaches neither a
`ToastOverlay` nor the `MetadataNavigator`. Both follow patterns already in the
codebase — a `glib::WeakRef<adw::ToastOverlay>` with a `set_toast_overlay` setter
(`track_list.rs:258,550`), and a callback slot for navigation intents in the
shape of the view's existing `set_on_refreshed` (`releases_view.rs:276`), wired
at the window where the track list's navigation is wired.

The radio table solves its own undo differently — SRC-4a tombstones a removed
station and commits only when the toast expires. That is the right shape when
the write is a deletion. Hiding is a boolean on a row that stays, so writing
immediately and writing back on undo is simpler and survives an app exit with a
pending toast without needing a commit-on-launch rule.

### 6. The inline button is removed

The hide/restore button in the status column (`releases_columns.rs:149-309`) is
deleted: the `Button`, the `Stack` that swaps it against the status pill, and the
motion/focus controllers that reveal it. The pill itself stays. No test covers
that button today, so this removes no coverage — it closes a gap the new menu
tests fill.

There is precedent: SRC-4a took the same step for radio favorites — *"Removing a
favorite is operated from the context menu alone; there is no hover star."*

Stated tradeoff, recorded here so it is not rediscovered later as a bug: after
this change hiding is reachable by secondary click and by the Menu key /
`Shift+F10` path, and by nothing else. Touch loses its affordance entirely, and
a plain left-click no longer offers any way to hide. The recovery side is
unaffected; the filter bar keeps its `N hidden · Show` affordance and the hidden
scope keeps working. It is the entrance that narrows, not the exit.

## UX rules

- A new rule in the `R. New releases` block, worded after SRC-14 — the podcast
  episode table's active rule for exactly this grammar — so both tables read the
  same. It cites CTX-2 (right-click claims an unselected row), CTX-4 (navigation
  needs an unambiguous target) and CTX-6 (count currency).
- **NR-4 is not the rule to touch.** It was superseded long ago: NR-4 → NR-12 →
  NR-12a → NR-16 → NR-24. The rule this work falsifies is **NR-39** (line 2561),
  which claims that hiding the Status column "removes the visible routes for
  hiding a release" — after this change the Status column is not a route to
  hiding at all. NR-39's hide clause is struck; its link-column clause stands.
- NAV-17 is restated to cover both tables, matching the shared module, and to
  name the branch that applies when a table has no playing row.
- The lost touch path is written into the rule text as a known limitation.

## Verification

| Claim | How it is shown |
| --- | --- |
| Shift anchors from the anchor, Ctrl+Shift extends | unit tests on `ui/table_selection/`, the migrated track-list cases plus releases cases |
| Right-click keeps an existing multi-selection, replaces a single unselected row | unit test on the context-click rule |
| Menu key and `Shift+F10` open the menu, other keys do not | the migrated `track_list_context_keys.rs` cases, now covering both tables |
| Menu labels, counts, presence gate, hidden scope | menu-model tests over a built selection summary — assert the entry list, not just that a menu exists |
| Hide writes every selected row | test against a real SQLite database, reading the rows back |
| Undo restores exactly the hidden set | control arm: a second release that was not part of the selection must stay untouched; assert row content before and after, not a return value |
| Selection after hide is not stale | assert the selected MBID after the write, including the empty-list case |

## Work location

This work starts from a worktree on `origin/dev`
(`feature/releases-multiselect-context-menu`). The `local-centering-rebuild`
branch in the main checkout carries unrelated unfinished centering work and is
not a base for this.
