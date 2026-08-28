#!/usr/bin/env bash
# The cinematic layer for the short cut.
#
# Three differences against film.sh, all driven by the film being half as long
# and playing muted on a landing page:
#
#  * the band grows 92 -> 120 rows and the type 34 -> 48 px, because with the
#    sound off the captions are the only narration there is;
#  * the push can aim. film.sh always pushed at the centre of the frame, which
#    is wrong whenever the thing being described sits in a corner — a push onto
#    the search entry reads as "look here", a push onto the middle of a list
#    reads as drift;
#  * a statement class exists: large, centred, on a scrim. It is allowed only
#    on shots carrying no competing text, because the 60 s film already proved
#    what happens otherwise — "Library Doctor" landed on the app's own Library
#    Doctor entry.
#
# Colours stay 0xRRGGBB: ffmpeg's parser reads an unquoted # as a comment and
# silently drops the rest of the filter chain.

# Two faces, and the split is deliberate. Callouts describe the interface, so
# they speak in the interface's own font. Statements speak for the project, so
# they use the project's own — the same Fraunces the cards and the showroom use.
FILM2_FONT=${FILM2_FONT:-/usr/share/fonts/Adwaita/AdwaitaSans-Regular.ttf}
FILM2_BRAND_FONT=${FILM2_BRAND_FONT:-data/brand/fonts/Fraunces-SemiBold.ttf}
export FILM2_GROUND=0x0D1014
export FILM2_TEAL=0x4FDBD4
export FILM2_INK=0xF2F6F6
export FILM2_MUTED=0x8C9B9E
export FILM2_STAGE_H=960
export FILM2_BAND_TOP=960
export FILM2_FADE=0.28

# A push that can aim. fx/fy are normalised: 0.5,0.5 is the centre and matches
# film.sh's behaviour exactly, 0.46,0.10 is the search entry in the header bar.
# The supersample in front of zoompan is what keeps it smooth — zoompan's crop
# rectangle is integral, so at 1x the window steps by whole output pixels and
# the motion visibly ticks.
# The optional ease matters only where the move is large. A linear ramp across
# a 3x dive reads as a machine pulling the frame; accelerating into it reads as
# a dive. Written without pow() on purpose — a comma inside a filter option ends
# the option, and escaping it through two layers of shell is not worth it.
# The camera. Three states, and the film is mostly in the first one.
#
# `hold` frames the shot once and does not move again: a constant scale about
# (fx, fy), which is how a shot gets close enough to read without the picture
# drifting for its whole length. It exists because the alternative this film
# used to take — a two-to-four percent push on every single shot, alternating
# direction — never sat still long enough for anything to be read, and reads as
# nervousness rather than as motion. The app moving inside a locked frame is the
# motion; typing, scrolling and playback supply it for free.
#
# `in` and `out` remain for the handful of shots where the app itself is static
# and for the bridge, which is the one designed camera move in the film. They
# only work because everything around them holds.
film2_push() { # frames direction zoom target_w target_h fx fy [lin|accel|decel]
  local n=$1 dir=$2 amt=$3 w=$4 h=$5 fx=${6:-0.5} fy=${7:-0.5} ease=${8:-lin} z p
  local last=$((n - 1))
  ((last > 0)) || last=1
  case $ease in
    accel) p="(on/$last)*(on/$last)" ;;
    decel) p="(1-(1-on/$last)*(1-on/$last))" ;;
    *) p="on/$last" ;;
  esac
  if [[ $dir == hold ]]; then
    z="1+$amt"
  elif [[ $dir == out ]]; then
    z="1+$amt-$amt*$p"
  else
    z="1+$amt*$p"
  fi
  printf 'scale=%d:%d:flags=lanczos,zoompan=z=%s:x=(iw-iw/zoom)*%s:y=(ih-ih/zoom)*%s:d=1:s=%dx%d:fps=30' \
    "$((w * 2))" "$((h * 2))" "$z" "$fx" "$fy" "$w" "$h"
}

# ffmpeg's drawtext eats these; a caption carrying an apostrophe or a colon
# otherwise truncates the whole chain without an error.
film2_escape() {
  printf '%s' "$1" | sed -e "s/\\\\/\\\\\\\\\\\\\\\\/g" -e "s/:/\\\\\\\\:/g" -e "s/'/\\\\\\\\'/g" -e "s/%/\\\\\\\\%/g"
}

# The band and its comment. An empty line yields the band alone, which is how a
# shot is given a beat with no claim attached to it.
film2_callout() { # line sub duration
  local line=$1 sub=$2 dur=$3 alpha rail
  rail="drawbox=x=0:y=$FILM2_BAND_TOP:w=iw:h=2:color=$FILM2_INK@0.07:t=fill"
  [[ -n $line ]] || {
    printf '%s' "$rail"
    return
  }
  alpha="if(lt(t,$FILM2_FADE),t/$FILM2_FADE,if(lt(t,$dur-$FILM2_FADE),1,max(0,($dur-t)/$FILM2_FADE)))"
  # The dash wipes in rather than fading: it is the one moving thing on a band
  # that is otherwise static, and it reads as the caption arriving.
  printf '%s,drawbox=x=112:y=1028:w=min(28\,28*t/0.22):h=3:color=%s:t=fill:enable=1' "$rail" "$FILM2_TEAL"
  if [[ -n $sub ]]; then
    printf ',drawtext=fontfile=%s:text=%s:fontsize=44:fontcolor=%s:x=158:y=996:alpha=%s' \
      "'$FILM2_FONT'" "'$(film2_escape "$line")'" "$FILM2_INK" "'$alpha'"
    printf ',drawtext=fontfile=%s:text=%s:fontsize=26:fontcolor=%s@0.62:x=158:y=1046:alpha=%s' \
      "'$FILM2_FONT'" "'$(film2_escape "$sub")'" "$FILM2_INK" "'$alpha'"
  else
    printf ',drawtext=fontfile=%s:text=%s:fontsize=48:fontcolor=%s:x=158:y=1012:alpha=%s' \
      "'$FILM2_FONT'" "'$(film2_escape "$line")'" "$FILM2_INK" "'$alpha'"
  fi
}

