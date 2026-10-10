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
# The playlist the mixed-sources profile carries, named by the hover mission's
# PLAYLIST_NAME fixture token. Four tracks: one more than the three the sweep needs.
FIXTURE_PLAYLIST_NAME = "Fixture Playlist"
FIXTURE_PLAYLIST_TRACKS = 4


class FixtureError(ValueError):
    """A fixture request could touch data outside the disposable boundary."""


@dataclass(frozen=True)
class FixtureTrack:
    """One writable track, as the database row and the FLAC tags must both state it."""

    title: str
    artist: str
    album: str
    album_artist: str
    genre: str
    year: int
    track_no: int
    rating: int

    def vorbis_tags(self) -> tuple[str, ...]:
        return (
            f"TITLE={self.title}",
            f"ARTIST={self.artist}",
            f"ALBUM={self.album}",
            f"ALBUMARTIST={self.album_artist}",
            f"GENRE={self.genre}",
            f"DATE={self.year}",
            f"TRACKNUMBER={self.track_no}",
        )


def fixture_track(index: int) -> FixtureTrack:
    """The values of writable track `index`; the only place they are decided."""
    return FixtureTrack(
        title=f"Writable Batch {index + 1:04}",
        artist=f"Fixture Artist {index % 32:02}",
        album=f"Fixture Album {index % 64:02}",
        album_artist=f"Fixture Artist {index % 32:02}",
        genre=f"Fixture Genre {index % 8:02}",
        year=1980 + index % 45,
        track_no=index % 12 + 1,
        rating=index % 6,
    )


def _require_metaflac() -> str:
    binary = shutil.which("metaflac")
    if binary is None:
        raise FixtureError(
            "metaflac is required to tag the writable audio fixtures; "
            "install the flac package"
        )
    return binary


def _flac_stream_info(metaflac: str, path: pathlib.Path, flag: str) -> int:
    completed = subprocess.run(
        [metaflac, flag, str(path)], check=False, capture_output=True, text=True
    )
    try:
        return int(completed.stdout.strip())
    except ValueError as error:
        raise FixtureError(
            f"metaflac {flag} failed for {path.name}: {completed.stderr.strip()[:200]}"
        ) from error


def _source_duration_ms(metaflac: str) -> int:
    samples = _flac_stream_info(metaflac, SOURCE_FIXTURE, "--show-total-samples")
    rate = _flac_stream_info(metaflac, SOURCE_FIXTURE, "--show-sample-rate")
    # metaflac prints 0 when the stream does not record its length, and a zero
    # here would seed every database row with a silent 0 ms duration.
    if samples <= 0:
        raise FixtureError("committed audio fixture reports no total samples")
    if rate <= 0:
        raise FixtureError("committed audio fixture reports no sample rate")
    return samples * 1000 // rate


