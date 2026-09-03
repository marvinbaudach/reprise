#!/usr/bin/env bash
# The film: 58.2 s for a landing page, against a 120 BPM bed — 0.5 s to the beat.
#
# It was 60.0 s at 100 BPM, and every boundary was a multiple of that bed's
# 0.6 s. Both numbers changed when the bed did (2026-09-03), and the grid moved
# with them: the six desk shots run 4.5 s, which is not a multiple of 0.6 but is
# exactly nine beats of the bed that is actually under them. The rest of this
# file still reads in beats — check them against 0.5, not 0.6. The shot list and
# the reasoning behind it are in docs/plans/showreel-30s.md; what the 58.2 s cut
# is made of is in docs/plans/showreel-58s.HANDOFF-2026-09-03-nachmittag.md.
#
# Two things distinguish this cut from the 32 s one it replaces.
#
# Every feature runs 4.8 s. The short cut had 1.2 s bursts and a floor of 1.8 s,
# which is enough to register that a screen changed and not enough to read
# either the caption or the screen. 3.0 to 4.2 was the next answer and it was
# still too quick: a shot has to carry a caption, a page and whatever the page
# is doing, and the ones at 3.0 went past before the third of those registered.
# One length for all of them, so no feature reads as the minor one.
#
# And every shot holds. The short cut pushed two to four percent into every
# single shot, alternating in and out, which is what made it restless: at that
# amount the frame is not really closer, it is only never still. Here each shot
# is framed once, tight enough to read, and then left alone — the app moving
# inside a locked frame is the motion. The bridge is the one camera move left in
# the film, and it lands because nothing else moves.
#
# fps=30 comes first in every chain. The screencasts are variable-rate (their
# r_frame_rate reads 10000/1 and 4/1, really about 23.8 and 68), and zoompan
# counts input frames — fed VFR it renders the wrong number of them and the
# shot runs long.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh
source scripts/showreel/film2.sh

# The desk half is ONE take again, and that is the whole point of the
# 2026-09-03 re-record.
#
# Every desk shot opens on 1.9 s of the page it is leaving, and that lead is
# only right when the page it shows is the page of the shot before it. The
# 2026-09-02 cut mixed three desk takes, and mixing them is what put the wrong
# page in front of two shots: the Music pickup had been shot from My Stats, so
# the film opened on the stats page — the same page that closed the tour — and
# the stats shot was taken from the tour, whose Concerts station still stood at
# the old 500 km radius, so the shot after Concerts opened on a second, emptier
# Concerts. Both read as a station visited twice.
#
# So the stations are walked once, in cut order, in a single take:
# SHOWREEL_STATIONS=library,podcasts,youtube,releases,concerts,stats. The app
# is left on Radio before it starts, which is why the film opens on a page it
# never returns to. The handover's device page is the same walk's last station,
# so the desk half and the bridge come out of one continuous run.
IN1="${SHOWREEL_TOUR_TAKE:-$SHOWREEL_DIR/roh-gnome-tour-2026-09-03.mp4}"
# The device page is the last station of that same walk, so the handover leads
# with My Stats — the shot before it — exactly like every other shot does. It
# used to be a take of its own because the shot was a transfer that had to be
# running before the camera arrived; it is not a transfer any more.
INB="${SHOWREEL_SYNC_TAKE:-$IN1}"
# The phone is two takes now, not one bed take, because the app draws the cover
# and the spectrum into the same square and swaps them on a tap: one take
# cannot both navigate to a track and hold a cover that becomes a visualiser.
# So the navigation is one continuous gesture in its own take, and the sheet
# opening on the cover and turning into the spectrum is another. Both were shot
# while the handset played this film's own bed — that, and nothing else, is
# what lets the bars be claimed to move in time with the music.
INA="${SHOWREEL_ANDROID_TAKE:-$SHOWREEL_DIR/roh-android-gesture.mp4}"
INV="${SHOWREEL_ANDROID_VIS_TAKE:-$SHOWREEL_DIR/roh-android-nowplaying3.mp4}"
# The MCP take, shot by take-mcp.sh. Optional on purpose: the rest of the film
# has to stay renderable when that take is missing or has to be reshot.
INM="${SHOWREEL_MCP_TAKE:-$SHOWREEL_DIR/roh-gnome-mcp.mp4}"
# Takes 3 and 4 were shot for the two add flows — the YouTube channel by name
# and the podcast country chart. Both are out of the cut now: the film shows
# what the library holds, not the plumbing that fills it. The takes stay on
# disk and the shot lines below stay in the history, so putting either flow
# back is a matter of restoring two lines, not shooting again.
OUT="${1:-$SHOWREEL_DIR/reprise-showreel-cut.mp4}"
showreel_require "$IN1" "$INB" "$INA" "$INV"