# One word, hard in, no fade — the 1.2 s shots have no time for a ramp.
film2_burst() { # word duration
  local word=$1
  printf 'drawbox=x=0:y=%s:w=iw:h=2:color=%s@0.07:t=fill' "$FILM2_BAND_TOP" "$FILM2_INK"
  printf ',drawbox=x=112:y=1028:w=28:h=3:color=%s:t=fill' "$FILM2_TEAL"
  printf ',drawtext=fontfile=%s:text=%s:fontsize=48:fontcolor=%s:x=158:y=1012' \
    "'$FILM2_FONT'" "'$(film2_escape "$word")'" "$FILM2_INK"
}

# Centred over the stage on a scrim. Only where nothing else carries text.
# The optional sub-line arrives a beat after the headline and carries the claim
# the headline cannot: which toolkit each platform is actually built in. It is
# set in the UI font, not the brand one — it is a credit, not a slogan.
film2_statement() { # text at duration [sub]
  local text=$1 at=$2 dur=$3 sub=${4:-} a end sat sa head=432
  end=$(python3 -c "print(round($dur-0.15,3))")
  a="if(lt(t,$at),0,if(lt(t,$at+0.45),(t-$at)/0.45,if(lt(t,$end),1,max(0,($dur-t)/0.15))))"
  [[ -n $sub ]] && head=396
  printf 'drawbox=x=0:y=0:w=iw:h=%s:color=%s@0.55:t=fill:enable=%s' \
    "$FILM2_STAGE_H" "$FILM2_GROUND" "'between(t,$at,$dur)'"
  printf ',drawtext=fontfile=%s:text=%s:fontsize=64:fontcolor=%s:x=(w-text_w)/2:y=%s:alpha=%s' \
    "'$FILM2_BRAND_FONT'" "'$(film2_escape "$text")'" "$FILM2_INK" "$head" "'$a'"
  [[ -n $sub ]] || return 0
  sat=$(python3 -c "print(round($at+0.6,3))")
  sa="if(lt(t,$sat),0,if(lt(t,$sat+0.4),(t-$sat)/0.4,if(lt(t,$end),1,max(0,($dur-t)/0.15))))"
  printf ',drawbox=x=(iw-96)/2:y=500:w=96:h=1:color=%s@0.75:t=fill:enable=%s' \
    "$FILM2_TEAL" "'between(t,$sat,$dur)'"
  printf ',drawtext=fontfile=%s:text=%s:fontsize=29:fontcolor=%s:x=(w-text_w)/2:y=530:alpha=%s' \
    "'$FILM2_FONT'" "'$(film2_escape "$sub")'" "$FILM2_MUTED" "'$sa'"
}

# The wordmark, bottom right, for the whole film. Costs no runtime and brands
# every social crop that gets cut out of this later.
film2_bug() {
  printf 'drawtext=fontfile=%s:text=%s:fontsize=25:fontcolor=%s@0.5:x=w-text_w-64:y=1016' \
    "'$FILM2_FONT'" "'Reprise'" "$FILM2_INK"
}

film2_dip() { # in|out duration
  case $1 in
    in) printf ',fade=t=in:st=0:d=0.35:color=black' ;;
    out) printf ',fade=t=out:st=%s:d=0.35:color=black' "$(python3 -c "print(round($2-0.35,3))")" ;;
    *) ;;
  esac
}

# The request, over the app it is about to change. The window is blurred rather
# than hidden: the viewer has to know the library is sitting there untouched,
# because the whole claim is that it changes without anyone touching it.
film2_prompt() { # line sub duration
  local line=$1 sub=$2 dur=$3 a end
  end=$(python3 -c "print(round($dur-0.12,3))")
  a="if(lt(t,0.15),(t)/0.15,if(lt(t,$end),1,max(0,($dur-t)/0.12)))"
  printf 'boxblur=22:2,drawbox=x=0:y=0:w=iw:h=%s:color=%s@0.62:t=fill' \
    "$FILM2_STAGE_H" "$FILM2_GROUND"
  printf ',drawtext=fontfile=%s:text=%s:fontsize=46:fontcolor=%s:x=(w-text_w)/2:y=406:alpha=%s' \
    "'$FILM2_BRAND_FONT'" "'$(film2_escape "$line")'" "$FILM2_INK" "'$a'"
  printf ',drawtext=fontfile=%s:text=%s:fontsize=27:fontcolor=%s:x=(w-text_w)/2:y=492:alpha=%s' \
    "'$FILM2_FONT'" "'$(film2_escape "$sub")'" "$FILM2_MUTED" "'$a'"
}
