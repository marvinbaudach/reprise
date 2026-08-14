#!/usr/bin/env python3
"""Second pass: which primitive actually separates the silhouette from photographs.

Pass one showed dHash is useless here — the two known silhouette variants sit 16
bits apart because a gradient hash on a near-flat image measures JPEG noise. This
pass compares aHash and a downscaled-grey RMSE, both against both references, and
caches every image so the metric can be changed without refetching.
"""
import hashlib
import io
import json
import pathlib
import time
import urllib.parse
import urllib.request

import numpy as np
from PIL import Image

CACHE = pathlib.Path(__file__).with_name("probe-images")
CACHE.mkdir(exist_ok=True)
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
    key = CACHE / (hashlib.sha256(url.encode()).hexdigest()[:24] + ".bin")
    if key.exists():
        return key.read_bytes()
    time.sleep(delay)
    req = urllib.request.Request(url, headers=UA)
    with urllib.request.urlopen(req, timeout=20) as response:
        data = response.read()
    key.write_bytes(data)
    return data


def grey(data, size):
    img = Image.open(io.BytesIO(data)).convert("L").resize((size, size), Image.LANCZOS)
    return np.asarray(img, dtype=np.float64) / 255.0


def ahash(data, size=8):
    g = grey(data, size)
    return np.packbits(g > g.mean()).tobytes()


def hamming(a, b):
    return sum(bin(x ^ y).count("1") for x, y in zip(a, b))


def rmse(a, b):
    return float(np.sqrt(np.mean((a - b) ** 2)))


refs = {}
for name, identifier in REFERENCES.items():
    data = get(CDN.format(identifier))
    refs[name] = {"data": data, "a": ahash(data), "g16": grey(data, 16), "g8": grey(data, 8)}

print("reference-to-reference:"
      f" aHash={hamming(refs['empty-md5']['a'], refs['oceano']['a'])}"
      f" rmse16={rmse(refs['empty-md5']['g16'], refs['oceano']['g16']):.4f}"
      f" rmse8={rmse(refs['empty-md5']['g8'], refs['oceano']['g8']):.4f}\n")

rows = []
seen = set(REFERENCES.values())
for artist in ARTISTS:
    url = "https://api.deezer.com/search/artist?q=" + urllib.parse.quote(artist) + "&limit=10"
    payload = json.loads(get(url))
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
        data = get(picture)
        rows.append({
            "artist": artist,
            "identifier": identifier,
            "a": min(hamming(ahash(data), refs[n]["a"]) for n in refs),
            "r16": min(rmse(grey(data, 16), refs[n]["g16"]) for n in refs),
            "r8": min(rmse(grey(data, 8), refs[n]["g8"]) for n in refs),
        })

rows.sort(key=lambda r: r["r16"])
print(f"{'artist':24s} {'identifier':34s} {'aHash':>6s} {'rmse16':>8s} {'rmse8':>8s}")
for row in rows:
    print(f"{row['artist'][:24]:24s} {row['identifier']:34s} {row['a']:6d} {row['r16']:8.4f} {row['r8']:8.4f}")

print(f"\ncandidates: {len(rows)}")
print(f"nearest photograph: aHash={min(r['a'] for r in rows)} "
      f"rmse16={min(r['r16'] for r in rows):.4f} rmse8={min(r['r8'] for r in rows):.4f}")
