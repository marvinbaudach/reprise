# GUI-C: Browse Bar + Rhythmbox Column Layout Import — Implementation Plan

**Goal:** Add exact Genre/Artist/Album library faceting and an explicit,
read-only import of Rhythmbox's visible column order.

**Baseline:** 423 passed; 1 ignored. Core must remain free of
GTK/libadwaita/GStreamer/zbus. Every task follows RED → GREEN, all gates,
one commit, adversarial review, and ledger update. No real Reprise DB/music
or user dconf is used by tests/smokes.

## Global constraints

- Every SQL value is bound; no facet value is interpolated.
- Browse filters affect Library only and combine with text search by `AND`.
- Window/count/ids/stats must describe the same set.
- Rhythmbox GSettings are read-only and frontend-only.
- Every created/substantially edited source file is `< 800` lines.
- Every app smoke contains dbus-run-session, Xvfb, isolated XDG data/cache,
  X11 backend, unset Wayland display, and fakesink.

## Task 1 — Pure browse filter and bound SQL clauses

**Files:** create `crates/reprise-core/src/queries/browse.rs`; modify
`queries/mod.rs`, `queries/clauses.rs`.

**Interfaces:**

```rust
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BrowseFilter {
    pub genre: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
}

impl BrowseFilter { pub fn is_empty(&self) -> bool; }

pub(super) fn browse_clause(filter: &BrowseFilter, first_param: usize)
    -> (String, Vec<String>);
```

The clause appends `AND genre = ?N`, then artist, then album for present
fields, returning values in exactly placeholder order. Values including
quotes, `%`, `_`, and SQL-looking text remain bound data.

RED tests:

```rust
#[test] fn empty_browse_filter_has_no_clause_or_values();
#[test] fn browse_clause_numbers_only_present_fields_in_canonical_order();
#[test] fn hostile_facet_value_never_appears_in_sql_text();
```

Expected: 426 passed; 1 ignored.

Commit: `feat: add bound library browse filter clauses`

## Task 2 — Consistent Library window/count/ids/stats filtering

**Files:** modify `queries/library.rs`, `queries/clauses.rs`, `queries/mod.rs`,
query tests, `track_list_model.rs`, `track_list.rs`, `status_bar.rs` as needed.

**Interfaces:** add `browse: &BrowseFilter` to Library-facing public query
entry points and model query state. Non-Library sources pass/ignore
`BrowseFilter::default()`. `query_library_stats` gets the same filter.

Use `rusqlite::params_from_iter` where dynamic browse arity makes fixed
`params![]` impossible. Parameter order is always fixed arguments first,
optional text-search LIKE next, then genre/artist/album.

RED integration test seeds four tracks and proves one combined
`genre=Rock, artist=A, search=live` selection yields the same one id from:

```rust
query_track_window(...)
query_track_count(...)
query_track_ids(...)
query_library_stats(...)
```

Also prove a BrowseFilter passed with `ViewSource::Playlist` has no effect.

Expected: 428 passed; 1 ignored.

Commit: `feat: apply browse facets consistently to library queries`

## Task 3 — Cascading facet value queries

**Files:** modify `queries/browse.rs`, `queries/mod.rs`; add tests.

**Interfaces:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseFacet { Genre, Artist, Album }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseValue { pub value: String, pub count: i64 }

