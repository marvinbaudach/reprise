#!/usr/bin/env bash
# The film: 60.0 s for a landing page, which is exactly 100 beats at 100 BPM.
#
# Every boundary is a multiple of 0.6 s, so the film can be laid against a bed
# later without a single cut falling between two beats. The shot list and the
# reasoning behind it are in docs/plans/showreel-30s.md.
#
# Two things distinguish this cut from the 32 s one it replaces.
#
# Nothing runs under 3.0 s. The short cut had 1.2 s bursts and a floor of 1.8 s,
# which is enough to register that a screen changed and not enough to read
# either the caption or the screen — the film went past faster than it could be
# taken in.
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
# take 2, not take 1: the first phone take predates the visualiser fix (#701
# landed 2026-08-26 00:01, that take was recorded 25.08 22:05) and never had an
# artist view in it at all.
INA="${SHOWREEL_ANDROID_TAKE:-$SHOWREEL_DIR/roh-android-take2.mp4}"
# The MCP take, shot by take-mcp.sh. Optional on purpose: the rest of the film
# has to stay renderable when that take is missing or has to be reshot.
INM="${SHOWREEL_MCP_TAKE:-$SHOWREEL_DIR/roh-gnome-mcp.mp4}"
# Takes 3 and 4 were shot for the two add flows — the YouTube channel by name
# and the podcast country chart. Both are out of the cut now: the film shows
# what the library holds, not the plumbing that fills it. The takes stay on
# disk and the shot lines below stay in the history, so putting either flow
# back is a matter of restoring two lines, not shooting again.
OUT="${1:-$SHOWREEL_DIR/reprise-showreel-cut.mp4}"
showreel_require "$IN1" "$IN2" "$INA"

SHELL="$SHOWREEL_WORK/phone-shell.png"
# 830 px of screen plus bezel and the room the shadow needs comes to exactly
# the 960 px stage, so the whole handset is in frame with nothing clipped.
read -r SHELL_W SHELL_H SHELL_X SHELL_Y SHELL_SCREEN_W SHELL_SCREEN_H < <(
  python3 scripts/showreel/device-frame.py "$SHELL" 400 830
)

O="$SHOWREEL_WORK/cut35"
LIST="$O/list.txt"
mkdir -p -- "$O"
rm -f -- "$O"/s-*.mp4 "$LIST"

CROP="crop=2880:1747:0:53"
DEBADGE="split[a][b];[b]crop=180:88:600:0[p];[a][p]overlay=128:0"
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
[f]crop=1080:2240:0:80,scale=${SHELL_SCREEN_W}:${SHELL_SCREEN_H},\
pad=${SHELL_W}:${SHELL_H}:${SHELL_X}:${SHELL_Y}:color=black@0[scr];\
[scr][1:v]overlay=0:0:format=auto[fg];\
[bg][fg]overlay=(W-w)/2:(H-h)/2,$(film2_push "$(frames "$dur")" "$dir" "$zoom" 1920 "$FILM2_STAGE_H" 0.5 0.5),\
pad=1920:1080:0:0:color=$FILM2_GROUND,$over,$(film2_bug)$(film2_dip "$dip" "$dur"),format=yuv420p[v]" \
    -map '[v]' "${ENC[@]}" -frames:v "$(frames "$dur")" -y "$O/s-$name.mp4"
  listed "$name"
}

