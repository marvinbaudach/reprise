#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
android_root="$repo_root/android"
result_dir="$android_root/app/build/test-results/testDebugUnitTest"
gradle_bin=${REPRISE_GRADLE_BIN:-./gradlew}
marker=$(mktemp)
trap 'rm -f "$marker"' EXIT

test_suites=(
  de.reprise.spike.scene.SceneStateTest
  de.reprise.spike.CoverFogBitmapTest
  de.reprise.spike.NowPlayingFogTest
  de.reprise.spike.NowPlayingBurstTest
  de.reprise.spike.NowPlayingBurstPixelsTest
  de.reprise.spike.SceneDriverTest
  de.reprise.spike.ScenePowerGateTest
  de.reprise.spike.MainActivityVisualizerTest
  de.reprise.spike.NowPlayingSceneVerificationTest
)

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

if [[ $xml_files -ne 9 || $tests -ne 29 || $failures -ne 0 || $errors -ne 0 || $skipped -ne 0 ]]; then
  echo "unexpected Now Playing verification totals: suites=$xml_files tests=$tests failures=$failures errors=$errors skipped=$skipped" >&2
  exit 1
fi

echo "Now Playing verification passed: 9 fresh suites, 29 tests, 0 failures."
