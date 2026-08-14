#!/usr/bin/env python3
"""Measures every artist portrait Deezer would serve for this library.

Produces the corpus and the numbers the plan's threshold rests on:

  * corpus   ~/.cache/reprise-portrait-corpus/<identifier>.jpg  (not in git —
             Deezer's bytes; the Rust measurement in E6 reads this directory)
  * table    library-sweep.csv next to this script
  * summary  stdout, kept as library-sweep.txt

Re-runnable: anything already in the corpus is reused, so a second run costs no
requests. This is the *exploration* — the shipped threshold is set by the Rust
measurement over the same corpus, because PIL and the image crate disagree on
both the resize kernel and the luma weights.
"""
import collections
import csv
import io
import json
import pathlib
import sqlite3
import time
import urllib.parse
import urllib.request

import numpy as np
from PIL import Image

HERE = pathlib.Path(__file__).parent
CORPUS = pathlib.Path.home() / ".cache" / "reprise-portrait-corpus"
SEARCHES = CORPUS / ".searches"
DB = "file:" + str(pathlib.Path.home() / ".local/share/reprise/reprise.db") + "?mode=ro"
CDN = "https://cdn-images.dzcdn.net/images/artist/{}/1000x1000-000000-80-0-0.jpg"
REFERENCES = {
    "empty-md5": "d41d8cd98f00b204e9800998ecf8427e",
    "oceano": "415714b66a5de709809dd3d05f58afe4",
}
SIZES = [16, 32, 64]
# Identified as a placeholder for grouping the summary only. The shipped code
# uses the threshold from the Rust run; this is a reporting convenience.
PLACEHOLDER_CUTOFF = 0.01
UA = {"User-Agent": "reprise-portrait-sweep/1.0"}

CORPUS.mkdir(parents=True, exist_ok=True)
SEARCHES.mkdir(parents=True, exist_ok=True)


def fetch(url, target, delay=0.35):
    if target.exists():
        return target.read_bytes()
    time.sleep(delay)
    request = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(request, timeout=20) as response:
        data = response.read()
    target.write_bytes(data)
    return data


def image(identifier):
    return fetch(CDN.format(identifier), CORPUS / f"{identifier}.jpg")


def search(artist):
    key = SEARCHES / (urllib.parse.quote(artist, safe="") + ".json")
    url = "https://api.deezer.com/search/artist?q=" + urllib.parse.quote(artist) + "&limit=10"
    return json.loads(fetch(url, key))


def grey(data, size):
    thumb = Image.open(io.BytesIO(data)).convert("L").resize((size, size), Image.LANCZOS)
    return np.asarray(thumb, dtype=np.float64) / 255.0


def rmse(a, b):
    return float(np.sqrt(np.mean((a - b) ** 2)))


def normalize(value):
    """Mirrors artist_portrait::normalize — whitespace collapse plus lowercase."""
    return " ".join(value.split()).lower()


references = {size: [grey(image(i), size) for i in REFERENCES.values()] for size in SIZES}
print("reference-to-reference distance (the two known drawings):")
for size in SIZES:
    print(f"  {size}x{size}: {rmse(*references[size]):.5f}")

with sqlite3.connect(DB, uri=True) as db:
    artists = sorted(row[0] for row in db.execute(
        "select distinct artist from tracks where artist is not null and trim(artist) <> ''"
    ))
print(f"\nartists in library: {len(artists)}", flush=True)

rows, failures, without_candidate = [], [], []
for index, artist in enumerate(artists, 1):
    try:
        payload = search(artist)
    except Exception as exc:  # noqa: BLE001 - a probe, not production code
        failures.append((artist, f"search: {exc}"))
        continue
    wanted = normalize(artist)
    matched = 0
    for candidate in payload.get("data", []):
        if normalize(candidate.get("name", "")) != wanted:
            continue
        picture = candidate.get("picture_xl") or candidate.get("picture_big") or ""
        segments = [s for s in urllib.parse.urlparse(picture).path.split("/") if s]
        # `/images/artist//1000x1000-…` — the structurally empty identifier.
        identifier = segments[2] if len(segments) > 3 else ""
        matched += 1
        if len(identifier) != 32:
            rows.append({"artist": artist, "identifier": "(empty)",
                         "fans": candidate.get("nb_fan", 0),
                         **{size: 0.0 for size in SIZES}})
            continue
        try:
            data = image(identifier)
        except Exception as exc:  # noqa: BLE001
            failures.append((artist, f"image {identifier}: {exc}"))
            continue
        rows.append({"artist": artist, "identifier": identifier,
                     "fans": candidate.get("nb_fan", 0),
                     **{size: min(rmse(grey(data, size), r) for r in references[size])
                        for size in SIZES}})
    if matched == 0:
        without_candidate.append(artist)
    if index % 25 == 0:
        print(f"  {index}/{len(artists)} artists, {len(rows)} candidates", flush=True)

