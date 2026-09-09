---
slug: episodes-show-their-runtime-before-the-download
worktree: /home/marvin/Projects/reprise-episodes-show-their-runtime-before-the-download
branch: feature/episodes-show-their-runtime-before-the-download
phase: shipped
codex_session:
created: 2026-09-07
---
# Episodes show their runtime before the download

## The goal

An episode row should say how long it runs *before* anything has been
downloaded or played. Today the runtime appears reliably only after the file is
on disk, which is exactly backwards: the number is most useful while deciding
whether to fetch the episode at all.

The trigger, 2026-09-09: in the YouTube channel detail for *Mystical Nordic
Ambience*, the three newest episodes show only a date, while every downloaded
one below them shows `· 7 h 00`.

## What is already there

The display is not the problem. Both episode views already render the runtime:

- `crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs:596` (RSS channel detail)
- `crates/reprise-gnome/src/ui/podcasts/youtube_channel_detail.rs:694` (YouTube channel detail)

Both call `duration()` (`podcasts_presentation.rs:249`), which formats via
`strings::podcast_duration_minutes()` / `podcast_duration_hours()`; the
underlying constants `PODCAST_DURATION_MINUTES` / `PODCAST_DURATION_HOURS` are
at `strings_podcasts.rs:39-40` and are already translated.

**No UI work is needed, and no new translatable string.** What is missing is
the value in `podcast_episodes.duration_secs`.

The refresh completion path already re-reads the list: `Refreshed(summary)` in
`podcasts_view_requests.rs:241` calls `view.refresh()`. Since the fill below runs
inside the refresh, the filled runtimes appear as soon as the refresh reports
done — no extra invalidation, no UI change.

## Where the value comes from today

| Path | Source | When |
|---|---|---|
| RSS feed parse | `itunes:duration` → `feed.rs` | every refresh |
| YouTube listing | yt-dlp `--flat-playlist` → `ytdlp.rs:159` | first import, or RSS fallback |
| YouTube resolve | `save_youtube_resolution` (`store_metadata.rs:7`, writes duration) | first play of a YouTube episode |
| Stream duration | GStreamer → `store::save_duration` (`store.rs:430`) | first play of any episode |

The last two only fire once the user has already committed to the episode. That
is the gap.

## Why YouTube episodes arrive without a runtime

`read_youtube_feed` (`pipeline_sync.rs:356`) prefers the official `videos.xml`
feed whenever `long_form_feed_url` yields one, and falls back to
`youtube_fetcher.list()` (yt-dlp, which *does* carry durations) only on
`Err(_)` (`pipeline_sync.rs:393`).

YouTube's RSS feed carries no duration. So the cheap, etag-backed path that runs
on every refresh is precisely the path that drops the runtime, while the
expensive path that has it runs almost never.

## Evidence (measured 2026-09-09 against the live library)

Gaps by subscription kind — every gap is YouTube, none is RSS:

| kind | episodes | without runtime |
|---|---|---|
| rss (8 subs) | 205 | **0** |
| youtube (7 subs) | 176 | **29** |

RSS is clean on *both* sides of the download line (14 downloaded / 0 gaps,
191 never downloaded / 0 gaps). That is the control arm for the claim that this
is a YouTube-path defect and not a general one.

Split by whether the YouTube episode was ever downloaded:

| YouTube episodes | count | without runtime |
|---|---|---|
| downloaded | 134 | 4 (3 %) |
| never downloaded | 42 | **25 (60 %)** |

**This is a leak, not a backlog.** The same query on 2026-09-07 counted 25 gaps;
on 2026-09-09 it counts 29. Four new gaps in two days, and the gap episodes are
the *newest* ones (08.09., 06.09., 05.09., 03.09. …) — exactly the rows the user
is looking at when deciding what to fetch. Every refresh that pulls a new video
through the RSS path adds one.

### Per channel, and what a listing window actually reaches

Measured with the exact call `ytdlp.rs` already issues
(`--no-warnings --flat-playlist --extractor-args youtubetab:approximate_date -J`):