def _tag_flac(metaflac: str, path: pathlib.Path, track: FixtureTrack) -> None:
    """Replace every tag of the copy in place; the audio frames are not re-encoded."""
    completed = subprocess.run(
        [
            metaflac,
            "--remove-all-tags",
            *(f"--set-tag={tag}" for tag in track.vorbis_tags()),
            str(path),
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    if completed.returncode != 0:
        raise FixtureError(f"metaflac could not tag {path.name}: {completed.stderr.strip()[:200]}")


@dataclass(frozen=True)
class FixturePlan:
    profile: str
    track_count: int
    writable_track_count: int
    generated_metadata_only: bool
    all_paths_disposable: bool
    metadata_dimensions: tuple[str, ...]
    podcast_episode_count: int = 0
    youtube_episode_count: int = 0
    radio_station_count: int = 0
    # Tracks of the one playlist that gives the sidebar a Playlists entry.
    playlist_track_count: int = 0


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
        youtube_episode_count=1,
        radio_station_count=1,
        playlist_track_count=FIXTURE_PLAYLIST_TRACKS,
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
    bases = approved_scratch_bases()
    worktree_base = WORKTREE_SCRATCH_BASE.expanduser().resolve(strict=False)
    # A checkout may itself live under the cache scratch base, so being inside
    # an approved base is not enough: inside the checkout only its own scratch
    # parent is allowed.
    in_checkout = _is_within(root, REPO_ROOT.resolve()) and not _is_within(root, worktree_base)
    if in_checkout or not any(root != base and _is_within(root, base) for base in bases):
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
    metaflac = _require_metaflac()
    duration_ms = _source_duration_ms(metaflac)
    music_root.mkdir(parents=True)
    updates = []
    for index in range(count):
        path = music_root / f"Writable Batch {index + 1:04}.flac"
        track = fixture_track(index)
        shutil.copyfile(SOURCE_FIXTURE, path)
        _tag_flac(metaflac, path, track)
        updates.append(
            (
                str(path),
                track.title,
                track.artist,
                track.album,
                track.album_artist,
                track.genre,
                track.year,
                track.track_no,
                track.rating,
                duration_ms,
                index + 1,
            )
        )
    conn.executemany(
        """
        UPDATE tracks
        SET path = ?, title = ?, artist = ?, album = ?, album_artist = ?,
            genre = ?, year = ?, track_no = ?, rating = ?, duration_ms = ?
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


def _audio_baseline(music_root: pathlib.Path) -> dict[str, str]:
    """File name to sha256 of every writable copy as it stands, tags included."""
    return {
        path.name: _sha256(path)
        for path in sorted(music_root.glob("Writable Batch *.flac"))
    }


def _audio_bytes(music_root: pathlib.Path) -> int:
    """Combined size of every writable copy as it stands; tagged copies outgrow the source."""
    return sum(path.stat().st_size for path in music_root.glob("Writable Batch *.flac"))


def _seed_source_rows(conn: sqlite3.Connection) -> None:
    for key in (
        "online-sources-enabled",
        "online_sources.first_enable_completed",
        "module.podcasts.enabled",
        "module.youtube.enabled",
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
        INSERT INTO podcast_subscriptions
            (id, kind, feed_url, title, author, added_at)
        VALUES (2, 'youtube', 'https://fixture.invalid/channel',
                'Fixture Channel', 'Fixture Author', 1)
        """
    )
    conn.execute(
        """
        INSERT INTO podcast_episodes
            (id, subscription_id, guid, title, audio_url,
             published_at, duration_secs, first_seen_at)
        VALUES (2, 2, 'fixture-youtube-needle', 'Fixture YouTube Needle',
                'https://fixture.invalid/video.flac', 2, 60, 2)
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


def _seed_playlist(conn: sqlite3.Connection, track_count: int) -> None:
    """One playlist over the first tracks, so the sidebar has a Playlists entry.

    The sidebar shows a Playlists heading and a new-playlist button even when no
    playlist exists, but neither is an accessible target, so the section can only
    be visited through the playlist that lives under it.
    """
    conn.execute(
        "INSERT INTO playlists (id, name, position) VALUES (1, ?, 0)",
        (FIXTURE_PLAYLIST_NAME,),
    )
    conn.executemany(
        "INSERT INTO playlist_tracks (playlist_id, track_id, position) VALUES (1, ?, ?)",
        [(track_id, track_id - 1) for track_id in range(1, track_count + 1)],
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
        baseline_by_file = dict(manifest["writable_audio_sha256_by_file"])
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
    # A copy the manifest never saw cannot be compared, but it is not unchanged
    # either: it is reported, and it keeps the audit from claiming completeness.
    unbaselined = [path.name for path in audio_files if path.name not in baseline_by_file]
    audio_files_changed = sum(
        path.name not in baseline_by_file or _sha256(path) != baseline_by_file[path.name]
        for path in audio_files
    )
    complete = (
        database_rows_updated == expected
        and all_matching_rows == expected
        and len(audio_files) == expected
        and audio_files_changed == expected
        and not unbaselined
    )
    return {
        "kind": "batch-edit",
        "expected": expected,
        "database_rows_updated": database_rows_updated,
        "database_rows_with_values": all_matching_rows,
        "audio_files_found": len(audio_files),
        "audio_files_changed": audio_files_changed,
        "audio_files_without_baseline": unbaselined,
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
            if (
                plan.podcast_episode_count
                or plan.youtube_episode_count
                or plan.radio_station_count
            ):
                _seed_source_rows(conn)
            if plan.playlist_track_count:
                _seed_playlist(conn, plan.playlist_track_count)
            conn.commit()

    manifest = {
        "schema_version": 1,
        **asdict(plan),
        "private_xdg": True,
        "real_library_access": False,
        "writable_audio_bytes": (
            _audio_bytes(music_root) if plan.writable_track_count else 0
        ),
        "writable_audio_sha256_by_file": (
            _audio_baseline(music_root) if plan.writable_track_count else {}
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
