#!/usr/bin/env bash

# Isolated Scrobbling lifecycle: UI setup, Secret Service persistence, real
# loopback submissions, provider status, disconnect, and a second restart.

SCROBBLING_API_PID=""
SCROBBLING_API_LOG=""
SCROBBLING_API_ROOT=""
SCROBBLING_KEYRING_PID=""

start_scrobbling_keyring() {
  local control_dir="$XDG_RUNTIME_DIR/reprise-e2e-keyring"

  mkdir -m 700 "$control_dir"
  gnome-keyring-daemon \
    --foreground \
    --unlock \
    --components=secrets \
    --control-directory="$control_dir" \
    < <(printf '\n') \
    >"$CUA_E2E_OUT_DIR/scrobbling-keyring.log" 2>&1 &
  SCROBBLING_KEYRING_PID=$!
  for _ in $(seq 1 40); do
    if gdbus call --session \
      --dest org.freedesktop.DBus \
      --object-path /org/freedesktop/DBus \
      --method org.freedesktop.DBus.NameHasOwner \
      org.freedesktop.secrets 2>/dev/null | rg --quiet 'true'; then
      return 0
    fi
    kill -0 "$SCROBBLING_KEYRING_PID" 2>/dev/null || break
    sleep 0.1
  done
  echo "temporary Secret Service did not own org.freedesktop.secrets" >&2
  tail -n 30 "$CUA_E2E_OUT_DIR/scrobbling-keyring.log" >&2 || true
  return 1
}

start_scrobbling_api() {
  local port_file="$CUA_E2E_SCRATCH_ROOT/scrobbling-api-port"

  SCROBBLING_API_LOG="$CUA_E2E_OUT_DIR/scrobbling-api.jsonl"
  python3 "$repo_root/scripts/cua-e2e/scrobbling_api.py" \
    --port-file "$port_file" \
    --log-file "$SCROBBLING_API_LOG" \
    >"$CUA_E2E_OUT_DIR/scrobbling-api.log" 2>&1 &
  SCROBBLING_API_PID=$!
  for _ in $(seq 1 40); do
    [[ -s "$port_file" ]] && break
    kill -0 "$SCROBBLING_API_PID" 2>/dev/null || break
    sleep 0.1
  done
  if [[ ! -s "$port_file" ]]; then
    echo "scrobbling loopback API did not publish a port" >&2
    tail -n 30 "$CUA_E2E_OUT_DIR/scrobbling-api.log" >&2 || true
    return 1
  fi
  SCROBBLING_API_ROOT="http://127.0.0.1:$(tr -d '[:space:]' <"$port_file")"
}

stop_scrobbling_services() {
  if [[ -n "$SCROBBLING_API_PID" ]]; then
    kill -TERM "$SCROBBLING_API_PID" 2>/dev/null || true
    wait "$SCROBBLING_API_PID" 2>/dev/null || true
    SCROBBLING_API_PID=""
  fi
  if [[ -n "$SCROBBLING_KEYRING_PID" ]]; then
    kill -TERM "$SCROBBLING_KEYRING_PID" 2>/dev/null || true
    wait "$SCROBBLING_KEYRING_PID" 2>/dev/null || true
    SCROBBLING_KEYRING_PID=""
  fi
}

start_scrobbling_app() {
  local phase=$1 exercise=${2:-no}
  local profile_root="$CUA_E2E_SCRATCH_ROOT/scrobbling-profile"
  local focus_state="$CUA_E2E_OUT_DIR/$phase-focus-state.txt"
  local -a scenario_env=(
    -u REPRISE_SCAN_DIR
    -u REPRISE_SMOKE_TAG_EDIT
    -u REPRISE_SMOKE_LISTENBRAINZ
    -u REPRISE_SMOKE_LASTFM
  )

  if [[ "$exercise" == yes ]]; then
    scenario_env+=(
      REPRISE_SMOKE_LISTENBRAINZ=exercise
      REPRISE_SMOKE_LASTFM=exercise
    )
  fi
  CUA_E2E_FOCUS_STATE="$focus_state"
  export CUA_E2E_FOCUS_STATE
  mkdir -p "$profile_root/data" "$profile_root/cache" "$profile_root/config"
  APP_LOG="$CUA_E2E_OUT_DIR/$phase-app.log"
  env "${scenario_env[@]}" \
    XDG_DATA_HOME="$profile_root/data" \
    XDG_CACHE_HOME="$profile_root/cache" \
    XDG_CONFIG_HOME="$profile_root/config" \
    GDK_BACKEND=x11 \
    WAYLAND_DISPLAY= \
    GTK_A11Y=atspi \
    NO_AT_BRIDGE=0 \
    REPRISE_AUDIO_SINK=fakesink \
    REPRISE_SMOKE_LISTENBRAINZ_API_ROOT="$SCROBBLING_API_ROOT" \
    REPRISE_SMOKE_LASTFM_API_ROOT="$SCROBBLING_API_ROOT/2.0" \
    REPRISE_SMOKE_LASTFM_AUTH_ROOT="$SCROBBLING_API_ROOT/auth" \
    REPRISE_SMOKE_QUIT=1 \
    REPRISE_SMOKE_QUIT_DELAY_SECS=300 \
    REPRISE_SMOKE_FOCUS_STATE="$focus_state" \
    REPRISE_LOG=debug \
    "$CUA_E2E_BIN_PATH" >"$APP_LOG" 2>&1 &
  APP_PID=$!
  export CUA_E2E_APP_PID="$APP_PID"
  if ! WINDOW_ID=$(wait_for_window "$APP_PID"); then
    echo "$phase did not expose a Reprise window" >&2
    tail -n 60 "$APP_LOG" >&2 || true
    return 1
  fi
}

