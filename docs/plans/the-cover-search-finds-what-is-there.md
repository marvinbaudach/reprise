---
slug: the-cover-search-finds-what-is-there
worktree: /home/marvin/Projects/reprise-the-cover-search-finds-what-is-there
branch: feature/the-cover-search-finds-what-is-there
phase: shipped
codex_session:
created: 2026-09-09
---
# The cover search finds what is there

## Why

#843 made the *detection* of a shared embedded cover library-wide, and it
worked: four of six Chelsea Grin albums were repaired. It left the *download*
untouched, and two defects there keep the last two albums — plus at least two
more elsewhere in the library — on the wrong picture.

Both are measured against the live APIs, not inferred. Full evidence in
[cover-collision-leaves-decorated-titles-behind.findings.md](./cover-collision-leaves-decorated-titles-behind.findings.md).

**Defect 1 — a store decoration returns zero search results.**

```
Chelsea Grin — "Evolve [Explicit]"               → NO RELEASES IN RESPONSE
Chelsea Grin — "Evolve"                          → score 100, exact match
Chelsea Grin — "Self Inflicted (Deluxe Edition)" → NO RELEASES IN RESPONSE
Chelsea Grin — "Self Inflicted"                  → score 100, exact match
```

The decoration is escaped by `escape_lucene` into the Lucene query
(`cover_download.rs:117-128`) and MusicBrainz then finds nothing. The failure is
in the **query**, not in the comparison — `parse_best_release` never sees a
candidate to reject. 132 albums in the library carry such a decoration (106 with
a `- Single`/`- EP` suffix, 26 with a trailing bracket group) and **not one of
them has ever produced a downloaded cover**.

**Defect 2 — only the first matching release is ever asked.**
`parse_best_release` returns on the first release passing `score >= 90` and
normalized title+artist equality (`cover_download.rs:250-290`);
`fetch_and_cache_with` then asks CAA for that one release
(`cover_download.rs:347`). Art availability varies per release:

```
Suicide Silence — "No Time To Bleed"       5 candidates pass
   <-- chosen #0 eb682a44 NO ART   #1 0b8d4e6f HAS ART   #2 1835e583 HAS ART
Killswitch Engage — "The End Of Heartache" 5 candidates pass
   <-- chosen #0 6e1b254e NO ART   #1 acd4c126 NO ART    #2 12571d60 HAS ART
```

The cover is in the archive. The app asks one release, gets a 404, gives up.

## The change

Four parts. Parts 1, 2 and 4 fix the lookup; part 3 is the one-time
invalidation without which none of them reaches an already-settled library.

### 1. A second search attempt with the decoration stripped

**A fallback, never a replacement.** The strict attempt runs first and
unchanged; the stripped attempt runs only when the strict one produced no
usable match.

```
strict query (raw tag) ──match──> done, exactly as today
        │
      no match
        ▼
stripped title differs? ──no──> NoMatch (as today)
        │ yes
        ▼
second query (stripped) ──match──> done
        │
      no match
        ▼
      NoMatch
```

Why a fallback and not always stripping: an album genuinely titled `Songs
(Live)` in MusicBrainz matches today on the strict pass. Under an always-
stripped query that release could fall below `MIN_MB_SCORE` and never be
considered. As a fallback, stripping **cannot break any album that works
today** — the second attempt only ever runs where a negative marker is about to
be written anyway. Cost: exactly one extra MusicBrainz request per album that
currently fails, zero for every album that succeeds.

`strip_release_decoration(album: &str) -> Option<String>` — pure, and it removes
**exactly one thing**, never cascading:

1. if the title ends in ` - Single` or ` - EP` (case-insensitive, surrounding
   whitespace tolerated) → remove that suffix and stop;
2. otherwise, if the title ends in a `(...)` or `[...]` group → remove that one
   group and stop;
3. `None` when nothing matched, or the remainder is empty or whitespace only.

**One step is not a simplification, it is the correct rule.** Seven titles in
the library carry a real parenthetical *before* the suffix — `Leave (Get Out) -
Single`, `Loathe (Remastered) - Single`, `Apeirophobia (There Will Be No End) -
Single` and four more. Cascading would reduce the first to `Leave`, which is
the wrong release. Only one title has two stacked decorations (`Number[s]
(Deluxe Version)`), and one step gives it `Number[s]`, which is also the right
answer. Rule 1 before rule 2 is what makes both come out correct.

Worked examples, all from the library:

