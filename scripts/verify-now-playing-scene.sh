#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
android_root="$repo_root/android"
result_dir="$android_root/app/build/test-results/testDebugUnitTest"
gradle_bin=${REPRISE_GRADLE_BIN:-./gradlew}
marker=$(mktemp)
trap 'rm -f "$marker"' EXIT

test_suites=(
  io.github.marvinbaudach.reprise.scene.SceneStateTest
  io.github.marvinbaudach.reprise.scene.BandEnvelopeTest
  io.github.marvinbaudach.reprise.scene.SceneColourTest
  io.github.marvinbaudach.reprise.CoverFogBitmapTest
  io.github.marvinbaudach.reprise.NowPlayingFogTest
  io.github.marvinbaudach.reprise.NowPlayingLegibilityTest
  io.github.marvinbaudach.reprise.SceneDriverTest
  io.github.marvinbaudach.reprise.ScenePowerGateTest
  io.github.marvinbaudach.reprise.NowPlayingSceneVerificationTest
)

# The expected totals are read off the suite list and its sources, never typed
# out a second time: a suite added above must not need a number edited below,
# and a suite whose tests silently stop being run must not still add up.
expected_suites=${#test_suites[@]}
expected_tests=0
for suite in "${test_suites[@]}"; do
  source_file="$android_root/app/src/test/java/${suite//./\/}.kt"
  if [[ ! -f $source_file ]]; then
    echo "missing test source: $suite" >&2
    exit 1
  fi
  suite_tests=$(grep -c '^[[:space:]]*@Test' "$source_file" || true)
  if [[ $suite_tests -eq 0 ]]; then
    echo "no @Test found in: $source_file" >&2
    exit 1
  fi
  expected_tests=$((expected_tests + suite_tests))
done

gradle_args=(
  --max-workers=2
  -Pkotlin.compiler.execution.strategy=in-process
  :app:cleanTestDebugUnitTest
  :app:testDebugUnitTest
)
for suite in "${test_suites[@]}"; do
  gradle_args+=(--tests "$suite")
done

(
  cd "$android_root"
  "$gradle_bin" "${gradle_args[@]}"
)

for suite in "${test_suites[@]}"; do
  result="$result_dir/TEST-$suite.xml"
  if [[ ! -f $result ]]; then
    echo "missing fresh test result: $suite" >&2
    exit 1
  fi
  if [[ ! $result -nt $marker ]]; then
    echo "stale test result: $suite" >&2
    exit 1
  fi
done

read -r xml_files tests failures errors skipped < <(
  awk '
    BEGIN { files = ARGC - 1; tests = failures = errors = skipped = 0 }
    /<testsuite / {
      for (i = 1; i <= NF; i++) {
        value = $i
        gsub(/[^0-9]/, "", value)
        if ($i ~ /^tests=/) tests += value
        else if ($i ~ /^failures=/) failures += value
        else if ($i ~ /^errors=/) errors += value
        else if ($i ~ /^skipped=/) skipped += value
      }
    }
    END { print files, tests, failures, errors, skipped }
  ' "$result_dir"/TEST-*.xml
)

if [[
  $xml_files -ne $expected_suites || $tests -ne $expected_tests ||
  $failures -ne 0 || $errors -ne 0 || $skipped -ne 0
]]; then
  echo "unexpected Now Playing verification totals: suites=$xml_files/$expected_suites tests=$tests/$expected_tests failures=$failures errors=$errors skipped=$skipped" >&2
  exit 1
fi

echo "Now Playing verification passed: $xml_files fresh suites, $tests tests, 0 failures."
