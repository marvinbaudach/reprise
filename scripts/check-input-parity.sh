#!/usr/bin/env bash
set -euo pipefail

repo_root=${INPUT_PARITY_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
ui_root="$repo_root/crates/reprise-gnome/src/ui"

[[ -d $ui_root ]] || {
  echo "check-input-parity: $ui_root is missing" >&2
  exit 1
}

pointer_pattern='GestureClick::(new|builder)|GestureDrag::(new|builder)|GestureLongPress::(new|builder)|DragSource::(new|builder)|DropTarget::(new|builder)|EventControllerScroll::new|set_cursor_from_name\(Some\("pointer"'
marker_pattern='^[[:space:]]*// input-parity: ACC-8 keyboard=[a-z0-9._/-]+[[:space:]]*$'
failed=0

while IFS=: read -r file line _; do
  source_line=$(sed -n "${line}p" "$file")
  if [[ $source_line =~ ^[[:space:]]*// ]]; then
    continue
  fi
  marker_start=$((line > 2 ? line - 2 : 1))
  marker=$(sed -n "${marker_start},$((line - 1))p" "$file")
  if ! printf '%s\n' "$marker" | rg --quiet "$marker_pattern"; then
    echo "input-parity marker missing before $file:$line" >&2
    echo "expected: // input-parity: ACC-8 keyboard=<tested-partner>" >&2
    failed=1
  fi
done < <(rg --line-number --no-heading "$pointer_pattern" "$ui_root" --glob '*.rs' --glob '!*_tests.rs' || true)

while IFS= read -r file; do
  [[ -z $file ]] && continue
  if ! rg --quiet ':focus-visible|:focus-within' "$file"; then
    echo "outline removed without a focus-visible replacement in $file" >&2
    failed=1
  fi
done < <(rg -l 'outline:[[:space:]]*none' "$ui_root" --glob '*.rs' || true)

if (( failed != 0 )); then
  exit 1
fi

echo "Input parity lint passed"
