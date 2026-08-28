# Showreel — state at the end of 2026-08-28

Read `showreel-60s.HANDOFF.md` first for the reasoning. This file is only what
changed today and what is still open.

## The film is locked

`~/Videos/reprise-showreel/reprise-showreel-60s.mp4` — 60.000000 s, 1920x1080,
1800 frames, −16.1 LUFS, −3.7 dBTP. The showroom carries it. Suite 85/85.

Commits on `showreel-recut-and-drivers`, **none pushed**:

| | |
|---|---|
| `ce5cc6fec2` | the agent shot, `wait-active.py`, the seed/caption fix, `pick-window.py` recalibrated |
| `dc48fcc631` | showroom carries the 60 s film, cue sheet rewritten |
| `3bec417ae1` | handoff status |
| `41bfc49a9e` | framing fixes: lyrics anchor, bridge target, the playlist mark |

## Acceptance, every time

`ffprobe` must say **60.000000**. `cut-film.sh` skips a missing take silently
and exits 0 — an exit status proves nothing here. Second check: the per-2 s
loudness profile must have no block below about −30 dB except the tail.

## Open

**The agent shot has no phone half.** `take-mcp.sh` is ready, the Pixel is
connected, the app with the device-sync fix runs from
`/home/marvin/Projects/reprise-external-changes-reach-device-sync/target/debug/reprise`,
and `Open Pixel 10 Pro XL` is in the sidebar. It needs one thing: somebody
clicking the Reprise window forward inside the preroll. Two attempts aborted
cleanly — `wait-active.py` refuses to record the wrong window, and
`org.freedesktop.Application.Activate` is accepted but does not raise the
window, so Mutter's focus-stealing prevention is confirmed for that route too.

**And having the footage is not having it in the film.** `mcp()` builds the
shot from two 2.5 s halves. A third beat for the sync costs 2.4 s that must
come off another shot, or the film stops being 100 beats.

**The music is a pulse, not an arc.** Variant C (the original, 31 s run),
windowed from 7.2 s, match 0.293. The regeneration on 2026-08-28 failed
because the prompts carried *timestamps* — ElevenLabs does not honour them.
Style words are the lever that works. A composed bed (`bed.py` extended to
60 s, acceptance: correlation > 0.80) was being built when this session ended;
check `~/.cache/reprise-showreel/musik/bed-60s.wav`.

**The lyrics shot** is legible but the lyrics are still a narrow column at the
edge. No crop fixes that — it wants a recording with the pane given the width.

**PR #728** (external changes reach device sync) is open against `dev`. Its
green check means nothing: it finished in 6 s with every real suite skipping.

## Two traps this session paid for

- **`ui.session.v1` restores `browser_place`.** `ui.window_view_mode=library`
  does not stop the app opening on the YouTube page. One take died to this.
- **`sqlite3` writes bypass `change_log`,** so the running app does not see
  them. Restart it after editing the library out from under it.
