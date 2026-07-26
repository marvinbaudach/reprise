#!/usr/bin/env bash
set -euo pipefail

repo_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
worker=${1:-}
if [[ -z "$worker" || ! -x "$worker" ]]; then
  echo "usage: $0 /path/to/installed/reprise-worker" >&2
  exit 2
fi
worker=$(realpath "$worker")

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
db="$scratch/reprise.db"
staging="$scratch/staging"
source_audio="$scratch/source.flac"
data_home="$scratch/data"
cache_home="$scratch/cache"
cp "$repo_root/crates/reprise-cli/tests/fixtures/sine.flac" "$source_audio"

run_worker() {
  env \
    XDG_DATA_HOME="$data_home" \
    XDG_CACHE_HOME="$cache_home" \
    "$worker" --db "$db" --staging-dir "$staging" --json "$@"
}

# Opening any command creates and migrates the isolated database through the
# packaged binary itself. Python only arranges one source row afterward.
run_worker jobs status >/dev/null
python3 - "$db" "$source_audio" <<'PY'
import sqlite3
import sys

db, source = sys.argv[1:]
with sqlite3.connect(db) as conn:
    conn.execute(
        """
        INSERT INTO tracks
            (id, path, title, artist, album, album_artist, genre,
             duration_ms, added_at, file_mtime, file_size)
        VALUES
            (1, ?, 'Packaged fixture', 'Reprise QA', 'Acceptance',
             'Reprise QA', 'Test', 1000, 1, 1, 1)
        """,
        (source,),
    )
PY

create_json=$(run_worker instrumental create 1 --stage)
work_json=$(run_worker jobs work --once --fake-backend)
status_json=$(run_worker jobs status)

CREATE_JSON="$create_json" WORK_JSON="$work_json" STATUS_JSON="$status_json" \
  python3 - "$db" "$staging" <<'PY'
import json
import os
from pathlib import Path
import sqlite3
import sys

db = sys.argv[1]
staging = Path(sys.argv[2])
created = json.loads(os.environ["CREATE_JSON"])
worked = json.loads(os.environ["WORK_JSON"])
status = json.loads(os.environ["STATUS_JSON"])

assert created["jobs"][0]["outcome"] == "created", created
job_id = created["jobs"][0]["job_id"]
assert worked["processed"] == 1 and worked["done"] == 1, worked
assert len(status) == 1 and status[0]["id"] == job_id, status
assert status[0]["state"] == "done", status
assert status[0]["progress_permille"] == 1000, status
assert status[0]["result_track_id"] is None, status
assert (staging / f"job-{job_id}.flac").is_file(), "staged render is missing"

with sqlite3.connect(db) as conn:
    events = conn.execute(
        """
        SELECT op FROM change_log
        WHERE entity = 'ai_job' AND entity_id = ?
          AND op IN ('start', 'done')
        ORDER BY id
        """,
        (str(job_id),),
    ).fetchall()
assert events == [("start",), ("done",)], events
PY

echo "Packaged instrumental E2E passed"
