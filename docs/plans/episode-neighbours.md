---
slug: episode-neighbours
worktree: /home/marvin/Projects/reprise-episode-neighbours
branch: feature/episode-neighbours
phase: coded
codex_session:
created: 2026-07-30
base: origin/dev @ 31d8fa062a
---
# Episode neighbours — ⏮/⏭ for external playback

A playing podcast or YouTube episode gains neighbours: ⏮/⏭ move to the adjacent
row of the list the episode was started from. The right-hand panel stops claiming
"Nothing playing" while something is playing, and hides the lyrics tab for the
duration of any external session.

**Episodes do not become queue members.** No `QueueItem` sum type, no database
migration, no drag & drop, no MCP protocol change. That is the separate, much
larger `docs/plans/episodes-as-queue-citizens.md`, and this plan is deliberately
the small alternative to it.

The one structural piece this plan does add — *an external session knows the
context it was started from* — is exactly what that larger plan requires first
(`episodes-as-queue-citizens.md:58-62`: "`PodcastSession` needs to carry where it
was started from"). Nothing here is throwaway work if the big plan later lands.

## Decisions already taken — do not re-litigate

1. **Neighbours are the display order of the list the episode was started from** —
   the flat, rendered order across channel groups, newest first, *including*
   already-played episodes. Not the show, not the channel, not chronological.
2. **The neighbour list is a snapshot frozen at start.** A feed refresh mid-playback
   must not shift the neighbours under the user's feet.
3. **⏮ goes to the previous episode**, it does not first rewind the current one.
   `queue.rs`'s `previous()` behaves the same way, and one button with two meanings
   is worse than the minor inconvenience on an hour-long episode. The waveform is
   how you get back to the start.
4. **No wrap-around.** At the end of the list ⏭ is insensitive.
5. **Radio is excluded.** A stream has no neighbours; both buttons stay grey there,
   exactly as today.
6. **The view is not the session.** Playing from the queue and navigating to the
   YouTube category must leave ⏮/⏭ on the queue. The snapshot belongs to the
   session, never to whatever view happens to be open.
7. **Failure during an automatic advance skips on**; three consecutive failures
   stop with a toast. A *direct* click on a broken episode keeps showing the error
   it shows today — that behaviour was intentional.
8. **The lyrics tab is hidden for every external session** — podcast, YouTube and
   radio alike — not just for the ones with neighbours. The queue tab and the
   visualizer stay reachable.

## Verified against `origin/dev` @ 31d8fa062a

The local main checkout lags dev badly, so every line reference below was checked
against dev, not against `/home/marvin/Projects/reprise`:

- `crates/reprise-gnome/src/ui/player_bar/player_bar_external.rs` — **identical to
  the local checkout**; `:41-42` hard-sets `prev_button`/`next_button` to
  `set_sensitive(false)` for every external snapshot. This is the whole reason ⏮/⏭
  are grey.
- `crates/reprise-core/src/media_integration.rs` — **identical**; `:231-235`
  `can_go_next`/`can_go_previous` are `external_ref.is_none() && can_next/can_prev`.
- `crates/reprise-gnome/src/ui/playback/external_media_state.rs` — structurally
  unchanged (dev only dropped `preview_path()`). `PodcastSession` `:59-70`,
  `ExternalPlaybackState` `:101-110`, `mode()` `:113-120`.
- `crates/reprise-core/src/podcasts/query.rs` — **changed by the `Db` handle
  refactor (PR #173): these take `db: &Db`, no longer `conn: &Connection`.**
  `list_episodes` `:19` and `episodes_for_subscription` `:37` both order
  `published_at DESC` (`:30`, `:56`) — newest first, matching the rendered list.
  `next_unplayed_of_show` `:121` orders **ASC** and filters `played_at IS NULL`
  (`:136`).
- `crates/reprise-gnome/src/ui/now_playing/` — **zero occurrences of `external` or
  `ExternalPlaybackSnapshot` anywhere in the directory.** The panel genuinely does
  not know external playback exists; that is the "Nothing playing" bug.
- `crates/reprise-gnome/src/ui/now_playing/now_playing.rs` — `tab_stack` with
  `add_titled_with_icon` (`:143`, `:149`, `:155`), `LYRICS_PAGE` (`:151`),
  `PanelTab::{UpNext, Lyrics, Visual}` (`:198-200`, `:230-239`).
- The Podcasts view was **rewritten in dev** (+8413 lines: `podcasts_groups.rs`,
  `youtube_channel_detail.rs`, `podcasts_view_actions.rs`, …). Do not trust older
  notes about it. The anchors that survived: `PodcastsModel::replace(rows)`
  (`podcasts_model.rs:65`) and `::store()` (`:72`) hold the episodes in rendered
  order; activation runs through `podcasts_view_actions.rs:11`
  → `callbacks.on_episode_activated(row)` → `ExternalPlayback::play_external`
  (`external_media.rs:89`).

## The trap that shapes the design

`next_unplayed_of_show` already exists and already finds "a following episode" —
which makes it the obvious thing to wire ⏭ to. **Do not.** It orders ASC and skips
anything already played, while the list renders DESC including played rows. Wired
to ⏭ it would jump *upwards* in the visible list and silently skip rows the user
can see. It stays exactly where it is, serving the post-episode "Play next" offer,
untouched.

---

# Section 1 — the session remembers its neighbours

`PodcastSession` (`external_media_state.rs:59-70`) gains a neighbour context:

```rust
/// Episode ids in rendered order, plus this session's index in them.
/// Frozen at start: a feed refresh must not move the user's neighbours.
pub(super) struct NeighbourContext {
    episode_ids: Vec<i64>,
    index: usize,
}
```

Empty/absent context ⇒ today's behaviour in full. That is the case for the
post-episode "Play next" hand-off and for any caller that does not supply a list.

`play_external` (`external_media.rs:89`) takes the context alongside the media.
Radio passes none — `radio_view.rs:438` is a caller and must keep compiling
without gaining a concept it has no use for. Prefer a dedicated entry point or a
defaulted parameter over threading `Option<NeighbourContext>` through the radio
path.

The Podcasts view supplies it at activation: the ids from `PodcastsModel::store()`
in store order, which is the rendered order across all channel groups. Take the
**whole model**, not the currently expanded subset — a collapsed "Show all 27
episodes" group must not truncate the neighbours. `youtube_channel_detail.rs` is a
second activation site with its own list; it supplies its own list the same way.

Sizing: a `Vec<i64>` of every episode is a few hundred bytes at realistic library
sizes. Do not build an index structure for this.

## Section 2 — ⏮/⏭ drive those neighbours

`player_bar_external.rs:41-42` stops hard-setting `false`. Sensitivity becomes
"a neighbour exists in that direction": `index > 0` for ⏮, `index + 1 <
episode_ids.len()` for ⏭. Radio and a context-less session keep both grey.

The buttons themselves are wired once in
`playback/player_controller_wiring.rs:138,146` (`connect_previous`/`connect_next`)
and again for the compact player at `:254,261`. Route on the **session**, not on
the visible view: an active external session with a context takes the neighbour
path, everything else keeps going to the queue. Both call sites get the same
treatment — the mini player must not disagree with the main bar.

Activating a neighbour starts it like any episode: resolve, play from its own
stored resume position. It is a new session with the same frozen list and a
shifted index; it does not re-read the model.

## Section 3 — dead links do not trap the user

Six of eleven visible rows in the reported case fail to resolve
("YouTube source could not be read with yt-dlp"), so this is the common path, not
the edge case.

Today resolution failure ends in `fail_podcast` (`external_media.rs`, the `Err`
arm of the resolve future) and stops. When the session was entered by an
**automatic advance**, it must instead move to the next neighbour. Track
consecutive failures on the advance chain; at three, stop and toast. A direct
click on a broken episode keeps today's behaviour — that distinction is the point,
do not collapse it.

Note this is a *streaming* resolve, not a download: `YtDlp::resolve()` returns a
stream url which goes to `player.play_uri()`. Nothing is written to disk;
downloads remain the separate explicit action. Do not "fix" this by downloading.

## Section 4 — the right-hand panel

**Header.** The panel learns `ExternalPlaybackSnapshot` and shows the running
episode — title, show, its tile — instead of "Nothing playing". Reuse whatever
`player_bar_state.rs`'s `external_bar_display` already derives rather than
deriving the same strings a second time somewhere else; a duplicated predicate in
this runtime has produced an audible bug twice before.

**Lyrics tab.** Hidden while any external session is active — podcast, YouTube and
radio. If `PanelTab::Lyrics` is the selected page when the session starts, fall
back to the queue tab; when the session ends, the tab returns. Hide the page
rather than emptying it, and make sure the footer text (`:198-200`) follows.

**Queue tab and visualizer stay.** The user's queue still exists during an episode
and must remain reachable.

## Section 5 — media keys follow the buttons

`media_integration.rs:231-235`: `can_go_next`/`can_go_previous` currently go false
the moment `external_ref` is set. They become "external **without neighbours**",
so a headphone button does not sit dead next to a working on-screen button.
`MprisState` needs to carry whether neighbours exist; the MPRIS `Next`/`Previous`
commands route to the same path as the buttons.

Nothing else about the MPRIS surface changes — no new namespace, no metadata
change. Those belong to the queue-citizens plan.

---

## Rules and tests

`docs/ux-rules.md` section **AF. Podcasts & Radio** (`:3388`). Highest existing id
is `POD-19`; add:

> **POD-20** [active] [gtk] — A playing podcast or YouTube episode has neighbours:
> ⏮/⏭ move to the adjacent row of the list it was started from, in rendered order,
> without wrapping. The neighbour list is frozen when playback starts. Radio has no
> neighbours. While any external session is active the lyrics tab is hidden and the
> panel header shows the episode instead of "Nothing playing".

`scripts/check-ux-traceability.sh` requires at least one test named `pod_20_…` for
an `[active]` rule. `scripts/check-input-parity.sh` applies if any new
`DragSource`/`DropTarget` appears — none should here.

Also correct `docs/plans/podcasts-radio.md`, which states the opposite at `:374-375`
("`can_go_next`/`can_go_previous` = false, sobald `external_ref` gesetzt ist").
Record that it was narrowed to "external without neighbours" and why. A reversed
decision gets documented where the original decision lives.

Tests to write:
- ⏭ from the middle of a list lands on the row below; ⏮ on the row above.
- Both ends: ⏮ at index 0 and ⏭ at the last index are insensitive, no wrap.
- The snapshot is frozen: mutating the model after start does not change neighbours.
- Radio: both stay insensitive.
- Queue playing + Podcasts view open ⇒ ⏮/⏭ still drive the queue.
- Automatic advance onto a failing episode skips to the next; three consecutive
  failures stop and toast; a direct click on a failing episode does not skip.
- Lyrics tab hidden for a podcast, a YouTube and a radio session; selected-tab
  fallback to queue; tab returns when the session ends.
- `can_go_next`/`can_go_previous` true for an external session with a neighbour,
  false without one, false for radio.

## Definition of done

`scripts/check-merge-readiness.sh` green: `check-architecture.sh`,
`check-device-sync-gstreamer.sh`, `check-accessibility-semantics.sh`,
`check-input-parity.sh`, `check-frontend-thinness.sh`, `check-ux-traceability.sh`,
`check-motion-tokens.sh`, `cargo fmt --check`,
`cargo clippy --locked --all-targets --workspace -- -D warnings`,
`cargo test --locked --workspace`.

Display tests cannot run in a headless sandbox without Xvfb. **Write them; do not
claim to have run them.** Files stay under the project's 800-line limit — extract
rather than grow `external_media.rs`, which is already large.

## Relationship to the other plans

`docs/plans/episodes-as-queue-citizens.md` remains the full solution and is not
cancelled by this. If it later lands, section 1's neighbour context is the natural
place from which its `PlaybackMode::QueuedEpisode` origin grows, and sections 2–5
either survive unchanged or get superseded cleanly by real queue membership.

This plan does **not** depend on `podcast-remote-artwork` or
`podcast-multi-select`, unlike that one.