pub fn query_browse_values(
    conn: &Connection,
    facet: BrowseFacet,
    filter: &BrowseFilter,
) -> Result<Vec<BrowseValue>, rusqlite::Error>;
```

Genre ignores all facet selections; Artist applies Genre; Album applies
Genre+Artist. Always `missing=0`, group exact values, sort NOCASE with empty
first. Counts are track counts, not album/artist counts.

RED tests:

```rust
#[test] fn artist_values_are_constrained_by_genre();
#[test] fn album_values_are_constrained_by_genre_and_artist();
#[test] fn empty_metadata_is_returned_as_typed_empty_value();
```

Expected: 431 passed; 1 ignored.

Commit: `feat: query cascading genre artist and album facets`

## Task 4 — GTK browse bar and exact handler integration

**Files:** create `crates/reprise-gnome/src/ui/browse_bar.rs`; modify
`ui/mod.rs`, `track_list.rs`, `track_list_model.rs`, `strings.rs`.

**Interfaces:**

```rust
pub struct BrowseBar { /* GTK widgets + values + update guard */ }
impl BrowseBar {
    pub fn new(conn: Rc<RefCell<Connection>>) -> Self;
    pub fn widget(&self) -> &gtk4::Widget;
    pub fn set_on_changed(&self, f: impl Fn(BrowseFilter) + 'static);
    pub fn set_library_visible(&self, visible: bool);
    pub fn refresh(&self);
}
```

Build a homogeneous horizontal box with labelled DropDowns. Store raw values
separately from display labels (`Unknown …`). During model replacement set an
update guard so GTK selection notifications never recursively reload.

TrackList root becomes a vertical Box containing BrowseBar + existing Stack;
`widget()` returns that root. Genre change resets Artist+Album; Artist change
resets Album. Source changes only toggle visibility; the in-session filter is
retained but ignored outside Library. Reload refreshes facet counts after
library mutations without re-entry.

Add `REPRISE_SMOKE_BROWSE=genre:<g>|artist:<a>|album:<b>`; it selects by raw
value through the same handler and logs the final BrowseFilter/count/ids.
Isolated E2E combines it with `REPRISE_SMOKE_FILTER` against copied fixtures.

Expected: 433 passed; 1 ignored (pure selection/reset helper tests).

Commit: `feat: add cascading library browse bar`

## Task 5 — Column IDs, persistence, and GTK registry

**Files:** create `ui/column_layout.rs`; modify `track_list.rs`,
`track_list_columns.rs`, `strings.rs`, core `library/settings.rs`.

**Interfaces:**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColumnId { Cover, Title, TrackNumber, Artist, Album, Genre, Year, Duration, Rating }

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnLayout { pub order: Vec<ColumnId>, pub visible: HashSet<ColumnId> }

pub const COLUMN_LAYOUT_KEY: &str = "ui.column_layout";
pub fn serialize_layout(layout: &ColumnLayout) -> String;
pub fn parse_layout(value: &str) -> Option<ColumnLayout>;
```

Add Track number and Genre columns; hidden in default layout. Build every
column once, collect `(ColumnId, ColumnViewColumn)`, apply persisted/default
visibility and order via GTK's reorder API. Cover/Title are forcibly present,
visible and first even for a hand-edited setting. Persist only canonical IDs.

RED tests cover round-trip, duplicate/unknown rejection, forced Cover/Title,
and corrupted-setting fallback.

Expected: 437 passed; 1 ignored.

Commit: `feat: persist and apply typed track column layouts`

## Task 6 — Read-only Rhythmbox mapper and import action

**Files:** modify `column_layout.rs`, `primary_menu.rs`, `window.rs`,
`strings.rs`.

**Interfaces:**

```rust
pub fn import_rhythmbox_tokens(tokens: &[String]) -> ColumnLayout;
pub fn read_rhythmbox_visible_columns() -> Result<Vec<String>, ImportError>;
pub fn TrackList::apply_column_layout(&self, layout: &ColumnLayout)
    -> Result<(), rusqlite::Error>;
```

The pure mapper uses the design table, stably deduplicates, ignores unknown
tokens, fixes Cover/Title first, and appends hidden supported columns. The
reader first looks up the schema through `gio::SettingsSchemaSource`; absence
is a typed error, never a constructor panic. It only calls `strv`/reads the
key—never `set_*`, `reset`, or dconf commands.

Move `primary_menu::install` after TrackList construction (net zero lines in
`window.rs`) and pass `&Rc<TrackList>`. Add `win.import-rhythmbox-columns`.
On read+persist success apply immediately and toast; on either error keep the
old layout and toast.

`REPRISE_SMOKE_RHYTHMBOX_COLUMNS=rating,duration,album,artist,date,post-time`
bypasses real user GSettings, invokes the same mapping/apply/persist handler,
and logs `cover,title,rating,duration,album,artist,year` plus the scratch DB
setting.

RED tests:

```rust
#[test] fn rhythmbox_mapping_preserves_supported_order_and_ignores_unknown();
#[test] fn rhythmbox_mapping_stably_deduplicates_tokens();
#[test] fn rhythmbox_empty_list_still_keeps_cover_and_title();
```

Expected: 440 passed; 1 ignored.

Commit: `feat: import Rhythmbox visible column layout read-only`

## Task 7 — GUI-C integration smoke and stage close-out

**Files:** smoke helpers/docs/ledger only unless a verified integration fix
is required.

Run all gates, audit (only accepted paste advisory), standalone core build,
purity and touched-file size proof. Run fully isolated:

1. Browse + text-search smoke: exact final count/ids.
2. Rhythmbox env-fixture import: exact visible order + scratch setting.
3. Source switch Library → Playlist/Queue: browse bar hidden and no facet
   leakage.

Whole-branch review must compare window/count/ids/stats, callback borrow
lifetimes, dropdown rebuild guard, source isolation, GTK column registry, and
strict read-only GSettings use. Record manual checks: real dropdown rendering,
keyboard navigation, narrow window, real Rhythmbox import.

Commit: `docs: close GUI-C browse and column import stage`
