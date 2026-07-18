#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

english=README.md
german=README.de.md

for path in "$english" "$german"; do
  if [[ ! -f "$path" ]]; then
    echo "$path must exist" >&2
    exit 1
  fi
done

require_fixed() {
  local value=$1
  local path=$2
  if ! rg --fixed-strings --quiet "$value" "$path"; then
    echo "$path must contain: $value" >&2
    exit 1
  fi
}

reject_fixed() {
  local value=$1
  local path=$2
  if rg --fixed-strings --quiet "$value" "$path"; then
    echo "$path must not contain stale claim: $value" >&2
    exit 1
  fi
}

require_fixed '[Deutsch](README.de.md)' "$english"
require_fixed '[English](README.md)' "$german"
require_fixed 'Started on 11 July 2026' "$english"
require_fixed 'Gestartet am 11. Juli 2026' "$german"

require_fixed '## Architecture: one Rust core, native edges' "$english"
require_fixed '## Architektur: ein Rust-Core, native Ränder' "$german"
require_fixed '```mermaid' "$english"
require_fixed '```mermaid' "$german"

require_fixed '## Performance: measured, not assumed' "$english"
require_fixed '## Performance: messen statt vermuten' "$german"
for path in "$english" "$german"; do
  require_fixed '1,600' "$path"
  require_fixed 'scripts/performance-baseline.sh' "$path"
  require_fixed 'scripts/performance-query-compare.sh' "$path"
done
require_fixed '53,605' "$english"
require_fixed '1,333' "$english"
require_fixed '97.51%' "$english"
require_fixed '+9.85%' "$english"
require_fixed '16.10 bytes/track' "$english"
require_fixed '53.605' "$german"
require_fixed '1.333' "$german"
require_fixed '97,51 %' "$german"
require_fixed '+9,85 %' "$german"
require_fixed '16,10 Byte/Track' "$german"

require_fixed '## Quality is executable policy' "$english"
require_fixed '## Qualität als ausführbare Policy' "$german"
require_fixed '1,482 passing tests' "$english"
require_fixed '1.482 bestandene Tests' "$german"
require_fixed '88,789 Rust code lines' "$english"
require_fixed '58,100 product + 30,700 test = 88,800 total' "$english"
require_fixed '88.789 Rust-Codezeilen' "$german"
require_fixed '58.100 Produkt + 30.700 Tests = 88.800 gesamt' "$german"
require_fixed '60 active UX rules' "$english"
require_fixed '60 aktive UX-Regeln' "$german"
require_fixed '[UX rulebook](docs/ux-rules.md)' "$english"
require_fixed '[UX-Regelwerk](docs/ux-rules.md)' "$german"

require_fixed '## Roadmap: the same core beyond today’s player' "$english"
require_fixed '## Roadmap: derselbe Core über den heutigen Player hinaus' "$german"
require_fixed 'MCP server' "$english"
require_fixed 'AI-generated music' "$english"
require_fixed 'AI visual effects' "$english"
require_fixed 'native frontends' "$english"
require_fixed 'MCP-Server' "$german"
require_fixed 'KI-generierte Musik' "$german"
require_fixed 'Visuelle KI-Effekte' "$german"
require_fixed 'native Frontends' "$german"
require_fixed 'These are architectural directions, not shipped features.' "$english"
require_fixed 'Das sind Architekturziele, keine bereits ausgelieferten Features.' "$german"

reject_fixed 'feature-complete and locally release-ready' "$english"
reject_fixed 'has not yet been published to a public source host' "$english"

echo "Bilingual README showroom contract passed"
