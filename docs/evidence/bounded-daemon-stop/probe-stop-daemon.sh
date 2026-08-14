#!/usr/bin/env bash
# Measures whether cua_common_stop_daemon returns when the daemon ignores
# SIGTERM. Usage: probe-stop-daemon.sh <path-to-session.sh>
set -uo pipefail

# shellcheck source=../../../scripts/cua-common/session.sh
source "$1"

# The caller owns cua_driver; the probe only needs it to be a no-op.
cua_driver() { return 0; }

# SIG_IGN survives execve, so this is a single process that can never be
# terminated by SIGTERM — exactly the daemon shape that blocks a bare `wait`.
bash -c 'trap "" TERM; exec sleep 300' &
CUA_COMMON_DAEMON_PID=$!
echo "child_pid=$CUA_COMMON_DAEMON_PID"

start=$SECONDS
cua_common_stop_daemon probe-session
echo "returned_after=$((SECONDS - start))s"

if kill -0 "$CUA_COMMON_DAEMON_PID" 2>/dev/null; then
  echo "child_still_alive=yes"
else
  echo "child_still_alive=no"
fi
