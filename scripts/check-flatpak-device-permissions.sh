#!/usr/bin/env bash
set -euo pipefail

manifest=${1:-io.github.marvinbaudach.Reprise.yml}

require_once() {
  local permission=$1
  local count
  count=$(grep -Fxc "  - $permission" "$manifest" || true)
  if [[ $count -ne 1 ]]; then
    echo "Flatpak manifest must contain exactly one $permission permission" >&2
    exit 1
  fi
}

require_once '--talk-name=org.gtk.vfs.*'
require_once '--filesystem=xdg-run/gvfsd'
require_once '--filesystem=xdg-music'
require_once '--filesystem=~/.local/share/rhythmbox:ro'
require_once '--filesystem=~/.var/app/org.gnome.Rhythmbox3/data/rhythmbox:ro'

while IFS= read -r permission; do
  case "$permission" in
    '--talk-name=org.gtk.vfs.*'|'--filesystem=xdg-run/gvfsd'|'--filesystem=xdg-music'|'--filesystem=~/.local/share/rhythmbox:ro'|'--filesystem=~/.var/app/org.gnome.Rhythmbox3/data/rhythmbox:ro')
      ;;
    --filesystem=*|--talk-name=*|--socket=session-bus|--socket=system-bus|--device=all)
      echo "Flatpak manifest contains forbidden broad permission: $permission" >&2
      exit 1
      ;;
  esac
done < <(sed -n '/^finish-args:/,/^[^[:space:]]/p' "$manifest" | sed -n 's/^  - //p')