stop_scrobbling_app() {
  local phase=$1 x_window_id

  cua_snapshot "$APP_PID" "$WINDOW_ID" "$phase-before-window-close" >/dev/null
  x_window_id=$(printf '0x%x' "$WINDOW_ID")
  wmctrl -i -c "$x_window_id"
  for _ in $(seq 1 40); do
    kill -0 "$APP_PID" 2>/dev/null || break
    sleep 0.25
  done
  if kill -0 "$APP_PID" 2>/dev/null; then
    echo "$phase did not close through its native window contract" >&2
    return 1
  fi
  wait "$APP_PID"
  assert_clean_app_log "$APP_LOG" "$phase"
  assert_app_log_contains "$APP_LOG" "starting Reprise" "$phase"
  assert_app_log_contains "$APP_LOG" "database ready" "$phase"
  APP_PID=""
  CUA_E2E_APP_PID=""
  export CUA_E2E_APP_PID
  WINDOW_ID=""
}

wait_for_snapshot_fragment() {
  local pid=$1 window_id=$2 fragment=$3 stem=$4 snapshot_path

  for attempt in $(seq 1 40); do
    snapshot_path=$(cua_snapshot "$pid" "$window_id" "$stem-$attempt")
    if rg --quiet --fixed-strings "$fragment" "$snapshot_path"; then
      printf '%s\n' "$snapshot_path"
      return 0
    fi
    sleep 0.25
  done
  echo "window never exposed expected text '$fragment'" >&2
  return 1
}

assert_scrobbling_focus_widget() {
  local stem=$1 widget=$2
  local focus_path="$CUA_E2E_OUT_DIR/$stem-after-focus.txt"

  if [[ ! -s "$focus_path" ]] \
    || ! rg --quiet --fixed-strings --line-regexp "widget=$widget" "$focus_path"; then
    echo "CUA focus is not on $widget: $focus_path" >&2
    return 1
  fi
}

