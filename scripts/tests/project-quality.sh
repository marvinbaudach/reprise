#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
gate="$repo_root/scripts/check-project-quality.sh"
fixture=$(mktemp -d)
trap 'rm -rf "$fixture"' EXIT

log="$fixture/commands.log"
mkdir -p "$fixture/bin"

cat >"$fixture/bin/npm" <<'EOF'
#!/usr/bin/env bash
printf 'npm %s\n' "$*" >> "$PROJECT_QUALITY_TEST_LOG"
if [[ ${PROJECT_QUALITY_FAIL_ON:-} == "$*" ]]; then
  exit 9
fi
EOF
cat >"$fixture/bin/node" <<'EOF'
#!/usr/bin/env bash
printf 'node %s\n' "$*" >> "$PROJECT_QUALITY_TEST_LOG"
EOF
cat >"$fixture/bin/uvx" <<'EOF'
#!/usr/bin/env bash
exit 0
EOF
chmod +x "$fixture/bin/npm" "$fixture/bin/node" "$fixture/bin/uvx"

run_gate() {
  : >"$log"
  PROJECT_QUALITY_TEST_LOG="$log" \
    PATH="$fixture/bin:/usr/bin:/bin" \
    "$gate" "$@"
}

run_gate
diff -u - "$log" <<'EOF'
npm --prefix quality ci
npm --prefix quality run lint
npm --prefix quality test
npm --prefix showroom ci
npm --prefix showroom run lint
npm --prefix showroom run typecheck
node --test showroom/tests/lint-contract.test.mjs
npm --prefix showroom test
npm --prefix android run lint
npm --prefix android run test:lint
EOF

run_gate --project
diff -u - "$log" <<'EOF'
npm --prefix quality ci
npm --prefix quality run lint
npm --prefix quality test
EOF

run_gate --showroom --android
diff -u - "$log" <<'EOF'
npm --prefix showroom ci
npm --prefix showroom run lint
npm --prefix showroom run typecheck
node --test showroom/tests/lint-contract.test.mjs
npm --prefix showroom test
npm --prefix android run lint
npm --prefix android run test:lint
EOF

if run_gate --unknown 2>/dev/null; then
  echo "project-quality gate accepted an unknown selection" >&2
  exit 1
fi
if [[ -s "$log" ]]; then
  echo "project-quality gate ran commands for an invalid selection" >&2
  exit 1
fi

if PROJECT_QUALITY_FAIL_ON='--prefix showroom run lint' run_gate --showroom --android; then
  echo "project-quality gate ignored a failing Showroom lint" >&2
  exit 1
fi
diff -u - "$log" <<'EOF'
npm --prefix showroom ci
npm --prefix showroom run lint
EOF

echo "Project quality gate contract passed"
