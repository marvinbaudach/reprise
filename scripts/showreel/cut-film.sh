#!/usr/bin/env bash
# The film: 60.0 s for a landing page, which is exactly 100 beats at 100 BPM.
#
# Every boundary is a multiple of 0.6 s, so the film can be laid against a bed
# later without a single cut falling between two beats. The shot list and the
# reasoning behind it are in docs/plans/showreel-30s.md.
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

IN1="$SHOWREEL_DIR/roh-gnome-tour.mp4"
IN2="$SHOWREEL_DIR/roh-gnome-pickup.mp4"
# The bed take, not take 2: this is the only phone take shot while the handset
# played the film's own music, which is what lets the bars be claimed to move
# in time with it. Takes 1 and 2 were shot against whatever was in the library
# and are kept only as fallbacks — take 1 also predates the visualiser fix
# (#701, 2026-08-26 00:01, that take was recorded 25.08 22:05).
INA="${SHOWREEL_ANDROID_TAKE:-$SHOWREEL_DIR/roh-android-bed.mp4}"
# The MCP take, shot by take-mcp.sh. Optional on purpose: the rest of the film
# has to stay renderable when that take is missing or has to be reshot.
INM="${SHOWREEL_MCP_TAKE:-$SHOWREEL_DIR/roh-gnome-mcp.mp4}"
# Takes 3 and 4 were shot for the two add flows — the YouTube channel by name
# and the podcast country chart. Both are out of the cut now: the film shows
# what the library holds, not the plumbing that fills it. The takes stay on
# disk and the shot lines below stay in the history, so putting either flow
# back is a matter of restoring two lines, not shooting again.
OUT="${1:-$SHOWREEL_DIR/reprise-showreel-cut.mp4}"
showreel_require "$IN1" "$INA"

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
desk() { # take name start dur dir zoom fx fy over [dip] [ease]
  local take=$1 name=$2 start=$3 dur=$4 dir=$5 zoom=$6 fx=$7 fy=$8 over=$9 dip=${10:-} ease=${11:-lin}
  local input="$IN1" pre=""
  # The new pickup take carries no SCROLL-LOG badge — that was a debug overlay
  # on the take this one replaces. Patching it now would copy a strip of pixels
  # over an image that has nothing wrong with it, so T2 is passed through.
  [[ $take == T2 ]] && input="$IN2"
  [[ $take == TM ]] && input="$INM"
  # Take 3 was shot without the SCROLL-LOG badge, so it needs no patch.
  ffmpeg -v error -ss "$start" -t "$dur" -i "$input" \
    -vf "fps=30,$CROP,$pre$(film2_push "$(frames "$dur")" "$dir" "$zoom" "$STAGE_W" "$FILM2_STAGE_H" "$fx" "$fy" "$ease"),$PAD,$over,$(film2_bug)$(film2_dip "$dip" "$dur"),format=yuv420p" \
    "${ENC[@]}" -frames:v "$(frames "$dur")" -y "$O/s-$name.mp4"
  listed "$name"
}

