#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

expected_locales=(ar bn de es fr hi zh_CN)
complete_locales=(de es)
minimum_seed_messages=100
mapfile -t actual_locales < <(sed '/^[[:space:]]*#/d; /^[[:space:]]*$/d' po/LINGUAS | sort)

if [[ "${actual_locales[*]}" != "${expected_locales[*]}" ]]; then
  printf 'Expected gettext locales: %s\n' "${expected_locales[*]}" >&2
  printf 'Actual gettext locales:   %s\n' "${actual_locales[*]}" >&2
  exit 1
fi

tmp_root=$(mktemp -d)
trap 'find "$tmp_root" -type f -delete; rmdir "$tmp_root"' EXIT

scripts/update-catalogs.sh --extract-to "$tmp_root/reprise.pot"

# The committed template is the one translators and the merge see; it must say exactly what the
# sources say, in both directions (msgcmp alone only reports template entries the catalog lacks).
msgcmp --use-fuzzy --use-untranslated "$tmp_root/reprise.pot" po/reprise.pot
msgcmp --use-fuzzy --use-untranslated po/reprise.pot "$tmp_root/reprise.pot"

for locale in "${expected_locales[@]}"; do
  catalog="po/$locale.po"
  msgfmt --check --check-format -o "$tmp_root/$locale.mo" "$catalog"
  msgcmp --use-fuzzy --use-untranslated "$catalog" "$tmp_root/reprise.pot"
  test -z "$(msgattrib --only-fuzzy "$catalog")"
  test -z "$(msgattrib --only-obsolete "$catalog")"

  translated=$(msgattrib --translated --no-obsolete "$catalog" \
    | awk '/^msgid / { count++ } END { print count + 0 }')
  if (( translated < minimum_seed_messages )); then
    printf '%s has only %d translated messages; expected at least %d\n' \
      "$locale" "$translated" "$minimum_seed_messages" >&2
    exit 1
  fi

  if [[ " ${complete_locales[*]} " == *" $locale "* ]]; then
    test -z "$(msgattrib --untranslated --no-obsolete "$catalog")"
  fi
done
