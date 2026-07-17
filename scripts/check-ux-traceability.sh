#!/usr/bin/env bash
# Traceability-Gate: docs/ux-rules.md <-> regelbenannte Tests.
#
# Prüft drei Richtungen:
#   1. Jede [aktiv]-Regel hat >= 1 Test, der ihre ID im Namen trägt
#      (Rust-fn snake_case oder cua-e2e-Szenario kebab-case).
#   2. Kein Test referenziert eine ID, die im Dokument fehlt oder
#      [ersetzt ...] ist.
#   3. Kein #[ignore] auf einem Test, dessen Regel [aktiv] ist.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

doc=docs/ux-rules.md
[[ -f $doc ]] || { echo "check-ux-traceability: $doc fehlt" >&2; exit 1; }

prefixes='p|nav|play|alb|art|fx|mtp|set|fb|os|start|que'
fail=0

# --- Dokument einlesen: ID -> Status (aktiv|geplant|ersetzt) ---
declare -A status_of
while read -r id st; do
  status_of[$id]=$st
done < <(grep -oE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[(aktiv|geplant|ersetzt)' "$doc" \
  | sed -E 's/^- \*\*([A-Z]+-[0-9]+[a-z]?)\*\* \[(aktiv|geplant|ersetzt)/\1 \2/')

# --- Test-Referenzen einsammeln (snake aus Rust, kebab aus cua-e2e) ---
snake_refs=$(grep -rhoE "fn (${prefixes})_[0-9]+[a-z]?_" crates --include='*.rs' 2>/dev/null \
  | sed -E 's/^fn //; s/_$//' | sort -u || true)
kebab_refs=$(grep -rhoE "(${prefixes})-[0-9]+[a-z]?-[a-z0-9-]+" scripts/cua-e2e 2>/dev/null \
  | grep -oE "^(${prefixes})-[0-9]+[a-z]?" | sort -u || true)

to_id() { # play_1a bzw. play-1a -> PLAY-1a
  local raw=${1//-/_} prefix nr
  prefix=${raw%%_*}; nr=${raw#*_}
  printf '%s-%s' "${prefix^^}" "$nr"
}

declare -A tested
for ref in $snake_refs $kebab_refs; do
  id=$(to_id "$ref")
  tested[$id]=1
  case "${status_of[$id]:-fehlt}" in
    fehlt)   echo "FEHLER: Test referenziert unbekannte Regel $id" >&2; fail=1 ;;
    ersetzt) echo "FEHLER: Test referenziert ersetzte Regel $id — umhängen" >&2; fail=1 ;;
  esac
done

# --- Richtung 1: jede [aktiv]-Regel hat einen Test ---
for id in "${!status_of[@]}"; do
  if [[ ${status_of[$id]} == aktiv && -z ${tested[$id]:-} ]]; then
    echo "FEHLER: [aktiv]-Regel $id hat keinen regelbenannten Test" >&2; fail=1
  fi
done

# --- Richtung 3: kein #[ignore] auf [aktiv]-Regeln ---
while read -r fn_name; do
  id=$(to_id "$fn_name")
  if [[ ${status_of[$id]:-} == aktiv ]]; then
    echo "FEHLER: Test $fn_name ist ignored, aber Regel $id ist [aktiv]" >&2; fail=1
  fi
done < <(grep -rA3 --include='*.rs' '#\[ignore' crates 2>/dev/null \
  | grep -oE "fn (${prefixes})_[0-9]+[a-z]?_" | sed -E 's/^fn //; s/_$//' | sort -u || true)

if (( fail )); then exit 1; fi
active_count=$(grep -cE '^- \*\*[A-Z]+-[0-9]+[a-z]?\*\* \[aktiv\]' "$doc" || true)
echo "UX-Traceability ok: $active_count aktive Regeln abgedeckt"