# A phone shot: the portrait frame centred on its own blurred enlargement, so
# the sides are not dead black.
phone() { # name start dur dir zoom over [dip] [ease] [fx] [fy]
  local name=$1 start=$2 dur=$3 dir=$4 zoom=$5 over=$6 dip=${7:-} ease=${8:-lin} fx=${9:-0.5} fy=${10:-0.5}
  ffmpeg -v error -ss "$start" -t "$dur" -i "$INA" -loop 1 -framerate 30 -i "$SHELL" \
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
  LIST_OFF=1
  case ${SHOWREEL_BRIDGE:-devicesync} in
    # The visualiser panel, at the right edge of the window.
    visualizer) desk T1 "$name-a" "$dstart" "$dhalf" in 2.2 1.00 0.574 null "" accel ;;
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
    # The caption announces the Android app and names what it shares. The page
    # under it already says the mechanical half — MTP connected, the playlists,
    # the transfer counting up — so the words are spent on the claim the picture
    # cannot make on its own: it is not a companion app, it is the same core.
    *)          desk T1 "$name-a" "$dstart" "$dhalf" hold 0.00 0.50 0.50 \
                     "$(film2_callout 'A second frontend' 'the same core, now on Android' "$dhalf")" ;;
  esac
  phone "$name-b" "$pstart" "$phalf" hold 0.00 null ""
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
# The shots run in sidebar order, top to bottom: Music, Podcasts, Releases,
# Concerts, My Stats, Library Doctor. They used to run hook, releases, concerts,
# podcasts, doctor, stats — which reads as a tour that has lost its place,
# because the selected row jumps back up the sidebar twice while the captions
# claim a guided walk. The one inversion left is deliberate: the device page
# sits above Library Doctor in the sidebar, and it is last here because it is
# the handover and nothing can follow it.
#
# Every desk in-point below is a station mark from timeline-roh-gnome-tour.tsv
# plus 0.3 s. They are not chosen numbers: the take writes down when it aimed at
# each row, and the shot is that moment plus the reach, the click and the page.
# Library Doctor is out and the five that are left hold 5.4 s instead of 4.8 —
# dropping a station buys time, and the shots are what should get it. Intro plus
# desk still comes to 30.0, and the 1.8 s the missing station left goes into the
# handover rather than the title card, whose animation is choreographed to its
# own 3.0 s and would only sit still for longer.
#
# In-points are placeholders until they are measured on the take that is
# actually cut. `find-page-turns.py TAKE TIMELINE` gives the moment each page
# turns; the in-point is that moment minus 1.9 s, which is the lead the six
# in-points of the 2026-08-29 11:11 take all sit at (median 1.90, spread 1.55
# to 1.98). Do not carry the old `+0.3` over: it was the mark plus a constant
# from a take script that wrote its mark at a different moment.
desk T1 01-hook   "${SHOWREEL_IN_HOOK:-7.2}"      5.4 hold 0.00 0.50 0.50 "$(film2_statement 'One player. Everything you listen to.' 0.4 5.4)"
desk T1 02-podcasts "${SHOWREEL_IN_PODCASTS:-19.2}" 5.4 hold 0.00 0.50 0.50 "$(film2_callout 'Podcasts' 'shows, episodes, where you stopped' 5.4)"
desk T1 03-releases "${SHOWREEL_IN_RELEASES:-71.5}" 5.4 hold 0.00 0.50 0.50 "$(film2_callout 'New releases' 'from the artists you keep' 5.4)"
desk T1 04-concerts "${SHOWREEL_IN_CONCERTS:-84.8}" 5.4 hold 0.00 0.50 0.50 "$(film2_callout 'Concerts nearby' 'for the same artists' 5.4)"
desk T1 05-stats    "${SHOWREEL_IN_STATS:-97.1}"    5.4 hold 0.00 0.50 0.50 "$(film2_callout 'Your listening, counted' '' 5.4)"
# The agent shot is out. It asked the viewer to read a prompt, a page and a
# highlighted fourteen-pixel row inside four and a half seconds, and what it
# actually delivered was two screens of text going past. What MCP does is worth
# a film of its own; it is not worth the last shot before the handover. mcp()
# stays below, and putting the shot back is this one line.
# The in-point follows the choice: in the 2026-08-29 tour the device row is
# clicked at 123.3 and the page is up from about 124.5, with a sync already
# running on it — the phone auto-syncs when it is plugged in, so the tour
# arrived to find the transfer under way rather than starting one.
bridge 09-handover "${SHOWREEL_BRIDGE_IN:-125.0}" "${SHOWREEL_PHONE_IN:-45.2}"

