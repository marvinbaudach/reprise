#!/usr/bin/env bash
set -euo pipefail

# ARCH_LINT_SIZE_ROOT points the size rules at a fixture tree so
# scripts/tests/architecture-size-limits.sh can prove they all report before
# the script exits. It covers the size section only; everything below it needs
# a real cargo workspace, and a fixture run stops at the exit above it.
repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "${ARCH_LINT_SIZE_ROOT:-$repo_root}"

echo "== Architecture lint =="

# Every size rule reports before any of them exits. A single oversized file
# used to abort the script here, which silently skipped the tighter 600-line
# limits below — so a composition root could drift over its budget and nobody
# saw it until the unrelated 800-line offender was fixed. Collect, then exit.
failed=0
while IFS= read -r file; do
  lines=$(wc -l < "$file")
  if (( lines >= 800 )); then
    echo "$file has $lines lines; Rust source files must stay below 800" >&2
    failed=1
  fi
done < <(find crates -name '*.rs' -type f | sort)

window_lines=$(wc -l < crates/reprise-gnome/src/ui/window/window.rs)
if (( window_lines >= 600 )); then
  echo "window.rs has $window_lines lines; the composition root must stay below 600" >&2
  failed=1
fi

for orchestrator in \
  crates/reprise-gnome/src/ui/track_list/track_list.rs \
  crates/reprise-gnome/src/ui/sidebar/sidebar.rs; do
  lines=$(wc -l < "$orchestrator")
  if (( lines >= 600 )); then
    echo "$orchestrator has $lines lines; UI orchestrators must stay below 600" >&2
    failed=1
  fi
done

if (( failed != 0 )); then
  exit 1
fi

# Everything below needs a real cargo workspace. A fixture run stops here.
if [[ -n ${ARCH_LINT_SIZE_ROOT:-} ]]; then
  echo "size rules passed (fixture run)"
  exit 0
fi

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
#   * reprise-stems depends only on reprise-core. GTK may enable its
#     provisioning-only slice, while only the CLI worker enables native
#     inference; the MCP server and engine never depend on the crate;
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

# The shared presentation layer is consumed by GTK, by a Compose app on
# Android and by a Tauri app on the desktop (multi-surface spec §1). A
# toolkit or bus edge here would silently re-couple every surface to the
# GNOME process — the exact failure this crate exists to prevent. It may
# depend on the engine and on nothing else in the workspace.
view_tree=$(run_dependency_probe "reprise-view all features" \
  -p reprise-view --all-features -e normal --prefix none --target all) || exit 1
if printf '%s\n' "$view_tree" | rg --quiet "$banned_dependency_families"; then
  echo "reprise-view must not depend on GTK, libadwaita, GLib, GStreamer, or zbus" >&2
  printf '%s\n' "$view_tree" | rg "$banned_dependency_families" >&2
  exit 1
fi
stray_view_edge=$(printf '%s\n' "$view_tree" \
  | rg '^reprise-[a-z-]+ ' \
  | rg -v '^(reprise-core|reprise-view) ' \
  | sort -u || true)
if [[ -n "$stray_view_edge" ]]; then
  echo "reprise-view may depend on reprise-core only; found:" >&2
  echo "$stray_view_edge" >&2
  exit 1
fi

# The Android UniFFI bridge is an adapter over the engine, not a second
# application composition root. Keeping every other workspace crate out of
# its normal dependency graph prevents Android from silently inheriting GTK,
# Linux platform services, or another frontend's presentation layer.
#
# `reprise-view` is the exception, and deliberately so: it is not another
# frontend's presentation layer but the toolkit-neutral one every frontend
# shares, and the rule immediately above holds it to `reprise-core` alone —
# so allowing it here adds no third-party dependency and no byte of GTK.
# Forbidding it would mean Android re-implementing the shaping and the colour
# axis in Kotlin, which is exactly the drift the crate exists to prevent.
android_ffi_tree=$(run_dependency_probe "reprise-android-ffi all features" \
  -p reprise-android-ffi --all-features -e normal --prefix none --target all) || exit 1
if printf '%s\n' "$android_ffi_tree" | rg --quiet "$banned_dependency_families"; then
  echo "reprise-android-ffi must not depend on GTK, libadwaita, GLib, GStreamer, or zbus" >&2
  printf '%s\n' "$android_ffi_tree" | rg "$banned_dependency_families" >&2
  exit 1
