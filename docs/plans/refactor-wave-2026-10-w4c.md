---
slug: refactor-wave-2026-10-w4c
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-w4c
branch: feature/refactor-wave-2026-10-w4c
phase: refactored
codex_session:
created: 2026-10-05
---
# Refactor wave 2026-10, wave 4 — strand C: the gettext catalogs

Mother plan: `docs/plans/refactor-wave-2026-10-w4.md`; its "Shared context" binds this strand.

This plan is your only channel. When the files disagree with a number below, the files win:
re-measure, keep what you measured and say so in your final message.

## Purpose

The seven catalogs under `po/` carry 546 obsolete (`#~`) entries and 54 msgids that no longer
exist in the sources, because nobody has re-extracted since the strings were deleted (#1082 and
earlier). After this strand the template and the catalogs are regenerated from the sources, hold
no obsolete entries, `de` and `es` are still 100 % translated, one script holds the recipe, and
the gate refuses obsolete entries and a stale template from now on.

**Not behaviour-preserving for the files** (that is the point) — but behaviour-preserving for the
program: no translation of a live string is lost, no msgid changes, no Rust file changes.

## Evidence (origin/dev @ 9465e997e8, 2026-10-05; all commands run on a scratch copy)

- `po/`: `reprise.pot` (1383 `msgid`, 394 `rust-format` flags), `ar bn de es fr hi zh_CN` `.po`
  (1383 `msgid` each), `LINGUAS` (7 locales), `POTFILES.in` (40 source files), `meson.build`
  (`i18n.gettext('reprise', preset: 'glib')`). Obsolete entries: de 202, es 202, fr 39, hi 27,
  ar 26, zh_CN 26, bn 24. Fuzzy: 0 everywhere. Untranslated live msgids: de 0, es 0, fr 1056,
  ar 1169, hi 1167, zh_CN 1157, bn 1185.
- **There is no regeneration tooling.** The only extraction lives inside the test
  `scripts/tests/gettext-catalogs.sh:21-25`:
  ```
  xgettext --directory=. --files-from=po/POTFILES.in --output="$tmp_root/reprise.pot" \
    --from-code=UTF-8 --language=Rust '--keyword=N_!:1' --keyword=plural:1,2 \
    '--keyword=history_plural:1c,2,3' '--keyword=history_outcome:1c,2' \
    --package-name=Reprise --package-version=0.1.1 \
    --msgid-bugs-address='Marvin Baudach' --copyright-holder='Marvin Baudach'
  ```
  Then per locale: `msgfmt --check --check-format`, `msgcmp --use-fuzzy --use-untranslated
  "$catalog" "$tmp_root/reprise.pot"`, no fuzzy (`msgattrib --only-fuzzy` empty), at least 100
  translated, and for `de`/`es` no untranslated live entry (`msgattrib --untranslated
  --no-obsolete` empty). The gate is called from `scripts/check-merge-readiness.sh:103` and
  `scripts/check-release.sh:26-27`; CI installs `gettext` (`.github/workflows/ci.yml:188,233,294`).
- **What the gate does not see.** `msgcmp catalog template` reports an error only for template
  msgids missing from the catalog; a catalog msgid missing from the template is a warning. Measured
  with the direction reversed: **54 msgids of the committed template are not in the sources**
  (`msgcmp --use-fuzzy --use-untranslated fresh.pot po/reprise.pot` → 54 × "used but not
  defined"). The committed `po/reprise.pot` was extracted from an older tree (its `#:` references
  point at `strings.rs:134` where the sources now say `:127`).
- **Dry run of the full regeneration** (xgettext as above → `msgmerge --quiet
  --no-fuzzy-matching` → `msgattrib --no-obsolete`): every catalog ends with 1329 live msgids,
  0 obsolete, 0 fuzzy, 385 `rust-format` flags, `msgfmt --check --check-format` green; `de` and
  `es` keep 0 untranslated; `fr` 1008, `ar` 1121 untranslated (the dead entries were untranslated
  there). The merge marks 256 entries obsolete in de/es (202 old + 54 dead), 45 in fr, 32 in ar.
