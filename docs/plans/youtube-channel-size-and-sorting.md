---
slug: youtube-channel-size-and-sorting
worktree: /home/marvin/Projects/reprise-youtube-channel-size-and-sorting
branch: feature/youtube-channel-size-and-sorting
phase: shipped
created: 2026-08-16
reviewed: 2026-08-29
---

# YouTube channel size and sorting

## Goal

Add Channel keeps the fast relevance-ordered video search, then enriches every
visible channel with its published subscriber count in one background wave. A
YouTube-only `Largest first` toggle can reorder those existing rows by count.
A hidden, malformed, failed or timed-out count is omitted rather than displayed
as zero or `unknown`.

This completes `SRC-9`, whose optional follower field was fully projected but
never populated by production's flat `ytsearch20:` response, and adds the
ordering contract `SRC-23`.

## Measured constraint

On 2026-08-29, `ytsearch20:` produced 17 unique channels for `viking music`
and 12 for `sabaton carolus rex`. The flat search took about 1.5 to 1.9 seconds
and omitted `channel_follower_count`. A playlist-head request with
`--flat-playlist -I 0 -J <channel-url>` returned the count in about 1.1 seconds.
Seventeen requests with four workers took 6.07 seconds, so blocking the initial
render on enrichment would turn a roughly 1.7-second interaction into a roughly
7.8-second interaction.

YouTube's channel-filtered search was rejected: it searches channel names and
descriptions rather than channels with matching videos, overlapped only one of
the 17 video-search channels, and would lose the existing matching-video count.

## Contracts

- `search_channels()` stays unchanged and supplies the first render.
- `channel_follower_count()` uses the shared `YtDlp::run()` boundary so browser
  cookies, metadata language, process groups, redaction and timeouts remain
  consistent with every other yt-dlp request.
- `enrich_follower_counts()` fetches only missing counts, in relevance order,
  with four scoped workers, a 15-second channel-head timeout, a 20-second pass
  budget and a 20-channel ceiling.
- Per-channel failures and worker panics cost only that optional count.
- A shared atomic cancellation flag is checked before each channel is claimed.
  A new submit or dialog close sets it, bounding stale work to the four requests
  that can already be in flight.
- Add Channel renders wave one immediately. Wave two returns one complete
  channel-id/count vector, joins by canonical channel id, patches stored
  subtitle labels through the existing highlighted-markup path and moves stored
  row roots without clearing the list.
- `Largest first` remains sensitive while counts are pending. Toggling it records
  intent but moves no rows until wave two arrives. Published counts sort
  descending; ties and missing counts retain relevance order, with missing
  counts partitioned to the tail. Turning it off restores relevance order.
- The network-consent gate is read again immediately before wave two starts.
- MCP performs the same enrichment synchronously after its search and before DTO
  projection. Its existing omission behavior remains the public representation
  of an absent count.

## Verification contract

The fixtures must distinguish the two argv shapes. Coverage includes:

- search plus head fetch reaching the rendered subscriber subtitle;
- partial failure with one populated and one omitted count;
- absent, null and floating-point head values without an invented zero;
- whole-pass budget, cancellation, four-worker cap and 20-channel ceiling;
- stable descending ordering with ties and missing counts;
- channel-id joining while preserving query highlighting;
- a focusable pending sort toggle; and
- MCP returning a populated `subscriber_count` from the two-argv fixture.

The implementation gate is the normal repository gate plus focused Core,
GNOME and MCP tests, architecture, Core purity, gettext and UX traceability.

## Manual check

Run the app with fully isolated XDG directories, private D-Bus, Xvfb and fake
audio. Search `viking music`; confirm rows render before enrichment, gain counts
together after roughly six seconds, and reorder only if `Largest first` is
active. No live hidden-count channel was identified during the measurements, so
that case remains fixture-verified unless this check finds one; if it does, add
its channel id here.

### 2026-08-29 result

The check ran through CUA on an isolated Xvfb/Openbox display with private
D-Bus/AT-SPI, disposable XDG data and cache, and the fake audio sink. The first
snapshot showed 15 relevance-ordered rows without counts and an enabled
`Largest first` toggle. With the toggle activated while counts were pending,
the first enriched snapshot arrived about 8.3 seconds later; all 15 rows gained
counts together and the stored rows were ordered 834k, 629k, 229k, 199k, and
downward. No channel in that result set hid its count, so hidden-count behavior
remains fixture-verified rather than live-verified.

## Non-goals

- caching counts across searches;
- displaying playlist or total-video counts from the head response;
- persisting the sort choice;
- live or debounced search; and
- changing RSS or Radio result sections.