SHELL="$SHOWREEL_WORK/phone-shell.png"
# 830 px of screen plus bezel and the room the shadow needs comes to exactly
# the 960 px stage, so the whole handset is in frame with nothing clipped.
#
# The width follows the recording rather than the other way round. The take is
# 1080x2400; the frame used to be built for 1080x2240 and the cut threw away
# 80 px at each end to make it fit, which sliced the status bar off the top of
# every phone shot. 830 * 1080 / 2400 = 373.
read -r SHELL_W SHELL_H SHELL_X SHELL_Y SHELL_SCREEN_W SHELL_SCREEN_H < <(
  python3 scripts/showreel/device-frame.py "$SHELL" 373 830
)

O="$SHOWREEL_WORK/cut35"
LIST="$O/list.txt"
mkdir -p -- "$O"
# Two runs sharing $O do not collide loudly, which is the problem. The second
# one deletes list.txt while the first is still appending to it, so the concat
# that follows can succeed — right duration, playable file, shots in the wrong
# order. It cost one render that came out with the end card first. A lock is
# better than a check for a running process because it also covers the run that
# is started a second later, while the check is still deciding.
exec 9>"$O/.lock"
flock -n 9 || {
  echo "another cut-film.sh holds $O — let it finish, or point SHOWREEL_WORK elsewhere" >&2
  exit 1
}
rm -f -- "$O"/s-*.mp4 "$LIST"

CROP="crop=2880:1747:0:53"
STAGE_W=1582
PAD="pad=1920:1080:169:0:color=$FILM2_GROUND"
# -frames:v is not belt-and-braces on top of -t: the takes are VFR (avg 23.8
# and 68 fps against an r_frame_rate of 10000/1 and 4/1), so -t alone yields a
# frame or eleven too many and the cut drifts off the 100 BPM grid. The frame
# count is the authority; -t only bounds the decode.
ENC=(-an -c:v libx264 -preset medium -crf 19 -pix_fmt yuv420p)

frames() { python3 -c "print(round($1*30))"; }
# A shot that is only half of a transition is rendered but not listed: the
# bridge stitches its two halves into one segment and lists that instead.
listed() { [[ -n ${LIST_OFF:-} ]] && return 0; printf "file '%s'\n" "$O/s-$1.mp4" >>"$LIST"; }

