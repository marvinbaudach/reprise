# Always download episodes — design

Date: 2026-08-20
Status: approved design, not yet planned

## Problem

Two separate things are wrong today.

A YouTube episode is played by streaming it: `resolve` asks yt-dlp for a
`bestaudio` URL, `stream_proxy` pulls that URL in 1 MB windows and serves them
back to `playbin3` over loopback. Nothing of that reaches the disk. The episode
stays a network dependency forever — offline it is gone, and every replay costs
the traffic again.

Separately, the disk never fills up on its own. `subscription.auto_download`
downloads at most `MAX_AUTO_DOWNLOADS_PER_SUBSCRIPTION` (3) episodes, and only
those a refresh *newly discovered*. An episode that existed before the flag was
switched on is never fetched, however new it is.

## What we are building

One rule, stated once: **the newest N episodes of every subscription are on
disk**, and **playback always comes from disk**.

"Newest" is by `published_at` descending — the top of the episode list, which
is what "die ersten 10" means here.

N is `keep_downloaded` — the number that today only deletes. It becomes the
target in both directions: fill up to N, and, where a cleanup policy deletes at
all, delete beyond N. `DEFAULT_KEEP_DOWNLOADED` moves from 5 to 10.

The two directions are not symmetric, and the spec must not pretend they are.
Filling runs unconditionally. Deleting runs only under
`CleanupPolicy::KeepLast5`, and the default policy is `KeepAll`. So out of the
box the disk holds *at least* the newest 10 per subscription, plus whatever
older material was downloaded manually or before the change. Nothing is deleted
until the user chooses a cleanup policy. That is deliberate: a change that
starts downloading more must not also start deleting on its own.

Raising the default from 5 to 10 changes behaviour for anyone who never set the
value *and* runs `KeepLast5` — they keep ten instead of five. That is a larger
disk footprint, never a deletion, so no stored download is lost by the change.

Streaming is dropped from the playback path.

## Decisions taken

| Question | Decision |
| --- | --- |
| Streaming keeps material? | No — streaming is dropped entirely. |
| Where does N come from? | `keep_downloaded`, existing setting, now two-directional. |
| Which subscriptions? | All. `subscription.auto_download` becomes dead. |
| Play with no file yet? | Download is dispatched, playback starts by itself when the file lands. |
| Backlog on first run? | Own background worker operation, not inline in refresh. |
| `stream_proxy.rs` | Left in place for now, no longer entered by playback. |

## Architecture

### One ranking, two consumers

The fill-up and the cleanup must agree on what "newest" means, or they fight:
one fetches what the other just deleted.

They do **not** rank the same population, and the plan must not paper over
that. `cleanup_candidates` ranks over downloaded episodes only
(`downloads.rs:386`), deliberately — a review finding (P1) recorded in the
comment above it: ranking over all episodes let undownloaded ones consume rank
positions, so a show with three downloads could have all three ranked past N
and deleted. The fill-up has the opposite need: it ranks over *all* live
episodes, because its job is to find the newest ones that are missing.

What must be shared is therefore the ordering, not the query:

```sql
ORDER BY e.published_at IS NULL, e.published_at DESC,
         e.first_seen_at DESC, e.id DESC
```

It becomes one named constant used by both window functions. Two populations,
one definition of "newest" — that is the only part that can drift.

The two sets converge rather than conflict: once the newest N overall are
downloaded, each of them ranks within the newest N *downloaded*, so the cleanup
finds nothing among them. Only older material — manual downloads, leftovers
from before this change — ranks beyond N and is eligible for deletion.

The fill-up does not slide its window past skipped episodes. If one of the
newest N is played and therefore skipped, the fill-up fetches N−1, not the
(N+1)th. Sliding would pull episodes into the download set that the cleanup
ranks outside it, and the two would start fighting again.

### Fill-up worker

A refresh no longer downloads. It records what is missing and returns; the
summary gains no new blocking work. A new `PodcastsOperation::FillDownloads`
runs afterwards in `podcasts_worker`, walks subscriptions, and dispatches
`download_episode_in` per missing episode, reporting through the existing
`on_download` / `PodcastsWorkerResult::DownloadState` channel so the episode
rows show the same progress they show for a manual download.

`subscription.auto_download` is no longer read by the pipeline, and its **UI
switches go away** — in the add dialog and in preferences
(`auto_download_default`). A switch that silently does nothing is worse than no
switch. Removing a switch orphans whatever `docs/ux-rules.md` says about it;
that file is checked and updated in the same change, not afterwards.