```
Leave (Get Out) - Single           → Leave (Get Out)      ✓
Number[s] (Deluxe Version)         → Number[s]            ✓
Self Inflicted (Deluxe Edition)    → Self Inflicted       ✓
Evolve [Explicit]                  → Evolve               ✓
My Forever Drug - Single           → My Forever Drug      ✓
The Black Crown (2011)             → The Black Crown      ✓
(What's the Story) Morning Glory?  → None (not trailing)  ✓
Genesi[s]                          → Genesi               ✗ accepted
The Wall                           → None                 ✓
```

`Genesi[s]` is stylised, not decorated, and the rule mangles it. Accepted
deliberately: the artist-equality guard means it cannot produce a *wrong* cover,
only no cover, so the cost is one wasted request on an album that has none
today either. A keyword allowlist would fix it and would cost a list that needs
maintaining, plus it would lose `Dead by April (09 | 26)`.

**The comparison uses the same stripped title as the query.** The second attempt
calls `parse_best_release(&body, album_artist, &stripped)`. Passing the raw
title into a stripped query is the mistake that would make this whole part a
no-op.

**Invariant that must not break: `album_key` keeps using the raw album tag.**
The cache key is `hash_hex(norm(album_artist) ␁ norm(album))`
(`cover_download.rs:76-84`). Deriving it from the stripped title would orphan
all 43 currently valid cache entries and re-download the library. Stripping is
for the query and its comparison only, never for the key.

### 2. Walk every matching release until one has art

`parse_best_release` returns every passing release instead of the first:

```rust
enum ReleaseSearchResult {
    Match(Vec<String>),   // was: Match(String)
    NoMatch,
    Malformed,
}
```

`Match` is never empty — an empty candidate list is `NoMatch`. Candidates keep
**the order the response gave them**, so today's chosen release stays the first
step of the new loop. Do not assert a descending-score order in a test: the
probe that produced the evidence above returned five candidates all at score
100, and the order within a score tier is not something the API promises. Since
the walk now tries all of them, the order does not affect the outcome anyway.
Bounded by `MAX_RELEASE_CANDIDATES: usize = 5`, matching the `limit=5` already
in `musicbrainz_search_url`.

`fetch_and_cache_with` walks the list, remembering whether anything went
transient:

- `Found` → store and return, as today;
- `NotFound` → try the next candidate;
- `TransientFailure` → **remember it and keep going.**

After the walk:

- any art found → `Downloaded`;
- else any transient seen → `CoverFetchOutcome::TransientFailure`, and **no
  marker is written**. A network blip must never be recorded as "this album has
  no cover";
- else → `write_negative` **once**, then `CoverFetchOutcome::NotFound`.

Continuing past a transient rather than bailing costs nothing in practice: a
real outage already fails the MusicBrainz search before the walk is reached.
Bailing early would throw away a reachable cover whenever candidate #0 is the
one that times out.

The embedded-MBID path (`mbid: Some(id)`) keeps its single candidate and is
unchanged. Verified: no track of any affected album carries a release MBID, so
nothing in this plan exercises that path.

### 3. Reaching a library that has already settled

Three gates were written under the old answers and **each one alone** is enough
to keep this change away from the existing library. All three must move.

**a. The negative marker generation.** `NEGATIVE_MARKER_GENERATION: u32 = 2`,
folded into the filename:

```rust
pub fn negative_marker_path(key: &str) -> PathBuf {
    downloaded_dir().join(format!("{key}.notfound{NEGATIVE_MARKER_GENERATION}"))
}
```

Old markers are then simply not seen — no migration, no ordering, atomic. This
is the same idiom `RESOLUTION_FORMAT` already establishes. The 238 stale 0-byte
files stay where they are; a startup directory walk to delete them is not worth
it. Release-group markers are invalidated along with the album ones, which is
harmless — they are fetched lazily and expire on their own anyway.

The generation bump is required *in addition to* part 4: the current markers
were written 2026-09-06 and are three days old, so a 7-day TTL alone would keep
blocking them until 2026-09-13.

**`NEGATIVE_MARKER_GENERATION` is a one-shot and never bumps again.** Once part
4 lands, the TTL is the standing mechanism for retiring a stale marker; the
generation exists solely to clear the markers that are younger than the TTL on
the day this ships. A future wave that needs markers reopened should not reach
for this constant — it should ask why the TTL did not already do it. Say so in
the constant's doc comment.

