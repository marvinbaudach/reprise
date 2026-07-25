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

xgettext --directory=. --files-from=po/POTFILES.in --output="$tmp_root/reprise.pot" \
  --from-code=UTF-8 --language=Rust '--keyword=N_!:1' --keyword=plural:1,2 \
  --package-name=Reprise --package-version=0.1.1 \
  --msgid-bugs-address='Marvin Baudach' --copyright-holder='Marvin Baudach'

for locale in "${expected_locales[@]}"; do
  catalog="po/$locale.po"
  msgfmt --check --check-format -o "$tmp_root/$locale.mo" "$catalog"
  msgcmp --use-fuzzy --use-untranslated "$catalog" "$tmp_root/reprise.pot"
  test -z "$(msgattrib --only-fuzzy "$catalog")"

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
