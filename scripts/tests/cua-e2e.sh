#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

runner="$repo_root/scripts/cua-e2e/run.sh"
if [[ ! -x "$runner" ]]; then
  echo "$runner must exist and be executable" >&2
  exit 1
fi
for pattern in \
  'dbus-run-session' \
  'XDG_DATA_HOME=' \
  'XDG_CACHE_HOME=' \
  'GDK_BACKEND=x11' \
  'WAYLAND_DISPLAY=' \
  'GTK_A11Y=atspi' \
  'NO_AT_BRIDGE=0' \
  'REPRISE_AUDIO_SINK=fakesink' \
  'Xvfb did not allocate a private display' \
  'first-run wizard presented' \
  'first-run setup completed' \
  'dev scan complete' \
  'smoke-quit timer fired' \
  'run-manifest.txt' \
  'run_fresh_install_scenario' \
  'run_populated_library_scenario' \
  'REPRISE_SMOKE_TAG_EDIT=' \
  'run_tag_1_no_jump_after_save_scenario' \
  'run_tag_3_multi_dialog_structure_scenario'
do
  if ! rg --quiet "$pattern" "$runner"; then
    echo "$runner must contain isolation/coverage pattern: $pattern" >&2
    exit 1
  fi
done

# shellcheck source=../cua-e2e/lib.sh
source "$repo_root/scripts/cua-e2e/lib.sh"

tmp_root=$(mktemp -d)
trap 'rm -rf "$tmp_root"' EXIT

export CUA_E2E_OUT_DIR="$tmp_root/output"
export CUA_E2E_CALL_LOG="$tmp_root/calls.log"
export CUA_E2E_SNAPSHOT_COUNT="$tmp_root/snapshot-count"
mkdir -p "$CUA_E2E_OUT_DIR" "$tmp_root/bin"

cat >"$tmp_root/bin/cua-driver" <<'FAKE'
#!/usr/bin/env bash
set -euo pipefail

tool=$1
payload=${2:-'{}'}
printf '%s\t%s\n' "$tool" "$payload" >>"$CUA_E2E_CALL_LOG"

case "$tool" in
  get_window_state)
    screenshot_path=$(jq -r '.screenshot_out_file' <<<"$payload")
    printf 'fake screenshot\n' >"$screenshot_path"
    count=0
    if [[ -f "$CUA_E2E_SNAPSHOT_COUNT" ]]; then
      count=$(<"$CUA_E2E_SNAPSHOT_COUNT")
    fi
    count=$((count + 1))
    printf '%s' "$count" >"$CUA_E2E_SNAPSHOT_COUNT"
    case "$count" in
      1)
        label="Skip for Now"
        index=2
        ;;
      2)
        label="No music yet"
        index=3
        ;;
      3)
        label="Search all fields"
        index=4
        ;;
      *)
        label="No results"
        index=5
        ;;
    esac
    jq -n --arg label "$label" --argjson index "$index" '{
      degraded: false,
      structuredContent: {
        elements: [{element_index: $index, role: "button", label: $label}]
      },
      tree_markdown: ("[element_index " + ($index | tostring) + "] " + $label)
    }'
    ;;
  click)
    jq -e '.element_index == 2 and .pid == 42 and .window_id == 7' \
      <<<"$payload" >/dev/null
    jq -n '{effect: "confirmed", verified: true}'
    ;;
  type_text)
    jq -e '.element_index == 4 and .pid == 42 and .window_id == 7 and .text == "nomatch"' \
      <<<"$payload" >/dev/null
    jq -n '{effect: "confirmed", verified: true}'
    ;;
  *)
    echo "unexpected fake cua-driver tool: $tool" >&2
    exit 2
    ;;
esac
FAKE
chmod +x "$tmp_root/bin/cua-driver"
export CUA_DRIVER_BIN="$tmp_root/bin/cua-driver"

cua_click_label 42 7 "Skip for Now" "fresh-install-skip"
if [[ ! -s "$CUA_E2E_OUT_DIR/fresh-install-skip-after.png" ]]; then
  echo "every real CUA snapshot must retain screenshot evidence" >&2
  exit 1
fi
assert_snapshot_contains \
  "$CUA_E2E_OUT_DIR/fresh-install-skip-after.json" \
  "No music yet"

cua_type_text_label 42 7 "Search all fields" "nomatch" "populated-search"
assert_snapshot_contains \
  "$CUA_E2E_OUT_DIR/populated-search-after.json" \
  "No results"

mapfile -t calls < <(cut -f1 "$CUA_E2E_CALL_LOG")
expected=(
  get_window_state
  click
  get_window_state
  get_window_state
  type_text
  get_window_state
)
if [[ "${calls[*]}" != "${expected[*]}" ]]; then
  echo "CUA actions must be bracketed by fresh before/after snapshots" >&2
  printf 'expected: %s\nactual:   %s\n' "${expected[*]}" "${calls[*]}" >&2
  exit 1
fi

echo "CUA E2E helper contract passed"
