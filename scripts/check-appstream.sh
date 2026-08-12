#!/usr/bin/env bash
# GP-12/GP-13/GP-16: AppStream metainfo, desktop file, and Flathub text limits.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"
source scripts/lib/rulebook.sh

skip_gate_if_tool_missing appstreamcli
skip_gate_if_tool_missing desktop-file-validate
skip_gate_if_tool_missing xmllint

metainfo=$(find data -name '*.metainfo.xml' -print -quit)
desktop=$(find data -name '*.desktop' -print -quit)
[[ -n $metainfo ]] || { echo "ERROR: no metainfo file under data/" >&2; exit 1; }
[[ -n $desktop ]] || { echo "ERROR: no desktop file under data/" >&2; exit 1; }

# GP-12
if ! output=$(appstreamcli validate --no-net --explain "$metainfo" 2>&1); then
  report_violation GP-12 "appstreamcli validate failed:
$output"
fi

# GP-13
if ! output=$(desktop-file-validate "$desktop" 2>&1); then
  report_violation GP-13 "desktop-file-validate failed:
$output"
fi

# GP-16 — read the untranslated top-level name and summary. A structural query
# cannot confuse the app name with the nested developer name.
if ! name=$(xmllint --xpath \
  'string(/component/name[not(@xml:lang)][1])' "$metainfo" 2>/dev/null); then
  report_violation GP-12 "xmllint could not parse $metainfo"
  name=""
fi
if ! summary=$(xmllint --xpath \
  'string(/component/summary[not(@xml:lang)][1])' "$metainfo" 2>/dev/null); then
  report_violation GP-12 "xmllint could not parse $metainfo"
  summary=""
fi

(( ${#name} < 15 )) || report_violation GP-16 \
  "app name \"$name\" is ${#name} characters, must be below 15"
(( ${#summary} <= 35 )) || report_violation GP-16 \
  "summary is ${#summary} characters, must be at most 35: \"$summary\""
[[ $summary != *. ]] || report_violation GP-16 \
  "summary must not end in a period: \"$summary\""
[[ $summary != *"$name"* ]] || report_violation GP-16 \
  "summary must not repeat the app name: \"$summary\""

rulebook_exit
