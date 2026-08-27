# Table column editing everywhere, and one system date (design, 2026-08-09)

Anchored against `origin/dev` at `d9e13dd216`. The local checkout was 74
commits behind when this was surveyed; every file reference below was read
from `origin/dev`, not from the working tree.

## Occasion

Two complaints, one cause.

**One.** The music library lets you right-click a column header and edit the
table: toggle a column, drag it into place, reset. Releases and Concerts do
not, so a behaviour learned in one place stops working in the next. A user
does not experience that as a missing feature in Releases — they experience
it as the app forgetting what it taught them.

**Two.** Dates are written fourteen different ways across twelve files. The
Releases table renders a
full date as `29 May 26` and a month-precision date as `May 2026` — two- and
four-digit years in the same column. The Concerts table writes `Sat, Oct 17`
while the Updates panel writes `Sat, 17 Oct` for the same event: the day and
month even swap places between two views of one fact. Meanwhile the one date
column Reprise fully owns, the library's "Added", already renders
`2026-05-29 14:03`.

Both are the same defect at different scales: a rule that exists in one place
and was re-invented, or forgotten, in the others.

## Decisions

Settled before design, in order:

1. **Scope is app-wide, not two views.** The rule is "every table with a
   column header offers the editor", so future tables are covered by
   construction. Today that means Music, Releases, Concerts and Radio.
2. **The feature set is identical to the music library.** Popover editor
   (visibility, drag reorder, reset), header-drag reorder, header-drag widths
   with persistence. A reduced variant would be a second concept to learn.
3. **The primary-menu entry follows the active view.** "Edit column layout…"
   addresses the table on screen and is insensitive where no table is shown.
   This is also the keyboard route the accessibility rule requires.
4. **Action columns are fixed.** Releases and Concerts have no row context
   menu, so their action columns are the only access to those actions. They
   stay visible and are absent from the editor — the precedent the music
   library's Cover column already sets.
5. **Architecture: typed core, erased surface.** Per-table column enums in
   `reprise-view` behind one `ColumnKey` trait; layout and width persistence
   generic over it; the GTK editor works on a flat `{id, label}` list. The
   persisted format stays compiler-checked and portable; widget code stays
   generic-free.
6. **Releases gets a leading Cover column.** It was the only one of the four
   tables without a leading fixed column, and the cover widget already exists.
7. **Dates follow the system, always.** Not one hard-coded format: the
   locale's date pattern, with the year always four digits. Time of day
   follows the system's 12/24-hour convention.
8. **Concerts drops the weekday.** One date, no exceptions.

## Part A — Column editing in every table

### A.1 Core: `reprise-view/src/columns/`

`columns.rs` becomes a directory. Nothing here knows GTK.

- **`key.rs`** — `trait ColumnKey: Copy + Eq + Hash + 'static` with
  `as_str()`, `parse(&str)`, `all() -> &'static [Self]` and
  `pin() -> Option<Pin>`, where `Pin` is `Leading` or `Trailing`.
- **`layout.rs`** — `Layout<K>` (`order: Vec<K>`, `visible: HashSet<K>`) plus
  `normalize`, `serialize`, `parse`, `set_visible`, `move_before`,
  `move_after`. Generalized from today's
  `track_list/column_layout.rs`; the serialized shape (`order;visible`) is
  unchanged.
- **`widths.rs`** — today's `column_widths.rs`, generic over `K`. Format
  (`id:width` pairs, sorted by id) unchanged.
- **`track.rs`** — today's `ColumnId`, unchanged in its serialized names.
  **`release.rs`**, **`concert.rs`**, **`radio.rs`** — the new enums.

`normalize` generalizes the current Cover special case into the pin model:

1. `Leading` pins first, in the order `all()` declares them.
2. Then the free columns, in the user's stored order.
3. Then `Trailing` pins, in the order `all()` declares them.
4. Pins are forced visible regardless of what was stored.
5. Any column absent from the stored value is appended in `all()` order, so a
   column added in a later release can never become unreachable.

### A.2 Surface: `reprise-gnome/src/ui/table_columns/`

New directory. Sees no `K`.

- **`descriptor.rs`** — `ColumnDescriptor { id: String, label: String }` and
  `trait EditorModel { columns(); is_visible(&str); set_visible(&str, bool);
  move_column(&str, target: &str, after: bool); reset(); title() }`.
- **`editor.rs`**, **`editor_dnd.rs`** — the editor surface lifted out of
  `track_list/column_layout_editor.rs` and detached from `TrackList`. Split in
  two because the original is 653 lines and the 800-line gate is real.
- **`header_popover.rs`** — `install(view, model)`. Keeps the existing
  capture-phase gesture with its claim: `GtkColumnViewTitle` claims every
  press at the target, so a bubble-phase ancestor never sees a header
  right-click. That reasoning is load-bearing and travels with the code.
