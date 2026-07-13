#!/usr/bin/env bash
set -euo pipefail

manifest=${1:-org.reprise.Reprise.yml}

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

while IFS= read -r permission; do
  case "$permission" in
    '--talk-name=org.gtk.vfs.*'|'--filesystem=xdg-run/gvfsd')
      ;;
    --filesystem=*|--talk-name=*|--socket=session-bus|--socket=system-bus|--device=all)
      echo "Flatpak manifest contains forbidden broad permission: $permission" >&2
      exit 1
      ;;
  esac
done < <(sed -n '/^finish-args:/,/^[^[:space:]]/p' "$manifest" | sed -n 's/^  - //p')