fi
stray_android_ffi_edge=$(printf '%s\n' "$android_ffi_tree" \
  | rg '^reprise-[a-z-]+ ' \
  | rg -v '^(reprise-android-ffi|reprise-core|reprise-view) ' \
  | sort -u || true)
if [[ -n "$stray_android_ffi_edge" ]]; then
  echo "reprise-android-ffi may depend on reprise-core and reprise-view only; found:" >&2
  echo "$stray_android_ffi_edge" >&2
  exit 1
fi

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

# The default GTK build remains core-only. The stem-backend feature may link
# reprise-stems' provisioning slice, but never the `ort` inference runtime:
# rendering belongs exclusively in the separately packaged worker process.
gnome_tree=$(run_dependency_probe "reprise-gnome default build" \
  -p reprise-gnome -e normal --prefix none --target all) || exit 1
if printf '%s\n' "$gnome_tree" | rg --quiet '^reprise-stems '; then
  echo "reprise-gnome default build must not depend on reprise-stems" >&2
  exit 1
fi
gnome_feature_tree=$(run_dependency_probe "reprise-gnome all features" \
  -p reprise-gnome --all-features -e normal --prefix none --target all) || exit 1
if printf '%s\n' "$gnome_feature_tree" | rg --quiet '^ort '; then
  echo "reprise-gnome must not depend on ort (rendering belongs in reprise-worker)" >&2
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

echo "== Engine HTTP boundaries =="

# One shared HTTP boundary is the plan (docs/plans/architecture-consolidation.md
# §4.4, docs/plans/consolidation-plan.md package 2.1). Until it exists, this
# budget stops the problem from growing while the waves run: every
# `ureq::Agent::config_builder()` in the engine is a separate agent, and
# therefore a separate timeout, user agent, rate limiter and error fold.
#
# The number is a CEILING and a FLOOR, exactly like the frontend-thinness
# budgets. Adding a boundary fails here; removing one fails here too until the
# budget comes down in the same commit. A budget nobody lowers is a budget
# nobody believes.
#
# This is not theoretical: the count went from 13 to 16 in two commits when the
# lyrics path grew its own lrclib and netease agents, and nothing said a word.
http_boundary_budget=16
http_boundaries=$(rg --count-matches 'ureq::Agent::config_builder' \
  crates/reprise-core/src --glob '*.rs' 2>/dev/null \
  | awk -F: '{ total += $2 } END { print total + 0 }')
if (( http_boundaries > http_boundary_budget )); then
  echo "engine HTTP boundaries grew from $http_boundary_budget to $http_boundaries" >&2
  echo "  route the new fetch through the shared boundary instead of building a second agent" >&2
  echo "  (docs/plans/consolidation-plan.md, package 2.1)" >&2
  exit 1
elif (( http_boundaries < http_boundary_budget )); then
  echo "engine HTTP boundaries are down to $http_boundaries (budget still says $http_boundary_budget)" >&2
  echo "  lower http_boundary_budget in scripts/check-architecture.sh to $http_boundaries" >&2
  exit 1
else
  echo "  ureq agents in reprise-core: $http_boundaries (at budget)"
fi

echo "== Documentation references from code =="

# Source and scripts cite design documents by path — reprise-stems points at
# the stem-separation report and queries/maintenance.rs at ADR 002. A doc
# deletion that leaves those
# pointing at nothing is silent rot: nothing compiles differently, and the next
# reader follows a path that is not there.
#
# Deliberately narrow. Markdown-to-markdown links are NOT checked, because two
# legitimate cases would need carve-outs and a gate whose exception list is as
# interesting as its rule does not survive contact: the append-only ledger
# records plans that have since been deleted, and a plan may forward-declare
# the file it is going to create. Code has neither excuse.
missing_doc_reference=0
while IFS= read -r reference; do
  source_file=${reference%%:*}
  doc_path=${reference#*:}
  if [[ ! -f $doc_path ]]; then
    echo "$source_file cites $doc_path, which does not exist" >&2
    missing_doc_reference=1
  fi
done < <(
  rg --only-matching --no-line-number --with-filename \
    'docs/[A-Za-z0-9._/-]+\.md' crates scripts 2>/dev/null | sort -u
)
if (( missing_doc_reference != 0 )); then
  echo "  update the citation or restore the document" >&2
  exit 1
fi
echo "  every documentation path cited from code and scripts resolves"

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
scripts/check-android-theme.sh

git diff --check

echo "Architecture lint passed"