**b. The startup task record.** `startup_tasks.completed.covers` currently holds
`{"completed_at":1788897173,"signature":853}` against a
`startup_tasks.library_signature` of `853`, so `exact_signature_decision`
answers `AlreadyCompleted` and the batch never starts. Add `migrate_v84` to
`crates/reprise-core/src/db_cover_download.rs` deleting that settings row,
registered in `db.rs` right after `migrate_v83` (currently line 765), and raise
`SUPPORTED_SCHEMA_VERSION` (`db.rs:26`) from 83 to 84. Copy the shape of
`migrate_v83` exactly, including its early return on `version >= 84`.

**c. The per-track resolution index.** `outcome_settles_track`
(`cover_download_batch.rs:355`) counts `DownloadOutcome::Unavailable` as
settled, so every track of those 16 albums carries `download_settled` and
`open_paths` filters it out of the next pass. `RESOLUTION_FORMAT` 3 → 4
(`cover.rs:378`) — precisely what that constant documents itself as being for.
Accepted cost, as in #843: the first launch after the update re-reads tags for
the whole library once.

### 4. Album negative markers expire like release-group ones

Today the album path is a permanent negative cache: `fetch_and_cache_with` step
1 is `if negative_marker_path(&key).exists() { return NotFound; }`, with no age
check. `NEGATIVE_MARKER_MAX_AGE` (7 days) applies only to
`release_group_cover_state_from`. Give the album path the same expiry, so a
cover that appears on the Cover Art Archive later is picked up without another
generation bump.

`release_group_cover_state_from(cached, marker_modified, now)` is already
generic over its three inputs despite its name — **rename it to
`cover_state_from` and use it for both paths** rather than writing a second copy.
Add `album_cover_state_at(key, now)` mirroring the existing
`release_group_cover_state_at`, and have `fetch_and_cache_with` take its step-1
decision from it.

**The rename is contained; here is its full blast radius**, so it is not a
surprise in the diff. `release_group_cover_state_from` is a private `fn` with
exactly six references on `origin/dev`: two in `cover_download.rs` and four in
`cover_download_tests.rs`. The two `pub` functions that reach outside the module
— `release_group_cover_state` and `release_group_cover_path`, called from
`crates/reprise-gnome/src/ui/updates/release_cover.rs:401` and `:408` — **keep
their names and signatures**. Nothing outside `reprise-core` changes.

Rewrite the doc comment at `cover_download.rs:20-24`, which currently records
the permanence as deliberate ("Unlike the album path (permanent negative
cache)…"). It becomes the shared rule for both paths.

Accepted consequence, stated plainly: every album that genuinely has no cover
goes back to the network every 7 days. That is the price of self-healing and it
was chosen with that in view.

## Rejected

- **Cascading decoration stripping.** Would break the seven `(...) - Single`
  titles for the sake of one stacked case that one-step handles correctly.
- **A keyword allowlist for bracket groups.** Rescues `Genesi[s]` and `Burn
  This World [EVOLVED]`, costs a maintained list, loses `Dead by April
  (09 | 26)`. Not worth it while the failure mode is a wasted request.
- **Deriving `album_key` from the stripped title.** Orphans 43 cache entries.
- **Fuzzy matching or lowering `MIN_MB_SCORE`.** The strict artist+title
  equality is what keeps the `Punk Goes Pop, Vol. 6` compilation (one release,
  two album artists) harmless. Do not loosen the safety net while widening the
  search.
- **Falling back to the search when an embedded release MBID yields no art.**
  Same shape as defect 2, but no affected track carries an MBID. YAGNI.
- **Repairing the user's tags.** #843 settled that the repair stays in the
  download cache and never rewrites audio files.

## Tasks

1. `strip_release_decoration` — pure function, one-step rule, suffix before
   bracket group.
2. `ReleaseSearchResult::Match(Vec<String>)`; `parse_best_release` collects all
   passing releases, capped at `MAX_RELEASE_CANDIDATES`.
3. `fetch_and_cache_with`: the candidate walk with the transient rule, and the
   second search attempt with the stripped title when the first yields
   `NoMatch`.
4. `NEGATIVE_MARKER_GENERATION` in `negative_marker_path`.
5. Rename `release_group_cover_state_from` → `cover_state_from`; add
   `album_cover_state_at`; step 1 of `fetch_and_cache_with` uses it; rewrite the
   doc comment at `cover_download.rs:20-24`.
