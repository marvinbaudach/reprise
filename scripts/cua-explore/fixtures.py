#!/usr/bin/env python3
"""Create bounded, disposable library profiles for exploratory CUA runs."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import sqlite3
import subprocess
import sys
from dataclasses import asdict, dataclass


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
SOURCE_FIXTURE = REPO_ROOT / "crates" / "reprise-core" / "tests" / "fixtures" / "sine.flac"
SCRATCH_PREFIX = "reprise-cua-explore-"
CACHE_SCRATCH_BASE = pathlib.Path.home() / ".cache" / "reprise-scratch"
WORKTREE_SCRATCH_BASE = REPO_ROOT / ".worktrees" / "cua-explore-scratch"


class FixtureError(ValueError):
    """A fixture request could touch data outside the disposable boundary."""


@dataclass(frozen=True)
class FixturePlan:
    profile: str
    track_count: int
    writable_track_count: int
    generated_metadata_only: bool
    all_paths_disposable: bool
    metadata_dimensions: tuple[str, ...]
    podcast_episode_count: int = 0
    radio_station_count: int = 0


PLANS = {
    "empty": FixturePlan("empty", 0, 0, True, True, ()),
    "mixed-128": FixturePlan(
        "mixed-128", 128, 128, True, True,
        ("title", "artist", "album", "genre", "year", "rating"),
    ),
    "mixed-sources-128": FixturePlan(
        "mixed-sources-128", 128, 128, True, True,
        ("title", "artist", "album", "genre", "year", "rating"),
        podcast_episode_count=1,
        radio_station_count=1,
    ),
    "writable-512": FixturePlan(
        "writable-512", 512, 512, True, True,
        ("title", "artist", "album", "genre", "year", "rating"),
    ),
    "stress-10k": FixturePlan(
        "stress-10k", 10_000, 256, True, True,
        ("title", "artist", "album", "genre", "year", "rating"),
    ),
    "stress-100k": FixturePlan(
        "stress-100k", 100_000, 512, True, True,
        ("title", "artist", "album", "genre", "year", "rating"),
    ),
}


def build_plan(profile: str) -> FixturePlan:
    try:
        return PLANS[profile]
    except KeyError as error:
        raise FixtureError(f"unknown fixture profile: {profile}") from error


def _is_within(path: pathlib.Path, parent: pathlib.Path) -> bool:
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def approved_scratch_bases() -> tuple[pathlib.Path, ...]:
    """Disk-backed parents approved for large generated profiles."""
    return tuple(
        base.expanduser().resolve(strict=False)
        for base in (CACHE_SCRATCH_BASE, WORKTREE_SCRATCH_BASE)
    )


def validate_scratch_base(path: pathlib.Path | str) -> pathlib.Path:
    """Accept only one of the exact non-RAM scratch parents."""
    base = pathlib.Path(path).expanduser().resolve(strict=False)
    if base not in approved_scratch_bases():
        raise FixtureError(
            "scratch base is protected; use an approved disk-backed Reprise scratch parent"
        )
    return base


def validate_scratch_root(path: pathlib.Path | str) -> pathlib.Path:
    root = pathlib.Path(path).expanduser().resolve(strict=False)
    if root.exists():
        raise FixtureError(f"scratch root already exists: {root}")
    if not any(root != base and _is_within(root, base) for base in approved_scratch_bases()):
        raise FixtureError(
            "scratch root is protected; use an approved disk-backed Reprise scratch parent"
        )
    if not root.name.startswith(SCRATCH_PREFIX):
        raise FixtureError(f"scratch root name must start with {SCRATCH_PREFIX}")
    return root


def _seed_database(seed_binary: pathlib.Path, db_path: pathlib.Path, count: int) -> dict:
    binary = seed_binary.expanduser().resolve()
    if not binary.is_file() or not os.access(binary, os.X_OK):
        raise FixtureError(f"scalability seed binary is not executable: {binary}")
    completed = subprocess.run(
        [
            str(binary),
            "--db",
            str(db_path),
            "--tracks",
            str(count),
            "--iterations",
            "1",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise FixtureError(
            "scalability seed failed: " + completed.stderr.strip()[:400]
        )
    try:
        report = json.loads(completed.stdout)
    except json.JSONDecodeError as error:
        raise FixtureError("scalability seed returned invalid JSON") from error
    if report.get("generated_tracks") != count:
        raise FixtureError("scalability seed returned the wrong track count")
    return report


def _write_disposable_tracks(
    conn: sqlite3.Connection, music_root: pathlib.Path, count: int, profile: str
) -> None:
    if not SOURCE_FIXTURE.is_file():
        raise FixtureError(f"committed audio fixture is missing: {SOURCE_FIXTURE}")
    music_root.mkdir(parents=True)
    updates = []
    for index in range(count):
        path = music_root / f"Writable Batch {index + 1:04}.flac"
        shutil.copyfile(SOURCE_FIXTURE, path)
        updates.append(
            (
                str(path),
                f"Writable Batch {index + 1:04}",
                f"Fixture Artist {index % 32:02}",
                f"Fixture Album {index % 64:02}",
                f"Fixture Artist {index % 32:02}",
                f"Fixture Genre {index % 8:02}",
                1980 + index % 45,
                index % 6,
                index + 1,
            )
        )
    conn.executemany(
        """
        UPDATE tracks
        SET path = ?, title = ?, artist = ?, album = ?, album_artist = ?,
            genre = ?, year = ?, rating = ?
        WHERE id = ?
        """,
        updates,
    )
    changed = conn.execute(
        "SELECT COUNT(*) FROM tracks WHERE path LIKE ?",
        (str(music_root / "Writable Batch %.flac"),),
    ).fetchone()[0]
    if changed != count:
        raise FixtureError(
            f"writable overlay changed {changed} rows, expected {count} for {profile}"
        )


def _sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _seed_source_rows(conn: sqlite3.Connection) -> None:
    for key in (
        "online-sources-enabled",
        "online_sources.first_enable_completed",
        "module.podcasts.enabled",
        "module.radio.enabled",
    ):
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?, '1') "
            "ON CONFLICT(key) DO UPDATE SET value = '1'",
            (key,),
        )
    conn.execute(
        """
        INSERT INTO podcast_subscriptions
            (id, kind, feed_url, title, author, added_at)
        VALUES (1, 'rss', 'https://fixture.invalid/feed',
                'Fixture Podcast', 'Fixture Author', 1)
        """
    )
    conn.execute(
        """
        INSERT INTO podcast_episodes
            (id, subscription_id, guid, title, audio_url,
             published_at, duration_secs, first_seen_at)
        VALUES (1, 1, 'fixture-podcast-needle', 'Fixture Podcast Needle',
                'https://fixture.invalid/episode.flac', 2, 60, 2)
        """
    )
    conn.execute(
        """
        INSERT INTO radio_stations
            (id, uuid, name, stream_url, genre, added_at)
        VALUES (1, 'fixture-radio-needle', 'Fixture Radio Needle',
                'https://fixture.invalid/radio', 'Fixture Genre', 1)
        """
    )


def audit_batch_edit(
    profile_root: pathlib.Path,
    workload: dict | object,
    fixture_tokens: dict | object,
) -> dict:
    """Verify the stress edit in both the private database and audio copies."""
    if not isinstance(workload, dict) or not isinstance(fixture_tokens, dict):
        raise FixtureError("batch audit requires workload and fixture-token objects")
    manifest_path = profile_root / "fixture-manifest.json"
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        expected = int(workload["selection_count"])
        field_tokens = workload["field_tokens"]
        genre = str(fixture_tokens[field_tokens["genre"]])
        year = int(fixture_tokens[field_tokens["year"]])
        baseline_sha = str(manifest["writable_audio_sha256"])
    except (OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as error:
        raise FixtureError(f"batch audit contract is incomplete: {error}") from error
    if manifest.get("writable_track_count") != expected:
        raise FixtureError("batch audit count differs from the fixture manifest")

    db_path = profile_root / "data" / "reprise" / "reprise.db"
    music_root = profile_root / "music"
    with sqlite3.connect(db_path) as conn:
        database_rows_updated = conn.execute(
            """
            SELECT COUNT(*) FROM tracks
            WHERE title LIKE 'Writable Batch %' AND genre = ? AND year = ?
            """,
            (genre, year),
        ).fetchone()[0]
        all_matching_rows = conn.execute(
            "SELECT COUNT(*) FROM tracks WHERE genre = ? AND year = ?",
            (genre, year),
        ).fetchone()[0]
    audio_files = sorted(music_root.glob("Writable Batch *.flac"))
    audio_files_changed = sum(_sha256(path) != baseline_sha for path in audio_files)
    complete = (
        database_rows_updated == expected
        and all_matching_rows == expected
        and len(audio_files) == expected
        and audio_files_changed == expected
    )
    return {
        "kind": "batch-edit",
        "expected": expected,
        "database_rows_updated": database_rows_updated,
        "database_rows_with_values": all_matching_rows,
        "audio_files_found": len(audio_files),
        "audio_files_changed": audio_files_changed,
        "complete": complete,
    }


def prepare_profile(
    profile: str, root_path: pathlib.Path | str, seed_binary: pathlib.Path | str | None
) -> pathlib.Path:
    plan = build_plan(profile)
    root = validate_scratch_root(root_path)
    data_root = root / "data"
    db_root = data_root / "reprise"
    cache_root = root / "cache"
    config_root = root / "config"
    music_root = root / "music"
    db_root.mkdir(parents=True)
    cache_root.mkdir()
    config_root.mkdir()

    seed_report = None
    if plan.track_count:
        if seed_binary is None:
            raise FixtureError("non-empty profiles require --seed-binary")
        db_path = db_root / "reprise.db"
        seed_report = _seed_database(pathlib.Path(seed_binary), db_path, plan.track_count)
        with sqlite3.connect(db_path) as conn:
            _write_disposable_tracks(
                conn, music_root, plan.writable_track_count, profile
            )
            if plan.podcast_episode_count or plan.radio_station_count:
                _seed_source_rows(conn)
            conn.commit()

    manifest = {
        "schema_version": 1,
        **asdict(plan),
        "private_xdg": True,
        "real_library_access": False,
        "writable_audio_bytes": (
            SOURCE_FIXTURE.stat().st_size * plan.writable_track_count
            if plan.writable_track_count
            else 0
        ),
        "writable_audio_sha256": (
            _sha256(SOURCE_FIXTURE) if plan.writable_track_count else None
        ),
    }
    (root / "fixture-manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    if seed_report is not None:
        (root / "seed-report.json").write_text(
            json.dumps(seed_report, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    return root


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    plan_parser = subparsers.add_parser("plan")
    plan_parser.add_argument("profile", choices=sorted(PLANS))
    validate_base_parser = subparsers.add_parser("validate-base")
    validate_base_parser.add_argument("base", type=pathlib.Path)
    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("profile", choices=sorted(PLANS))
    prepare_parser.add_argument("root", type=pathlib.Path)
    prepare_parser.add_argument("--seed-binary", type=pathlib.Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "plan":
            print(json.dumps(asdict(build_plan(args.profile)), sort_keys=True))
        elif args.command == "validate-base":
            print(validate_scratch_base(args.base))
        else:
            root = prepare_profile(args.profile, args.root, args.seed_binary)
            print(root)
        return 0
    except FixtureError as error:
        print(f"fixture rejected: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