The `podcast_subscriptions.auto_download` **column stays** for now. It has 84
non-test references, and this repo's own history with `ALTER TABLE ... DROP
COLUMN` (`db_new_releases_accent.rs`) shows the migration is not free. Dropping
a column nothing reads is a safe, separate change; bundling it here would
double the diff of a behaviour change for no behavioural gain.

**Played episodes are skipped by the fill-up.** Otherwise `CleanupPolicy::`
`DeletePlayedAfter7Days` and the fill-up form a loop: cleanup deletes a played
episode after 7 days, the fill-up re-fetches it the same day because it is
still within the newest N, forever.

### Playback

`media_from_episode` decides today between `EpisodeSource::File` and
`EpisodeSource::Url`. It has one definition
(`external_media_toast.rs:38`); `external_media.rs:797` only re-exports it.
The decision therefore already lives in one place and stays there.

New behaviour for `PodcastKind::Youtube` with no `downloaded_path`:

1. Run the download on a named background task, the same shape `resolve_youtube`
   uses today (`one_shot_task::spawn_with_progress`, so progress reaches the
   session).
2. Hold the session in `PodcastPhase::Resolving`, which now means "fetching",
   and show download progress on the row.
3. On `DownloadState::Downloaded`, play the local file.
4. On `DownloadState::Failed`, fail the session with the download's own
   message.

The resolve path and `stream_proxy` stay in the tree, unreferenced by playback.
Removing them is a separate, later change.

### One download per episode, whoever asks

There are now three callers that can start a download of the same episode: the
download button, the fill-up, and playback. Two concurrent runs for one episode
write the same `partial_path` — the `.part` file — and corrupt each other.

Today's only guard is `download_request_allowed` (`podcasts_removal.rs:44`),
which checks the podcasts view's own `download_states` map. It cannot see a
download playback started, and playback cannot see it. A second UI-side map
would be the same mistake twice.

The guard therefore moves into core, next to the executor every caller already
shares: `download_episode_in` takes a per-episode in-flight claim and returns
without starting a second run if one is held. One rule, one place, and it holds
for MCP's `music_manage_episodes` too, which is a fourth caller nobody would
have remembered to update.

### Duration

`duration_secs` is persisted today by `save_youtube_resolution`
(`external_media.rs:329`) out of `ResolvedAudio` — that is, out of the resolve
call that is going away. The listing already carries it: `YtDlpVideo`
`.duration_secs` is read on every refresh. Duration moves to the ingest path
and is persisted when episodes are stored, not when they are played.

`media_category` needs no change: the download path already persists it
(`downloads.rs:101-121`).

### Error handling

`PodcastError::classify()` folds every `YtDlpFailure` kind onto the single
sentence "YouTube source could not be read with yt-dlp". The specific,
actionable messages already exist in `ytdlp_failure.rs` — `ExtractorOutdated`
says "update yt-dlp and try again" — and never reach the user. That mattered
little while a failed download was one row among many; it matters a great deal
once a failed download is the reason playback did not start. `classify()` gains
a case per `YtDlpFailureKind` so the kind's own `user_message()` is what the
row and the playback failure show.

Offline, an episode that is not on disk cannot be played at all. That is new,
and it is the honest consequence of dropping streaming.

## Testing

The load-bearing test is the one that runs both halves together: after a
refresh plus fill-up with `keep_downloaded = N`, exactly the newest N live
unplayed episodes have a `downloaded_path`, and `cleanup_candidates` returns
none of them. Run it twice — a second pass must download nothing and delete
nothing. That is what rules out the fill/delete loop; a test of either half
alone cannot.

Alongside it:

- Fill-up skips played episodes, with `DeletePlayedAfter7Days` active.
- Playback of a YouTube episode with no file dispatches exactly one download
  even when the download button is pressed at the same moment.
- Playback starts on `Downloaded` and fails with the download's own message on
  `Failed`.
- Duration survives the move: an episode ingested from a listing carries
  `duration_secs` without any resolve having run.
- `classify()` returns a distinct sentence per `YtDlpFailureKind`.

Every one of these must be shown to fail against unmutated production code
before it is trusted — a fill-up test that passes with the fill-up removed is
measuring nothing.

## Out of scope

- Removing `stream_proxy.rs` and the resolve-before-playback path.
- Dropping the now-unread `podcast_subscriptions.auto_download` column.
- Switching a running stream to the local file mid-playback.
- Applying any of this to RSS podcasts' playback path, which does not use
  yt-dlp or the proxy.
