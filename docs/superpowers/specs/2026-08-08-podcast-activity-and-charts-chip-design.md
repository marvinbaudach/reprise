# Add Podcast: say when the last episode landed, and open on what a country listens to

Date: 2026-08-08
Status: design approved, not yet implemented

## Problem

Two complaints about the same dialog, reported together.

**A search result never says whether the show is still running.** A row in the
Add Podcast dialog carries a cover, a title, and — for an RSS result — nothing
but the author underneath (`add_dialog_results.rs:30` sets
`subtitle: row.author…`). Searching "Metalcore" today returns *Metalcore Nerds*,
which published four days ago, directly beside *MetalCore & More*, whose last
episode is from October 2019. The two rows look identical. The listener
subscribes and only finds out later.

The data is not missing — it is discarded. Apple's search returns `releaseDate`
per result, and for a podcast that field is the date of the most recent episode.
Verified live against the reported search on 2026-08-08:

| Result | trackCount | releaseDate |
|---|---|---|
| Metalcore Nerds | 301 | 2026-08-04 |
| Metalcore & Muscle | 106 | 2026-08-01 |
| MetalCore & More | 5 | 2019-10-10 |
| The Jasta Show | 20 | 2026-06-18 |

`SearchRow` (`crates/reprise-core/src/podcasts/itunes.rs:26`) does not declare
the field, so serde drops it. `episode_count` *is* parsed and then thrown away
by `rss_candidate`.

**The suggestion chip suggests the wrong thing.** `SRC-15` puts one library-derived
pill under the search entry: the genre this library has spent the most time
listening to, rendered as "Metalcore podcasts" (`add_dialog.rs:95`,
`library/taste.rs:41`). For radio that derivation earns its place — stations are
catalogued by genre, so "Metal in DE" is a real query. For podcasts a bare genre
word is a weak search term, and the chip occupies the one spot where the dialog
could instead answer "what do people around me actually listen to".

## Decisions

Settled in conversation on 2026-08-08:

1. The row states **freshness only** — not episode count, not an inferred
   cadence. A cadence guessed from `trackCount ÷ age` says nothing about the
   last six months and would make a dormant show look busy.
2. Freshness applies to **RSS results only**. yt-dlp's channel search yields no
   trustworthy channel date — only the upload dates of whichever videos the
   relevance ranking surfaced, so a daily channel could read "last March".
3. The wording is **relative first, absolute after a year**. "6 years ago" is
   vaguer than "Oct 2019", and a bare "4 Aug" — what the existing episode-list
   scale would print — makes the reader do the calendar arithmetic the feature
   exists to remove.
4. In the RSS dialog the genre chip is **replaced** by a country charts chip.
5. The YouTube dialog **keeps** its genre chip; there are no country charts for
   channels, and losing the chip would leave that dialog with no entry point at
   all.
6. The country comes from the **stored location, falling back to the system
   locale**.

## A — Freshness in the result row

`releaseDate` is read in core and handed on as a Unix second — the currency
`relative_date` (`podcasts_presentation.rs:131`) already deals in.

```rust
struct SearchRow  { …, release_date: Option<String> }   // ISO-8601 from the API
pub struct SearchResult { …, pub last_episode: Option<i64> }  // parsed in core
```

Parsing belongs in `itunes.rs`, beside the rest of the projection: an
unparseable or absent date becomes `None` rather than an error, because one
malformed row must not cost the listener the other eleven results.

The sentence itself is assembled in `add_dialog_results.rs`, next to the
existing `youtube_subtitle`, using the same `·` separator:

| Age of the last episode | Subtitle |
|---|---|
| today, or dated ahead | `Sean Mott · New today` |
| yesterday | `Sean Mott · New yesterday` |
| 2–6 days | `Sean Mott · New 4 days ago` |
| 7–34 days | `Sean Mott · New 2 weeks ago` |
| 35–364 days | `Sean Mott · 3 months ago` |
| ≥ 365 days | `Ada Lovelace · Last Oct 2019` |
| no date | `Ada Lovelace` — the segment is simply absent |
| no author | `New 4 days ago` — no leading separator |

"New" carries only while the show is fresh; past five weeks the phrasing drops
to "… ago" and past a year to "Last …", so the wording itself signals decay.

Counts round **down**: 20 days is "2 weeks ago", not 3, and the unit is a plain
divisor — 7 days to the week, 30 to the month — not a calendar walk. Rounding
down never claims a show is staler than it is, and the coarse unit is the point
at this distance: nobody subscribes differently for 3 versus 4 months.

A feed dated in the future — a mis-set timezone, a scheduled episode — reads as
"New today" rather than producing a negative age.

The strings live in `strings_podcasts.rs` beside the existing podcast vocabulary.
This is a second time scale next to `relative_date`'s, deliberately: that one
orders episodes you already subscribe to, this one judges a stranger.