# A desktop shot. `over` is the caption layer, already built by the caller, so
# one function serves callouts, bursts and statements without branching on kind.
#
# `speed` is a time-lapse factor, not the push's easing curve — that is `ease`,
# the one after it. At speed N the shot reads `dur * N` seconds of source
# starting at `start` and compresses them back to `dur` seconds of output: the
# decode window (`-t`) grows, but the frame budget (`-frames:v`, and therefore
# everything measured in output frames — the push, the dip, the caption) does
# not. `setpts=PTS/N` has to run before `fps=30` and before the push: zoompan
# stamps its own timestamps from an internal counter, so anything upstream of
# it that only rewrote PTS without also being resampled by an `fps` filter is
# silently discarded.
desk() { # take name start dur dir zoom fx fy over [dip] [ease] [speed]
  local take=$1 name=$2 start=$3 dur=$4 dir=$5 zoom=$6 fx=$7 fy=$8 over=$9 dip=${10:-} ease=${11:-lin} speed=${12:-1}
  local input="$IN1" pre="" decode=$dur
  # T1 is the desk tour and the default; the other two takes are the sync shot
  # and the optional MCP shot.
  [[ $take == TB ]] && input="$INB"
  [[ $take == TM ]] && input="$INM"
  if [[ $speed != 1 ]]; then
    decode=$(python3 -c "print(round($dur * $speed, 3))")
    pre="setpts=PTS/$speed,fps=30,"
  fi
  # Take 3 was shot without the SCROLL-LOG badge, so it needs no patch.
  ffmpeg -v error -ss "$start" -t "$decode" -i "$input" \
    -vf "fps=30,$CROP,$pre$(film2_push "$(frames "$dur")" "$dir" "$zoom" "$STAGE_W" "$FILM2_STAGE_H" "$fx" "$fy" "$ease"),$PAD,$over,$(film2_bug)$(film2_dip "$dip" "$dur"),format=yuv420p" \
    "${ENC[@]}" -frames:v "$(frames "$dur")" -y "$O/s-$name.mp4"
  listed "$name"
}

# A phone shot: the portrait frame centred on its own blurred enlargement, so
# the sides are not dead black.
phone() { # take name start dur dir zoom over [dip] [ease] [fx] [fy]
  local take=$1 name=$2 start=$3 dur=$4 dir=$5 zoom=$6 over=$7 dip=${8:-} ease=${9:-lin} fx=${10:-0.5} fy=${11:-0.5}
  local input="$INA"
  [[ $take == PV ]] && input="$INV"
  ffmpeg -v error -ss "$start" -t "$dur" -i "$input" -loop 1 -framerate 30 -i "$SHELL" \
    -filter_complex "[0:v]fps=30,split[b][f];\
[b]scale=1920:-2,boxblur=64:3,crop=1920:${FILM2_STAGE_H},eq=brightness=-0.16:saturation=0.7[bg];\
[f]scale=${SHELL_SCREEN_W}:${SHELL_SCREEN_H},\
pad=${SHELL_W}:${SHELL_H}:${SHELL_X}:${SHELL_Y}:color=black@0[scr];\
[scr][1:v]overlay=0:0:format=auto[fg];\
[bg][fg]overlay=(W-w)/2:(H-h)/2,$(film2_push "$(frames "$dur")" "$dir" "$zoom" 1920 "$FILM2_STAGE_H" 0.5 0.5),\
pad=1920:1080:0:0:color=$FILM2_GROUND,$over,$(film2_bug)$(film2_dip "$dip" "$dur"),format=yuv420p[v]" \
    -map '[v]' "${ENC[@]}" -frames:v "$(frames "$dur")" -y "$O/s-$name.mp4"
  listed "$name"
}

