#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
service="$repo_root/docs/automation/reprise-worktree-gc.service"
timer="$repo_root/docs/automation/reprise-worktree-gc.timer"
installer="$repo_root/scripts/install-worktree-gc-timer.sh"
guide="$repo_root/docs/automation/worktree-cleanup.md"

[[ -x $installer ]]
[[ -f $service ]]
[[ -f $timer ]]
[[ -f $guide ]]

rg -Fq 'Type=oneshot' "$service"
rg -Fq 'REPRISE_GC_STATE_ROOT=%h/.local/state/reprise-worktree-gc' "$service"
rg -Fq 'ExecStart=/home/marvin/Projects/reprise/scripts/reprise-worktree-gc.sh sweep --scope /home/marvin/Projects/reprise --apply --target-max-age-days 7 --target-min-kib 1048576' "$service"
rg -Fq 'OnCalendar=Sun *-*-* 04:15:00' "$timer"
rg -Fq 'Persistent=true' "$timer"
rg -Fq 'RandomizedDelaySec=30m' "$timer"
rg -Fq 'WantedBy=timers.target' "$timer"

fixture=$(mktemp -d "${TMPDIR:-/tmp}/reprise-worktree-gc-schedule.XXXXXX")
cleanup_fixture() {
  find "$fixture" -xdev -depth -delete 2>/dev/null || true
}
trap cleanup_fixture EXIT

fake_systemctl="$fixture/systemctl"
systemctl_log="$fixture/systemctl.log"
cat > "$fake_systemctl" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$*" >> "$SYSTEMCTL_LOG"
EOF
chmod +x "$fake_systemctl"

unit_dir="$fixture/systemd/user"
WORKTREE_GC_SYSTEMD_USER_DIR="$unit_dir" \
SYSTEMCTL_BIN="$fake_systemctl" \
SYSTEMCTL_LOG="$systemctl_log" \
  "$installer"

cmp "$service" "$unit_dir/reprise-worktree-gc.service"
cmp "$timer" "$unit_dir/reprise-worktree-gc.timer"
rg -Fxq -- '--user daemon-reload' "$systemctl_log"
rg -Fxq -- '--user enable --now reprise-worktree-gc.timer' "$systemctl_log"

echo "Worktree GC schedule: OK"
