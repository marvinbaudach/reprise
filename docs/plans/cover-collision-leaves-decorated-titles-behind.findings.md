---
slug: cover-collision-leaves-decorated-titles-behind
kind: findings
created: 2026-09-09
follows: every-album-gets-its-own-cover (#843, landed on dev 2026-09-05)
---

# The collision rule works; two defects downstream of it do not

Reported as: *"die covers scheinen nicht zu stimmen. 3 alben und alle haben das
gleiche cover"*, with a screenshot of the track list filtered to `chelse`
showing **Evolve [Explicit]**, **Self Inflicted (Deluxe Edition)** and
**Eternal Nightmare** on one and the same picture.

## Verdict

**#843 did its job.** Four of the six Chelsea Grin albums were repaired by it,
and of the three rows in the screenshot **one is already correct**. What is left
are two defects in the *download* path that #843 never touched, both proven
against the live MusicBrainz and Cover Art Archive APIs:

1. **A decorated album title returns zero search results** — the tag decoration
   breaks the query, not the comparison.
2. **Only the first matching release is ever tried**, and it is frequently the
   one release without cover art, while a later candidate has it.

Defect 1 explains the two wrong rows in the report. Defect 2 explains at least
two further albums whose covers are available in the archive today.

## The measurement

`~/Music/Chelsea Grin/Chelsea Grin (2008)/` — 88 files, 7 album tag values,
`album_artist` present and identical (`Chelsea Grin`) on every one of them.

Embedded picture per album, md5 of the extracted stream:

```
Chelsea Grin                    | 2c7220fa
Desolation of Eden              | 2c7220fa
Desolation Of Eden              | 2c7220fa
Eternal Nightmare               | 2c7220fa
Evolve [Explicit]               | 2c7220fa
My Damnation                    | 2c7220fa
Self Inflicted (Deluxe Edition) | 2c7220fa
```

One file carries no embedded picture at all (`d41d8cd9`, the md5 of empty
input). It sits under `Desolation of Eden`, which *does* have a downloaded
cover, so it is unaffected — noted only so it is not chased later.

Extracted and viewed side by side with the downloaded Eternal Nightmare art
(`aaa3d3f705b55880.jpg`): **same artwork, confirmed by eye.** Different
resolutions (1400² vs 1500²) and different md5s, so this is a visual
identification, not a byte comparison. `2c7220fa` is the *Eternal Nightmare*
front cover.

## What the shipped fix actually did

Cache state per album key (`album_key` = `hash_hex(norm(album_artist) ␁
norm(album))`, reproduced in a standalone binary and validated against the
cache: 43 of the library's album keys resolve to a real cache entry, so the
reproduction is sound):

| Album | Key | Cache | Written |
|---|---|---|---|
| Chelsea Grin (self-titled) | `ba49cced99308134` | `.jpg` 1024² | 2026-09-06 09:28 |
| Eternal Nightmare | `aaa3d3f705b55880` | `.jpg` 1500² | 2026-09-06 09:28 |
| My Damnation | `e31dd3a533c2fd53` | `.jpg` 1500² | 2026-09-07 10:24 |
| Desolation Of Eden | `6125e69de4359292` | `.jpg` 1000² | 2026-09-06 09:28 |
| **Evolve [Explicit]** | `5798445666a420ea` | **`.notfound`** | 2026-09-06 09:28 |
| **Self Inflicted (Deluxe Edition)** | `522df0f682aa657d` | **`.notfound`** | 2026-09-06 09:28 |

All four downloaded covers are distinct from each other (md5 `f1d4374c`,
`db94b0cc`, `f1ec446d`, `654078ed`). The collision rule fired, the batch ran,
and the repair landed — the writes are dated the day after #843 shipped.

`Desolation of Eden` and `Desolation Of Eden` collapse to one key, because
`norm` lowercases. That part of the normalization works.

## Why the screenshot still shows three identical thumbnails

- **Eternal Nightmare** — resolves to its downloaded cover, which *is* that
  artwork. **Correct, and it always was.**
- **Evolve [Explicit]** — negative marker, so `resolve_source` falls through to
  the embedded picture, which is the Eternal Nightmare cover. **Wrong.**
- **Self Inflicted (Deluxe Edition)** — same. **Wrong.**

Three rows, one picture, but only two of them are a defect. That is why the
report reads worse than the remaining bug is.

## Defect 1 — the decoration breaks the search, not the comparison

Queried live against `musicbrainz_search_url`, reproducing `escape_lucene` and
the `artist:"…" AND release:"…"` Lucene query exactly:

```
Chelsea Grin — "Evolve [Explicit]"                 → NO RELEASES IN RESPONSE
Chelsea Grin — "Evolve"                            → score 100, exact match (d0675fed, 80775eac)
Chelsea Grin — "Self Inflicted (Deluxe Edition)"   → NO RELEASES IN RESPONSE
Chelsea Grin — "Self Inflicted"                    → score 100, exact match (3c9263a3, ac37d90e, 295a356a)
```

This corrects the obvious first guess. The failure is **not** that
`parse_best_release` compares `"evolve [explicit]"` against `"evolve"` and
rejects it — the response contains nothing to compare at all. `[Explicit]` and
`(Deluxe Edition)` are store decorations, they are escaped into the Lucene
query, and MusicBrainz then finds no release.

Consequence for the fix: the decoration must be stripped **before the query is
built**, in `musicbrainz_search_url`. Stripping it only before the comparison in
`parse_best_release` would change nothing.

## Defect 2 — only the first matching release is tried

`parse_best_release` (`cover_download.rs:255-290`) returns
`ReleaseSearchResult::Match(id)` on the **first** release passing `score >= 90`
plus normalized title and artist equality, and `fetch_and_cache_with` then asks
CAA for that one release (`cover_download.rs:347`). CAA art availability varies
per release, and the first candidate is often the wrong one to ask:

```
Suicide Silence — "No Time To Bleed"      5 candidates pass the matcher
   <-- chosen  #0 eb682a44  NO ART (404)
               #1 0b8d4e6f  has front art
               #2 1835e583  has front art
               #3 353b8c8d  NO ART (404)
               #4 c2ee9875  NO ART (404)

Killswitch Engage — "The End Of Heartache" 5 candidates pass the matcher
   <-- chosen  #0 6e1b254e  NO ART (404)
               #1 acd4c126  NO ART (404)
               #2 12571d60  has front art
               #3 e59296c6  has front art
               #4 a82cbc55  NO ART (404)
```

Both albums match the matcher perfectly — casing was never the issue. The cover
**is in the archive**. The app asks exactly one release, gets a 404, and writes
a permanent-ish negative marker.

## The negative marker conflates two causes

`write_negative` has three call sites (`cover_download.rs:236, 339, 350`): a
`NoMatch` from the search, a CAA 404 for a release, and a CAA 404 for a
release-group. A `.notfound` file therefore does not say which happened, which
is why the 16 markers below cannot be classified without re-querying.

## It will not heal by itself

**The album negative marker is permanent.** `fetch_and_cache_with` step 1 is
`if negative_marker_path(&key).exists() { return NotFound; }` — no age check at
all. `NEGATIVE_MARKER_MAX_AGE` (7 days, `cover_download.rs:24`) applies *only*
to the release-group path in `release_group_cover_state_from`; the doc comment
there records the album permanence as deliberate. So these markers never expire
on their own, and even if they did, a retry would hit the identical query with
the identical tags and write the identical marker. On top of that the batch is
gated twice — `startup_tasks.completed.covers` (currently `completed_at`
2026-09-08 21:52, `signature` 853, matching `startup_tasks.library_signature`)
and `download_settled` in the resolution index — so it will not even be
attempted again while the library is unchanged.

## Scope in the rest of the library

16 albums currently carry a negative marker:

```
Asking Alexandria  — Punk Goes Pop 3
Chelsea Grin       — Evolve [Explicit]                      ← defect 1 (confirmed)
Chelsea Grin       — Self Inflicted (Deluxe Edition)        ← defect 1 (confirmed)
Killswitch Engage  — The End Of Heartache                   ← defect 2 (confirmed)
King Conquer       — Americas Most Haunted                  ← missing apostrophe, unverified
Oceano             — Contagion
Oceans Ate Alaska  — Punk Goes Pop, Vol. 6
Ocean Sleeper      — My Forever Drug - Single               ← "- Single" suffix, unverified
Ocean Sleeper      — Peace When I'm Dead - Single           ← "- Single" suffix, unverified
PVRIS              — Punk Goes Pop, Vol. 6
Suicide Silence    — No Time To Bleed                       ← defect 2 (confirmed)
Suicide Silence    — No Time to Bleed (Bonus Track Version) ← decoration, unverified
Suicide Silence    — The Black Crown
Wage War           — Deadweight
We Rise The Tides  — Death Walk
We Rise The Tides  — Death Walks                            ← typo in the tag
```

Four are confirmed by live query. The remaining twelve were not re-queried; the
marker alone cannot say which defect they belong to.

## What would change it — not implemented, for decision

1. **Strip decorations from the query side.** In `musicbrainz_search_url`, drop
   a trailing `[...]`/`(...)` group and a `- Single`/`- EP` suffix from the
   album before escaping. Keep the strict comparison in `parse_best_release`
   against the *stripped* title, so the artist equality still guards it.
2. **Return candidates, not one id.** Make `parse_best_release` yield every
   passing release in score order and let `fetch_and_cache_with` walk them until
   CAA returns art. Bound the walk (the searches already use `limit=5`) and only
   write the negative marker once every candidate has 404'd. This alone recovers
   two of the sixteen and costs at most four extra CAA requests per album.
3. **Invalidation, again.** Whatever lands has to reopen the 16 markers plus the
   startup-task record, the same way #843 did (it deleted
   `startup_tasks.completed.covers` in `migrate_v83`,
   `crates/reprise-core/src/db_cover_download.rs`).

Deliberately *not* proposed: touching the user's tags. The wrong data is in the
files, but #843 already settled that the repair stays in the download cache.

## Verification notes for whoever picks this up

- `parse_best_release` and `musicbrainz_search_url` are pure functions — test
  them directly with recorded MusicBrainz payloads. Do **not** test through
  `fetch_and_cache`; it performs live network calls and hangs in CI. Only
  `fetch_and_cache_with` takes injectable fetchers. (Same seam warning as #843.)
- Regression cases that must keep their current answer: `Punk Goes Pop, Vol. 6`
  under two different album artists (a real compilation — the artist equality
  check is what makes it harmless), and `Desolation of Eden` / `Desolation Of
  Eden` collapsing to one key.
- A stripping rule must not eat a title where the bracket is part of the release
  name. Check the library for such titles before choosing the regex.
