#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
source_dir="$repo_root/docs/automation"
unit_dir=${WORKTREE_GC_SYSTEMD_USER_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user}
systemctl_bin=${SYSTEMCTL_BIN:-systemctl}

service_source="$source_dir/reprise-worktree-gc.service"
timer_source="$source_dir/reprise-worktree-gc.timer"
[[ -f $service_source ]] || {
  echo "Missing worktree cleanup service: $service_source" >&2
  exit 1
}
[[ -f $timer_source ]] || {
  echo "Missing worktree cleanup timer: $timer_source" >&2
  exit 1
}

install -Dm644 "$service_source" "$unit_dir/reprise-worktree-gc.service"
install -Dm644 "$timer_source" "$unit_dir/reprise-worktree-gc.timer"

"$systemctl_bin" --user daemon-reload
"$systemctl_bin" --user enable --now reprise-worktree-gc.timer

echo "Enabled weekly Reprise worktree cleanup."
echo "Inspect it with: systemctl --user status reprise-worktree-gc.timer"