# -------------------------------------------------------------------- the phone
# The phone shots hold at nothing at all, and that is not laziness.
#
# device-frame.py sizes the handset so that its body, bezel and the room its
# shadow needs come to exactly the 960 px stage — there is no margin left over.
# So any amount above zero crops the device itself, which is why the handset
# looked shaved off top and bottom. The framing for these shots already
# happened, in device-frame.py; the cut has nothing to add to it.
# Two shots, not four. The phone used to run search, artist, play and visuals at
# 4.8 s each — nineteen seconds of Android navigation in a film whose claim is
# that the library is the same everywhere. The navigation is the desktop's job;
# what the phone has to show is that the music actually plays there and that the
# visualiser is the same one. So: the cover, and then the bars moving to the
# track you can hear. 7.2 s each, which is the length at which a visualiser
# reads as following the music rather than as an animation.
#
# In-points are provisional until the take is reshot against the finished mix —
# the bars only land on the beat if the phone was playing the film's own bed
# while it was recorded.
# One shot, not two, and the take is what decided it. The app draws the cover
# and the spectrum into the same square and swaps them when the square is
# tapped (NowPlayingSheet's onTap, guarded by coverBounds), and this take was
# recorded with the spectrum already chosen — so it holds one picture for its
# whole length, not a cover followed by a visualiser. Two shots would have put
# a cut where nothing changes, which is the one cut a viewer notices and cannot
# explain. So the picture runs on for 9.6 s and the caption band changes under
# it, which is also closer to what was asked for: the phone shows the library
# playing, and the bars are already moving to the beat while it does.
#
# The in-point is measured, and it took three renders because the arithmetic
# had the sign backwards.
#
# The take plays the 55.8 s bed while the film runs on the 51 s one; both
# rejoin the original track at 66.0 and only their A segments differ (41.518
# against 36.516), so film time t sits at t + 5.002 in the take's bed for
# everything after the bass, which is the whole phone half. Reading bed time 0
# off the on-screen clock gave 4.2 and put this shot at 47.0.
#
# Then the finished cut was measured: the spectrum's own bar heights against
# the film's own audio, band for band — the cyan third against 40-160 Hz, the
# magenta third against 1-3.5 kHz, RMS per frame on both sides. The bars ran
# 0.6 s AHEAD of the sound. The shot was moved 0.6 s later to compensate and
# the lead went to 1.2 s, because a later in-point shows a further-advanced
# moment of the handset's playback at every film second — it adds to the lead
# rather than cancelling it. Two renders, slope +1.0, so zero lag is at 46.4.
#
# The lesson is the render, not the reasoning: the clock says one thing, the
# picture says another, and only the picture is watching what the viewer will
# watch. Re-measure after any change to the shot or the bed.
# Two shots again, and this time the take can carry them. The single 9.6 s
# picture above was forced by a take recorded with the spectrum already chosen —
# cover and spectrum share one square and swap on a tap, so that take could not
# show a cover followed by a visualiser. The reshoot is driven: titles, albums,
# artist, play in one continuous gesture, and only then the square tapped over
# to the spectrum. So the navigation is one shot with no cut inside it, which is
# what keeps it reading as 'the same library, in your hand' rather than as a
# second tour of an app the desktop half has already toured.
#
# BOTH IN-POINTS ARE UNMEASURED. The bars only sit on the beat if the handset
# was playing this film's own bed while it was recorded, and the on-screen clock
# is not good enough to find the offset: aligning by it once put the visualiser
# 0.6 s ahead and the readout gave no hint. Measure on the finished cut, by
# correlating the spectrum square's own bar heights against the film's audio in
# two bands — cyan third against 40-160 Hz, magenta third against 1-3.5 kHz. A
# later in-point makes a leading picture lead more, so take the slope over two
# renders rather than reasoning about the direction.
phone 12-android-nav "${SHOWREEL_PHONE_NAV_IN:-46.4}" 9.6 hold 0.00 \
  "$(film2_callout 'Reprise on Android' 'the same library, in your hand' 9.6)"
phone 13-android-vis "${SHOWREEL_PHONE_VIS_IN:-56.0}" 7.2 hold 0.00 \
  "$(film2_callout 'The same visuals' 'in time with the music' 7.2)" out

card endcard 14-end

ffmpeg -v error -f concat -safe 0 -i "$LIST" -c copy -y "$OUT"
printf '%s  %s s\n' "$OUT" "$(showreel_duration "$OUT")"
