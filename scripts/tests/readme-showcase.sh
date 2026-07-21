#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

english=README.md
german=README.de.md
performance_visual=docs/assets/reprise-performance.svg
architecture_visual=docs/assets/reprise-architecture.svg

fail() {
  echo "$1" >&2
  exit 1
}

require_fixed() {
  local value=$1
  local path=$2
  rg --fixed-strings --quiet "$value" "$path" || fail "$path must contain: $value"
}

reject_fixed() {
  local value=$1
  local path=$2
  if rg --fixed-strings --quiet "$value" "$path"; then
    fail "$path must not contain portfolio copy: $value"
  fi
}

for path in "$english" "$german"; do
  [[ -f $path ]] || fail "$path must exist"
  (( $(wc -l < "$path") <= 170 )) || fail "$path must remain a concise developer entry point"
  [[ $(rg -c 'docs/assets/reprise-architecture\.svg' "$path") -eq 1 ]] ||
    fail "$path must contain exactly one architecture visual"
  reject_fixed '```mermaid' "$path"
  reject_fixed 'docs/assets/reprise-performance.svg' "$path"
  reject_fixed 'Rust code lines' "$path"
  reject_fixed 'Rust-Codezeilen' "$path"
done

[[ -f $performance_visual ]] || fail "$performance_visual must exist"
for value in \
  'WHAT CHANGED' \
  'partial present-track index' \
  'full scan + temporary sort' \
  '40.2× faster' \
  '+9.85% storage'; do
  require_fixed "$value" "$performance_visual"
done

[[ -f $architecture_visual ]] || fail "$architecture_visual must exist"
for value in \
  'DOMAIN AUTHORITY' \
  'COMMANDS + QUERIES' \
  'IMPLEMENTS CORE CONTRACTS' \
  'FORBIDDEN IN reprise-core'; do
  require_fixed "$value" "$architecture_visual"
done
reject_fixed 'Future native frontends' "$architecture_visual"

require_fixed '[Deutsch](README.de.md)' "$english"
require_fixed '[English](README.md)' "$german"

for value in \
  '## Architecture' \
  '## Engineering contracts' \
  '## Build and run' \
  '## Verification' \
  '## Documentation' \
  '## License'; do
  require_fixed "$value" "$english"
done

for value in \
  '## Architektur' \
  '## Engineering-Verträge' \
  '## Bauen und starten' \
  '## Verifikation' \
  '## Dokumentation' \
  '## Lizenz'; do
  require_fixed "$value" "$german"
done

for path in "$english" "$german"; do
  for crate in reprise-core reprise-platform-linux reprise-gnome; do
    require_fixed "$crate" "$path"
  done
  require_fixed 'cargo build --locked --workspace' "$path"
  require_fixed 'cargo test --locked --workspace' "$path"
  require_fixed 'scripts/check-merge-readiness.sh --no-fetch' "$path"
  require_fixed 'docs/showcase.md' "$path"
  require_fixed 'docs/ux-rules.md' "$path"
  require_fixed 'TESTING.md' "$path"
done

reject_fixed '## Product surface today' "$english"
reject_fixed '## Heutiger Produktumfang' "$german"
reject_fixed '## Roadmap: the same core beyond today’s player' "$english"
reject_fixed '## Roadmap: derselbe Core über den heutigen Player hinaus' "$german"

echo "Bilingual developer README contract passed"
