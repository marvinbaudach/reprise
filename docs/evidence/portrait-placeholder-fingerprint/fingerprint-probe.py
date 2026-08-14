#!/usr/bin/env python3
"""Measures how far Deezer's silhouette sits from real portraits in hash space.

Downloads every exact-name candidate picture for the library's top artists plus
the two known silhouette identifiers, then reports dHash/aHash Hamming distances
against the silhouette references. The point is the gap between the two
populations, not any single number.
"""
import io
import json
import time
import urllib.parse
import urllib.request

import numpy as np
from PIL import Image

CDN = "https://cdn-images.dzcdn.net/images/artist/{}/1000x1000-000000-80-0-0.jpg"
REFERENCES = {
    "empty-md5": "d41d8cd98f00b204e9800998ecf8427e",
    "oceano": "415714b66a5de709809dd3d05f58afe4",
}
ARTISTS = [
    "Lorna Shore", "Falling in Reverse", "The Devil Wears Prada", "Annisokay",
    "Chelsea Grin", "Woe, Is Me", "The Browning", "Bring Me The Horizon",
    "From Ashes to New", "Oceano", "Bury Tomorrow", "Suicide Silence",
    "As I Lay Dying", "Asking Alexandria", "A Day to Remember", "Dead by April",
    "Electric Callboy", "Until I Wake",
]
UA = {"User-Agent": "reprise-fingerprint-probe/1.0"}


def get(url, delay=0.35):
    time.sleep(delay)
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=20) as response:
        return response.read()


def dhash(img, size=8):
    grey = np.asarray(img.convert("L").resize((size + 1, size), Image.LANCZOS), dtype=np.int16)
    return np.packbits(grey[:, 1:] > grey[:, :-1]).tobytes()


def ahash(img, size=8):
    grey = np.asarray(img.convert("L").resize((size, size), Image.LANCZOS), dtype=np.int16)
    return np.packbits(grey > grey.mean()).tobytes()


def hamming(a, b):
    return sum(bin(x ^ y).count("1") for x, y in zip(a, b))


def describe(identifier, data):
    img = Image.open(io.BytesIO(data))
    rgb = img.convert("RGB")
    colours = len(set(map(tuple, np.asarray(rgb).reshape(-1, 3))))
    return {
        "identifier": identifier,
        "bytes": len(data),
        "size": f"{img.width}x{img.height}",
        "colours": colours,
        "dhash": dhash(img),
        "ahash": ahash(img),
    }


refs = {}
for name, identifier in REFERENCES.items():
    refs[name] = describe(identifier, get(CDN.format(identifier)))
    print(f"reference {name:10s} {identifier} colours={refs[name]['colours']:6d} bytes={refs[name]['bytes']}")

print(f"\nreference-to-reference: dhash={hamming(refs['empty-md5']['dhash'], refs['oceano']['dhash'])} "
      f"ahash={hamming(refs['empty-md5']['ahash'], refs['oceano']['ahash'])}\n")

rows = []
seen = set(REFERENCES.values())
for artist in ARTISTS:
    url = "https://api.deezer.com/search/artist?q=" + urllib.parse.quote(artist) + "&limit=10"
    try:
        payload = json.loads(get(url))
    except Exception as exc:  # noqa: BLE001 - probe, not production
        print(f"!! search failed for {artist}: {exc}")
        continue
    wanted = "".join(ch.lower() for ch in artist if ch.isalnum())
    for candidate in payload.get("data", []):
        normalized = "".join(ch.lower() for ch in candidate.get("name", "") if ch.isalnum())
        if normalized != wanted:
            continue
        picture = candidate.get("picture_xl") or candidate.get("picture_big") or ""
        parts = [p for p in urllib.parse.urlparse(picture).path.split("/") if p]
        identifier = parts[2] if len(parts) > 2 else ""
        if not identifier or identifier in seen:
            continue
        seen.add(identifier)
        try:
            info = describe(identifier, get(picture))
        except Exception as exc:  # noqa: BLE001
            print(f"!! download failed for {artist} {identifier}: {exc}")
            continue
        info["artist"] = artist
        info["fans"] = candidate.get("nb_fan", 0)
        info["d_empty"] = hamming(info["dhash"], refs["empty-md5"]["dhash"])
        info["d_oceano"] = hamming(info["dhash"], refs["oceano"]["dhash"])
        info["a_empty"] = hamming(info["ahash"], refs["empty-md5"]["ahash"])
        info["a_oceano"] = hamming(info["ahash"], refs["oceano"]["ahash"])
        rows.append(info)

print(f"{'artist':24s} {'identifier':34s} {'fans':>8s} {'colours':>8s} {'dHash':>12s} {'aHash':>12s}")
for row in sorted(rows, key=lambda r: min(r["d_empty"], r["d_oceano"])):
    print(f"{row['artist'][:24]:24s} {row['identifier']:34s} {row['fans']:8d} {row['colours']:8d} "
          f"{row['d_empty']:5d}/{row['d_oceano']:<6d} {row['a_empty']:5d}/{row['a_oceano']:<6d}")

if rows:
    closest = min(min(r["d_empty"], r["d_oceano"]) for r in rows)
    print(f"\ncandidates measured: {len(rows)}")
    print(f"closest non-reference dHash distance to a silhouette: {closest}")
    flat = [r for r in rows if r["colours"] < 1000]
    print(f"candidates under 1000 unique colours: {len(flat)} "
          f"{[(r['artist'], r['colours']) for r in flat]}")
