#!/usr/bin/env bash

# Shared CUA primitives for Reprise acceptance tests. Callers keep `set -e`
# ownership so this file can also be sourced by the small contract test.

CUA_DRIVER_BIN="${CUA_DRIVER_BIN:-cua-driver}"
CUA_E2E_OUT_DIR="${CUA_E2E_OUT_DIR:-/tmp/reprise-cua-e2e}"
CUA_E2E_SESSION="${CUA_E2E_SESSION:-reprise-acceptance}"

# PID of the app under test, set by the runner. Used only to fail fast when the
# app has died: the driver waits out its full 120s timeout on a dead peer, so a
# crash mid-run costs two minutes per remaining call instead of stopping at the
# real failure. That is how one segfault turned into a twelve-minute stall whose
# logs pointed at the driver rather than at the crash.
CUA_E2E_APP_PID="${CUA_E2E_APP_PID:-}"

# PID of the private window manager. It is only checked at startup otherwise,
# and when it dies mid-run nothing says so: keys are accepted by the driver and
# delivered nowhere, so scenarios log 48 identical no-op snapshots and then fail
# on their own assertions. One such death cost a full misdiagnosis — a suspected
# keyboard focus trap that turned out to be an `XIO: fatal IO error` in openbox.
CUA_E2E_WM_PID="${CUA_E2E_WM_PID:-}"

cua_driver() {
  if [[ -n "$CUA_E2E_APP_PID" ]] && ! kill -0 "$CUA_E2E_APP_PID" 2>/dev/null; then
    echo "app under test (pid $CUA_E2E_APP_PID) is gone; not calling the driver" >&2
    return 1
  fi
  if [[ -n "$CUA_E2E_WM_PID" ]] && ! kill -0 "$CUA_E2E_WM_PID" 2>/dev/null; then
    echo "window manager (pid $CUA_E2E_WM_PID) is gone; keys would go nowhere" >&2
    return 1
  fi
  if [[ -n "${CUA_DRIVER_SOCKET:-}" ]]; then
    "$CUA_DRIVER_BIN" "$@" --socket "$CUA_DRIVER_SOCKET"
  else
    "$CUA_DRIVER_BIN" "$@"
  fi
}

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
  if ! cua_driver get_window_state "$payload" >"$json_path"; then
    echo "CUA snapshot command failed: $stem" >&2
    return 1
  fi

  if jq -e '.degraded == true' "$json_path" >/dev/null; then
    echo "CUA snapshot is degraded; the private AT-SPI bridge is unavailable: $stem" >&2
    return 1
  fi
  if [[ ! -s "$screenshot_path" ]]; then
    echo "CUA snapshot did not retain screenshot evidence: $stem" >&2
    return 1
  fi
  if [[ -n "${CUA_E2E_FOCUS_STATE:-}" && -s "$CUA_E2E_FOCUS_STATE" ]]; then
    cp "$CUA_E2E_FOCUS_STATE" "${json_path%.json}-focus.txt"
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

