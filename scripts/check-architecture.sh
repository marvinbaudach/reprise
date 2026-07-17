#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

echo "== Architecture lint =="

failed=0
while IFS= read -r file; do
  # UI string catalogs may exceed 800 lines: they are append-oriented
  # translation inventories, not behavioral modules that should be split by size.
  case "$file" in
    crates/reprise-gnome/src/ui/strings.rs | crates/reprise-gnome/src/ui/strings_*.rs)
      continue
      ;;
  esac

  lines=$(wc -l < "$file")
  if (( lines >= 800 )); then
    echo "$file has $lines lines; Rust source files must stay below 800" >&2
    failed=1
  fi
done < <(find crates -name '*.rs' -type f | sort)

if (( failed != 0 )); then
  exit 1
fi

window_lines=$(wc -l < crates/reprise-gnome/src/ui/window/window.rs)
if (( window_lines >= 600 )); then
  echo "window.rs has $window_lines lines; the composition root must stay below 600" >&2
  exit 1
fi

for orchestrator in \
  crates/reprise-gnome/src/ui/track_list/track_list.rs \
  crates/reprise-gnome/src/ui/sidebar/sidebar.rs; do
  lines=$(wc -l < "$orchestrator")
  if (( lines >= 600 )); then
    echo "$orchestrator has $lines lines; UI orchestrators must stay below 600" >&2
    exit 1
  fi
done

if cargo tree -p reprise-core | rg --quiet '(^| )(gtk4|libadwaita|gstreamer|zbus)( |$| v)'; then
  echo "reprise-core must not depend on GTK, libadwaita, GStreamer, or zbus" >&2
  exit 1
fi

if [[ -e crates/reprise-gnome/src/ui/compact/compact_player_state.rs ]]; then
  echo "orphan compact_player_state.rs must not be restored" >&2
  exit 1
fi

if rg --quiet '^#\[path = "[^"/]+/' crates/reprise-gnome/src/ui/mod.rs; then
  echo "ui/mod.rs must declare feature modules instead of flattening feature directories" >&2
  exit 1
fi

for feature in \
  browse compact cover device_sync info_panel library_views lyrics playback \
  player_bar playlists preferences scan scrobbling sidebar stats tag_edit \
  track_list window; do
  if [[ ! -f "crates/reprise-gnome/src/ui/$feature/mod.rs" ]]; then
    echo "frontend feature $feature must own an explicit mod.rs surface" >&2
    exit 1
  fi
done

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

# Productive frontend features consume database operations through named core
# facades.  Keep SQL ownership and schema migration at the engine boundary;
# test fixtures may still use SQL to arrange and inspect their own data.
for frontend_sql in \
  'SELECT title, artist, album, year FROM tracks WHERE id' \
  'SELECT id FROM tracks WHERE missing = 0' \
  'SELECT path FROM tracks WHERE missing = 0 ORDER BY path' \
  'SELECT id, path FROM tracks WHERE waveform_peaks IS NULL' \
  'SELECT title, id FROM tracks WHERE title IN' \
  'SELECT id FROM tracks ORDER BY title DESC'; do
  if rg --fixed-strings --quiet "$frontend_sql" crates/reprise-gnome/src --glob '*.rs'; then
    echo "productive GNOME code must use core database facades: $frontend_sql" >&2
    exit 1
  fi
done

if rg --quiet 'db::migrate\(&conn\)\.ok' \
  crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs \
  || rg --quiet 'db::migrate\(&worker_conn\)' \
    crates/reprise-gnome/src/ui/scan/scan_worker.rs \
  || rg --quiet 'db::open\(Some\(database_path\)\)' \
    crates/reprise-gnome/src/ui/scrobbling/scrobble_runtime.rs; then
  echo "frontend workers must open ready-to-use databases through the core facade" >&2
  exit 1
fi

check_frontend_allowlist 'unsafe[[:space:]]*\{' 'unsafe frontend block' \
  crates/reprise-gnome/src/ui/compact/compact_mode_controls.rs

if rg --quiet 'reqwest::blocking' crates/reprise-gnome/src --glob '*.rs'; then
  echo "blocking HTTP is forbidden in the GTK frontend" >&2
  exit 1
fi

for one_shot_consumer in \
  crates/reprise-gnome/src/ui/delete_tracks.rs \
  crates/reprise-gnome/src/ui/playback/now_playing_wiring.rs \
  crates/reprise-gnome/src/ui/preferences/preference_dependencies.rs \
  crates/reprise-gnome/src/ui/preferences/preference_lastfm.rs \
  crates/reprise-gnome/src/ui/preferences/preference_listenbrainz.rs \
  crates/reprise-gnome/src/ui/tag_edit/tag_edit_flow.rs \
  crates/reprise-gnome/src/ui/tag_edit/tag_editor_lookup.rs; do
  if rg --quiet 'std::thread::Builder::new|async_channel::bounded' "$one_shot_consumer"; then
    echo "$one_shot_consumer must use the shared one-shot task helper" >&2
    exit 1
  fi
done

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