| channel | episodes | gaps | covered by `-I 1:30` | covered by `-I 1:200` |
|---|---|---|---|---|
| Nordheim Melodies | 19 | 9 | 6 | **9** (98 entries, 3 s) |
| Mystical Nordic Ambience | 20 | 9 | **9** (30 entries, 1 s) | — |
| Bjorth | 15 | 5 | 4 | 4 (35 entries, 1 s) |
| VOID PREACHER | 60 | 3 | **3** | — |
| Danheim | 16 | 2 | 1 | 1 (107 entries, 2 s) |
| HOLLOW FALLEN | 30 | 1 | **1** | — |
| Heldom | 16 | 0 | — | — |
| **total** | | **29** | 24 | 27 |

Every entry a listing returns carries a duration (420/420 across all listings
fetched), and the durations are plain JSON integers.

The GUID stored for a YouTube episode is the bare 11-character video id, and the
listing returns the same id: **the two sources join on the GUID with no
transformation** — verified, 9 of 9 gap GUIDs for *Mystical Nordic Ambience*
matched a listing entry.

Drift against durations already stored (*Mystical Nordic Ambience*, 11 episodes
both sides know): 10 identical, 1 off by 1 s. An earlier *Bjorth* sample was
worse (9 of 11 off by 1 s, one by 11 s). **Never overwrite** an existing
runtime: a path that wrote the listing value over a measured one would nudge
already-correct rows on every refresh for no gain.

### The two ghost rows, and their reproduced cause

Two of the 29 gaps are not videos at all:

```
id 284  Bjorth   "Bjorth - Shorts"   guid UClDzr-KM5H2-bsO3xIC32mg
id 285  Danheim  "Danheim - Shorts"  guid UCLTQVYwu-M-MnfOJDKlFnOQ
        audio_url = https://www.youtube.com/watch?v=<that same id>
        page_url, published_at, duration_secs, downloaded_path all empty
```

Those GUIDs are 24-character **channel** ids (`UC…`), not 11-character video ids,
and both rows were created in the same second as their channel's import
(2026-08-17 06:57:23). They are user-visible: Bjorth has 15 rows and the UI shows
"15 episodes", so the ghost is listed as an episode with no date, no runtime and
a dead link.

**Cause reproduced.** Listing the *bare* channel URL returns channel tabs, not
videos:

```
$ yt-dlp --flat-playlist -J https://www.youtube.com/channel/UClDzr-KM5H2-bsO3xIC32mg
entries: 2
  UClDzr-KM5H2-bsO3xIC32mg  _type=playlist  "Bjorth - Videos"   duration=null
  UClDzr-KM5H2-bsO3xIC32mg  _type=playlist  "Bjorth - Shorts"   duration=null
```

Both carry the channel id as `id` and `_type: "playlist"`. The same URL with
`/videos` appended returns no such entry. This is a **live** bug: any channel
added by bare channel URL gets a fresh ghost row. Task 4 closes it.

Nothing prunes them automatically — `tombstone_episode` (`store.rs:461`) is
user-initiated, with undo; a refresh never removes vanished episodes.

Once the ghosts are excluded, a 200-entry listing covers **27 of 27** real gaps
in this library.

## The one thing not to do

Do **not** route the listing durations through `upsert_episode_in`.
`store.rs:293` reads

```sql
duration_secs = COALESCE(excluded.duration_secs, podcast_episodes.duration_secs)
```

which prefers the *incoming* value whenever it is non-NULL. That is right for
the feed (a corrected `itunes:duration` should win) and wrong for the listing
(see the drift measurement). The fill needs its own write path with
never-overwrite semantics — the same rule `store.rs:430` `save_duration` uses:

```sql
WHERE id = ?1 AND duration_secs IS NULL AND ?2 > 0
```

## Decisions taken in the grill (2026-09-09)