cua_click_scrobbling_switch() {
  local pid=$1 window_id=$2 label=$3 stem=$4
  local before_path index action_path payload

  before_path=$(cua_snapshot "$pid" "$window_id" "$stem-before")
  index=$(jq -r --arg label "$label" '
    [(.structuredContent.elements // .elements // [])[]
      | select(.label == $label and .role == "switch")
      | .element_index][0] // empty
  ' "$before_path")
  if [[ -z "$index" ]]; then
    echo "snapshot does not expose a switch labelled '$label'" >&2
    return 1
  fi
  action_path="$CUA_E2E_OUT_DIR/$stem-action.json"
  payload=$(jq -nc \
    --argjson pid "$pid" \
    --argjson window_id "$window_id" \
    --argjson element_index "$index" \
    --arg session "$CUA_E2E_SESSION" \
    '{pid: $pid, window_id: $window_id, element_index: $element_index,
      session: $session}')
  cua_driver click "$payload" >"$action_path"
  assert_action_landed "$action_path"
  cua_snapshot "$pid" "$window_id" "$stem-after" >/dev/null
}

open_scrobbling_page() {
  local stem=$1

  cua_hotkey_focused "$APP_PID" "$WINDOW_ID" "$stem-preferences" ctrl comma
  wait_for_label "$APP_PID" "$WINDOW_ID" "Plugins" "$stem-preferences-ready" >/dev/null
  cua_focus_label_via_key "$APP_PID" "$WINDOW_ID" "Plugins" down "$stem-plugins-focus" \
    >/dev/null
  cua_press_key_window "$APP_PID" "$WINDOW_ID" enter "$stem-plugins-enter"
  wait_for_label "$APP_PID" "$WINDOW_ID" "Connected Services" "$stem-plugins-ready" \
    >/dev/null
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Scrobbling" "$stem-scrobbling-open"
  wait_for_label "$APP_PID" "$WINDOW_ID" "ListenBrainz" "$stem-scrobbling-ready" \
    >/dev/null
}

close_scrobbling_preferences() {
  local stem=$1

  cua_hotkey_focused "$APP_PID" "$WINDOW_ID" "$stem-preferences-close" ctrl w
  wait_for_label "$APP_PID" "$WINDOW_ID" "Search all fields" "$stem-library-restored" \
    >/dev/null
}

wait_for_scrobbling_request() {
  local provider=$1 method=$2 track=${3:-}

  for _ in $(seq 1 80); do
    if jq -s -e \
      --arg provider "$provider" \
      --arg method "$method" \
      --arg track "$track" '
        any(.[];
          .provider == $provider
          and .method == $method
          and ($track == "" or .track == $track)
          and (.token_valid // true) == true
          and (.api_key_valid // true) == true
          and (.session_key_valid // true) == true
          and (.signature_valid // true) == true)
      ' "$SCROBBLING_API_LOG" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  echo "missing valid $provider $method request for '$track'" >&2
  cat "$SCROBBLING_API_LOG" >&2
  return 1
}

wait_for_scrobbling_request_count() {
  local provider=$1 method=$2 minimum=$3

  for _ in $(seq 1 80); do
    if jq -s -e \
      --arg provider "$provider" \
      --arg method "$method" \
      --argjson minimum "$minimum" '
        [.[] | select(.provider == $provider and .method == $method)] | length >= $minimum
      ' "$SCROBBLING_API_LOG" >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
  done
  echo "expected at least $minimum $provider $method requests" >&2
  cat "$SCROBBLING_API_LOG" >&2
  return 1
}

configure_scrobbling_providers() {
  local connected_path

  cua_click_scrobbling_switch \
    "$APP_PID" "$WINDOW_ID" "ListenBrainz" scrobbling-listenbrainz-enable
  wait_for_label "$APP_PID" "$WINDOW_ID" "User token" scrobbling-listenbrainz-token \
    >/dev/null
  cua_type_text_label \
    "$APP_PID" "$WINDOW_ID" "User token" \
    "reprise-e2e-listenbrainz-token" scrobbling-listenbrainz-token-entry
  cua_click_label "$APP_PID" "$WINDOW_ID" "Connect" scrobbling-listenbrainz-connect
  wait_for_scrobbling_request listenbrainz validate-token
  connected_path=$(wait_for_snapshot_fragment \
    "$APP_PID" "$WINDOW_ID" "Reprise E2E Listener" scrobbling-listenbrainz-connected)
  assert_snapshot_absent "$connected_path" "ListenBrainz Account"

  cua_click_scrobbling_switch \
    "$APP_PID" "$WINDOW_ID" "Last.fm" scrobbling-lastfm-enable
  wait_for_label "$APP_PID" "$WINDOW_ID" "Advanced setup" scrobbling-lastfm-advanced \
    >/dev/null
  cua_focus_label_via_tab \
    "$APP_PID" "$WINDOW_ID" "Advanced setup" scrobbling-lastfm-advanced-focus \
    >/dev/null
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" enter scrobbling-lastfm-advanced-open
  wait_for_label "$APP_PID" "$WINDOW_ID" "API key" scrobbling-lastfm-api-key >/dev/null
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" tab scrobbling-lastfm-api-key-focus
  assert_scrobbling_focus_widget scrobbling-lastfm-api-key-focus GtkText
  cua_type_text_window \
    "$APP_PID" "$WINDOW_ID" "reprise-e2e-api-key" scrobbling-lastfm-api-key-entry
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" tab scrobbling-lastfm-api-key-password-toggle
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" tab scrobbling-lastfm-secret-focus
  assert_scrobbling_focus_widget scrobbling-lastfm-secret-focus GtkText
  cua_type_text_window \
    "$APP_PID" "$WINDOW_ID" "reprise-e2e-shared-secret" scrobbling-lastfm-secret-entry
  cua_click_label "$APP_PID" "$WINDOW_ID" "Open Browser" scrobbling-lastfm-browser
  wait_for_label \
    "$APP_PID" "$WINDOW_ID" "Authorize Reprise in Your Browser" \
    scrobbling-lastfm-confirmation >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Continue" scrobbling-lastfm-continue
  wait_for_scrobbling_request lastfm auth.getSession
  wait_for_scrobbling_request lastfm user.getInfo
  connected_path=$(wait_for_snapshot_fragment \
    "$APP_PID" "$WINDOW_ID" "Reprise E2E Scrobbler" scrobbling-lastfm-connected)
  assert_snapshot_absent "$connected_path" "Last.fm Account"
}

disconnect_scrobbling_providers() {
  cua_focus_label_via_tab \
    "$APP_PID" "$WINDOW_ID" "ListenBrainz" \
    scrobbling-listenbrainz-expand-disconnect-focus >/dev/null
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" enter scrobbling-listenbrainz-expand-disconnect
  wait_for_label "$APP_PID" "$WINDOW_ID" "Disconnect" scrobbling-listenbrainz-disconnect-ready \
    >/dev/null
  cua_click_label \
    "$APP_PID" "$WINDOW_ID" "Disconnect" scrobbling-listenbrainz-disconnect
  cua_wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Reprise E2E Listener" scrobbling-listenbrainz-disabled \
    >/dev/null

  cua_focus_label_via_tab \
    "$APP_PID" "$WINDOW_ID" "Last.fm" \
    scrobbling-lastfm-expand-disconnect-focus >/dev/null
  cua_press_key_window \
    "$APP_PID" "$WINDOW_ID" enter scrobbling-lastfm-expand-disconnect
  wait_for_label "$APP_PID" "$WINDOW_ID" "Disconnect" scrobbling-lastfm-disconnect-ready \
    >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Disconnect" scrobbling-lastfm-disconnect
  cua_wait_for_label_absent \
    "$APP_PID" "$WINDOW_ID" "Reprise E2E Scrobbler" scrobbling-lastfm-disabled \
    >/dev/null
}

run_scrobbling_scenario() {
  local restart_path disconnected_path

  echo "[cua-e2e] scrobbling: setup -> restart -> submit -> disconnect -> restart"
  start_scrobbling_keyring
  start_scrobbling_api

  start_scrobbling_app scrobbling-setup
  wait_for_label "$APP_PID" "$WINDOW_ID" "Welcome to Reprise" scrobbling-welcome \
    >/dev/null
  cua_click_label "$APP_PID" "$WINDOW_ID" "Skip for Now" scrobbling-skip
  wait_for_label "$APP_PID" "$WINDOW_ID" "No music yet" scrobbling-empty >/dev/null
  open_scrobbling_page scrobbling-setup
  configure_scrobbling_providers
  close_scrobbling_preferences scrobbling-setup
  stop_scrobbling_app scrobbling-setup

  start_scrobbling_app scrobbling-restart yes
  wait_for_label "$APP_PID" "$WINDOW_ID" "Search all fields" scrobbling-restart-library \
    >/dev/null
  wait_for_scrobbling_request_count listenbrainz validate-token 2
  wait_for_scrobbling_request_count lastfm user.getInfo 2
  wait_for_scrobbling_request \
    listenbrainz submit-listens "Reprise Smoke Track"
  wait_for_scrobbling_request \
    lastfm track.scrobble "Reprise Last.fm Smoke Track"
  open_scrobbling_page scrobbling-restart
  restart_path=$(wait_for_snapshot_fragment \
    "$APP_PID" "$WINDOW_ID" "Reprise E2E Listener" \
    scrobbling-listenbrainz-restart-connected)
  assert_snapshot_contains "$restart_path" "ListenBrainz"
  echo "[cua-e2e] ListenBrainz connected after restart"
  restart_path=$(wait_for_snapshot_fragment \
    "$APP_PID" "$WINDOW_ID" "Reprise E2E Scrobbler" \
    scrobbling-lastfm-restart-connected)
  assert_snapshot_contains "$restart_path" "Last.fm"
  echo "[cua-e2e] Last.fm connected after restart"
  disconnect_scrobbling_providers
  close_scrobbling_preferences scrobbling-restart
  stop_scrobbling_app scrobbling-restart

  start_scrobbling_app scrobbling-disconnected
  wait_for_label "$APP_PID" "$WINDOW_ID" "Search all fields" scrobbling-final-library \
    >/dev/null
  open_scrobbling_page scrobbling-final
  disconnected_path=$(wait_for_snapshot_fragment \
    "$APP_PID" "$WINDOW_ID" \
    "Scrobble completed listens to ListenBrainz (network; off by default)" \
    scrobbling-final-disconnected)
  assert_snapshot_absent "$disconnected_path" "Reprise E2E Listener"
  echo "[cua-e2e] ListenBrainz disconnected after restart"
  assert_snapshot_absent "$disconnected_path" "Reprise E2E Scrobbler"
  echo "[cua-e2e] Last.fm disconnected after restart"
  close_scrobbling_preferences scrobbling-final
  stop_scrobbling_app scrobbling-disconnected
}