with open(HERE / "library-sweep.csv", "w", newline="") as handle:
    writer = csv.writer(handle)
    writer.writerow(["artist", "identifier", "fans", *(f"rmse{s}" for s in SIZES)])
    for row in rows:
        writer.writerow([row["artist"], row["identifier"], row["fans"],
                         *(f"{row[s]:.6f}" for s in SIZES)])

measured = [r for r in rows if r["identifier"] != "(empty)"]
placeholders = [r for r in measured if r[32] <= PLACEHOLDER_CUTOFF]
photographs = [r for r in measured if r[32] > PLACEHOLDER_CUTOFF]

print(f"\ncandidates: {len(rows)}  measured images: {len(measured)}  "
      f"structurally empty: {len(rows) - len(measured)}")
print(f"artists with no exact-name candidate: {len(without_candidate)}")
print(f"failures: {len(failures)}")
for artist, why in failures[:10]:
    print(f"  !! {artist}: {why}")

known = set(REFERENCES.values())
print(f"\nplaceholder instances: {len(placeholders)} "
      f"({len({r['identifier'] for r in placeholders} - known)} under ordinary identifiers)")
for row in sorted(placeholders, key=lambda r: (r["identifier"] in known, r["artist"])):
    mark = "reference" if row["identifier"] in known else "ORDINARY "
    print(f"  {mark} {row['artist'][:26]:26s} {row['identifier']:34s} "
          + "  ".join(f"{s}:{row[s]:.5f}" for s in SIZES))

print("\nmargin per resolution:")
for size in SIZES:
    worst = max(r[size] for r in placeholders)
    nearest = min(r[size] for r in photographs)
    who = min(photographs, key=lambda r: r[size])
    print(f"  {size:2d}x{size:<2d} worst placeholder {worst:.5f}   "
          f"nearest photograph {nearest:.5f} ({who['artist'][:24]})   "
          f"ratio {nearest / max(worst, 1e-9):7.1f}x")

print("\nten nearest photographs (sorted by 32x32):")
for row in sorted(photographs, key=lambda r: r[32])[:10]:
    print(f"  {row['artist'][:26]:26s} {row['identifier']:34s} "
          + "  ".join(f"{s}:{row[s]:.4f}" for s in SIZES))

# Does the fallback question matter? Count artists whose best candidate is a
# placeholder while a lesser namesake carries a real photograph.
by_artist = collections.defaultdict(list)
for row in rows:
    by_artist[row["artist"]].append(row)
sentinel = {"(empty)", "d41d8cd98f00b204e9800998ecf8427e"}
affected = top_ranked = rescuable = 0
for artist, candidates in by_artist.items():
    ordinary = [c for c in candidates
                if c["identifier"] not in sentinel and c[32] <= PLACEHOLDER_CUTOFF]
    if not ordinary:
        continue
    affected += 1
    ranked = sorted(candidates, key=lambda c: (c["identifier"] not in sentinel, c["fans"]),
                    reverse=True)
    if ranked[0][32] <= PLACEHOLDER_CUTOFF:
        top_ranked += 1
        if any(c[32] > PLACEHOLDER_CUTOFF for c in candidates):
            rescuable += 1
print(f"\nartists with a placeholder under an ordinary identifier: {affected}")
print(f"  of those, placeholder ranks first: {top_ranked}")
print(f"  of those, a namesake carries a real photograph: {rescuable}")
print("done", flush=True)