- **Fixed 200-entry window via `list_range`, not a formula.** The earlier draft
  proposed `gaps + config.youtube_import_count`. That is dead:
  `youtube_import_count` defaults to **10** and is clamped to a maximum of **50**
  (`config.rs:34,45-46`), so *Nordheim Melodies* would get `9 + 10 = 19` where
  covering its 9 gaps took ~98. A window that leaves a residue also re-lists on
  every later refresh, so the cheap-looking option is the expensive one.
- **Use `list_range`, not `list`.** `YoutubeFetcher::list(url, limit)` for
  `YtDlp` (`pipeline.rs:114`) calls `YtDlp::list(self, url)`, which passes **no**
  `-I` flag and fetches the whole channel, then truncates in Rust via
  `project_youtube_feed`. Its `limit` costs nothing and saves nothing.
  `YtDlp::list_range` (`ytdlp_range.rs:8`) passes the real `-I 1:end`. That is
  the bounded primitive this plan needs.
- **No new trait method.** The draft's `episode_durations` is dropped. The trait
  already has `list_range` with a default body forwarding to `list`
  (`pipeline.rs:58`), so all 14 implementations work unchanged and the caller
  projects the pairs itself. A new trait method for exactly one caller is
  speculative generality.
- **Fix the ghost cause, do not migrate the two rows away.** Task 4 stops new
  ghosts; the gap filter in task 1 stops the existing two from driving a listing
  forever. The two rows themselves stay until removed by hand in the UI — a
  schema migration for two rows is not worth it, and migrations are a known trap
  area in this repo.
- **A deleted video stays a documented gap.** After tasks 1 and 4 the only
  remaining permanent gap is a *real* video (11-char GUID) deleted or made
  private after the feed carried it. It never appears in a listing, so it drives
  one ~3 s listing per refresh of that channel, forever. **Accepted and
  documented — no marker column, no throttle.** 0 episodes are affected today.
  A reviewer meeting this must not file it as a bug.

## Scope decisions

- **The mechanism stays kind-agnostic where it cheaply can.** Gap detection and
  the batched write are plain SQL over `podcast_episodes`; only the *source* of
  the durations is YouTube-specific.
- **RSS feeds without `itunes:duration` are out of scope.** No cheap
  pre-download source exists (HTTP HEAD gives bytes, not seconds; bitrate
  estimation is wrong for VBR). 0 of 205 RSS episodes are affected, and such
  episodes already get a runtime on first play via `save_duration`.
- **No backfill migration.** The existing gaps are filled by the next refresh of
  their channel, through the same code path as new episodes.
- **No float handling needed.** `ytdlp_playlist.rs:67` already parses duration as
  `f64` (and from a string), filters non-finite/negative, and truncates to `i64`.

## Tasks

### 1. `store::episodes_missing_duration_in` — find the gaps

`crates/reprise-core/src/podcasts/store.rs`

```rust
pub(crate) fn episodes_missing_duration_in(
    conn: &Connection,
    subscription_id: i64,
) -> Result<usize, rusqlite::Error>
```

Counts rows of that subscription with `removed_at IS NULL`,
`(duration_secs IS NULL OR duration_secs = 0)`, **and `length(guid) = 11`**.

That last clause is the ghost guard: a YouTube video id is always 11 characters,
while the two Shorts-tab rows carry a 24-character `UC…` channel id. Without it,
*Bjorth* and *Danheim* would issue a yt-dlp listing on every refresh forever and
never close the gap. **Put the reason in a comment above the SQL, naming both
ids**, or a later "simplification" will delete the clause.

The function is only ever called for YouTube subscriptions (task 3 step 1), so
the 11-character rule cannot affect RSS GUIDs.

Return the count, not a bool: task 3 uses it for the early return.

### 2. `store::fill_missing_durations_in` — the write path

`crates/reprise-core/src/podcasts/store.rs`, next to `save_duration`

```rust
pub(crate) fn fill_missing_durations_in(
    conn: &Connection,
    subscription_id: i64,
    durations: &[(String, i64)],
) -> Result<usize, rusqlite::Error>
```

