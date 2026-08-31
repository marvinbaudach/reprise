#!/usr/bin/env bash
# The cinematic layer: the push-in, and the rail the captions live on.
#
# Captions do not sit on top of the app. The frame is split into a stage of
# 988 rows carrying the app and a 92-row band underneath carrying the comment,
# so a caption can never collide with the interface it is describing — the
# first attempt put "Library Doctor" straight on top of the app's own Library
# Doctor entry. A hairline separates the two.
#
# Colours are given as 0xRRGGBB, never #RRGGBB: ffmpeg's filtergraph parser
# treats an unquoted # as the start of a comment and silently drops the rest of
# the chain.

FILM_FONT=${FILM_FONT:-/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf}
export FILM_GROUND=0x0D1014
export FILM_TEAL=0x4FDBD4
export FILM_INK=0xF2F6F6
export FILM_STAGE_H=988
export FILM_BAND_TOP=988
export FILM_ZOOM=0.02   # 2 % over a shot: alive, without turning the text to mush
export FILM_FADE=0.3

# A push over the whole shot, alternating direction so a run of shots does not
# read as one mechanical drift. The supersample in front of zoompan is what
# keeps it smooth: zoompan's crop rectangle is integral, so at 1x the window
# steps by whole output pixels and the motion visibly ticks.
film_push() { # frames direction target_w target_h
  local n=$1 dir=$2 w=$3 h=$4 z
  local last=$((n - 1))
  ((last > 0)) || last=1
  if [[ $dir == out ]]; then
    z="1+$FILM_ZOOM-$FILM_ZOOM*on/$last"
  else
    z="1+$FILM_ZOOM*on/$last"
  fi
  printf 'scale=%d:%d:flags=lanczos,zoompan=z=%s:x=(iw-iw/zoom)/2:y=(ih-ih/zoom)/2:d=1:s=%dx%d:fps=30' \
    "$((w * 2))" "$((h * 2))" "$z" "$w" "$h"
}

# The band, its hairline, and the comment on it. An empty text yields the band
# alone, which is how a shot is given a beat without a claim attached to it.
film_rail() { # text duration
  local text=$1 dur=$2 alpha rail
  rail="drawbox=x=0:y=$FILM_BAND_TOP:w=iw:h=2:color=$FILM_INK@0.07:t=fill"
  [[ -n $text ]] || {
    printf '%s' "$rail"
    return
  }
  alpha="if(lt(t,$FILM_FADE),t/$FILM_FADE,if(lt(t,$dur-$FILM_FADE),1,max(0,($dur-t)/$FILM_FADE)))"
  printf '%s,drawtext=fontfile=%s:text=%s:fontsize=34:fontcolor=%s:x=112:y=h-62:alpha=%s' \
    "$rail" "'$FILM_FONT'" "'—'" "$FILM_TEAL" "'$alpha'"
  printf ',drawtext=fontfile=%s:text=%s:fontsize=34:fontcolor=%s:x=158:y=h-62:alpha=%s' \
    "'$FILM_FONT'" "'$text'" "$FILM_INK" "'$alpha'"
}

# A dip to black is a fade out against a fade in — it needs no xfade, which is
# the point: xfade will not work across a concat-demuxer output. Fed the
# assembled desktop half it silently returns that half alone (39.6 s in, 39.6 s
# out), and normalising the timebase first only changes the wrong answer to
# 34.4 s. Baked into the segments instead, the join stays a stream copy.
film_dip() { # in|out duration
  case $1 in
    in) printf ',fade=t=in:st=0:d=0.3:color=black' ;;
    out) printf ',fade=t=out:st=%s:d=0.3:color=black' "$(python3 -c "print(round($2-0.3,3))")" ;;
    *) ;;
  esac
}
