#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
checker="$repo_root/scripts/check-release-metadata.sh"
bumper="$repo_root/scripts/bump-version.sh"
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT

fail() {
    printf 'Release metadata contract failed: %s\n' "$1" >&2
    exit 1
}

[[ -x $checker ]] || fail "$checker must exist and be executable"

fixture="$scratch/repo"
mkdir -p "$fixture/scripts" "$fixture/android/app" "$fixture/data"
cp "$checker" "$fixture/scripts/check-release-metadata.sh"
cp "$bumper" "$fixture/scripts/bump-version.sh"
chmod +x "$fixture/scripts/"*.sh

printf '%s\n' \
    '[workspace.package]' \
    'version = "1.2.3"' \
    > "$fixture/Cargo.toml"
printf '%s\n' \
    'project(' \
    "  'reprise'," \
    "  version: '1.2.3'," \
    ')' \
    > "$fixture/meson.build"
printf '%s\n' \
    'android {' \
    '    defaultConfig {' \
    '        versionCode = 7' \
    '        versionName = "0.4.2"' \
    '    }' \
    '}' \
    > "$fixture/android/app/build.gradle.kts"
printf '%s\n' \
    '# Changelog' \
    '' \
    '## [1.2.3] - 2026-08-21' \
    '' \
    '- Current release.' \
    > "$fixture/CHANGELOG.md"
printf '%s\n' \
    '<component>' \
    '  <releases>' \
    '    <release version="1.2.3" date="2026-08-21">' \
    '      <description>' \
    '        <p>Current release.</p>' \
    '        <p xml:lang="de">Aktuelle Veröffentlichung.</p>' \
    '      </description>' \
    '    </release>' \
    '  </releases>' \
    '</component>' \
    > "$fixture/data/io.github.marvinbaudach.Reprise.metainfo.xml"

(cd "$fixture" && scripts/check-release-metadata.sh --gate) >/dev/null || \
    fail "matching Meson and Cargo versions must pass gate mode"
(cd "$fixture" && scripts/check-release-metadata.sh) >/dev/null || \
    fail "complete current release metadata must pass full mode"

sed -i "s/version: '1.2.3'/version: '9.8.7'/" "$fixture/meson.build"
if output=$(cd "$fixture" && scripts/check-release-metadata.sh --gate 2>&1); then
    fail "gate mode accepted a mismatched Meson version"
fi
[[ $output == *"meson.build"* && $output == *"9.8.7"* && $output == *"1.2.3"* ]] || \
    fail "Meson mismatch must name meson.build and both values: $output"
sed -i "s/version: '9.8.7'/version: '1.2.3'/" "$fixture/meson.build"

sed -i 's/## \[1.2.3\]/## [1.2.2]/' "$fixture/CHANGELOG.md"
if output=$(cd "$fixture" && scripts/check-release-metadata.sh 2>&1); then
    fail "full mode accepted a missing current changelog section"
fi
[[ $output == *"CHANGELOG.md"* && $output == *"1.2.3"* ]] || \
    fail "missing changelog section must name CHANGELOG.md and the version: $output"
sed -i 's/## \[1.2.2\]/## [1.2.3]/' "$fixture/CHANGELOG.md"

sed -i '/xml:lang="de"/d' "$fixture/data/io.github.marvinbaudach.Reprise.metainfo.xml"
if output=$(cd "$fixture" && scripts/check-release-metadata.sh 2>&1); then
    fail "full mode accepted a release without German AppStream text"
fi
[[ $output == *"metainfo.xml"* && $output == *"1.2.3"* && $output == *"German"* ]] || \
    fail "missing German text must name the metainfo file and version: $output"

echo "Release metadata contracts passed"