- **`header_dnd.rs`** — the header drag from `column_header_dnd.rs` (629
  lines), generalized.
- **`registry.rs`** — `ColumnRegistry<K>`. Visibility flips
  `set_visible` and leaves the column in the model, so scroll offset,
  selection and sorting survive; the column list is rebuilt only when the
  order genuinely changed.
- **`width_persistence.rs`** — the 500 ms debounce and the filler handling.

Each table then owns a thin adapter (labels, default order, width policy,
settings keys), roughly 40–60 lines:
`track_list/column_layout.rs` (shrinks to the music adapter),
`releases/releases_column_layout.rs`, `concerts/concerts_column_layout.rs`,
`radio/radio_column_layout.rs`.

### A.3 Fixed and free columns

| Table | Fixed | Free |
|---|---|---|
| Music | Cover (leading) | Title, Artist, Album, Year, Added, Duration, Rating, Play count, Track no., Genre |
| Releases | Cover (leading), Status + Buy (trailing) | Date, Title, Artist, Type |
| Concerts | Tickets (trailing) | Date, Artist, City, Venue, Distance |
| Radio | Artwork, State (leading) | Station, Genre, Bitrate, Country, Now playing |

### A.4 Persistence

`ui.column_layout` and `ui.column_widths` keep meaning the music table —
no migration, no risk to layouts already stored. The other tables get
`ui.column_layout.releases|concerts|radio` and the matching
`ui.column_widths.*`. The key is supplied by the per-table adapter.

### A.5 Reachability

Right-click on the header band in all four tables. "Edit column layout…" in
the primary menu addresses the active view's table through a registry held by
the window and set on view change; it is insensitive where no table is
shown. Preferences → Layout keeps editing the music table, because inside
Preferences there is no "active view" to follow.

### A.6 Edge cases

- **Filler.** Exactly one visible column expands (STYLE-9). When the user
  hides it, the role moves to the first visible free column in the table's own
  column order — stated as order, not as "leftmost", so it stays correct
  under a right-to-left locale. This also closes an existing gap: hiding
  Title in the music library today leaves the table with nothing absorbing
  the slack.
- **Sorting.** Hiding uses `set_visible(false)` and does not remove the
  column from the model, so the sort is untouched. Existing behaviour, kept.
- **Everything hidden.** Each table keeps at least one fixed column, so the
  table never becomes an empty frame.
- **Narrow window.** STYLE-6's temporary collapse still never writes to the
  stored layout.

### A.7 The Releases cover column

`updates/release_cover.rs`'s `LazyReleaseCover` is reused: an accent tile
with the artist's initials immediately, replaced by the real image once
`fetch_release_group_cover` (Cover Art Archive, `front-250`) answers. Pinned
at 40 px like the music library's Cover, leading, never in the editor.

Three things must change for a recycled cell:

1. **Rebinding.** The widget takes its MBID in the constructor today and
   latches a `started` flag on first `map`. In a `ColumnView` cell, GTK
   recycles row widgets, so the next row would inherit the previous release's
   artwork. It needs `set_release(mbid, artist)`, resetting picture, initials
   and flag; `bind` sets, `unbind` clears.
2. **No flicker.** When `release_group_cover_path(mbid)` already resolves, the
   image is set during `bind` rather than through the background task, so
   scrolling does not flash the initials tile on every pass.
3. **Network.** One request per release group, only for rows actually mapped,
   cached on disk afterwards. This matches what the Updates panel already
   does ungated; Releases is an opt-in module (section T) and needs no second
   gate.

Concerts gets no cover: its artist cell is text only (name plus caption).

The new column also touches the rulebook and two tests. **NR-25** pins the
releases table as `Date · Title · Artist · Type · Status`; it gains a
sentence that a leading, fixed cover column precedes those, which stay
unchanged in name and order. `releases_columns.rs`'s
`nr_25_table_has_the_five_named_columns` and
`nr_20_table_adds_a_bandcamp_purchase_column` assert `column_contract()`
and are updated with it.

## Part B — One date, taken from the system

### B.1 Where the locale already is

`i18n.rs` calls `setlocale(LC_ALL, "")` at startup, so `LC_TIME` is set and
nobody reads it. `chrono` cannot help: its `%x` is locale-blind and always
formats in English. That is precisely why fourteen hand-written formats
exist.

### B.2 Split: rendering in core, detection per platform

**Core — `reprise-core/src/format.rs`**, beside the existing
`civil_from_days`. A `DatePattern` value holding a numeric-only strftime
subset: `%d`, `%m`, `%Y` and literals. No chrono, no GLib.

