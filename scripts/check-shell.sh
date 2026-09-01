#!/usr/bin/env bash
set -euo pipefail

cd "$(git rev-parse --show-toplevel)"
source scripts/lib/rulebook.sh

skip_gate_if_tool_missing shellcheck

mapfile -d '' files < <(git ls-files -z '*.sh' '.githooks/*')
mapfile -d '' workflow_files < <(git ls-files -z '.github/workflows/*.yml')

workflow_run_blocks=$(mktemp -d)
trap 'rm -rf "$workflow_run_blocks"' EXIT

extract_workflow_run_blocks() {
  local workflow
  for workflow in "${workflow_files[@]}"; do
    awk -v output_dir="$workflow_run_blocks" -v workflow_name="${workflow##*/}" \
      -f scripts/lib/extract-workflow-run-blocks.awk "$workflow"
  done
}

extract_workflow_run_blocks
mapfile -d '' workflow_scripts < <(find "$workflow_run_blocks" -type f -print0 | sort -z)
shellcheck_version=$(shellcheck --version | awk '$1 == "version:" { print $2; exit }')
printf 'ShellCheck %s: checking %d tracked shell files and %d workflow run blocks\n' \
  "$shellcheck_version" "${#files[@]}" "${#workflow_scripts[@]}"

shellcheck -x -P SCRIPTDIR -S warning -f gcc -- "${files[@]}" "${workflow_scripts[@]}"
shellcheck -x -P SCRIPTDIR -S style -i SC2251,SC2004,SC2181 \
  -f gcc -- "${files[@]}" "${workflow_scripts[@]}"

disable_marker='shellcheck '"disable="
unexplained=0
for file in "${files[@]}"; do
  while IFS=: read -r line directive; do
    [[ -n $line ]] || continue
    above=''
    if ((line > 1)); then
      above=$(sed -n "$((line - 1))p" -- "$file")
    fi
    if [[ $directive =~ shellcheck\ disable=SC[0-9]+(,SC[0-9]+)*[[:space:]]+#[[:space:]]*[^[:space:]] ]] ||
      [[ $above =~ ^[[:space:]]*#[[:space:]]+[^[:space:]] && $above != *"$disable_marker"* ]]; then
      continue
    fi
    unexplained=$((unexplained + 1))
    printf '  %s:%s\n' "$file" "$line" >&2
  done < <(grep -n -- "$disable_marker" "$file" 2>/dev/null || true)
done

if ((unexplained != 0)); then
  printf '%d %s directive(s) without a stated reason (listed above)\n' \
    "$unexplained" "$disable_marker" >&2
  exit 1
fi