One prepared statement reused across the slice, inside the caller's transaction:

```sql
UPDATE podcast_episodes
   SET duration_secs = ?3
 WHERE subscription_id = ?1
   AND guid = ?2
   AND (duration_secs IS NULL OR duration_secs = 0)
   AND ?3 > 0
```

Returns the number of rows actually changed. The never-overwrite guard lives in
SQL rather than in Rust so no future caller can bypass it.

### 3. Hook the fill into the refresh

`crates/reprise-core/src/podcasts/pipeline_sync.rs`, in `refresh_one_in`
(function at line 175)

**After `transaction.commit()` (line 313), not inside the transaction.** The
write transaction is opened `IMMEDIATE` (line 243) and holds the database write
lock; the listing call takes 1–3 s of network. Running it inside would block
every other writer for the duration of a yt-dlp process. This is the single most
important placement detail in the plan. It belongs with the existing post-commit
steps (`clear_retry` at 314, `summary.refreshed += 1` at 315).

The step, in order:

1. Skip unless `subscription.kind == PodcastKind::Youtube`.
2. Skip unless `youtube_allowed` — the NET-1a gate, already threaded through this
   function (from `source_network_allowed_in`, line 121; checked at 139, 185,
   202, 347). A refresh must not spawn yt-dlp when the gate is closed.
3. `let gaps = episodes_missing_duration_in(conn, subscription.id)?;`
   Return early when `gaps == 0`. **This is what keeps the common case free** — a
   channel whose runtimes are all known does no extra work at all.
4. Resolve the channel URL. Reuse the derivation `read_youtube_feed` performs
   (`pipeline_sync.rs:362-369`) rather than duplicating it — extract a small
   helper if that is what it takes.
5. `youtube_fetcher.list_range(&channel_url, DURATION_FILL_WINDOW)` with

   ```rust
   /// Entries to ask yt-dlp for when filling missing runtimes. Fixed, not
   /// derived from the gap count: the gaps are not always the newest episodes
   /// (Nordheim Melodies needed ~98 entries for gaps that a 30-entry window
   /// missed), and a window that leaves a residue re-lists on every later
   /// refresh. Measured cost at 200 entries: ~3 s.
   const DURATION_FILL_WINDOW: usize = 200;
   ```

6. Project the pairs from the returned `ParsedFeed`:

   ```rust
   let durations: Vec<(String, i64)> = feed
       .episodes
       .into_iter()
       .filter_map(|episode| Some((episode.guid, episode.duration_secs?)))
       .filter(|(_, secs)| *secs > 0)
       .collect();
   ```

7. On `Err` from the listing, log and continue. A failed listing must not fail
   the refresh: the feed read already succeeded and the episodes are already
   stored. Record it the way other non-fatal steps in this function do; do not
   swallow it silently and do not turn it into `record_failure`.
8. Open a short transaction and call `fill_missing_durations_in`.

### 4. Stop the channel-tab entries from becoming episodes

`crates/reprise-core/src/podcasts/ytdlp_playlist.rs`, in `parse` (line 7)

The `filter_map` that builds `YtDlpVideo` from each raw entry must skip entries
whose `_type` is `"playlist"`. Those are channel tabs ("… - Videos", "… - Shorts"),
not videos: they carry the channel id as `id` and a null duration.

Two things make this safe, and both should be stated in the code comment:

- **`source_url` is unaffected.** It is derived from `raw_entries` above the
  entry filter (`channel_id` lookup, then `stable_source_url`), so channel
  resolution — `resolve_channel_url` uses only `.source_url` — cannot change.
- `parse` is shared by `list`, `list_range` and `search`, and a playlist entry is
  never wanted as an episode on any of those paths. `search_channels` uses a
  different parser (`parse_search_channels`) and is untouched.

This does **not** remove the two existing ghost rows; nothing prunes episodes
automatically, and they are removed by hand in the UI. It stops new ones.

### 5. Tests

