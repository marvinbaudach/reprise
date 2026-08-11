#!/usr/bin/env bash
# GP-12/GP-13/GP-16: AppStream metainfo, desktop file, and Flathub text limits.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/lib/rulebook.sh

require_tool appstreamcli
require_tool desktop-file-validate

metainfo=$(find data -name '*.metainfo.xml' -print -quit)
desktop=$(find data -name '*.desktop' -print -quit)
[[ -n $metainfo ]] || { echo "ERROR: no metainfo file under data/" >&2; exit 1; }
[[ -n $desktop ]] || { echo "ERROR: no desktop file under data/" >&2; exit 1; }

# GP-12
if ! output=$(appstreamcli validate --pedantic --explain "$metainfo" 2>&1); then
  report_violation GP-12 "appstreamcli validate --pedantic failed:
$output"
fi

# GP-13
if ! output=$(desktop-file-validate "$desktop" 2>&1); then
  report_violation GP-13 "desktop-file-validate failed:
$output"
fi

# GP-16 — read the untranslated name and summary, i.e. the elements without
# an xml:lang attribute.
name=$(sed -n 's|^[[:space:]]*<name>\(.*\)</name>[[:space:]]*$|\1|p' "$metainfo" | head -1)
summary=$(sed -n 's|^[[:space:]]*<summary>\(.*\)</summary>[[:space:]]*$|\1|p' "$metainfo" | head -1)

(( ${#name} < 15 )) || report_violation GP-16 \
  "app name \"$name\" is ${#name} characters, must be below 15"
(( ${#summary} <= 35 )) || report_violation GP-16 \
  "summary is ${#summary} characters, must be at most 35: \"$summary\""
[[ $summary != *. ]] || report_violation GP-16 \
  "summary must not end in a period: \"$summary\""
[[ $summary != *"$name"* ]] || report_violation GP-16 \
  "summary must not repeat the app name: \"$summary\""

rulebook_exit
