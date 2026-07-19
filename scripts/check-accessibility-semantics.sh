#!/usr/bin/env bash
set -euo pipefail

repo_root=${A11Y_SEMANTICS_ROOT:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}
ui_root="$repo_root/crates/reprise-gnome/src/ui"

[[ -d $ui_root ]] || {
  echo "check-accessibility-semantics: $ui_root is missing" >&2
  exit 1
}

marker_pattern='^[[:space:]]*// a11y-semantics: role=[a-z0-9._/-]+ name=[a-z0-9._/-]+ state=[a-z0-9._/-]+ action=[a-z0-9._/+:-]+[[:space:]]*$'
failed=0

while IFS=: read -r file line _; do
  previous_line=$((line - 1))
  marker=$(sed -n "${previous_line}p" "$file")
  if [[ ! $marker =~ $marker_pattern ]]; then
    echo "semantic contract marker missing before $file:$line" >&2
    echo "expected: // a11y-semantics: role=<role> name=<name> state=<state> action=<action>" >&2
    failed=1
  fi
done < <(rg --line-number --no-heading 'set_focusable\(true\)' "$ui_root" \
  --glob '*.rs' --glob '!*_tests.rs' --glob '!accessibility_semantics.rs' || true)

while IFS= read -r file; do
  [[ -z $file ]] && continue
  for property in Label ValueMin ValueMax ValueNow ValueText; do
    if ! rg --quiet "Property::$property" "$(dirname "$file")" --glob '*.rs'; then
      echo "slider semantics missing Property::$property near $file" >&2
      failed=1
    fi
  done
done < <(rg -l 'AccessibleRole::Slider' "$ui_root" --glob '*.rs' --glob '!*_tests.rs' || true)

while IFS= read -r file; do
  [[ -z $file ]] && continue
  if ! rg --quiet 'State::Selected' "$file"; then
    echo "tab semantics missing State::Selected in $file" >&2
    failed=1
  fi
  if ! rg --quiet 'Relation::Controls' "$file"; then
    echo "tab semantics missing Relation::Controls in $file" >&2
    failed=1
  fi
done < <(rg -l 'AccessibleRole::Tab\b' "$ui_root" --glob '*.rs' --glob '!*_tests.rs' || true)

if ((failed != 0)); then
  exit 1
fi

echo "Accessibility semantics lint passed"