# The handover from desktop to phone, as one segment.
#
# The visualiser is in the film exactly once, and it is the handover: the
# desktop pushes into its own visualiser, the phone pulls back out of the same
# picture. The two platforms are joined by what is on screen instead of
# announced by a caption — that is why the statement that used to sit here is
# gone. The match is the claim.
#
# This is the only camera move in the film, and it lands because nothing else
# moves. It used to dive into the bottom of the desktop frame, which is the
# seek bar's waveform — a teal squiggle in the player chrome, not the visualiser
# at all. The visualiser is the panel on the right of the window, and take 1 has
# it open from 110 s to 121 s.
#
# The push ends at z=3.2, which is where the visualiser's own height fills the
# frame: 1747 / 3.2 = 546, against the 545 px from its tab bar to the bottom of
# its readings. fy=0.574 centres that band; fx=1.0 is the window's right edge,
# which is the edge the panel sits against.
#
# The two halves are not the same length any more. At 1.3 s the device page was
# gone before it had been read: the eye arrives, finds a window it has never
# seen, and the slide is already taking it away. The sync tab is a claim that
# needs its furniture read — MTP connected, the playlists, the transfer counting
# up — so the desk half now holds 6.2 s and the phone half stays short, because
# the phone gets two whole shots of its own straight after.
#
# 5.0 + 1.2 - 0.2 = 6.0 s, ten beats. The slide starts 4.8 s in, which puts it
# on the 0.6 s grid at 41.4 s in the film — where the arc's recovery is set in
# arc_steps().
bridge() { # name deskstart phonestart
  local name=$1 dstart=$2 pstart=$3
  # The desk half took the 1.8 s that Library Doctor left. It is the shot the
  # user asked to be readable, it is the only one whose subject is time passing,
  # and lengthening it here is also what keeps the slide where the music wants
  # it: the slide starts at dhalf - xf into the bridge, so with the desk run
  # ending at 30.0 the bass at 36.5 and the slide at 36.6 still meet.
  local dhalf=${SHOWREEL_BRIDGE_DESK:-6.8} phalf=1.2 xf=0.2
  local dur
  dur=$(python3 -c "print(round($dhalf + $phalf - $xf, 3))")
  # SHOWREEL_BRIDGE_SPEED is a time-lapse factor on the desk half only — the
  # phone half and the slide are unaffected, and dhalf, and therefore the
  # slide's own offset at dhalf - xf, do not move: the shot still lands at 6.8 s
  # of output, it just carries dhalf * speed seconds of source.
  #
  # 1x now, and that is a decision about what the shot shows. It used to be 8x,
  # because the shot was a transfer already under way and a real transfer's
  # progress bar takes minutes to move: at normal speed it sat on "Syncing ·
  # 0 of 74 files, 0%" for the whole 6.8 s, and at 8x the counter walked. The
  # shot is not a transfer any more — it is the device page standing still,
  # with MTP connected, the playlists and the transfer profile on it — so there
  # is nothing to compress, and a time-lapse would only make a still page's
  # clock jump. Set it back to 8 if the shot is ever filmed mid-sync again.
  local speed=${SHOWREEL_BRIDGE_SPEED:-1}
  LIST_OFF=1
  case ${SHOWREEL_BRIDGE:-devicesync} in
    # The visualiser panel, at the right edge of the window.
    visualizer) desk TB "$name-a" "$dstart" "$dhalf" in 2.2 1.00 0.574 null "" accel "$speed" ;;
    # No push at all. The device page carries the claim in its own furniture —
    # MTP connected, the playlists, and at the bottom the transfer actually
    # running — and a push to 1.4 towards the top takes the transfer out of the
    # frame in the second half of the shot. The handover shows the whole window
    # and the whole handset; the slide between them is the move.
    # The device page: the desktop is syncing to the very phone that is about
    # to fill the screen, and says so in words — MTP connected, last synced,
    # 743 Reprise tracks on device. A rhyme between two spectra is decoration;
    # this is the claim the handover is there to make.
    #
    # The caption says what this page is, and this page is a sync. It used to
    # read 'A second frontend / the same core, now on Android' — a claim about
    # the handset, not about the window on screen, and the shot straight after
    # it makes that claim already, over the phone itself. What the page actually
    # shows is MTP, the playlists chosen for the device and Opus 160 in the
    # transfer profile, so that is what the words are spent on.
    *)          desk TB "$name-a" "$dstart" "$dhalf" hold 0.00 0.50 0.50 \
                     "$(film2_callout 'Sync to your phone' 'playlists over MTP, transcoded on the way' "$dhalf")" \
                     "" lin "$speed" ;;
  esac
  phone PA "$name-b" "$pstart" "$phalf" hold 0.00 null ""
  LIST_OFF=
  ffmpeg -v error -i "$O/s-$name-a.mp4" -i "$O/s-$name-b.mp4" \
    -filter_complex "[0:v][1:v]xfade=transition=slideleft:duration=$xf:offset=$(python3 -c "print(round($dhalf - $xf, 3))"),format=yuv420p[v]" \
    -map '[v]' "${ENC[@]}" -frames:v "$(frames "$dur")" -y "$O/s-$name.mp4"
  listed "$name"
}

