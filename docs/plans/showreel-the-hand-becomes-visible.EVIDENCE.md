# R5/R6 measured — the phone, 2026-08-29

Every coordinate below was read off the running app with `uiautomator dump`, not
adapted from `take-android2.sh`. The plan requires that, and it was right to:
the old coordinates are stale in three places and the search bar moves the whole
page under them.

Device: Pixel 10 Pro XL, `59100DLCQ006SB`, 1080x2404, density 390,
Android 17 (SDK 37). App `io.github.marvinbaudach.reprise` 0.1.64 (versionCode
64), installed 2026-08-28 13:13.

`adb` did not see the phone when it was first plugged back in although
`lsusb` showed `18d1:4ee7` (charging + debug). `adb kill-server`,
`adb start-server`, then a few seconds' wait brought it back. Worth trying
before suspecting the cable.

## The open question is answered: yes, you can play from the artist view

`take-android2.sh` taps through to the newest *album* and plays from there, and
the plan would not fix the shot list until playing from the artist view was
confirmed to exist. It does: the artist view carries **`Play Lorna Shore`**, a
labelled affordance of its own, and tapping it starts the artist and raises the
mini player. Verified end to end — mini player showed `Prison of Flesh` /
`Lorna Shore` with a `Pause` button, so the music was running.

That makes the plan's shot 2 possible as written: the tap that starts the music
is inside the picture that shows the artist.

## The trap: the search bar moves everything by 177 px

The header is collapsible, and its state changes every y below it:

| element in the artist view | search bar expanded | collapsed |
|---|---|---|
| `Back to artists` / title | 510 | 333 |
| `Play <artist>` | **634** | **457** |

Arriving from a search, the bar is expanded — so the take wants **634**. Scroll
or a `BACK` collapses it. Nothing in the old script accounts for this, and a tap
177 px off lands on the first album instead of Play.

Both columns are read values, not one value and an offset: the collapsed pair
was dumped in the artist view itself (`Back to artists` 333, `Lorna Shore` 333,
`Play Lorna Shore` 457) after returning from an album, which is how the bar ends
up collapsed there. The album view shows the same 177: `Play <album>` at 693
expanded, 516 collapsed. The take should still not rely on the constant — read
the state, or keep the arrival path fixed so the bar is always expanded.

## Measured coordinates, in shot order

    TAB_ARTISTS    539 2292   bottom tab bar ("Titles 172 / Artists 539 / Queue 906")
    SEARCH_ICON    894 219    header, reads "Search library"
    ARTIST_ROW     400 1455   results: "Lorna Shore" 1428, "4 albums • 33 tracks" 1482
    ARTIST_PLAY    127 634    "Play Lorna Shore", search bar expanded
    MINI_PLAYER    400 2045   track text; transport at y 2042
    COVER          540 925    Now Playing artwork — tapping it swaps in the visualiser

Changed against `take-android2.sh`: the tab bar moved 2240 -> 2292, the mini
player 2016 -> 2045, and `ALBUM_NEWEST`/`ALBUM_PLAY` are gone with the album
detour. The search icon and artist row survive.

Also changed since the old take's comment: the search field reads
**"Search library"** on every tab now, not "Search albums and artists" in the
Artists tab. The old note about searching the wrong tab no longer applies, but
the Artists tab is still where the flow starts.

## The visualiser is a tap on the cover

Now Playing opens on artwork plus a coloured waveform seek bar. **Tapping the
cover swaps the artwork for the live spectrum**, and the fog behind it responds.
That is shot 3, and it needs an explicit tap — arriving at Now Playing is not
enough. Confirmed by screenshot, playing, at 3:23 of `Prison of Flesh`.

**It holds once swapped**, which the 4.8 s shot depends on: still running after
15 s of continuous playback (3:46 -> 4:07), and still running after leaving Now
Playing and re-entering through the mini player (4:24). It does not revert on
its own.

**But the state is sticky, and that is a trap.** A later run found the
visualiser *already* active on arrival — it survives across app starts. The tap
is a toggle, so a take that assumes artwork would switch the visualiser off on
camera. The take must read the state first, or force artwork before the shot.

## Settling is not optional

One run tapped `400 1455` and opened an *album* instead of the artist; an
identical run 3.5 s later opened the artist correctly. The dump was taken while
the results list was still settling, so the rendered row had moved under the
coordinate by the time the tap landed. The take must either wait longer after
the search or verify the screen it reached — a tap that misses does not fail,
it films the wrong page.

`Back to artists` in the album view is a *logical* parent, not history: it goes
to the artist however the album was reached. So "Back landed on the artist view"
is not evidence that the previous tap opened the artist.

## The next decision, before any code

The touch indicator is composited in post, so the take must log each tap. The
log's shape is the interface between the two halves, and two things in it are
undecided — getting either wrong offsets every indicator by a constant that
nobody sees until the frames are read:

- **A tap is an instant; an indicator is a window.** Arriving, hovering and
  pressing needs a duration, so the log records either a duration per tap or
  the compositor owns a fixed one.
- **Which zero?** `take-android2.sh` measures against its own `t0`, but the
  screen recording starts later. The offset between the two has to be written
  down by whichever side knows it, or the indicators drift as a block.

Nothing should be built until both are settled.

## Decided: the search closes before the artist shot

Asked for on 2026-08-29. The expanded search bar is left over from how the shot
was *reached*, not part of what the shot shows, and it pushes the page down by
the 177 px above. So the flow closes it once the artist view is open, and the
shot is framed on the artist.

That also removes the reason to carry two coordinate sets: with the bar closed
every take, `Play <artist>` is at **457** and the expanded 634 is only a
waypoint.

**Measured: it is two taps on the same spot, `983 260`.** The button relabels
between them, which is why one tap is not enough:

1. first tap — `Clear search`: the field empties, the bar stays open, and the
   same button becomes `Close search`
2. second tap — `Close search`: the bar collapses

The artist view survives both; `Back to artists` and `Lorna Shore` are still
there afterwards, and `Play Lorna Shore` lands on **457** — so the collapsed
number is now measured on the real arrival path, not only after a `BACK`.
Playing from that state was confirmed: the tap raised the mini player with the
transport showing `Pause`.

Do not shorten it to one tap because the field looks empty. Arriving from a
search it never is, and a single tap leaves the bar standing.

## Still not built

Nothing draws a touch indicator. `tap()` in `take-android2.sh` shells straight
to `adb shell input tap` and records nothing; only the six phase marks reach the
timeline. The indicator has to be composited in post, which needs the take to
log each tap's coordinate **and** its time first.
