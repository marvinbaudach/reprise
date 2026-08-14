#!/usr/bin/env bash
# Generates host UniFFI bindings, runs the Android JVM suite, and proves that
# every reported JUnit result belongs to this invocation.
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
results_dir="$repo_root/android/app/build/test-results/testDebugUnitTest"
# Measured at dd67122fc7: 66 suites executed 334 tests on the clean base commit.
readonly ANDROID_TEST_FLOOR=334

parse_results() {
  local start_time=$1
  local directory=$2

  if [[ ! "$start_time" =~ ^[0-9]+$ ]]; then
    echo "Invalid Android suite start timestamp: $start_time" >&2
    return 64
  fi
  if [[ ! -d "$directory" ]]; then
    echo "suites=0 tests=0 failures=0 errors=0 skips=0 verdict=missing"
    return 4
  fi

  local -a xml_files
  shopt -s nullglob
  xml_files=("$directory"/*.xml)
  shopt -u nullglob
  if (( ${#xml_files[@]} == 0 )); then
    echo "suites=0 tests=0 failures=0 errors=0 skips=0 verdict=empty"
    return 3
  fi

  python3 - "$start_time" "${xml_files[@]}" <<'PY'
import os
import sys
import xml.etree.ElementTree as ET


def local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1]


start_ns = int(sys.argv[1]) * 1_000_000_000
paths = sys.argv[2:]
totals = {"suites": 0, "tests": 0, "failures": 0, "errors": 0, "skips": 0}
stale = False

try:
    for path in paths:
        stale = stale or os.stat(path).st_mtime_ns < start_ns
        root = ET.parse(path).getroot()
        if local_name(root.tag) == "testsuite":
            suites = [root]
        elif local_name(root.tag) == "testsuites":
            suites = [child for child in root if local_name(child.tag) == "testsuite"]
        else:
            raise ValueError(f"unexpected JUnit root element in {path}: {root.tag}")
        if not suites:
            raise ValueError(f"no JUnit test suites in {path}")
        for suite in suites:
            totals["suites"] += 1
            totals["tests"] += int(suite.attrib["tests"])
            totals["failures"] += int(suite.attrib["failures"])
            totals["errors"] += int(suite.attrib["errors"])
            totals["skips"] += int(suite.attrib.get("skipped", "0"))
except (OSError, ET.ParseError, KeyError, ValueError) as error:
    print(f"Invalid Android JUnit results: {error}", file=sys.stderr)
    sys.exit(5)

verdict = "stale" if stale else "fresh"
print(
    f"suites={totals['suites']} tests={totals['tests']} "
    f"failures={totals['failures']} errors={totals['errors']} "
    f"skips={totals['skips']} verdict={verdict}"
)
sys.exit(2 if stale else 0)
PY
}

if [[ ${1:-} == "--parse-results" ]]; then
  if (( $# != 3 )); then
    echo "usage: $0 --parse-results START_TIMESTAMP JUNIT_DIRECTORY" >&2
    exit 64
  fi
  parse_results "$2" "$3"
  exit $?
fi
if (( $# != 0 )); then
  echo "usage: $0 [--parse-results START_TIMESTAMP JUNIT_DIRECTORY]" >&2
  exit 64
fi

sdk_root=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}
if [[ -z "$sdk_root" || ! -d "$sdk_root" ]]; then
  echo "Android SDK not found; set ANDROID_HOME or ANDROID_SDK_ROOT to an existing directory" >&2
  exit 1
fi
export ANDROID_HOME="$sdk_root"
export ANDROID_SDK_ROOT="$sdk_root"

cd "$repo_root"
cargo build --locked --release -p reprise-android-ffi
if [[ -d android/app/src/main/java/uniffi ]]; then
  rm -rf android/app/src/main/java/uniffi
fi
cargo run --locked --release --bin uniffi-bindgen -p reprise-android-ffi -- \
  generate --library target/release/libreprise_android_ffi.so \
  --language kotlin --out-dir android/app/src/main/java

rm -rf "$results_dir"
start_time=$(date +%s)
android/gradlew --project-dir android :app:testDebugUnitTest
android/gradlew --project-dir android :app:assembleDebug

set +e
summary=$(parse_results "$start_time" "$results_dir")
parse_status=$?
set -e
printf '%s\n' "$summary"
if (( parse_status != 0 )); then
  exit "$parse_status"
fi

tests=${summary#* tests=}
tests=${tests%% *}
failures=${summary#* failures=}
failures=${failures%% *}
errors=${summary#* errors=}
errors=${errors%% *}
if (( failures != 0 || errors != 0 )); then
  echo "Android unit suite failed: $failures failures and $errors errors" >&2
  exit 1
fi
if (( tests < ANDROID_TEST_FLOOR )); then
  echo "Android test floor missed: executed $tests, required at least $ANDROID_TEST_FLOOR (measured at dd67122fc7)" >&2
  exit 1
fi

echo "Android unit-suite gate passed"