mcp() { # name start_ask start_result
  local name=$1 ask=$2 result=$3
  local half=2.5 xf=0.2 dur=4.8
  [[ -f $INM ]] || {
    printf 'mcp: no take at %s — shot skipped\n' "$INM" >&2
    return 0
  }
  LIST_OFF=1
  desk TM "$name-a" "$ask" "$half" hold 0.00 0.50 0.50 \
    "$(film2_prompt 'Build me a playlist like Lorna Shore.' 'asked of an agent, over Reprise MCP' "$half")"
  # The row is the whole payoff of the shot and it is fourteen pixels tall in a
  # 1920-wide frame. Two and a half seconds is not long enough to find it
  # unaided, so it gets marked. The box lands 0.4 s in, after the eye has taken
  # the screen — a marker that is already there when the cut arrives reads as
  # chrome, one that lands reads as an answer. Coordinates are measured off the
  # finished frame, which is what $over sees: it runs after $PAD, so they are
  # 1920x1080 coordinates and they move if this shot is ever reframed.
  desk TM "$name-b" "$result" "$half" hold 0.00 0.50 0.50 \
    "$(film2_callout 'The agent wrote it' 'MCP, straight into the library' "$half"),drawbox=x=171:y=298:w=214:h=40:color=0x49C9D2@0.95:t=3:enable='gte(t,0.4)'"
  LIST_OFF=
  ffmpeg -v error -i "$O/s-$name-a.mp4" -i "$O/s-$name-b.mp4" \
    -filter_complex "[0:v][1:v]xfade=transition=fade:duration=$xf:offset=$(python3 -c "print(round($half - $xf, 3))"),format=yuv420p[v]" \
    -map '[v]' "${ENC[@]}" -frames:v "$(frames "$dur")" -y "$O/s-$name.mp4"
  listed "$name"
}

# Both cards are animated, so they are composited in Python and arrive already
# encoded — see introcard.py and endcard.py, which are two scores over the same
# cardkit. The intro lands the mark and names the two toolkits in four beats;
# the end card lands the same mark, raises the claim, draws a hairline, gives
# the address the last word and goes to black in six.
card() { # script name
  python3 "scripts/showreel/$1.py" "$O" "$O/s-$2.mp4" >/dev/null
  listed "$2"
}

card introcard 00-intro

