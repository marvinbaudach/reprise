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

expect_undeclared_copy_to_fail() {
  local name=$1
  local source=$2
  local relative_path=${3:-crates/reprise-core/src/attack.rs}
  local fixture
  local status
  fixture=$(new_fixture)
  trap 'rm -rf "$fixture"' RETURN

  mkdir -p "$fixture/$(dirname "$relative_path")"
  printf '%s\n' "$source" > "$fixture/$relative_path"
  set +e
  "$fixture/scripts/check-shared-literals.sh" > "$fixture/output" 2>&1
  status=$?
  set -e
  if (( status == 0 )); then
    echo "shared literal filtering: $name hid an undeclared copy" >&2
    return 1
  fi
  grep -Fq "a new copy appeared in $relative_path" "$fixture/output"
  printf 'shared literal filtering: %s gate exit: %s\n' "$name" "$status"
}

expect_declared_rename_to_fail() {
  local fixture
  local status
  fixture=$(new_fixture)
  trap 'rm -rf "$fixture"' RETURN

  sed -i \
    's/reprise-listens-back-ack\.rpl/reprise-listens-back-renamed.rpl/' \
    "$fixture/crates/reprise-core/src/device_sync/listen_report.rs"

  set +e
  "$fixture/scripts/check-shared-literals.sh" > "$fixture/output" 2>&1
  status=$?
  set -e
  if (( status == 0 )); then
    echo "shared literal filtering: a one-sided rename passed" >&2
    return 1
  fi
  grep -Fq "renamed on one side only?" "$fixture/output"
  printf 'shared literal filtering: one-sided-rename gate exit: %s\n' "$status"
}

expect_whole_line_comments_to_pass() {
  local fixture
  local status
  fixture=$(new_fixture)
  trap 'rm -rf "$fixture"' RETURN

  printf '%s\n' \
    '// reprise-listens-back.rpl is named in prose.' \
    '/* org.mpris.MediaPlayer2.reprise is named in prose. */' \
    ' * /org/mpris/MediaPlayer2 is named in prose.' \
    > "$fixture/crates/reprise-core/src/comments.rs"

  set +e
  "$fixture/scripts/check-shared-literals.sh" > "$fixture/output" 2>&1
  status=$?
  set -e
  if (( status != 0 )); then
    cat "$fixture/output" >&2
    echo "shared literal filtering: whole-line comments were treated as code" >&2
    return 1
  fi
  printf 'shared literal filtering: whole-line-comments gate exit: %s\n' "$status"
}

expect_undeclared_copy_to_fail plain-copy \
  'const COPY: &str = "https://example.test/reprise-listens-back.rpl";'
expect_undeclared_copy_to_fail rust-raw-string $'const TRAP: &str = r"trap\\";\nconst COPY: &str = "https://example.test/reprise-listens-back.rpl";'
expect_undeclared_copy_to_fail rust-quote-char $'const TRAP: char = '\''"'\'';\nconst COPY: &str = "https://example.test/reprise-listens-back.rpl";'
expect_undeclared_copy_to_fail kotlin-triple-string $'val trap = """trap " here"""\nval copy = "https://example.test/reprise-listens-back.rpl"' android/app/src/main/java/io/github/marvinbaudach/reprise/Attack.kt
expect_undeclared_copy_to_fail block-close-before-construct $'/* open block\n*/ val trap = """closed"""\nval copy = "https://example.test/reprise-listens-back.rpl"' android/app/src/main/java/io/github/marvinbaudach/reprise/Attack.kt
expect_undeclared_copy_to_fail opaque-line-opens-multiline-string $'val quote = '\''"'\''; val trap = """\ninside // https://example.test/reprise-listens-back.rpl\n"""' android/app/src/main/java/io/github/marvinbaudach/reprise/Attack.kt
expect_undeclared_copy_to_fail ordinary-multiline-string-with-quote-token $'const TRAP: &str = "ordinary\ncontent '\''"'\''\n"; prefix // https://example.test/reprise-listens-back.rpl'
expect_declared_rename_to_fail
expect_whole_line_comments_to_pass

echo "shared literal filtering: live lines stay visible and whole-line comments stay ignored"