- `DatePattern::from_platform(raw: &str) -> DatePattern` normalizes: `%y`
  becomes `%Y`, so the year is **always** four digits — glibc hands out
  `%m/%d/%y` for `en_US`, which is exactly the reported defect. Any
  non-numeric field (`%b`, `%B`, `%a`, `%A`, `%e`, `%j`, …) makes the whole
  pattern fall back to ISO `%Y-%m-%d` rather than guessing.
- `DatePattern::render(y, m: Option<u32>, d: Option<u32>) -> String`. A day
  without a month is not representable; `render` treats `d` as absent in that
  case, and the ISO-prefix parser that feeds it never produces the
  combination.

**Partial precision.** MusicBrainz supplies `2026-05` and `2026`. Each
literal run attaches as a *suffix* to the field preceding it (a leading run
attaches as a prefix to the first field); dropping a field drops its suffix,
and the result is trimmed. That handles both separator conventions with one
rule:

| Pattern | y+m+d | y+m | y |
|---|---|---|---|
| `%d.%m.%Y` | `29.05.2026` | `05.2026` | `2026` |
| `%m/%d/%Y` | `05/29/2026` | `05/2026` | `2026` |
| `%Y年%m月%d日` | `2026年05月29日` | `2026年05月` | `2026年` |
| `%Y. %m. %d.` | `2026. 05. 29.` | `2026. 05.` | `2026.` |

**Frontend — `reprise-gnome`.** Reads the pattern once at startup, after
`setlocale`, via `nl_langinfo(D_FMT)` (adds a direct `libc` dependency;
`unsafe` for the `*const c_char`, wrapped in one function). On a non-unix
target, or when the value is empty or unparseable, the pattern is ISO.
Android and Tauri later supply the same value from `java.text.DateFormat`
and `Intl.DateTimeFormat`; the renderer is shared.

### B.3 Time of day

`format_unix_timestamp` keeps rendering minute precision and never seconds,
but the 12/24-hour choice comes from the system: if `T_FMT` contains `%p`,
`%r` or `%I`, the time renders as `2:03 PM`, otherwise `14:03`. Seconds are
dropped regardless — a table cell is not a log line.

### B.4 Call sites replaced

All fourteen formats collapse into the shared renderer:

| File | Today |
|---|---|
| `releases/releases_presentation.rs` | `%-d %b %y`, `%b %Y`, `%Y` |
| `concerts/concerts_presentation.rs` | `%a, %b %-d`, `%a, %b %-d, %Y` |
| `updates/concerts_section.rs` | `%a, %-d %b`, `%a, %-d %b %Y` |
| `updates/release_row.rs` (Updates popover) | `%-d. %b`, `%-d. %b %y`, `%b`, `%b %y`, `%Y` |
| `podcasts/add_dialog_results.rs` | `%b %Y` |
| `podcasts/podcasts_presentation.rs` (episode date) | `%-d. %b`, `%-d. %b %Y` |
| `library_doctor/start_page.rs` | `%Y-%m-%d %H:%M` |
| `issues/missing_view.rs` | `%b %-d` |
| `device_sync/device_sync_page_copy.rs` | `%b %-d, %Y at %H:%M` |
| `core/library/stats_period.rs` (axis) | `%b %-d`, `Week of %b %-d`, `%b` |
| `track_list/column_layout.rs` (Added) | `%Y-%m-%d %H:%M` |

Podcast episodes carry the same current-year year-omission as the Updates
popover, and lose it for the same reason.

**Deliberately not touched**, verified by sweeping every `.format("…")` in
the three crates: `lastfm_stats.rs` and `listenbrainz.rs` (`%Y-%m` API query
keys), `concerts_view.rs` (`to_rfc3339` stored failure stamp),
`row_loss_watchdog.rs` (`%Y%m%d-%H%M%S` debug-dump filename), and every
`%Y-%m-%d` that parses an API payload. Those are machine strings, not
displayed dates. Relative phrasings that name an interval rather than a day —
`new_releases_updated_ago`, `concerts_updated_ago` — also stay.

**The Updates popover loses its year-omission rule.** `release_row.rs`
carries a second `format_release_date` — same name as the table's, different
behaviour: it drops the year entirely inside the current year (`15. Aug`),
writes it two-digit otherwise (`15. Aug 25`), and punctuates the day with a
period inside an otherwise English string. Under STYLE-11 all of that
collapses to the one system pattern with a four-digit year, so a release from
this year now reads `15.08.2026` in the popover as well. That is longer than
before in a deliberately compact surface — the accepted price of the rule,
recorded here so it is not rediscovered as a regression. The five
`format_release_date_*` unit tests in that file are rewritten with it, and
the duplicate function disappears.

