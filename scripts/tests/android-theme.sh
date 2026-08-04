#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
checker="$repo_root/scripts/check-android-theme.sh"
fixture_root=$(mktemp -d)
trap 'rm -rf "$fixture_root"' EXIT

source_root="$fixture_root/main/java"
theme_file="$source_root/de/reprise/spike/ui/theme/NocturneTheme.kt"
dynamic_theme_file="$source_root/de/reprise/spike/ui/theme/DynamicTheme.kt"
screen_file="$source_root/de/reprise/spike/LibraryScreen.kt"
mkdir -p "$(dirname "$theme_file")" "$(dirname "$screen_file")"

printf '%s\n' \
  'package de.reprise.spike.ui.theme' \
  'import androidx.compose.ui.graphics.Color' \
  'internal val Ground = Color(0xFF161826)' >"$theme_file"
printf '%s\n' \
  'package de.reprise.spike.ui.theme' \
  'import androidx.compose.ui.graphics.Color' \
  'internal val WallpaperFallback = Color(0xFF232532)' >"$dynamic_theme_file"
printf '%s\n' \
  'package de.reprise.spike' \
  'internal const val ScreenName = "Library"' >"$screen_file"

ANDROID_THEME_SOURCE_ROOT="$source_root" \
  ANDROID_THEME_FILE="$theme_file" \
  bash "$checker"

printf '%s\n' \
  'package de.reprise.spike' \
  'import androidx.compose.ui.graphics.Color' \
  'internal val BypassedTheme = Color(0xFF4FDBD4)' >"$screen_file"

if ANDROID_THEME_SOURCE_ROOT="$source_root" \
  ANDROID_THEME_FILE="$theme_file" \
  bash "$checker"; then
  echo "Android theme lint accepted a raw colour outside the theme file" >&2
  exit 1
fi

echo "Android theme lint tests passed"
