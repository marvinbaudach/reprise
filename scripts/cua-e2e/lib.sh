#!/usr/bin/env bash

# Shared CUA primitives for Reprise acceptance tests. Callers keep `set -e`
# ownership so this file can also be sourced by the small contract test.

CUA_DRIVER_BIN="${CUA_DRIVER_BIN:-cua-driver}"
CUA_E2E_OUT_DIR="${CUA_E2E_OUT_DIR:-/tmp/reprise-cua-e2e}"
CUA_E2E_SESSION="${CUA_E2E_SESSION:-reprise-acceptance}"

cua_snapshot() {
  local pid=$1 window_id=$2 stem=$3
  local json_path="$CUA_E2E_OUT_DIR/$stem.json"
  local screenshot_path="$CUA_E2E_OUT_DIR/$stem.png"
  local payload

  mkdir -p "$CUA_E2E_OUT_DIR"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --arg session "$CUA_E2E_SESSION" \
    --arg screenshot_out_file "$screenshot_path" \
    '{pid: $pid, window_id: $window_id, session: $session,
      screenshot_out_file: $screenshot_out_file}')
  "$CUA_DRIVER_BIN" get_window_state "$payload" >"$json_path"

  if jq -e '.degraded == true' "$json_path" >/dev/null; then
    echo "CUA snapshot is degraded; the private AT-SPI bridge is unavailable: $stem" >&2
    return 1
  fi
  if [[ ! -s "$screenshot_path" ]]; then
    echo "CUA snapshot did not retain screenshot evidence: $stem" >&2
    return 1
  fi
  printf '%s\n' "$json_path"
}

element_index_for_label() {
  local snapshot_path=$1 label=$2
  local matches

  matches=$(jq -r --arg label "$label" '
    [(.structuredContent.elements // .elements // [])[]
      | select(.label == $label)
      | select(.role == "button" or .role == "text" or .role == "toggle button"
               or .role == "check box" or .role == "entry" or .role == "switch"
               or (.actions // [] | any(. == "click")))
      | .element_index]
    | if length >= 1 then .[0] else empty end
  ' "$snapshot_path")
  if [[ -z "$matches" ]]; then
    matches=$(jq -r --arg label "$label" '
      [(.structuredContent.elements // .elements // [])[]
        | select(.label == $label)
        | .element_index]
      | if length >= 1 then .[0] else empty end
    ' "$snapshot_path")
  fi
  if [[ -z "$matches" ]]; then
    echo "snapshot does not expose any element labelled '$label': $snapshot_path" >&2
    return 1
  fi
  printf '%s\n' "$matches"
}

assert_snapshot_contains() {
  local snapshot_path=$1 label=$2

  if ! jq -e --arg label "$label" '
    any((.structuredContent.elements // .elements // [])[]; .label == $label)
  ' "$snapshot_path" >/dev/null; then
    echo "snapshot does not expose expected label '$label': $snapshot_path" >&2
    return 1
  fi
}

assert_snapshot_absent() {
  local snapshot_path=$1 label=$2

  if jq -e --arg label "$label" '
    any((.structuredContent.elements // .elements // [])[]; .label == $label)
  ' "$snapshot_path" >/dev/null; then
    echo "snapshot unexpectedly still exposes label '$label': $snapshot_path" >&2
    return 1
  fi
}

assert_action_landed() {
  local action_path=$1

  if jq -e '.effect == "suspected_noop" or .escalation.recommended != null' \
    "$action_path" >/dev/null; then
    echo "CUA action did not land cleanly: $action_path" >&2
    return 1
  fi
}

cua_click_label() {
  local pid=$1 window_id=$2 label=$3 stem=$4
  local before_path action_path index payload

  before_path=$(cua_snapshot "$pid" "$window_id" "$stem-before")
  index=$(element_index_for_label "$before_path" "$label")
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --argjson element_index "$index" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, element_index: $element_index,
      session: $session}')
  "$CUA_DRIVER_BIN" click "$payload" >"$action_path"
  assert_action_landed "$action_path"
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null
}

cua_type_text_label() {
  local pid=$1 window_id=$2 label=$3 value=$4 stem=$5
  local before_path action_path index payload

  before_path=$(cua_snapshot "$pid" "$window_id" "$stem-before")
  index=$(element_index_for_label "$before_path" "$label")
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --argjson element_index "$index" \
    --arg text "$value" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, element_index: $element_index,
      text: $text, session: $session}')
  "$CUA_DRIVER_BIN" type_text "$payload" >"$action_path"
  assert_action_landed "$action_path"
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null
}
