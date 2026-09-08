#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)

new_fixture() {
  local fixture
  fixture=$(mktemp -d)
  mkdir -p "$fixture/scripts/lib"
  cp "$repo_root/scripts/check-shared-literals.sh" "$fixture/scripts/"
  if [[ -f $repo_root/scripts/lib/source_code.py ]]; then
    cp "$repo_root/scripts/lib/source_code.py" "$fixture/scripts/lib/"
  fi

  mkdir -p \
    "$fixture/crates/reprise-core/src/device_sync" \
    "$fixture/crates/reprise-core/src" \
    "$fixture/crates/reprise-platform-linux/src" \
    "$fixture/crates/reprise-runtime-protocol/src" \
    "$fixture/crates/reprise-mcp/tests" \
    "$fixture/crates/reprise-cli/tests" \
    "$fixture/android/app/src/main/java/io/github/marvinbaudach/reprise"

  printf '%s\n' \
    'const REPORT: &str = "reprise-listens-back.rpl";' \
    'const ACK: &str = "reprise-listens-back-ack.rpl";' \
    > "$fixture/crates/reprise-core/src/device_sync/listen_report.rs"
  printf '%s\n' \
    'const REPORT: &str = "reprise-listens-back.rpl";' \
    'const ACK: &str = "reprise-listens-back-ack.rpl";' \
    > "$fixture/crates/reprise-core/src/device_sync/mirror_tests.rs"
  printf '%s\n' 'const REPORT: &str = "reprise-listens-back.rpl";' \
    > "$fixture/crates/reprise-platform-linux/src/device_sync_tests.rs"
  printf '%s\n' \
    'const val REPORT = "reprise-listens-back.rpl"' \
    'const val ACK = "reprise-listens-back-ack.rpl"' \
    > "$fixture/android/app/src/main/java/io/github/marvinbaudach/reprise/ListenReportWriter.kt"
  printf '%s\n' \
    'const BUS: &str = "org.mpris.MediaPlayer2.reprise";' \
    'const PATH: &str = "/org/mpris/MediaPlayer2";' \
    > "$fixture/crates/reprise-runtime-protocol/src/mpris.rs"
  printf '%s\n' \
    'const BUS: &str = "org.mpris.MediaPlayer2.reprise";' \
    'const PATH: &str = "/org/mpris/MediaPlayer2";' \
    > "$fixture/crates/reprise-mcp/tests/playback_roundtrip.rs"
  printf '%s\n' 'const BUS: &str = "org.mpris.MediaPlayer2.reprise";' \
    > "$fixture/crates/reprise-cli/tests/playback.rs"

  printf '%s\n' "$fixture"
}

expect_hidden_copy_to_fail() {
  local name=$1
  local source=$2
  local relative_path=${3:-crates/reprise-core/src/attack.rs}
  local fixture
  fixture=$(new_fixture)
  trap 'rm -rf "$fixture"' RETURN

  mkdir -p "$fixture/$(dirname "$relative_path")"
  printf '%s\n' "$source" > "$fixture/$relative_path"
  if "$fixture/scripts/check-shared-literals.sh" > "$fixture/output" 2>&1; then
    echo "shared literal comment stripping: $name hid an undeclared copy" >&2
    return 1
  fi
  grep -Fq "a new copy appeared in $relative_path" "$fixture/output"
}

expect_hidden_copy_to_fail rust-raw-string $'const TRAP: &str = r"trap\\";\nconst COPY: &str = "https://example.test/reprise-listens-back.rpl";'
expect_hidden_copy_to_fail rust-quote-char $'const TRAP: char = '\''"'\'';\nconst COPY: &str = "https://example.test/reprise-listens-back.rpl";'
expect_hidden_copy_to_fail kotlin-triple-string $'val trap = """trap " here"""\nval copy = "https://example.test/reprise-listens-back.rpl"' android/app/src/main/java/io/github/marvinbaudach/reprise/Attack.kt

echo "shared literal comment stripping: conservative fallbacks catch all regressions"