- **Why `msgattrib`, not a `#~` filter.** Obsolete entries keep their flag line (`#, rust-format`)
  *unprefixed* directly above the first `#~` line — 84 such lines in `de.po` and `es.po`, 6 in
  `fr.po`, 1 in each of the others. A line filter would leave them attached to the next live entry.
  `msgattrib --no-obsolete` removes the whole entry. (gettext-tools 1.0 on this machine;
  `msgattrib`, `msgmerge`, `xgettext`, `msgfmt`, `msgcmp`, `msgcat` are all installed.)
- Translatable strings are marked with the `N_!` macro in `crates/reprise-gnome/src/ui/strings*.rs`
  and `crates/reprise-view/src/strings/*.rs`; runtime lookup goes through
  `crates/reprise-gnome/src/i18n.rs` (`gettext`, `ngettext`). Nothing here changes.
- `scripts/tests/qa-linters.sh` registers scripts (`require_executable …`, lines ~99-143) and runs
  the self-tests it lists (~line 317); `scripts/check-shell.sh` runs shellcheck over `scripts/`.
  Read both before adding a script; follow their registration convention if one applies.
- `TESTING.md` lists `scripts/check-lyrics-smoke.sh` at line 242 among the manual checks; there is
  no paragraph about catalogs anywhere in `docs/`, `README.md` or `TESTING.md`.

## Decisions (fixed — do not re-open)

1. **One recipe, in a script.** `scripts/update-catalogs.sh` holds the xgettext line (identical
   keywords), the merge and the strip. The test calls the script's extraction so the keyword list
   exists once: `scripts/update-catalogs.sh --extract-to <path>` writes only the template to
   `<path>` and touches nothing else; without the flag the script regenerates `po/reprise.pot`,
   merges every locale in `po/LINGUAS` and strips obsolete entries in place.
2. **`msgmerge --no-fuzzy-matching --backup=none --quiet`.** The gate forbids fuzzy entries; a
   changed string must show up as untranslated (and fail `de`/`es` until translated), not as a
   guess. `--previous` is not used (it only matters with fuzzy matching).
3. **`msgattrib --no-obsolete`** for the strip (see evidence). The commit that lands the
   regenerated files is produced by running the script once — no hand edits in `po/`.
4. **The gate gains two checks:** `test -z "$(msgattrib --only-obsolete "$catalog")"` per locale,
   and the committed template must equal the fresh extraction in both directions
   (`msgcmp --use-fuzzy --use-untranslated "$tmp_root/reprise.pot" po/reprise.pot` and the
   reverse; `POT-Creation-Date` differs and is ignored by `msgcmp`).
5. **The template's `POT-Creation-Date` is accepted as noise.** Every string commit already
   rewrites it; a header-stripping trick is not worth the special case.
6. **Translations are never edited here.** The five incomplete locales stay incomplete; the 54
   dead entries' de/es translations are dropped with the entries (they are not strings the app
   shows). If the regenerated `de`/`es` show an untranslated live entry, something in the sources
   changed since this measurement — translate nothing, stop and report the msgid.

## Owns

- `po/reprise.pot`, `po/{ar,bn,de,es,fr,hi,zh_CN}.po`
- New: `scripts/update-catalogs.sh`
- `scripts/tests/gettext-catalogs.sh`
- `scripts/tests/qa-linters.sh` — only a registration line if its convention requires one for a
  new executable or self-test (read lines ~95-145 and ~310-320)
- `TESTING.md` — one short paragraph next to the existing manual-check list, saying how to
  regenerate the catalogs and that the gate refuses obsolete entries and a stale template

Not owned: any Rust file; `po/POTFILES.in`, `po/LINGUAS`, `po/meson.build`; `meson.build`;
`scripts/check-release.sh`; `scripts/check-merge-readiness.sh`.

## Tasks (in order, one commit each)

**C1 — the gate first (red).** In `scripts/tests/gettext-catalogs.sh` add, inside the per-locale
loop after the fuzzy check:

```bash
  test -z "$(msgattrib --only-obsolete "$catalog")"
```

and after the extraction, before the loop:

```bash
# The committed template is the one translators and the merge see; it must say exactly what the
# sources say, in both directions (msgcmp alone only reports template entries the catalog lacks).
msgcmp --use-fuzzy --use-untranslated "$tmp_root/reprise.pot" po/reprise.pot
msgcmp --use-fuzzy --use-untranslated po/reprise.pot "$tmp_root/reprise.pot"
```