## B — A charts chip instead of a genre chip

### One country for the whole dialog

```
app_location(conn).country_code   →  "CA"    set via city search
                  else locale_country()  →  "DE"    always available
```

`country_code` is populated **only when the location came from city search**
(`location.rs:30`); a location set through the XDG portal carries none. Radio
resolves that case to "Set location…"; here it falls through to the locale and
the chip keeps working.

The chain resolves once per dialog and feeds **both** the chip and the text
search. The search already sends a country (`itunes.rs:63`, derived from the
locale); routing it through the same chain keeps one country in force instead of
letting the chip and the results below it mean different catalogs.

### Fetching the charts

New in core: `itunes::top_podcasts(country) -> Vec<SearchResult>`, two keyless
calls.

1. `https://rss.marketingtools.apple.com/api/v2/{cc}/podcasts/top/12/podcasts.json`
   — returns chart order, but **no feed URL**: only `id`, `name`, `artistName`,
   `artworkUrl100`, `genres`, `url`.
2. `https://itunes.apple.com/lookup?id=a,b,c…` — one batched call for all twelve,
   returning feed URLs, artwork, authors and `releaseDate`.

The lookup answers in **the same envelope as the search**, so the charts arrive
as an ordinary `Vec<SearchResult>`. No second model, no second row widget, and
section A applies to chart rows for free.

`lookup` does not promise the order it was asked in, so the results are sorted
back into chart order by their position in the id list. That needs the id, which
`SearchResult` deliberately does not carry. Rather than widen a model that search
and MCP also use for a need only the charts have, the parsing splits in two:

```rust
parse_results_with_ids(json) -> Vec<(Option<i64>, SearchResult)>  // charts
parse_results(json)          -> Vec<SearchResult>                 // search, MCP
```

The second becomes a projection of the first, so every existing caller and its
tests are untouched. Ids the lookup drops — a show pulled since the chart was
cut — fall out silently rather than leaving a hole.

### What activating the chip does

`SRC-15`'s chip fills the search entry and lets the user press Search. The charts
have no search term to fill in with, so this chip **loads the results directly**:
one press, the list below populates, the entry stays untouched. The section
carries its own heading in the same style as the existing
`PODCASTS · APPLE PODCASTS` one, naming what is on screen, so a chart list is
never mistaken for the results of a search nobody typed.

Submitting a text search afterwards replaces the section, exactly as a second
search replaces the first.

### What each dialog shows

| Mode | Chip | Derived from |
|---|---|---|
| Apple Podcasts (RSS) | `Popular in DE` | the country chain |
| YouTube channels | `Metalcore channels` | listening time (unchanged) |

The label uses the country **code**, matching radio's "Metal in DE" — real
country names would need a translated table covering every Apple storefront.

Offline the chip is absent for the same reason search is unavailable (`NET-3`):
it is a network action, and a pill that only reports failure is worse than none.

### Rules

`SRC-15` governs radio and the YouTube mode and must lose the RSS podcast case —
but `ux-rules.md:18` forbids changing a rule's meaning in place: ids are
append-only, and a changed meaning gets a successor while the old id remains as a
signpost. So `SRC-15` retires to `[replaced by SRC-15a]`, and `SRC-15a` states the
narrowed scope. Its tests are re-hung onto the new id in the same commit, as that
same process rule requires.

Freshness and the chip are two independent behaviours with two independent
failure modes, and the traceability gate wants exactly one primary rule id per
test — hanging both off one id would let either regress while the gate stays
green. They therefore get **two** new rules: `SRC-18` for freshness, `SRC-19` for
the charts chip.

`library/taste.rs` stays in use — nothing is orphaned.

## C — Deliberately excluded

- Freshness on YouTube rows (decision 2).
- Any inferred publishing cadence (decision 1).
- The feed preview shown when adding by URL keeps its "N episodes" subtitle.
- Episode count in the search row — parsed but unused today, and a third
  segment would ellipsize away in this dialog's width.

## Testing

Everything new is pure logic and testable without a display:

- **Scale boundaries**: 0, 1, 2, 6, 7, 34, 35, 364 and 365 days; no date; no
  author; a date in the future.
- **Parsing**: `releaseDate` present, absent, and malformed — the last two both
  yielding `None` while the surrounding results survive.
- **Charts**: the two request URLs for a given country; chart order restored
  after the batch lookup; a dropped id skipped rather than faked.
- **Country chain**: location with a country, location without one, no location
  at all.
- **Chip per mode**: `src_15_the_library_chip_appears_only_with_a_genre_to_suggest`
  is amended to distinguish the two modes — RSS always shows the charts chip,
  YouTube still only shows one when the library has a genre.