element_center_for_index() {
  local snapshot_path=$1 index=$2

  jq -er --argjson index "$index" '
    [(.structuredContent.elements // .elements // [])[]
      | select(.element_index == $index)
      | select(.frame.x != null and .frame.y != null
        and .frame.w != null and .frame.h != null)
      | [(.frame.x + (.frame.w / 2)), (.frame.y + (.frame.h / 2))]][0]
    | select(. != null)
    | @tsv
  ' "$snapshot_path"
}

snapshot_exposes_label() {
  local snapshot_path=$1 label=$2

  jq -e --arg label "$label" '
    any((.structuredContent.elements // .elements // [])[]; .label == $label)
    or (
      (.structuredContent.tree_markdown // .tree_markdown // "")
      | contains("\"" + $label + "\"")
    )
  ' "$snapshot_path" >/dev/null
}

assert_snapshot_contains() {
  local snapshot_path=$1 label=$2

  if ! snapshot_exposes_label "$snapshot_path" "$label"; then
    echo "snapshot does not expose expected label '$label': $snapshot_path" >&2
    return 1
  fi
}

assert_snapshot_absent() {
  local snapshot_path=$1 label=$2

  if snapshot_exposes_label "$snapshot_path" "$label"; then
    echo "snapshot unexpectedly still exposes label '$label': $snapshot_path" >&2
    return 1
  fi
}

cua_wait_for_label() {
  local pid=$1 window_id=$2 label=$3 stem=$4 snapshot_path

  for attempt in $(seq 1 24); do
    snapshot_path=$(cua_snapshot "$pid" "$window_id" "$stem-$attempt")
    if assert_snapshot_contains "$snapshot_path" "$label" 2>/dev/null; then
      printf '%s\n' "$snapshot_path"
      return 0
    fi
    sleep 0.25
  done
  echo "window never exposed expected accessible label '$label'" >&2
  return 1
}

cua_wait_for_label_absent() {
  local pid=$1 window_id=$2 label=$3 stem=$4 snapshot_path

  for attempt in $(seq 1 24); do
    snapshot_path=$(cua_snapshot "$pid" "$window_id" "$stem-$attempt")
    if assert_snapshot_absent "$snapshot_path" "$label" 2>/dev/null; then
      printf '%s\n' "$snapshot_path"
      return 0
    fi
    sleep 0.25
  done
  echo "window still exposes unexpected accessible label '$label'" >&2
  return 1
}

assert_action_landed() {
  local action_path=$1

  if ! jq -e . "$action_path" >/dev/null 2>&1; then
    if [[ $(wc -l <"$action_path") -eq 1 ]] \
      && rg --quiet '^✅ .+\.$' "$action_path"; then
      return 0
    fi
    echo "CUA action did not land cleanly: $action_path" >&2
    return 1
  fi
  if ! jq -e '
    (
      .effect == "confirmed"
      or .effect == "unverifiable"
      or (
        .effect? == null
        and .delivery_mode? == "foreground"
        and .verified? == false
        and .code? == null
        and .error? == null
      )
    )
    and .escalation.recommended? == null
  ' \
    "$action_path" >/dev/null; then
    echo "CUA action did not land cleanly: $action_path" >&2
    return 1
  fi
}

assert_focused_label() {
  local snapshot_path=$1 label=$2

  if ! jq -e --arg label "$label" '
    [(.structuredContent.elements // .elements // [])[]
      | select(.label == $label)
      | select(.focused == true or ((.states // []) | any(. == "focused")))]
    | length == 1
  ' "$snapshot_path" >/dev/null; then
    echo "snapshot does not expose exactly one focused '$label': $snapshot_path" >&2
    return 1
  fi
}

assert_unique_focus() {
  local snapshot_path=$1

  if jq -e '
    [(.structuredContent.elements // .elements // [])[]
      | select(.focused == true or ((.states // []) | any(. == "focused")))]
    | length == 1
  ' "$snapshot_path" >/dev/null; then
    return 0
  fi
  local focus_path="${snapshot_path%.json}-focus.txt"
  if [[ -s "$focus_path" ]] && rg --quiet '^widget=.+$' "$focus_path" \
    && ! rg --quiet '^widget=none$' "$focus_path"; then
    return 0
  fi
  echo "snapshot does not expose exactly one focused element: $snapshot_path" >&2
  return 1
}

assert_focus_evidence_not() {
  local snapshot_path=$1 excluded_type=$2
  local focus_path="${snapshot_path%.json}-focus.txt"

  if [[ ! -s "$focus_path" ]] \
    || rg --quiet '^widget=none$' "$focus_path" \
    || rg --quiet --fixed-strings "widget=$excluded_type" "$focus_path"; then
    echo "GTK focus evidence is missing or still on $excluded_type: $focus_path" >&2
    return 1
  fi
}

assert_focus_evidence_label() {
  local snapshot_path=$1 expected_label=$2
  local focus_path="${snapshot_path%.json}-focus.txt"

  if [[ ! -s "$focus_path" ]] \
    || ! rg --quiet --fixed-strings --line-regexp \
      "label=$expected_label" "$focus_path"; then
    echo "GTK focus evidence does not identify '$expected_label': $focus_path" >&2
    return 1
  fi
}

cua_focus_label_via_tab() {
  local pid=$1 window_id=$2 expected_label=$3 stem=$4
  local snapshot_path

  snapshot_path=$(cua_snapshot "$pid" "$window_id" "$stem-initial")
  if assert_focus_evidence_label "$snapshot_path" "$expected_label" 2>/dev/null; then
    printf '%s\n' "$snapshot_path"
    return 0
  fi
  for attempt in $(seq 1 48); do
    cua_press_key_window "$pid" "$window_id" tab "$stem-tab-$attempt"
    snapshot_path="$CUA_E2E_OUT_DIR/$stem-tab-$attempt-after.json"
    if assert_focus_evidence_label "$snapshot_path" "$expected_label" 2>/dev/null; then
      printf '%s\n' "$snapshot_path"
      return 0
    fi
  done
  echo "Tab traversal never focused '$expected_label'" >&2
  return 1
}

cua_focus_label_via_key() {
  local pid=$1 window_id=$2 expected_label=$3 key=$4 stem=$5
  local snapshot_path

  snapshot_path=$(cua_snapshot "$pid" "$window_id" "$stem-initial")
  if assert_focus_evidence_label "$snapshot_path" "$expected_label" 2>/dev/null; then
    printf '%s\n' "$snapshot_path"
    return 0
  fi
  for attempt in $(seq 1 32); do
    cua_press_key_window "$pid" "$window_id" "$key" "$stem-key-$attempt"
    snapshot_path="$CUA_E2E_OUT_DIR/$stem-key-$attempt-after.json"
    if assert_focus_evidence_label "$snapshot_path" "$expected_label" 2>/dev/null; then
      printf '%s\n' "$snapshot_path"
      return 0
    fi
  done
  echo "Key traversal with '$key' never focused '$expected_label'" >&2
  return 1
}

cua_resize_window() {
  local pid=$1 window_id=$2 width=$3 height=$4 stem=$5
  local x_window_id

  x_window_id=$(printf '0x%x' "$window_id")
  if ! wmctrl -i -r "$x_window_id" -e "0,-1,-1,$width,$height"; then
    echo "could not resize CUA window to ${width}x${height}: $window_id" >&2
    return 1
  fi
  sleep 0.25
  cua_snapshot "$pid" "$window_id" "$stem-after-resize" >/dev/null
}

assert_focus_within() {
  local snapshot_path=$1 container_label=$2

  if ! jq -e --arg label "$container_label" '
    (.structuredContent.elements // .elements // []) as $elements
    | [$elements[]
        | select(.focused == true or ((.states // []) | any(. == "focused")))] as $focused
    | [$elements[] | select(.label == $label)] as $containers
    | def parent_chain($index):
        [$index] + (
          [$elements[] | select(.element_index == $index) | .parent_index][0] as $parent
          | if $parent == null then [] else parent_chain($parent) end
        );
      if ($focused | length) != 1 or ($containers | length) != 1 then
        false
      else
        parent_chain($focused[0].element_index)
        | index($containers[0].element_index) != null
      end
  ' "$snapshot_path" >/dev/null; then
    echo "focused element is not within '$container_label': $snapshot_path" >&2
    return 1
  fi
}

assert_focus_returns_to() {
  local before_path=$1 after_path=$2 label=$3

  assert_focused_label "$before_path" "$label"
  assert_focused_label "$after_path" "$label"
}

# Drive one pointer verb (click, double_click, ...) at a labelled element,
# snapshotting before and after so the caller has evidence either way.
cua_pointer_action_label() {
  local verb=$1 pid=$2 window_id=$3 label=$4 stem=$5
  local before_path action_path ax_action_path index payload x y

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
  if ! cua_driver "$verb" "$payload" >"$action_path"; then
    echo "CUA $verb command failed: $stem" >&2
    return 1
  fi
  if jq -e . "$action_path" >/dev/null 2>&1 \
    && jq -e '
    .effect == "suspected_noop" or .escalation.recommended? == "px"
  ' "$action_path" >/dev/null 2>&1; then
    ax_action_path="$CUA_E2E_OUT_DIR/$stem-ax-action.json"
    mv "$action_path" "$ax_action_path"
    read -r x y <<<"$(element_center_for_index "$before_path" "$index")"
    payload=$(jq -nc \
      --argjson pid "$pid" \
      --argjson window_id "$window_id" \
      --argjson x "$x" \
      --argjson y "$y" \
      --arg session "$CUA_E2E_SESSION" \
      '{pid: $pid, window_id: $window_id, x: $x, y: $y, session: $session}')
    if ! cua_driver "$verb" "$payload" >"$action_path"; then
      echo "CUA pixel $verb command failed: $stem" >&2
      return 1
    fi
  fi
  assert_action_landed "$action_path" || return 1
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null || return 1
}

cua_click_label() {
  cua_pointer_action_label click "$@"
}

# Click a known screen point when AT-SPI flattens every descendant to
# the native window's origin. That geometry defect makes a labelled-element
# click land on the title bar even though the retained screenshot proves the
# control's actual position. Keep this escape hatch explicit at the scenario
# call site instead of silently trusting bad accessibility bounds.
cua_click_screen_point() {
  local pid=$1 window_id=$2 x=$3 y=$4 stem=$5
  local action_path payload

  cua_snapshot "$pid" "$window_id" "$stem-before" >/dev/null || return 1
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --argjson x "$x" \
    --argjson y "$y" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, x: $x, y: $y, session: $session}')
  if ! cua_driver click "$payload" >"$action_path"; then
    echo "CUA screen-point click command failed: $stem" >&2
    return 1
  fi
  assert_action_landed "$action_path" || return 1
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null || return 1
}

cua_double_click_label() {
  cua_pointer_action_label double_click "$@"
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
  if ! cua_driver type_text "$payload" >"$action_path"; then
    echo "CUA type_text command failed: $stem" >&2
    return 1
  fi
  assert_action_landed "$action_path" || return 1
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null || return 1
}

cua_type_text_window() {
  local pid=$1 window_id=$2 value=$3 stem=$4
  local action_path payload

  cua_snapshot "$pid" "$window_id" "$stem-before" >/dev/null || return 1
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --arg text "$value" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, text: $text, session: $session,
      delivery_mode: "foreground"}')
  if ! cua_driver type_text "$payload" >"$action_path"; then
    echo "CUA focused type_text command failed: $stem" >&2
    return 1
  fi
  assert_action_landed "$action_path" || return 1
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null || return 1
}

cua_press_key_label() {
  local pid=$1 window_id=$2 label=$3 key=$4 stem=$5
  local before_path action_path background_action_path index payload

  before_path=$(cua_snapshot "$pid" "$window_id" "$stem-before")
  index=$(element_index_for_label "$before_path" "$label")
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --argjson element_index "$index" \
    --arg key "$key" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, element_index: $element_index,
      key: $key, session: $session}')
  if ! cua_driver press_key "$payload" >"$action_path"; then
    echo "CUA press_key command failed: $stem" >&2
    return 1
  fi
  if jq -e '
    .code == "background_unavailable"
    or .escalation.recommended? == "foreground"
  ' "$action_path" >/dev/null; then
    background_action_path="$CUA_E2E_OUT_DIR/$stem-background-action.json"
    mv "$action_path" "$background_action_path"
    payload=$(jq -c '. + {delivery_mode: "foreground"}' <<<"$payload")
    if ! cua_driver press_key "$payload" >"$action_path"; then
      echo "CUA foreground press_key command failed: $stem" >&2
      return 1
    fi
  fi
  assert_action_landed "$action_path" || return 1
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null || return 1
}

cua_press_key_focused() {
  local pid=$1 window_id=$2 key=$3 stem=$4
  local before_path action_path payload

  before_path=$(cua_snapshot "$pid" "$window_id" "$stem-before")
  if ! assert_unique_focus "$before_path"; then
    echo "cannot deliver '$key': snapshot has no unique focused element" >&2
    return 1
  fi
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --arg key "$key" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, key: $key, session: $session,
      delivery_mode: "foreground"}')
  if ! cua_driver press_key "$payload" >"$action_path"; then
    echo "CUA press_key command failed: $stem" >&2
    return 1
  fi
  assert_action_landed "$action_path" || return 1
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null || return 1
}

cua_press_key_window() {
  local pid=$1 window_id=$2 key=$3 stem=$4
  local action_path payload

  cua_snapshot "$pid" "$window_id" "$stem-before" >/dev/null || return 1
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --arg key "$key" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, key: $key, session: $session,
      delivery_mode: "foreground"}')
  if ! cua_driver press_key "$payload" >"$action_path"; then
    echo "CUA focused press_key command failed: $stem" >&2
    return 1
  fi
  assert_action_landed "$action_path" || return 1
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null || return 1
}

cua_hotkey() {
  local pid=$1 window_id=$2 stem=$3
  shift 3
  local before_path action_path background_action_path activation_path
  local keys payload activation_payload

  if (($# == 0)); then
    echo "cua_hotkey requires at least one key" >&2
    return 1
  fi
  before_path=$(cua_snapshot "$pid" "$window_id" "$stem-before")
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  keys=$(printf '%s\n' "$@" | jq -R . | jq -s .)
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --argjson keys "$keys" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, keys: $keys, session: $session}')
  if ! cua_driver hotkey "$payload" >"$action_path"; then
    echo "CUA hotkey command failed: $stem" >&2
    return 1
  fi
  if jq -e '.code == "background_unavailable"' "$action_path" >/dev/null; then
    background_action_path="$CUA_E2E_OUT_DIR/$stem-background-action.json"
    mv "$action_path" "$background_action_path"
    activation_path="$CUA_E2E_OUT_DIR/$stem-activation-action.json"
    activation_payload=$(jq -nc \
      --argjson pid "$pid" \
      --argjson window_id "$window_id" \
      '{pid: $pid, window_id: $window_id}')
    if ! cua_driver bring_to_front "$activation_payload" >"$activation_path"; then
      echo "CUA window activation failed: $stem" >&2
      return 1
    fi
    if ! jq -e --argjson window_id "$window_id" '.window_id == $window_id' \
      "$activation_path" >/dev/null; then
      echo "CUA window activation did not land: $activation_path" >&2
      return 1
    fi
    cua_snapshot "$pid" "$window_id" "$stem-foreground-before" >/dev/null || return 1
    payload=$(jq -c '. + {delivery_mode: "foreground"}' <<<"$payload")
    if ! cua_driver hotkey "$payload" >"$action_path"; then
      echo "CUA foreground hotkey command failed: $stem" >&2
      return 1
    fi
  fi
  assert_action_landed "$action_path" || return 1
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null || return 1
}