# The handover from desktop to phone, as one segment.
#
# The desktop dives into its own visualiser and the phone slides in already
# showing the same picture, so the two platforms are joined by what is on screen
# instead of announced by a caption. That is why the statement that used to sit
# here is gone: the match is the claim.
#
# 1.3 s + 1.3 s with a 0.2 s slide between them lands on 2.4 s exactly, which is
# one bar. The halves are aimed by hand: the desktop visualiser sits at 0.92,
# 0.54 of the stage and the phone's at 0.50, 0.375 of the composed frame.
bridge() { # name deskstart phonestart
  local name=$1 dstart=$2 pstart=$3
  local half=1.3 xf=0.2 dur=2.4
  LIST_OFF=1
  desk T1 "$name-a" "$dstart" "$half" in 2.0 0.55 1.00 null "" accel
  phone "$name-b" "$pstart" "$half" out 1.5 null "" decel 0.50 0.375
  LIST_OFF=
  ffmpeg -v error -i "$O/s-$name-a.mp4" -i "$O/s-$name-b.mp4" \
    -filter_complex "[0:v][1:v]xfade=transition=slideleft:duration=$xf:offset=$(python3 -c "print(round($half - $xf, 3))"),format=yuv420p[v]" \
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
  desk TM "$name-a" "$ask" "$half" hold 0.06 0.50 0.50 \
    "$(film2_prompt 'Build me a playlist like Lorna Shore.' 'asked of an agent, over Reprise MCP' "$half")"
  # The row is the whole payoff of the shot and it is fourteen pixels tall in a
  # 1920-wide frame. Two and a half seconds is not long enough to find it
  # unaided, so it gets marked. The box lands 0.4 s in, after the eye has taken
  # the screen — a marker that is already there when the cut arrives reads as
  # chrome, one that lands reads as an answer. Coordinates are measured off the
  # finished frame, which is what $over sees: it runs after $PAD, so they are
  # 1920x1080 coordinates and they move if this shot is ever reframed.
  desk TM "$name-b" "$result" "$half" hold 0.10 0.16 0.40 \
    "$(film2_callout 'The agent wrote it' 'MCP, straight into the library' "$half"),drawbox=x=158:y=290:w=228:h=44:color=0x49C9D2@0.95:t=3:enable='gte(t,0.4)'"
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
# The zoom column is a framing, not a move: `hold` holds it for the whole shot.
#
# What a framing may cut is the whole rule here. The sidebar and the right-hand
# panel are bounded objects — half of one reads as a broken screenshot, so a
# shot either contains it or starts past it. A track list is not bounded; it is
# meant to continue past the frame, so cutting a row off the bottom costs
# nothing. That is why every region shot below sits at fx=1.0: at these amounts
# the left edge lands in the list's own padding, clear of the sidebar, and the
# right edge is the window's.
#
# The hook is the exception and holds the whole application at 1:1. It is the
# establishing shot — the sidebar with its counts is the thing being
# established, and every shot after it is allowed to be a region because this
# one showed the whole.
desk T1 01-hook    104.0 4.2 hold 0.00 0.50 0.50 "$(film2_statement 'One player. Everything you listen to.' 0.4 4.2)"
desk T2 02-search   36.3 4.8 hold 0.20 1.00 0.00 "$(film2_callout 'Instant search' 'every field, as you type' 4.8)"
# Held at the same amount as the rest rather than pushed in on the lyrics
# pane: anything tighter has to start inside the track list, and there is no
# x where that edge does not land in the middle of a word. The shot wants a
# layout with the lyrics given the width — that is a thing to record, not a
# thing to crop.
desk T2 03-lyrics   50.5 4.2 hold 0.17 1.00 0.00 "$(film2_callout 'Lyrics, in time' '' 4.2)"
desk T1 04-releases 39.8 3.0 hold 0.20 1.00 0.00 "$(film2_callout 'New releases' 'from the artists you keep' 3.0)"
desk T1 05-concerts 49.5 3.6 hold 0.20 1.00 0.00 "$(film2_callout 'Concerts nearby' 'for the same artists' 3.6)"
desk T1 06-podcasts 62.8 4.2 hold 0.20 1.00 0.00 "$(film2_callout 'Podcasts' 'shows, episodes, where you stopped' 4.2)"
desk T1 07-doctor   93.0 4.2 hold 0.20 1.00 0.00 "$(film2_callout 'Library Doctor' 'finds what is broken' 4.2)"
desk T1 08-stats   137.0 3.0 hold 0.15 1.00 0.00 "$(film2_callout 'Your listening, counted' '' 3.0)"
# In-points are provisional until the take is shot; take-mcp.sh holds the
# library for four seconds before the write and six after the row is selected.
mcp 08b-agent       4.0 15.0
bridge 09-handover 106.0 46.0

# -------------------------------------------------------------------- the phone
# The phone shots hold at nothing at all, and that is not laziness.
#
# device-frame.py sizes the handset so that its body, bezel and the room its
# shadow needs come to exactly the 960 px stage — there is no margin left over.
# So any amount above zero crops the device itself, which is why the handset
# looked shaved off top and bottom. The framing for these shots already
# happened, in device-frame.py; the cut has nothing to add to it.
phone 10-search      11.0 4.2 hold 0.00 "$(film2_callout 'Reprise on Android' 'search, grouped by albums and artists' 4.2)"
# Held slower than anything else in the film on purpose: this page is meant to
# be read, not glanced at.
phone 11-artist      18.6 3.6 hold 0.00 "$(film2_callout 'Straight to the artist' '' 3.6)"
phone 12-play        30.0 3.6 hold 0.00 "$(film2_callout 'Play the newest album' '' 3.6)"
phone 13-visuals     48.6 3.6 hold 0.00 "$(film2_callout 'The same visuals' '' 3.6)" out

card endcard 14-end

ffmpeg -v error -f concat -safe 0 -i "$LIST" -c copy -y "$OUT"
printf '%s  %s s\n' "$OUT" "$(showreel_duration "$OUT")"
