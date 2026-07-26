#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$repo_root"

expected_msrv=1.92
metadata=$(cargo metadata --offline --locked --no-deps --format-version 1)

if ! jq -e --arg expected "$expected_msrv" '
  . as $metadata
  | [$metadata.packages[]
      | select(.id as $id | $metadata.workspace_members | index($id))] as $members
  | ($members | length > 0)
    and all($members[]; .rust_version == $expected)
' <<<"$metadata" >/dev/null; then
  echo "every workspace package must declare Rust $expected_msrv as its MSRV" >&2
  jq -r '
    . as $metadata
    | $metadata.packages[]
    | select(.id as $id | $metadata.workspace_members | index($id))
    | "\(.name): \(.rust_version // "missing")"
  ' <<<"$metadata" >&2
  exit 1
fi

echo "Workspace MSRV contract passed (Rust $expected_msrv)"
