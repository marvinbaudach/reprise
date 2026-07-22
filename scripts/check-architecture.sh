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

echo "== Multi-frontend core boundaries =="

# The headless surfaces (reprise-cli, reprise-mcp) and the removable stem-
# separation backend (reprise-stems) build on the MIT engine only. These gates
# make the multi-frontend-core dependency contract (docs/plans/multi-frontend-
# core.md §2.5) mechanical:
#   * among workspace crates, the DEFAULT builds of reprise-cli and reprise-mcp
#     pull in reprise-core and nothing else. The CLI's `mpris` (zbus) and
#     `worker` (reprise-stems) exceptions are feature-gated, so the default
#     tree must stay core-only — exactly what the plan pins as the enforced
#     probe;
#   * reprise-stems depends only on reprise-core, and only binary hosts (the
#     GTK app; the CLI behind `worker`) may depend on it — the MCP server and
#     the engine never may, so the feature stays removable;
#   * no GTK/libadwaita/GLib/GStreamer/zbus family crate links into the default
#     dependency tree of any of the three.
#
# `-e normal` scopes every probe to what actually links into the shipped
# binary (dev- and build-dependencies are irrelevant to the boundary), and
# `--prefix none` prints one `name vX.Y.Z (path)` line per crate so a workspace
# edge is a simple line-anchored match. `--target all` widens every probe from
# the host graph to every target's graph, so a Windows/Android-conditional edge
# (e.g. a `cfg(windows)` dependency) cannot smuggle a banned family or a stray
# workspace crate past a Linux-only run. It may list a crate once per target, so
# the stray-edge probes pipe through `sort -u`.
#
# Every probe runs `cargo tree` exactly once through `run_dependency_probe`,
# which captures the output and aborts the gate on a non-zero exit. Without
# that guard a failing `cargo tree` (unresolved package, broken manifest)
# prints nothing, and an empty result is indistinguishable from "no violation"
# — the gate would fail OPEN. Here a cargo-tree failure fails the gate CLOSED
# and loud instead.
banned_dependency_families='(^| )(gtk4|libadwaita|glib|gstreamer|zbus)( |$| v)'

# Run one `cargo tree` invocation with fail-closed handling. `return 1` (never
# `exit`, which would only leave the command-substitution subshell) lets each
# caller abort the whole script via `|| exit 1`. On success the captured
# stdout is echoed for the follow-up grep.
run_dependency_probe() {
  local label=$1
  shift
  local out
  if ! out=$(cargo tree "$@" 2>&1); then
    echo "cargo tree failed for $label; dependency boundaries cannot be verified:" >&2
    echo "$out" >&2
    return 1
  fi
  printf '%s\n' "$out"
}

for surface in reprise-cli reprise-mcp; do
  surface_tree=$(run_dependency_probe "$surface default build" \
    -p "$surface" -e normal --prefix none --target all) || exit 1
  stray_workspace_edge=$(printf '%s\n' "$surface_tree" \
    | rg '^reprise-[a-z-]+ ' \
    | rg -v "^(reprise-core|$surface) " \
    | sort -u || true)
  if [[ -n "$stray_workspace_edge" ]]; then
    echo "$surface default build may depend on reprise-core only; found:" >&2
    echo "$stray_workspace_edge" >&2
    exit 1
  fi
done

for surface in reprise-cli reprise-mcp reprise-stems; do
  surface_tree=$(run_dependency_probe "$surface default build" \
    -p "$surface" -e normal --target all) || exit 1
  if printf '%s\n' "$surface_tree" | rg --quiet "$banned_dependency_families"; then
    echo "$surface default build must not depend on GTK, libadwaita, GLib, GStreamer, or zbus" >&2
    exit 1
  fi
done