Run it: it must fail on the current files (obsolete entries present; template stale with 54
"used but not defined"). Commit the red gate.

**C2 — the script.** `scripts/update-catalogs.sh`, `#!/usr/bin/env bash`, `set -euo pipefail`,
`cd` to the repo root like its siblings, shellcheck-clean. Shape:

```bash
extract() {   # $1: output template path
  xgettext --directory=. --files-from=po/POTFILES.in --output="$1" \
    --from-code=UTF-8 --language=Rust '--keyword=N_!:1' --keyword=plural:1,2 \
    '--keyword=history_plural:1c,2,3' '--keyword=history_outcome:1c,2' \
    --package-name=Reprise --package-version=0.1.1 \
    --msgid-bugs-address='Marvin Baudach' --copyright-holder='Marvin Baudach'
}
if [[ ${1:-} == --extract-to ]]; then extract "$2"; exit 0; fi
extract po/reprise.pot
mapfile -t locales < <(sed '/^[[:space:]]*#/d; /^[[:space:]]*$/d' po/LINGUAS)
for locale in "${locales[@]}"; do
  catalog="po/$locale.po"
  msgmerge --quiet --no-fuzzy-matching --backup=none --update "$catalog" po/reprise.pot
  msgattrib --no-obsolete -o "$catalog" "$catalog"
done
```

Keep the header comment short: what it does, that the test reuses `--extract-to`, and that
obsolete entries are dropped on purpose because git history keeps the old translations. Then make
`scripts/tests/gettext-catalogs.sh` call `scripts/update-catalogs.sh --extract-to "$tmp_root/reprise.pot"`
instead of its own xgettext block. Register the script wherever `scripts/tests/qa-linters.sh`
expects executables to be listed (if it does). `scripts/check-shell.sh` must be green.

**C3 — the regeneration.** Run `scripts/update-catalogs.sh` once. Verify before committing:

```
rg -c '^#~' po/*.po                                   # 0 for every file
for l in de es; do test -z "$(msgattrib --untranslated --no-obsolete po/$l.po)"; done
grep -c '^msgid ' po/reprise.pot po/*.po              # 1329 everywhere (re-measure; the sources may have moved)
git diff --stat -- po                                  # eight files, nothing else
scripts/tests/gettext-catalogs.sh                      # green
```

Commit the eight files alone with a subject in the repository's prose style, e.g. `The catalogs
say exactly what the sources say`.

**C4 — docs.** One paragraph in `TESTING.md`: run `scripts/update-catalogs.sh` after adding,
changing or deleting a translatable string; then translate every new `de`/`es` entry (the gate
refuses untranslated entries there and fuzzy or obsolete entries anywhere).

## Known traps

- **Do not hand-edit `.po` files.** The diff of C3 must be reproducible by re-running the script
  on the parent commit; the reviewer will do exactly that.
- **`msgmerge --update` rewrites the `#:` reference comments** for every entry whose source line
  moved; the diff is large (thousands of lines) and mechanical. That is expected; do not try to
  minimise it.
- **The xgettext warning** `strings_scrobbling.rs:72: Message contains an embedded URL` is
  pre-existing and harmless; do not "fix" the string (that would be a string change, outside this
  strand).
- **Never cite a `docs/plans/…` path** from the script or the test.
- **Toolchain parity.** CI's `gettext` may be older than the local 1.0; the `rust-format` flag
  count (385) is what the local xgettext emits and CI's `msgfmt --check-format` accepts either way.
  If the CI gate disagrees with the local run, report the versions instead of editing files.
- English everywhere, focused commits, no agent attribution lines.

## Verification

```
scripts/check-shell.sh
scripts/tests/gettext-catalogs.sh
scripts/tests/qa-linters.sh
rg -c '^#~' po/*.po                 # all 0
git diff --name-only origin/dev...HEAD   # po/*, scripts/update-catalogs.sh, scripts/tests/gettext-catalogs.sh, (qa-linters.sh), TESTING.md — nothing under crates/
```

No cargo command is needed; run `cargo fmt --check` anyway to prove nothing under `crates/` moved.
The orchestrator runs `scripts/check-merge-readiness.sh` after the code phase.

Report: the live msgid count after regeneration, obsolete count (0), untranslated counts per
locale, and the two gate checks' first red run output (one line each).
