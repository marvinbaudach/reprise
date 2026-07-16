#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

echo "== Architecture lint =="

failed=0
while IFS= read -r file; do
  lines=$(wc -l < "$file")
  if (( lines >= 800 )); then
    echo "$file has $lines lines; Rust source files must stay below 800" >&2
    failed=1
  fi
done < <(find crates -name '*.rs' -type f | sort)

if (( failed != 0 )); then
  exit 1
fi

if cargo tree -p reprise-core | rg --quiet '(^| )(gtk4|libadwaita|gstreamer|zbus)( |$| v)'; then
  echo "reprise-core must not depend on GTK, libadwaita, GStreamer, or zbus" >&2
  exit 1
fi

if [[ -e crates/reprise-gnome/src/ui/compact/compact_player_state.rs ]]; then
  echo "orphan compact_player_state.rs must not be restored" >&2
  exit 1
fi

echo "== Frontend lint =="

# Keep known frontend debt explicit and prevent it from spreading. Refactoring
# tasks may remove allowlisted entries, but adding a new entry requires a
# deliberate policy change in the same reviewed diff.
check_frontend_allowlist() {
  local pattern=$1
  local description=$2
  shift 2

  local file allowed_file is_allowed
  while IFS= read -r file; do
    [[ -z "$file" ]] && continue
    is_allowed=0
    for allowed_file in "$@"; do
      if [[ "$file" == "$allowed_file" ]]; then
        is_allowed=1
        break
      fi
    done
    if (( is_allowed == 0 )); then
      echo "$description is not allowed in $file" >&2
      return 1
    fi
  done < <(rg -l "$pattern" crates/reprise-gnome/src --glob '*.rs' || true)
}

check_frontend_allowlist 'gtk4::CssProvider::new' 'direct CssProvider construction' \
  crates/reprise-gnome/src/ui/style/mod.rs \
  crates/reprise-gnome/src/ui/style/cover_accent.rs \
  crates/reprise-gnome/src/ui/library_views/artist_view_css.rs

check_frontend_allowlist 'style_context\(' 'deprecated per-widget style_context use'

check_frontend_allowlist '(^|[^[:alnum:]_])(gstreamer|gst)::|extern crate gstreamer' \
  'direct GStreamer coupling'

check_frontend_allowlist 'std::process::Command::new\("gst-launch-1\.0"\)' \
  'external gst-launch waveform extraction'

if rg --quiet '^gstreamer(-app)?[[:space:]]*=' crates/reprise-gnome/Cargo.toml; then
  echo "the GNOME frontend must not depend directly on GStreamer crates" >&2
  exit 1
fi

if rg --quiet 'reprise_platform_linux' \
  crates/reprise-gnome/src/ui/playback \
  crates/reprise-gnome/src/ui/scan; then
  echo "playback and scan feature modules must receive platform backends through core contracts" >&2
  exit 1
fi

check_frontend_allowlist 'unsafe[[:space:]]*\{' 'unsafe frontend block' \
  crates/reprise-gnome/src/ui/compact/compact_mode_controls.rs

if rg --quiet 'reqwest::blocking' crates/reprise-gnome/src --glob '*.rs'; then
  echo "blocking HTTP is forbidden in the GTK frontend" >&2
  exit 1
fi

for file in \
  crates/reprise-gnome/src/ui/strings_app_shell.rs \
  crates/reprise-gnome/src/ui/strings_artist.rs \
  crates/reprise-gnome/src/ui/strings_news.rs; do
  if ! rg --fixed-strings --quiet "$file" po/POTFILES.in; then
    echo "$file must be listed in po/POTFILES.in" >&2
    exit 1
  fi
done

git diff --check

echo "Architecture lint passed"
