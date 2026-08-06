#!/usr/bin/env python3
"""Score ranked sound neighbours against genre, artist and album.

Reads the CSV written by the sound_similarity_check example and the library it
was ranked from. Answers one question: do the neighbours agree with the
metadata *more than chance allows*? In a library where 51 % of everything is
metalcore, raw agreement is meaningless — only the lift over the base rate is.

    ./score.py <neighbours.csv> <library.db> [--exclusions product|none]
"""

import csv
import math
import sqlite3
import sys
from collections import Counter, defaultdict


def normalize(value):
    return (value or "").strip().lower()


def load_library(db_path):
    conn = sqlite3.connect(f"file:{db_path}?mode=ro", uri=True)
    rows = conn.execute(
        "SELECT id, COALESCE(genre,''), COALESCE(artist,''), COALESCE(album,''), "
        "COALESCE(title,'') FROM tracks "
        "WHERE missing_since IS NULL AND removed_at IS NULL"
    ).fetchall()
    conn.close()
    return {
        row[0]: {
            "genre": normalize(row[1]),
            "artist": normalize(row[2]),
            "album": normalize(row[3]),
            "title": row[4],
        }
        for row in rows
    }


def main():
    csv_path, db_path = sys.argv[1], sys.argv[2]
    exclusions = "product"
    if "--exclusions" in sys.argv:
        exclusions = sys.argv[sys.argv.index("--exclusions") + 1]

    library = load_library(db_path)

    pairs = defaultdict(list)  # seed_id -> [neighbour_id, ...]
    with open(csv_path) as handle:
        for row in csv.DictReader(handle):
            pairs[int(row["seed_id"])].append(int(row["neighbour_id"]))

    # Base rates over the tracks that actually carry a profile. Using the whole
    # library would understate the baseline if the backfill covered it unevenly.
    ranked_ids = set(pairs) | {n for ns in pairs.values() for n in ns}
    population = [library[i] for i in ranked_ids if i in library]

    def base_rates(field):
        counts = Counter(t[field] for t in population if t[field])
        total = sum(counts.values())
        return {k: v / total for k, v in counts.items()}, total

    print(f"seeds: {len(pairs)}   pairs: {sum(len(v) for v in pairs.values())}"
          f"   population with profile: {len(population)}   exclusions: {exclusions}")
    print()

    per_seed_by_field = {}
    for field in ("genre", "artist", "album"):
        if exclusions == "product" and field == "album":
            print("album: not measurable — the shipped default excludes same-album matches")
            print()
            continue

        rates, total = base_rates(field)
        if not rates:
            print(f"{field}: no data")
            continue

        # Per-seed expectation: for a seed whose value is v, a random neighbour
        # agrees with probability rate(v). Averaging that over the seeds is the
        # honest baseline for *these* seeds, not for an idealised library.
        observed_hits = 0
        observed_total = 0
        expected = 0.0
        per_seed = []
        for seed, neighbours in pairs.items():
            seed_value = library.get(seed, {}).get(field)
            if not seed_value or seed_value not in rates:
                continue
            hits = sum(
                1 for n in neighbours
                if library.get(n, {}).get(field) == seed_value
            )
            observed_hits += hits
            observed_total += len(neighbours)
            expected += rates[seed_value] * len(neighbours)
            per_seed.append((hits / len(neighbours), seed))

        if observed_total == 0:
            print(f"{field}: nothing comparable")
            continue

        observed_rate = observed_hits / observed_total
        expected_rate = expected / observed_total
        lift = observed_rate / expected_rate if expected_rate else float("nan")
        # Standard error over seeds, so a lift is not read as precise when it isn't.
        shares = [s for s, _ in per_seed]
        mean = sum(shares) / len(shares)
        variance = sum((s - mean) ** 2 for s in shares) / max(len(shares) - 1, 1)
        stderr = math.sqrt(variance / len(shares))

        print(f"{field}:")
        print(f"  observed agreement  {observed_rate:6.1%}  (±{stderr:.1%} over seeds)")
        print(f"  expected by chance  {expected_rate:6.1%}")
        print(f"  lift                {lift:6.2f}x")
        print()
        per_seed_by_field[field] = per_seed

    # Rank-1 only: the nearest neighbour is the most sensitive probe. A measure
    # can be right at the very top and mush below it, and the pooled number
    # would hide that.
    rates, _ = base_rates("genre")
    top1_hits = top1_total = 0
    top1_expected = 0.0
    for seed, neighbours in pairs.items():
        seed_genre = library.get(seed, {}).get("genre")
        if not seed_genre or seed_genre not in rates or not neighbours:
            continue
        top1_hits += 1 if library.get(neighbours[0], {}).get("genre") == seed_genre else 0
        top1_expected += rates[seed_genre]
        top1_total += 1
    if top1_total:
        observed = top1_hits / top1_total
        expected_rate = top1_expected / top1_total
        print(f"genre, nearest neighbour only:  {observed:6.1%} vs {expected_rate:6.1%} "
              f"by chance   lift {observed/expected_rate:5.2f}x  (n={top1_total})")
        print()

    # Per-genre breakdown: the aggregate hides that metalcore is easy and rock is hard.
    print("per genre (seed genre -> share of neighbours with the same genre):")
    by_genre = defaultdict(lambda: [0, 0])
    for seed, neighbours in pairs.items():
        seed_genre = library.get(seed, {}).get("genre")
        if not seed_genre:
            continue
        by_genre[seed_genre][0] += sum(
            1 for n in neighbours if library.get(n, {}).get("genre") == seed_genre
        )
        by_genre[seed_genre][1] += len(neighbours)
    macro = []
    for genre, (hits, total) in sorted(by_genre.items(), key=lambda kv: -kv[1][1]):
        base = rates.get(genre, 0.0)
        lift = (hits / total) / base if base else float("nan")
        if base:
            macro.append(lift)
        print(f"  {genre:42s} {hits/total:6.1%}  base {base:5.1%}  lift {lift:5.2f}x  (n={total})")
    if macro:
        # Every genre counted once. The pooled number above is really a
        # statement about metalcore, which is half the library; this one asks
        # whether the measure helps a genre at all, whatever its size.
        print(f"  {'MACRO (each genre counted once)':42s} {'':6s}  {'':10s}  "
              f"lift {sum(macro)/len(macro):5.2f}x")
    print()

    # The eyeball test: the strongest and weakest seeds by genre agreement,
    # which is the field the question was actually asked about.
    per_seed = sorted(per_seed_by_field.get("genre", []))
    if not per_seed:
        return
    def show(label, rows):
        print(label)
        for share, seed in rows:
            info = library.get(seed, {})
            print(f"  {share:5.0%}  {info.get('artist','?')} — {info.get('title','?')}")
            for n in pairs[seed][:5]:
                ninfo = library.get(n, {})
                print(f"         -> {ninfo.get('artist','?')} — {ninfo.get('title','?')}"
                      f"  [{ninfo.get('genre','?')}]")
        print()

    show("weakest seeds (neighbours share the least metadata):", per_seed[:3])
    show("strongest seeds:", per_seed[-3:])


if __name__ == "__main__":
    main()
