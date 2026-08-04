#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
source_root=${ANDROID_THEME_SOURCE_ROOT:-"$repo_root/android/app/src/main/java"}
theme_file=${ANDROID_THEME_FILE:-"$source_root/de/reprise/spike/ui/theme/NocturneTheme.kt"}
theme_directory=${ANDROID_THEME_DIRECTORY:-"$(dirname "$theme_file")"}

if [[ ! -d "$source_root" ]]; then
  echo "Android theme source root does not exist: $source_root" >&2
  exit 1
fi
if [[ ! -f "$theme_file" ]]; then
  echo "Android theme file does not exist: $theme_file" >&2
  exit 1
fi
if [[ ! -d "$theme_directory" ]]; then
  echo "Android theme directory does not exist: $theme_directory" >&2
  exit 1
fi

failed=0
pattern='\bColor[[:space:]]*(\(|\.(Black|White|Red|Green|Blue|Yellow|Cyan|Magenta|Gray|DarkGray|LightGray|Transparent|hsl|hsv|rgb)\b)'
shopt -s globstar nullglob
for file in "$source_root"/**/*.kt; do
  if [[ "$file" == "$theme_directory/"* ]]; then
    continue
  fi

  set +e
  matches=$(rg --line-number --with-filename "$pattern" "$file")
  status=$?
  set -e
  if (( status == 0 )); then
    echo "Raw Compose colour bypasses the Android theme directory:" >&2
    echo "$matches" >&2
    failed=1
  elif (( status > 1 )); then
    echo "Could not inspect Android theme colours in $file" >&2
    exit "$status"
  fi
done

if (( failed != 0 )); then
  exit 1
fi

echo "Android theme lint passed"
