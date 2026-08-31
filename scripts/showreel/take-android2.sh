#!/usr/bin/env bash
# The phone take for the short cut: search, the artist, the newest album, the
# visualiser — in that order, because that is the order a person discovers them.
#
# Every coordinate is pinned to a Pixel 10 Pro XL in portrait at 1080x2404. A
# different device, density or layout invalidates all of them; check the take
# before trusting the timeline it writes.
#
# Two things the first phone take got wrong and this one does not:
#
#  * the search lives per tab. In the Queue tab it reads "Search queue" and
#    filters the queue; the artist flow needs the Artists tab, where it reads
#    "Search albums and artists". The old take searched the wrong one.
#  * the keyboard has to go before the shot. BACK dismisses it and keeps the
#    results, otherwise half the frame is a keyboard nobody wants to film.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

APP=io.github.marvinbaudach.reprise
timeline="$SHOWREEL_WORK/timeline-android2.tsv"
t0=$(date +%s.%N)
: >"$timeline"

mark() {
  printf '%s\t%s\n' "$1" "$(echo "$(date +%s.%N) - $t0" | bc)" >>"$timeline"
  printf '[%s]\n' "$1"
}
tap() { adb shell input tap "$1" "$2"; }

TAB_ARTISTS='538 2240'
# The search is behind the header icon, not open on arrival: tapping where the
# field will be lands on the first artist row instead and the take walks off.
SEARCH_ICON='891 216'
ARTIST_ROW='400 1440'
ALBUM_NEWEST='400 1480'
ALBUM_PLAY='158 692'
MINI_PLAYER='400 2016'

sleep 2.5

mark search
tap $TAB_ARTISTS
sleep 2.0
tap $SEARCH_ICON
sleep 1.2
adb shell input text "lorna"
sleep 2.0
adb shell input keyevent BACK
sleep 6.0                       # grouped hits: four albums by year, then the artist

mark artist
tap $ARTIST_ROW
sleep 8.0                       # held on purpose — the page is meant to be read

mark album
tap $ALBUM_NEWEST
sleep 3.5

mark play
tap $ALBUM_PLAY
sleep 4.0
tap $MINI_PLAYER
sleep 6.0                       # mini player -> Now Playing

mark visualizer
sleep 14.0                      # the scene, on a build that carries #701

mark end