New `crates/reprise-core/src/podcasts/pipeline_youtube_duration_tests.rs`,
registered in **`pipeline.rs` (lines 446-480)** — not `podcasts.rs`, which does
not exist — with the existing `#[cfg(test)] #[path = "…"] mod …;` pattern. Use
the `conn()` helper (`pipeline_youtube_test_support.rs:9`) and the `FakeYoutube`
shape already used by `pipeline_youtube_projection_tests.rs`. Fakes override
`list_range`.

Each case pins a claim this plan makes:

1. **The gap is filled.** Feed read yields an episode with no duration; the
   fake's listing carries one for that GUID → stored `duration_secs` is the
   listing value.
2. **An existing runtime is never overwritten.** Episode stored with
   `duration_secs = 186`; listing says 175 → still 186. This is the measured
   drift, and the regression most likely to be introduced by a later
   "simplification".
3. **No gaps means no listing call.** `CountingYoutube` records call counts; with
   every episode carrying a duration, the count stays 0.
4. **A ghost row does not count as a gap.** An episode with a 24-character `UC…`
   GUID and no duration → listing count stays 0. This is the permanent
   re-listing regression, and nothing else catches it.
5. **The gate is respected.** `youtube_allowed = false` → listing count 0 even
   with gaps present.
6. **A failing listing leaves the refresh successful.** Fake returns `Err` from
   `list_range`; the refresh still reports success and the episodes are still
   stored.
7. **RSS subscriptions never trigger it.** `kind == Rss` with gaps → count 0.

For task 4, in the existing projection test module: a raw yt-dlp response whose
`entries` contain a `_type: "playlist"` tab entry alongside real videos yields
only the videos, **and still yields the same `source_url`**. The second half is
the point — it is what proves channel resolution did not regress.

Unit tests for `fill_missing_durations_in` in `store.rs`'s own test module: an
unknown GUID changes nothing, a zero duration changes nothing, a matching GUID in
a *different* subscription changes nothing. And for
`episodes_missing_duration_in`: a 24-character GUID with no duration is not
counted.

## Verification

- `cargo test -p reprise-core podcasts::` — the new module plus the existing
  YouTube suites, which must stay green (the projection and gate tests are the
  ones this change can plausibly disturb).
- `cargo clippy --all-targets -- -D warnings` for the touched crate.
- Manual, against the real library. Counting rule, written down before running:

  ```sql
  SELECT count(*) FROM podcast_episodes e
    JOIN podcast_subscriptions s ON s.id = e.subscription_id
   WHERE s.title = ? AND e.removed_at IS NULL
     AND (e.duration_secs IS NULL OR e.duration_secs = 0);
  ```

  | arm | channel | before | expected after |
  |---|---|---|---|
  | fix | Nordheim Melodies | 9 | 0 |
  | fix | Mystical Nordic Ambience | 9 | 0 |
  | ghost | Bjorth | 5 | **1** — and no second listing on an immediate re-refresh |
  | control | Heldom | 0 | 0, **and no yt-dlp listing call at all** |

  The control arm is checked via the yt-dlp invocation, not by looking at the UI.

## Parallelität

**No cut. This is a single strand.**

Tasks 1–3 are a chain in two adjacent files — `store.rs` (tasks 1 and 2) and
`pipeline_sync.rs` (task 3) — where each task is the next one's precondition: the
fill step cannot be written before the store functions it calls, and none of the
refresh tests go green until the whole chain exists.

Task 4 (`ytdlp_playlist.rs`) *is* file-disjoint from that chain, so it is the one
candidate for a second strand — and it is not worth cutting. It is a single
filter clause plus one assertion in an existing test module, and task 1's ghost
guard and task 4's ghost fix are two halves of one argument that a reviewer has
to read together. Handing it to a second agent would cost a worktree, a branch
and a merge to save a few minutes on a change that small, and would split the
evidence for the one decision most likely to be questioned.

- **Merge order:** n/a (one branch).
- **Post-merge cross-checks:** none — no comparison in this plan reads a file
  outside the single strand.
