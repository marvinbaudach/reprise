#!/usr/bin/env bash

prepare_navigation_playback_fixture() {
  local fixture_dir=$1 base_track=$2

  mkdir -p "$fixture_dir"
  ffmpeg -hide_banner -loglevel error -y \
    -f lavfi -i sine=frequency=440:duration=30 \
    -c:a flac "$base_track"
  for index in $(seq -w 1 16); do
    ffmpeg -hide_banner -loglevel error -y \
      -i "$base_track" -map 0:a -c:a copy \
      -metadata title="Navigation Track $index" \
      -metadata artist="Navigation Artist" \
      -metadata album_artist="Navigation Artist" \
      -metadata album="Navigation Album" \
      -metadata track="$index" \
      "$fixture_dir/navigation_$index.flac"
  done
  ffmpeg -hide_banner -loglevel error -y \
    -i "$base_track" -map 0:a -c:a copy \
    -metadata title="Sentinel Track" \
    -metadata artist="Sentinel Artist" \
    -metadata album_artist="Sentinel Artist" \
    -metadata album="Sentinel Album" \
    -metadata track="1" \
    "$fixture_dir/sentinel.flac"
}

assert_no_library_source_since() {
  local log_path=$1 first_line=$2 failure_message=$3

  if tail -n "+$((first_line + 1))" "$log_path" \
    | rg --quiet 'source=library'; then
    echo "$failure_message" >&2
    return 1
  fi
}

assert_app_log_contains_since() {
  local log_path=$1 first_line=$2 marker=$3 scenario=$4

  if ! tail -n "+$((first_line + 1))" "$log_path" \
    | rg --quiet --fixed-strings "$marker"; then
    echo "$scenario log tail is missing diagnostic marker '$marker'" >&2
    return 1
  fi
}
