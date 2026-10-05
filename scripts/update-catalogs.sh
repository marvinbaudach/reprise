#!/usr/bin/env bash
# Regenerates the gettext template from the sources and brings every locale in po/LINGUAS up to
# date: extract, merge without fuzzy guessing, then drop obsolete entries. A changed string
# therefore shows up as untranslated; git history keeps the old translations.
#
#   scripts/update-catalogs.sh                  regenerate po/reprise.pot and every po/<locale>.po
#   scripts/update-catalogs.sh --extract-to F   write only the template to F, touch nothing else
#                                               (scripts/tests/gettext-catalogs.sh uses this)
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
# A relative --extract-to path means the caller's directory, so resolve it before moving.
if [[ ${1:-} == --extract-to && -n ${2:-} ]]; then
  set -- "$1" "$(realpath -m -- "$2")"
fi
cd "$repo_root"

extract() {
  xgettext --directory=. --files-from=po/POTFILES.in --output="$1" \
    --from-code=UTF-8 --language=Rust '--keyword=N_!:1' --keyword=plural:1,2 \
    '--keyword=history_plural:1c,2,3' '--keyword=history_outcome:1c,2' \
    --package-name=Reprise --package-version=0.1.1 \
    --msgid-bugs-address='Marvin Baudach' --copyright-holder='Marvin Baudach'
}

if [[ ${1:-} == --extract-to ]]; then
  [[ -n ${2:-} ]] || {
    echo "usage: $0 [--extract-to <template path>]" >&2
    exit 2
  }
  extract "$2"
  exit 0
fi

extract po/reprise.pot
mapfile -t locales < <(sed '/^[[:space:]]*#/d; /^[[:space:]]*$/d' po/LINGUAS)
for locale in "${locales[@]}"; do
  catalog="po/$locale.po"
  msgmerge --quiet --no-fuzzy-matching --backup=none --update "$catalog" po/reprise.pot
  msgattrib --no-obsolete -o "$catalog" "$catalog"
done
