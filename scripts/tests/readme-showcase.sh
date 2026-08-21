#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

english=README.md
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

[[ -f $english ]] || fail "$english must exist"
[[ ! -e README.de.md ]] || fail "README.de.md must stay removed; the developer README is English only"
(( $(wc -l < "$english") <= 215 )) || fail "$english must remain a concise developer entry point"
[[ $(rg -c 'docs/assets/reprise-architecture\.svg' "$english") -eq 1 ]] ||
  fail "$english must contain exactly one architecture visual"
reject_fixed '```mermaid' "$english"
reject_fixed 'docs/assets/reprise-performance.svg' "$english"
reject_fixed 'Rust code lines' "$english"
reject_fixed 'Rust-Codezeilen' "$english"
reject_fixed 'README.de.md' "$english"

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

for value in \
  '## Downloads' \
  '## Architecture' \
  '## Engineering contracts' \
  '## Contributing' \
  '## Build and run' \
  '## Verification' \
  '## Documentation' \
  '## License'; do
  require_fixed "$value" "$english"
done

require_fixed 'Pick your entry point' "$english"
[[ $(rg -c 'https://github\.com/marvinbaudach/reprise/releases/latest' "$english") -eq 2 ]] ||
  fail "$english must link both downloadable applications to the latest release"
require_fixed 'flatpak install --user ./Reprise-<desktop-version>.flatpak' "$english"
require_fixed 'adb install -r ./Reprise-Android-<android-version>.apk' "$english"

for crate in reprise-core reprise-platform-linux reprise-gnome; do
  require_fixed "$crate" "$english"
done
require_fixed 'cargo build --locked --workspace' "$english"
require_fixed 'cargo test --locked --workspace' "$english"
require_fixed 'scripts/check-merge-readiness.sh --no-fetch' "$english"
require_fixed 'docs/showcase.md' "$english"
require_fixed 'docs/ux-rules.md' "$english"
require_fixed 'TESTING.md' "$english"

reject_fixed '## Product surface today' "$english"
reject_fixed "## Roadmap: the same core beyond today’s player" "$english"

echo "English developer README contract passed"