**Chart bucket labels follow the rule too.**
`core/library/stats_period.rs` renders `%b %-d`, `Week of %b %-d` and `%b`
for the My Stats axis. They move to the same pattern, at the precision of the
bucket they name: a day bucket renders day and month, a week bucket the same
behind the translated "Week of", a month bucket month and year. The
surrounding period selector already states which span is on screen, so the
year may be omitted on a day bucket without becoming ambiguous — the axis
would otherwise repeat the same four digits thirty times.

This makes the year optional in the renderer:
`render(y: Option<i32>, m: Option<u32>, d: Option<u32>)`, with the same
suffix-dropping rule. Omitting the year is allowed only where the surface
itself names the period; every other caller passes it. Under `%d.%m.%Y` a day
bucket therefore reads `15.08.`, a month bucket `08.2026`.

Concerts loses its weekday prefix by decision 8.

## UX rules

Two new rules in `docs/ux-rules.md`, section U, beside STYLE-6 and STYLE-9.

**STYLE-10 [active] [gtk] — Columns belong to the user, in every table.**
A right-click anywhere on a table's header band opens the column editor
popover: toggle visibility, drag to reorder, reset. The same editor is
reachable without a pointer through the primary menu's "Customize table…";
its labelled sort section covers every table with sortable columns. The action
addresses the table of the active view and is insensitive where no table is
shown. Order, visibility and header-dragged widths are stored per
table and survive a restart. A table may declare fixed columns — a leading
artwork column, a trailing action column — which stay visible, keep their
position and never appear in the editor; every other column belongs to the
user. Exactly one visible column is the filler (STYLE-9); when the user hides
it, the filler role moves to the leftmost visible free column. Hiding the
sorting column does not change the sort. **Test rule:** one rule-named
display test per table, plus a measured filler test.

**STYLE-11 [active] [core] [gtk] — A date looks the same everywhere.**
Every displayed calendar date follows the system locale's date pattern, with
a numeric month and an always four-digit year; a pattern the app cannot
render numerically falls back to ISO. Incomplete dates shorten within that
same pattern instead of switching to a different one. Times show minutes and
never seconds, in the system's 12- or 24-hour convention. No call site
formats dates itself, and no surface keeps a month name. A label may show
fewer fields than the pattern holds — a chart axis whose period is already
named on screen omits the year — but never a different pattern. **Test
rule:** the pattern renderer is unit-tested
against the day-first, month-first, year-first and suffixed conventions; one
display test renders the four tables under a set `LC_TIME`.

**Amendments.** BROWSE-9 currently says the Added column shows "the
ISO-formatted time"; that clause is replaced by a reference to STYLE-11.
NR-25 pins the releases table as `Date · Title · Artist · Type · Status` and
gains the leading fixed cover column described in A.7, with the named text
columns unchanged.

## Testing

**Core, no display:**
- Pattern normalization: `%y` upgrade, non-numeric rejection, empty input.
- Partial rendering: the four conventions in the table above, plus the
  year-omitted forms the My Stats axis uses.
- Layout normalization per table: leading and trailing pins placed and forced
  visible, missing columns appended, serialize/parse round-trip.
- Width serialization per table.

**Display (`xvfb-run`, run singly — the display suite is herd-flaky):**
- `style_10_<table>_header_right_click_opens_the_editor` for each of the four.
- `style_10_hiding_a_column_survives_a_rebuild` — toggle, rebuild the
  registry from stored settings, assert the column is still hidden.
- `style_10_filler_moves_when_hidden` — measured, not asserted: hide the
  filler and compare realized widths, in the spirit of STYLE-9's test rule.
- `style_11_every_table_renders_the_system_date` under a set `LC_TIME`.
- `style_10_releases_cover_rebinds_when_the_row_changes` — bind row A, rebind
  the recycled cell to row B, assert the picture no longer shows A's file.

## Gates

- `scripts/check-architecture.sh`: 800 lines per file. The three music files
  (653 / 629 / 648) shrink as their generic half moves out; no new file
  should exceed roughly 350 lines. `track_list.rs` and `window.rs` stay under
  600.
- `scripts/check-frontend-thinness.sh`: `view_floor` (1782) must be raised in
  the same commit, because the shared view layer grows.
- `libc` becomes a direct dependency of `reprise-gnome` only. It is not in
  the banned family list and must not reach `reprise-core`, `reprise-cli`,
  `reprise-mcp` or `reprise-stems`.

## Out of scope

- Row context menus for Releases and Concerts. Considered, and the reason
  their action columns are pinned instead — it is a separate feature.
- A Preferences setting to override the date format. The system value is the
  answer; a second source of truth would re-create the drift.
- Covers for Concerts. Its artist cell carries no image today.
- Per-table sort persistence beyond what BROWSE-2 and BROWSE-12 already store.
