#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
runner="$repo_root/scripts/weekly-portfolio-sync.sh"
cron_file="$repo_root/docs/automation/reprise-portfolio-sync.cron"
prompt_file="$repo_root/docs/automation/weekly-portfolio-sync.md"

if [[ ! -x $runner ]]; then
  echo "weekly portfolio runner is missing or not executable" >&2
  exit 1
fi
if [[ ! -f $cron_file ]]; then
  echo "weekly portfolio cron definition is missing" >&2
  exit 1
fi
if [[ ! -f $prompt_file ]]; then
  echo "weekly portfolio prompt is missing" >&2
  exit 1
fi
rg -q '^CRON_TZ=Europe/Zurich$' "$cron_file"
rg -q '^30 7 \* \* 1 ' "$cron_file"
rg -q '/home/marvin/Projects/reprise/scripts/weekly-portfolio-sync\.sh' "$cron_file"

for prompt_contract in \
  'developer README' \
  'developer-facing product story' \
  'technical entry point' \
  'English and German only' \
  'Keep architecture goals narrow' \
  'thin native frontends' \
  'MCP and CLI adapters' \
  'Do not classify product features, experiments, packaging, or release' \
  'work as architecture goals' \
  'Use natural prose' \
  'one architecture visual' \
  'Performance evidence defaults to a compact comparison table' \
  'Do not mirror the portfolio narrative' \
  'CV Reprise project summary' \
  'rebuild the versioned PDFs'; do
  rg -Fq "$prompt_contract" "$prompt_file"
done

fixture=$(mktemp -d "${TMPDIR:-/tmp}/reprise-weekly-sync.XXXXXX")
trap 'rm -rf "$fixture"' EXIT

init_repo() {
  local path=$1
  mkdir -p "$path"
  git -C "$path" init --initial-branch=main --quiet
  git -C "$path" config user.name "Weekly Sync Test"
  git -C "$path" config user.email "weekly-sync@example.invalid"
  printf '# Fixture\n' > "$path/README.md"
  git -C "$path" add README.md
  git -C "$path" commit --quiet -m "fixture"
}

reprise_repo="$fixture/reprise"
bewerbung_repo="$fixture/bewerbung"
init_repo "$reprise_repo"
init_repo "$bewerbung_repo"

fake_codex="$fixture/fake-codex"
cat > "$fake_codex" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "$@" > "$FAKE_CODEX_ARGS"

seen_exec=false
for argument in "$@"; do
  if [[ $argument == exec ]]; then
    seen_exec=true
  elif [[ $argument == --ask-for-approval && $seen_exec == true ]]; then
    echo "error: unexpected argument '--ask-for-approval' found" >&2
    exit 2
  fi
done

reprise_worktree=
bewerbung_worktree=
while (($#)); do
  case "$1" in
    -C|--cd)
      reprise_worktree=$2
      shift 2
      ;;
    --add-dir)
      bewerbung_worktree=$2
      shift 2
      ;;
    *) shift ;;
  esac
done

printf 'weekly refresh\n' >> "$reprise_worktree/README.md"
git -C "$reprise_worktree" add README.md
git -C "$reprise_worktree" commit --quiet -m "docs(showcase): refresh weekly Reprise evidence"

printf 'weekly refresh\n' >> "$bewerbung_worktree/README.md"
git -C "$bewerbung_worktree" add README.md
git -C "$bewerbung_worktree" commit --quiet -m "docs(cv): refresh weekly Reprise evidence"
EOF
chmod +x "$fake_codex"

args_log="$fixture/codex-args"
run_root="$fixture/run"
state_root="$fixture/state"
prompt="$fixture/prompt.md"
printf 'Update {{REPRISE_WORKTREE}} and {{BEWERBUNG_WORKTREE}}.\n' > "$prompt"

before_reprise=$(git -C "$reprise_repo" rev-parse HEAD)
before_bewerbung=$(git -C "$bewerbung_repo" rev-parse HEAD)
stale_branch="automation/reprise-portfolio-2099-W01"
git -C "$reprise_repo" branch "$stale_branch" main
git -C "$bewerbung_repo" branch "$stale_branch" main

env \
  REPRISE_REPO="$reprise_repo" \
  BEWERBUNG_REPO="$bewerbung_repo" \
  CODEX_BIN="$fake_codex" \
  WEEKLY_SYNC_PROMPT="$prompt" \
  WEEKLY_SYNC_RUN_ROOT="$run_root" \
  WEEKLY_SYNC_STATE_ROOT="$state_root" \
  WEEKLY_SYNC_ID="2099-W01" \
  FAKE_CODEX_ARGS="$args_log" \
  "$runner"

[[ $(git -C "$reprise_repo" rev-parse HEAD) == "$before_reprise" ]]
[[ $(git -C "$bewerbung_repo" rev-parse HEAD) == "$before_bewerbung" ]]

reprise_branch="automation/reprise-portfolio-2099-W01"
bewerbung_branch="automation/reprise-portfolio-2099-W01"
git -C "$reprise_repo" show-ref --verify --quiet "refs/heads/$reprise_branch"
git -C "$bewerbung_repo" show-ref --verify --quiet "refs/heads/$bewerbung_branch"
[[ $(git -C "$reprise_repo" rev-list --count "main..$reprise_branch") -eq 1 ]]
[[ $(git -C "$bewerbung_repo" rev-list --count "main..$bewerbung_branch") -eq 1 ]]

rg -Fx -- '--ephemeral' "$args_log"
rg -Fx -- '--sandbox' "$args_log"
rg -Fx -- 'workspace-write' "$args_log"
rg -Fx -- '--ask-for-approval' "$args_log"
rg -Fx -- 'never' "$args_log"

[[ ! -d $run_root/reprise ]]
[[ ! -d $run_root/bewerbung ]]
rg -q 'Reprise branch: automation/reprise-portfolio-2099-W01' "$state_root/2099-W01/result.md"
rg -q 'Bewerbung branch: automation/reprise-portfolio-2099-W01' "$state_root/2099-W01/result.md"

echo "Weekly portfolio sync: OK"
