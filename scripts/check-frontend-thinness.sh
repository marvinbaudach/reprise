#!/usr/bin/env bash
#
# Measures how much application logic still lives in the GTK frontend and
# refuses to let it grow.
#
# The thin-core plan turns `reprise-gnome` into an adapter: snapshot to
# widget, signal to command. That is a long migration, so instead of one
# unenforceable rule this script pins the current numbers and treats every
# one of them as a ceiling. A commit that adds a direct database call to the
# frontend fails here; a commit that extracts one fails too, until it lowers
# the budget in the same change. A budget nobody lowers is a budget nobody
# believes.
#
# Two categories are not budgets but bans: `reprise-gnome` reaches zero
# GStreamer and zero zbus today, and it stays there.
#
# Comments are stripped before counting. Without that, `ui/mod.rs`'s own
# module documentation — which explains that the frontend must not touch
# GStreamer or zbus — would trip the check that enforces it.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

frontend=crates/reprise-gnome/src

echo "== Frontend thinness =="

# Every budget below is a ceiling AND a floor: it must equal the measured
# count. Lower it in the same commit that removes a use. Never raise one
# without a reason recorded in the commit message.
# NET-3c (podcast-channel-redesign, F2): +2 rusqlite, irreducible thin
# wiring shaped exactly like an existing budgeted sibling. podcasts_worker.rs
# gained `run_queued`'s `conn: &rusqlite::Connection` (+2 matches on the one
# line, same as `download_episode` right above it already contributes) to
# reach `reprise_core::podcasts::queued_downloads::run_queued_downloads` —
# the selection/replay logic itself already lives in reprise-core; this is
# only the connection handle the worker needs to call in.
# NET-3 point 4 (podcast-channel-redesign, F4): +1 more rusqlite, same
# shape again. add_dialog.rs's `subscribe_offline` gained a
# `conn: &Rc<RefCell<Connection>>` (matching the existing `subscribe`
# wrapper right below it) to reach
# `reprise_core::podcasts::offline_add::offline_subscribe` — the
# already-subscribed check and the one DB write both live in reprise-core.
declare -A budget=(
  [rusqlite]=538
  [filesystem]=17
  [threads]=14
  [workers]=7
)

# Prints the frontend's *production* lines as `path:line:code`.
#
# Two exclusions, both deliberate:
#
#   - Comment-only lines. Without this, `ui/mod.rs`'s module documentation —
#     which explains that the frontend must not touch GStreamer or zbus —
#     would trip the check that enforces it.
#   - `#[cfg(test)]` blocks at column zero, up to their closing brace at
#     column zero. Test code legitimately opens databases and files;
#     counting it would punish writing tests. Skipping the block rather than
#     truncating the file matters: production code that happens to sit after
#     the test module stays measured.
frontend_code() {
  local file
  while IFS= read -r file; do
    awk -v path="$file" '
      /^#\[cfg\(test\)\]/ { skipping = 1; next }
      skipping && /^\}/      { skipping = 0; next }
      skipping                { next }
      { print path ":" FNR ":" $0 }
    ' "$file"
  done < <(find "$frontend" -name '*.rs' -type f ! -name '*_tests.rs' | sort) \
    | rg --invert-match ':[0-9]+:[[:space:]]*(//|/\*|\*)'
}

count() {
  local pattern=$1
  frontend_code | rg --count-matches "$pattern" || true
}

failed=0

check_budget() {
  local name=$1 pattern=$2
  local actual
  actual=$(count "$pattern")
  actual=${actual:-0}
  local allowed=${budget[$name]}
  if (( actual > allowed )); then
    echo "frontend thinness: $name grew from $allowed to $actual" >&2
    echo "  the frontend must get thinner, not thicker — move this into reprise-core" >&2
    failed=1
  elif (( actual < allowed )); then
    echo "frontend thinness: $name is down to $actual (budget still says $allowed)" >&2
    echo "  lower the budget in scripts/check-frontend-thinness.sh to $actual" >&2
    failed=1
  else
    echo "  $name: $actual (at budget)"
  fi
}

check_ban() {
  local name=$1 pattern=$2
  local actual
  actual=$(count "$pattern")
  actual=${actual:-0}
  if (( actual > 0 )); then
    echo "frontend thinness: $name is banned in the frontend, found $actual use(s)" >&2
    frontend_code | rg "$pattern" | head -5 >&2
    failed=1
  else
    echo "  $name: none (banned)"
  fi
}

check_budget rusqlite 'rusqlite::|use rusqlite|params!|\.prepare\(|\.query_row\(|Connection'
check_budget filesystem 'std::fs::|use std::fs|File::open|File::create|create_dir|remove_file'
check_budget threads 'thread::spawn|thread::Builder'
check_ban gstreamer 'gstreamer|\bgst::|use gst\b'
check_ban zbus 'zbus'

# Business workers are counted as whole files rather than call sites: the
# unit that has to move into the core is the worker, not the line.
worker_files=$(find "$frontend" -name '*worker*.rs' -not -name '*_tests.rs' | wc -l)
if (( worker_files > budget[workers] )); then
  echo "frontend thinness: ${worker_files} worker files, budget ${budget[workers]}" >&2
  failed=1
elif (( worker_files < budget[workers] )); then
  echo "frontend thinness: ${worker_files} worker files remain (budget says ${budget[workers]})" >&2
  echo "  lower the budget in scripts/check-frontend-thinness.sh to ${worker_files}" >&2
  failed=1