# reprise-stems stays a removable, binary-host-only backend: neither the engine
# nor the MCP server may pull it in under ANY feature set.
for stems_non_host in reprise-core reprise-mcp; do
  stems_host_tree=$(run_dependency_probe "$stems_non_host all features" \
    -p "$stems_non_host" --all-features -e normal --prefix none --target all) || exit 1
  if printf '%s\n' "$stems_host_tree" | rg --quiet '^reprise-stems '; then
    echo "$stems_non_host must never depend on reprise-stems (binary-host-only, removable backend)" >&2
    exit 1
  fi
done

# reprise-stems itself links only the engine.
stems_tree=$(run_dependency_probe "reprise-stems all features" \
  -p reprise-stems --all-features -e normal --prefix none --target all) || exit 1
stray_stems_edge=$(printf '%s\n' "$stems_tree" \
  | rg '^reprise-[a-z-]+ ' \
  | rg -v '^(reprise-core|reprise-stems) ' \
  | sort -u || true)
if [[ -n "$stray_stems_edge" ]]; then
  echo "reprise-stems may depend on reprise-core only; found:" >&2
  echo "$stray_stems_edge" >&2
  exit 1
fi

# The GTK app-hosted instrumental worker consumes the core `stem_separation`
# trait, never reprise-stems directly. The DEFAULT build must therefore not link
# reprise-stems. P3b wired the real backend behind the gnome `stem-backend` cargo
# feature (mirroring the CLI's `worker` feature): the GTK app is a sanctioned
# binary host for reprise-stems (see the §"binary hosts" note above), but the
# edge is feature-gated so this default-build probe stays green — a feature-gated
# edge does not appear in the default `-e normal` tree. Build with
# `--features stem-backend` for the real render.
gnome_tree=$(run_dependency_probe "reprise-gnome default build" \
  -p reprise-gnome -e normal --prefix none --target all) || exit 1
if printf '%s\n' "$gnome_tree" | rg --quiet '^reprise-stems '; then
  echo "reprise-gnome default build must not depend on reprise-stems (the worker consumes the core stem_separation trait)" >&2
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
  crates/reprise-gnome/src/ui/style/reduced_motion.rs \
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

# The headless surfaces route every database operation through named core
# facades too. They hold a rusqlite Connection only to open the migrated
# database and to read busy/lock error codes — never to assemble SQL. This is
# the "no SQL outside core" gate extended to reprise-cli/reprise-mcp (plan
# §2.5). Uppercase statement keywords match real queries, not prose; test
# fixtures (under tests/) may still use SQL to arrange and inspect their data.
# `rg -U` (multiline) plus `\s+`/`[\s\S]` gaps catch keywords split across a
# line break — e.g. `UPDATE` on one line and `foo SET …` on the next — which a
# line-anchored pattern would miss.
for headless_src in crates/reprise-cli/src crates/reprise-mcp/src; do
  if rg --quiet -U '\b(SELECT|INSERT\s+INTO|UPDATE\b[\s\S]{0,200}?\bSET\b|DELETE\s+FROM|CREATE\s+TABLE|CREATE\s+INDEX|DROP\s+TABLE|ALTER\s+TABLE)\b' \
    "$headless_src" --glob '*.rs'; then
    echo "productive SQL is not allowed outside reprise-core: $headless_src" >&2
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
  crates/reprise-gnome/src/ui/tag_edit/tag_edit_flow.rs; do
  if rg --quiet 'std::thread::Builder::new|async_channel::bounded' "$one_shot_consumer"; then
    echo "$one_shot_consumer must use the shared one-shot task helper" >&2
    exit 1
  fi
done

for file in \
  crates/reprise-gnome/src/ui/strings_app_shell.rs \
  crates/reprise-gnome/src/ui/strings_artist.rs \
  crates/reprise-gnome/src/ui/strings_issues.rs \
  crates/reprise-gnome/src/ui/strings_news.rs; do
  if ! rg --fixed-strings --quiet "$file" po/POTFILES.in; then
    echo "$file must be listed in po/POTFILES.in" >&2
    exit 1
  fi
done

scripts/check-accessibility-semantics.sh
scripts/check-input-parity.sh

git diff --check

echo "Architecture lint passed"
