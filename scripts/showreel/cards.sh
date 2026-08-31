#!/usr/bin/env bash
# The title and end card, built from the real brand assets rather than typeset
# by hand: the lockup comes out of data/brand/, the summary out of the AppStream
# metadata, so neither can drift away from what the project actually says.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/showreel/common.sh

# The outlined variant carries the wordmark as paths. The plain lockup keeps it
# as a live <text> in Fraunces, which is not installed system-wide — rsvg then
# falls back to a wider serif, the wordmark overruns the viewBox and the film
# says "Repris".
LOCKUP=data/brand/lockup-horizontal-outlined.svg
FONT=/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf
SUMMARY='Play and organize your music'
HOME_URL='marvinbaudach.github.io/reprise'
GROUND='#0D1014'
GLOW='#12615D'
MUTED='#8C9B9E'
TEAL='#4FDBD4'
showreel_require "$LOCKUP" "$FONT"

out="$SHOWREEL_WORK/cards"
mkdir -p -- "$out"

# The ground: the same near-black the film pads with, lifted by one soft teal
# glow off the top left — the gradient the showroom carries on its own hero.
# The gradient has to fade to black, not to the ground colour: Screen treats
# black as neutral, and anything lighter leaves the tile's own edge visible.
magick -size 1500x1500 radial-gradient:"$GLOW"-black "$out/glow.png"
magick -size 1920x1080 "xc:$GROUND" \
  "$out/glow.png" -geometry +-360+-620 -compose Screen -composite \
  "$out/ground.png"

# The wordmark is fill="currentColor" — rendered standalone it comes out black,
# which is invisible on this ground. Give it the ink the dark theme uses.
sed 's/currentColor/#EAF2F1/' "$LOCKUP" >"$out/lockup-light.svg"
rsvg-convert -w 760 -o "$out/lockup-title.png" "$out/lockup-light.svg"
rsvg-convert -w 520 -o "$out/lockup-end.png" "$out/lockup-light.svg"

magick "$out/ground.png" \
  "$out/lockup-title.png" -gravity center -geometry +0-70 -compose over -composite \
  -font "$FONT" -kerning 3 -pointsize 34 -fill "$MUTED" \
  -gravity center -annotate +0+90 "$SUMMARY" \
  "$SHOWREEL_DIR/card-title.png"

magick "$out/ground.png" \
  "$out/lockup-end.png" -gravity center -geometry +0-110 -compose over -composite \
  -font "$FONT" -kerning 3 -pointsize 32 -fill "$MUTED" \
  -gravity center -annotate +0+40 "$SUMMARY" \
  -font "$FONT" -kerning 2 -pointsize 28 -fill "$TEAL" \
  -gravity center -annotate +0+110 "$HOME_URL" \
  "$SHOWREEL_DIR/card-end.png"

printf 'cards -> %s/card-title.png, %s/card-end.png\n' "$SHOWREEL_DIR" "$SHOWREEL_DIR"
