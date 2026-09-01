#!/usr/bin/env bash
# Shared paths for the showreel drivers and cut scripts.
#
# SHOWREEL_DIR holds the raw takes and the finished films. It sits outside the
# repository on purpose: the footage is large, and it is a deliverable rather
# than source. SHOWREEL_WORK holds the per-shot intermediates a cut produces;
# nothing in it is worth keeping once the film is assembled.
export SHOWREEL_DIR="${SHOWREEL_DIR:-$HOME/Videos/reprise-showreel}"
export SHOWREEL_WORK="${SHOWREEL_WORK:-${XDG_CACHE_HOME:-$HOME/.cache}/reprise-showreel}"

mkdir -p -- "$SHOWREEL_WORK"

# Fail early and by name, so a missing take is not an ffmpeg error 40 lines on.
showreel_require() {
  local path
  for path in "$@"; do
    [[ -e $path ]] || {
      printf 'missing: %s\n' "$path" >&2
      return 1
    }
  done
}

showreel_duration() {
  ffprobe -v error -show_entries format=duration -of default=nw=1:nk=1 -- "$1"
}