else
  echo "  workers: ${worker_files} files (at budget)"
fi

echo "== Dead-code allowlist =="

# `#[allow(dead_code)]` is how an escape hatch survives review: it silences
# the one compiler warning that would have asked why the item still exists.
# The extraction ahead will produce plenty of candidates, so the existing
# ones are pinned per file. A new one has to justify itself by editing this
# list, in the commit that adds it.
#
# Both spellings count. The inner form `#![allow(dead_code)]` silences a
# whole *file* rather than one item, so it is the broader escape hatch of the
# two — and it used to slip past this check entirely, because the pattern
# only looked for the outer one. A gate that catches the narrow case and
# misses the wide one is worse than no gate: it reads as coverage.
allowlist=$(cat <<'ALLOWLIST'
crates/reprise-cli/tests/common/mod.rs:1
crates/reprise-core/src/library/playlists.rs:6
crates/reprise-gnome/src/ui/artist_news/artist_news_worker.rs:1
crates/reprise-gnome/src/ui/concerts/concerts_columns.rs:1
crates/reprise-gnome/src/ui/concerts/concerts_filter_bar.rs:1
crates/reprise-gnome/src/ui/concerts/concerts_model.rs:1
crates/reprise-gnome/src/ui/concerts/concerts_presentation.rs:1
crates/reprise-gnome/src/ui/concerts/concerts_view.rs:1
crates/reprise-gnome/src/ui/concerts/concerts_worker.rs:1
crates/reprise-gnome/src/ui/concerts/mod.rs:1
crates/reprise-gnome/src/ui/issues/mod.rs:1
crates/reprise-gnome/src/ui/lyrics/lyrics_view.rs:4
crates/reprise-gnome/src/ui/motion.rs:2
crates/reprise-gnome/src/ui/playback/external_media.rs:1
crates/reprise-gnome/src/ui/playback/external_media_state.rs:1
crates/reprise-gnome/src/ui/playback/session_player.rs:3
crates/reprise-gnome/src/ui/player_bar/player_bar_layout.rs:2
crates/reprise-gnome/src/ui/player_bar/waveform_seek.rs:3
crates/reprise-gnome/src/ui/podcasts/mod.rs:1
crates/reprise-gnome/src/ui/radio/mod.rs:1
crates/reprise-gnome/src/ui/releases/mod.rs:1
crates/reprise-gnome/src/ui/releases/releases_columns.rs:1
crates/reprise-gnome/src/ui/releases/releases_empty_state.rs:1
crates/reprise-gnome/src/ui/releases/releases_filter_bar.rs:1
crates/reprise-gnome/src/ui/releases/releases_model.rs:1
crates/reprise-gnome/src/ui/releases/releases_presentation.rs:1
crates/reprise-gnome/src/ui/releases/releases_view.rs:1
crates/reprise-gnome/src/ui/runtime/commands.rs:1
crates/reprise-gnome/src/ui/runtime/session.rs:2
crates/reprise-gnome/src/ui/strings_concerts.rs:1
crates/reprise-gnome/src/ui/strings_news.rs:1
crates/reprise-gnome/src/ui/strings_online_sources.rs:1
crates/reprise-gnome/src/ui/strings_podcasts.rs:1
crates/reprise-gnome/src/ui/strings_radio.rs:1
crates/reprise-gnome/src/ui/strings_releases.rs:1
crates/reprise-gnome/src/ui/strings.rs:4
crates/reprise-gnome/src/ui/strings_sources.rs:1
crates/reprise-gnome/src/ui/strings_tag_edit.rs:7
crates/reprise-gnome/src/ui/tag_edit/tag_edit_flow.rs:1
crates/reprise-gnome/src/ui/updates/release_cover.rs:1
crates/reprise-mcp/tests/common/mod.rs:1
ALLOWLIST
)

actual_allows=$(rg --no-heading --count '#!?\[allow\(dead_code\)\]' crates --glob '*.rs' | sort)

if [[ $actual_allows != "$allowlist" ]]; then
  echo "dead-code allowlist drifted:" >&2
  diff <(echo "$allowlist") <(echo "$actual_allows") >&2 || true
  echo "  a '<' line is an entry that disappeared — delete it from the allowlist here" >&2
  echo "  a '>' line is a new #[allow(dead_code)] — justify it in the commit and add it" >&2
  failed=1
else
  echo "  $(echo "$actual_allows" | wc -l) files, unchanged"
fi

echo "== Unused dependencies =="

# cargo-machete is not vendored; when it is missing this says so instead of
# passing quietly, because a skipped check that looks green is worse than a
# check that is honestly absent.
if command -v cargo-machete >/dev/null 2>&1; then
  if ! cargo machete; then
    echo "cargo machete found unused dependencies" >&2
    failed=1
  fi
else
  echo "  SKIPPED: cargo-machete is not installed (cargo install cargo-machete)"
fi

# Not enforced here, and deliberately so: `-D unreachable-pub` sounds like a
# lint flip but measures 1240 sites in reprise-gnome, roughly 60% of them
# `pub const` string-table entries inside modules that are already private to
# `crate::ui`. Turning it on means a mechanical edit touching most of the
# crate, which would collide with every open branch for very little signal.
# It belongs in its own commit at a quiet moment, not bolted onto this gate.

if (( failed != 0 )); then
  exit 1
fi

echo "Frontend thinness lint passed"