# ------------------------------------------------------------------ the desktop
# An in-point is measured, not chosen. Four shots used to open on the page
# before them and navigate a second into their own running time, with the
# caption already naming the page that had not arrived yet — the hook opened on
# Library Doctor, releases on the library, concerts on releases, the doctor on
# the device page. `settle.py` finds the moment by comparing the window title
# against the strip from the end of the shot, where the page is right. It has to
# compare the title at full size: downscaled, "Music" and "Releases" differ by
# less than the noise floor and every page reports as already settled.
# Every page shot holds the whole application at 1:1. The film has exactly one
# camera move, the handover, and it lands because nothing else moves.
#
# The rule this replaces claimed that at 0.20, fx=1.0 "the left edge lands in
# the list's own padding, clear of the sidebar". It does not. The window fills
# the recording from column 10 to column 2879, so 0.20 takes 480 px off the
# left and that lands on the Sort button — the film shipped with it sliced in
# half. There is no amount that reframes a full-bleed window without cutting
# something off it, so a page shot does not try: it shows the window.
# The shots run in sidebar order, top to bottom: Music, Podcasts, YouTube,
# Releases, Concerts, My Stats. They used to run hook, releases, concerts,
# podcasts, doctor, stats — which reads as a tour that has lost its place,
# because the selected row jumps back up the sidebar twice while the captions
# claim a guided walk. Radio and Queue are the two library pages the tour walks
# past: Radio is on screen when the film opens, as the page the first click
# leaves, and Queue shows the film's own furniture back to itself.
#
# Six shots at 4.5 s, where there were five at 5.4. The desk block is 27.0 s
# either way, which is the reason this is the shape the YouTube station was
# added in: every boundary after 30.0 — the handover, both phone shots, the end
# card, and the eight milliseconds shot 13's visualiser was measured to — is
# where it was, by construction. The price is the grid this file's header
# declares: 4.5 is not a multiple of 0.6. It is nine beats of the 120 BPM bed
# the film is scored to, so the desk cuts sit on the beat rather than beside it,
# but it is a 0.5 s grid inside a film whose other half is built on 0.6.
#
# In-points are measured, never chosen. `find-page-turns.py TAKE TIMELINE`
# gives the moment each page turns; the in-point is that moment minus 1.9 s,
# the lead the six in-points of the 2026-08-29 11:11 take all sit at (median
# 1.90, spread 1.55 to 1.98). Do not carry an older `+0.3` over: it was the
# mark plus a constant from a take script that wrote its mark at a different
# moment.
#
# Measured on roh-gnome-tour-2026-09-03.mp4: turns at 8.93, 20.12, 31.23,
# 41.96, 52.83, 64.30 and 73.78 for the device page, every peak-to-background
# ratio 630 or better against a threshold of 4, and the lag from mark to turn
# 2.20–2.60 s across all seven — where the 2026-09-02 take ran 3.6 to 4.7.
desk T1 01-hook     "${SHOWREEL_IN_HOOK:-7.03}"      4.5 hold 0.00 0.50 0.50 "$(film2_statement 'One player. Everything you listen to.' 0.4 4.5)"
desk T1 02-podcasts "${SHOWREEL_IN_PODCASTS:-18.22}" 4.5 hold 0.00 0.50 0.50 "$(film2_callout 'Podcasts' 'shows, episodes, where you stopped' 4.5)"
desk T1 03-youtube  "${SHOWREEL_IN_YOUTUBE:-29.33}"  4.5 hold 0.00 0.50 0.50 "$(film2_callout 'YouTube channels' 'new videos, played as audio' 4.5)"
desk T1 04-releases "${SHOWREEL_IN_RELEASES:-40.06}" 4.5 hold 0.00 0.50 0.50 "$(film2_callout 'New releases' 'from the artists you keep' 4.5)"
desk T1 05-concerts "${SHOWREEL_IN_CONCERTS:-50.93}" 4.5 hold 0.00 0.50 0.50 "$(film2_callout 'Concerts nearby' 'for the same artists' 4.5)"
desk T1 06-stats    "${SHOWREEL_IN_STATS:-62.40}"    4.5 hold 0.00 0.50 0.50 "$(film2_callout 'Your listening, counted' '' 4.5)"
# The agent shot is out. It asked the viewer to read a prompt, a page and a
# highlighted fourteen-pixel row inside four and a half seconds, and what it
# actually delivered was two screens of text going past. What MCP does is worth
# a film of its own; it is not worth the last shot before the handover. mcp()
# stays below, and putting the shot back is this one line.
# The device page is the seventh station of the same walk, and it is filmed
# with nothing running on it: take-gnome4.py drives to the page and holds
# (SHOWREEL_SYNC_DWELL), and its Sync-now click is off by default. The page
# turns at 73.78, so this in-point leads with My Stats, and the shot is the
# page itself — MTP connected, last synced, the playlists chosen for the
# device, Opus 160 in the transfer profile.
#
# It used to be a transfer, filmed mid-sync and run at 8x so the counter walked.
# A film whose every other shot holds still had one shot with a progress bar
# racing in it, and the desk shots behind it carried the sync's own card in the
# sidebar. Both are gone; the phone in the handover is connected, not busy.
# The phone half is the navigation take 1.2 s before shot 12's own in-point, so
# the slide hands over to a picture that then simply continues.
bridge 09-handover "${SHOWREEL_BRIDGE_IN:-71.88}" \
       "${SHOWREEL_PHONE_IN:-$(python3 -c "print(round(${SHOWREEL_PHONE_NAV_IN:-5.0} - 1.2, 3))")}"

