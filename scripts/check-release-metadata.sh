#!/usr/bin/env bash
# Verify maintained version and release-text metadata.
#
# Gate mode is intentionally cheap enough for every pull request. Full mode is
# reserved for release preparation because changelog and AppStream release text
# are curated once per promoted release, not once per development merge.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$repo_root"

mode=full
case ${1:-} in
    '') ;;
    --gate) mode=gate ;;
    *)
        printf 'Usage: %s [--gate]\n' "$0" >&2
        exit 2
        ;;
esac
[[ $# -le 1 ]] || {
    printf 'Usage: %s [--gate]\n' "$0" >&2
    exit 2
}

fail() {
    printf 'check-release-metadata.sh: %s\n' "$1" >&2
    exit 1
}

workspace_version=$(scripts/bump-version.sh current)
meson_version=$(sed -n "s/^[[:space:]]*version: '\([^']*\)'.*/\1/p" meson.build)
[[ -n $meson_version ]] || fail "meson.build has no project version; Cargo.toml workspace version is $workspace_version"
[[ $meson_version == "$workspace_version" ]] || \
    fail "meson.build version $meson_version does not match Cargo.toml workspace version $workspace_version"

if [[ $mode == gate ]]; then
    printf 'Release metadata gate passed: Cargo.toml and meson.build are %s\n' "$workspace_version"
    exit 0
fi

changelog_heading="## [$workspace_version] - "
grep -Fq "$changelog_heading" CHANGELOG.md || \
    fail "CHANGELOG.md has no dated section for Cargo.toml workspace version $workspace_version"

python3 - data/io.github.marvinbaudach.Reprise.metainfo.xml "$workspace_version" <<'PY'
import sys
import xml.etree.ElementTree as ET

path, version = sys.argv[1:3]
try:
    root = ET.parse(path).getroot()
except (ET.ParseError, OSError) as error:
    sys.exit(f"check-release-metadata.sh: {path} cannot be read as XML: {error}")

release = next(
    (candidate for candidate in root.findall(".//release")
     if candidate.get("version") == version),
    None,
)
if release is None:
    sys.exit(
        f"check-release-metadata.sh: {path} has no release for "
        f"Cargo.toml workspace version {version}"
    )

paragraphs = release.findall("./description/p")
xml_lang = "{http://www.w3.org/XML/1998/namespace}lang"
has_english = any(
    paragraph.get(xml_lang) is None and (paragraph.text or "").strip()
    for paragraph in paragraphs
)
has_german = any(
    paragraph.get(xml_lang) == "de" and (paragraph.text or "").strip()
    for paragraph in paragraphs
)
if not has_english:
    sys.exit(
        f"check-release-metadata.sh: {path} release {version} has no English <p>"
    )
if not has_german:
    sys.exit(
        f"check-release-metadata.sh: {path} release {version} has no German "
        '<p xml:lang="de">'
    )
PY

printf 'Full release metadata passed for %s\n' "$workspace_version"