6. `RESOLUTION_FORMAT` 3 → 4 in `cover.rs`.
7. `migrate_v84` in `db_cover_download.rs`; `SUPPORTED_SCHEMA_VERSION` 83 → 84;
   registration in `db.rs`; migration test in the v83 shape.
8. Tests, below.

## Verification

The seam is good and already in use: `fetch_and_cache_with` takes injectable
`mb_fetch`/`caa_fetch` closures, and both `cover_download_retry_tests.rs` and
`cover_download_tests.rs` drive it that way. **Never** test through
`fetch_and_cache` — it performs live network calls and hangs in CI.

Pure functions:

- `strip_release_decoration`, every row of the worked-examples table above,
  including `Leave (Get Out) - Single` → `Leave (Get Out)` and `Number[s]
  (Deluxe Version)` → `Number[s]`, which are the two that a cascading rule gets
  wrong, and `(What's the Story) Morning Glory?` → `None`.
- `parse_best_release`: five passing releases → all five ids in response order;
  one passing → one; none → `NoMatch`; still rejects `score < 90` and a
  mismatched artist (the compilation guard); garbage → `Malformed`.

Behavioural, through `fetch_and_cache_with` with recorded payloads:

- candidate #0 404s, #1 has art → `Downloaded`, **no marker written**;
- every candidate 404s → `NotFound`, marker written **once**;
- candidate #0 transient, #1 has art → `Downloaded`, no marker;
- candidate #0 transient, rest 404 → `TransientFailure`, **no marker**;
- strict query matches → the stripped attempt is never issued (assert the
  `mb_fetch` call count is exactly 1);
- strict query returns no releases and the stripped one matches → `Downloaded`,
  and the second URL contains the stripped title;
- an undecorated album that finds nothing → one request only, marker written
  (unchanged behaviour);
- a marker younger than 7 days blocks; one older than 7 days does not. **Write
  the fixture marker through `negative_marker_path`, not a hand-built
  `<key>.notfound`** — a hard-coded old-generation name would make this test
  pass for the wrong reason, since the new code no longer looks there.

Migration: the v83 test shape — the covers record goes, `spectrogram` stays,
`user_version` is 84.

Gate: `cargo test -p reprise-core -p reprise-gnome`, then the repo's normal gate
script. **The worktree must be based on `origin/dev`** — the main checkout sits
on `song-visuals-ask-the-stored-category`, 103 commits behind, which does not
even contain #843, and every line number and baseline in this plan is read from
`origin/dev`. `worktree.sh` already does the right thing here (`_base_ref`
prefers `origin/dev` and documents "never whatever HEAD the main checkout
happens to be on"), so this is a check to confirm, not a step to perform: after
`/code` creates the worktree, verify `git -C <wt> merge-base --is-ancestor
2a4c6cc07f HEAD` succeeds before trusting any measurement.

**The real measurement is the user's library**, and its baseline is recorded
here so it can be compared afterwards:

- 6 Chelsea Grin albums: 4 with distinct downloaded covers, `Evolve [Explicit]`
  and `Self Inflicted (Deluxe Edition)` on `.notfound`;
- 16 negative markers library-wide;
- 132 decorated album titles, none of which has ever downloaded a cover.

After the change the first launch must turn the two Chelsea Grin albums into
distinct downloaded covers (defect 1) and recover `No Time To Bleed` and `The
End Of Heartache` through the candidate walk (defect 2). The control arm is the
four albums that already work — they must keep exactly the cover files they
have, since `album_key` is unchanged.

## Parallelität

**No cut. One strand.**

Tasks 1–5 all edit `crates/reprise-core/src/cover_download.rs`, and several of
them edit the same function: the stripping fallback and the candidate walk are
both inside `fetch_and_cache_with`, tasks 2 and 3 are the two ends of one
signature change, and task 5 rewrites that function's step 1. Their tests all
land in `cover_download_tests.rs`. A cut here would put one strand's enum change
under another strand's consumer.

Tasks 6 and 7 do own disjoint files (`cover.rs`; `db_cover_download.rs`,
`db.rs`), so an "invalidation" strand is technically possible. It is not worth
it: two constant bumps plus one migration in the shape of the previous one,
against a second worktree, a second cargo build of the same crate, and a merge
order. The wall-clock saving is negative.

No merge order and no post-merge cross-checks, because there is one branch.

File size: `cover_download.rs` is 527 lines today and this adds roughly 80. It
stays under the 800-line cap, so no module extraction is required. Should it
cross the cap, extract `strip_release_decoration` and its tests into a sibling
module rather than trimming comments to fit.