# -------------------------------------------------------------------- the phone
# The phone shots hold at nothing at all, and that is not laziness.
#
# device-frame.py sizes the handset so that its body, bezel and the room its
# shadow needs come to exactly the 960 px stage — there is no margin left over.
# So any amount above zero crops the device itself, which is why the handset
# looked shaved off top and bottom. The framing for these shots already
# happened, in device-frame.py; the cut has nothing to add to it.
# Two shots, and each one is a whole phone in one continuous picture. The phone
# used to run search, artist, play and visuals at 4.8 s each — nineteen seconds
# of Android navigation in a film whose claim is that the library is the same
# everywhere. The navigation is the desktop's job; what the phone has to show is
# that the music plays there and that the visualiser is the same one.
#
# 7.2 and 9.6, where it was 9.6 and 7.2. The pair still ends at 54.6, so the
# end card does not move; the navigation gives up 2.4 s, which it had, and the
# visualiser takes them, which it needed: the bars hold for a little over seven
# seconds before the square is tapped back to the cover, and the cover is the
# last picture before the end card. That order is the point of the shot — the
# visualiser is what the phone is here to show, and the cover is where it lands.
#
# Shot 12's take: begin 4.01, Artists 5.52, search 6.98, typing 8.06, keyboard
# away 9.60, the artist 10.60, the album 12.48, play 14.26. An in-point of 7.4
# opens with the search field just tapped and still holds the track list after
# the music starts; the only thing it gives up is the reach for the Artists tab.
#
# Shot 13 is the one shot whose timing is measured rather than chosen, and the
# on-screen clock is not good enough to measure it: aligning by the readout once
# put the visualiser 0.6 s ahead of the sound and gave no hint of it. Two
# methods, and only one of them answered.
#
# `measure-vis-sync.py` correlates the spectrum square's own bar heights against
# the film's audio in two bands — the cyan third against 40-160 Hz, the magenta
# third against 1-3.5 kHz. On this material it reported r = 0.23 and 0.43 with
# its two bands a quarter second apart, and said the same thing when the shot
# was 43 s out as when it was right. It is a cross-check, not the witness.
#
# The witness is the readout tick: decode the handset's own position readout at
# 30 fps, find the frames where the digits change, and compare those to the
# whole second. That put the finished cut at −0.008 s.
#
# A later in-point shows a further-advanced moment of the handset's playback at
# every film second, so a picture that leads leads more — take the slope over
# two renders rather than reasoning about the direction, and re-measure after
# any change to this shot or to the bed.
phone PA 12-android-nav "${SHOWREEL_PHONE_NAV_IN:-7.4}" 7.2 hold 0.00 \
  "$(film2_callout 'Reprise on Android' 'the same library, in your hand' 7.2)"
phone PV 13-android-vis "${SHOWREEL_PHONE_VIS_IN:-55.751}" 9.6 hold 0.00 \
  "$(film2_callout 'The same visuals' 'in time with the music' 9.6)" out

card endcard 14-end

ffmpeg -v error -f concat -safe 0 -i "$LIST" -c copy -y "$OUT"
printf '%s  %s s\n' "$OUT" "$(showreel_duration "$OUT")"
