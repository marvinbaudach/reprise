# Visible acceptance — every artist-specific silhouette

## Run identity

`acceptance/deezer-placeholder-portraits/runs/20260814T145219Z`, exit 0,
2026-08-14 14:52-14:58 UTC. `run-manifest.txt` pins the baseline to `origin/dev`
`8b87ae8ada58829d7d3edd15063775fe48018863` and the candidate to
`baece97e1ef5b9db4dc8ae9286d06d6b633287a1`. Both arms ran under private Xvfb,
Openbox, D-Bus, AT-SPI and XDG roots against the live Deezer API; both database
copies were taken with a read-only SQLite online backup.

## What the pictures show

`before-my-stats.png` and `after-my-stats.png` are the same window, same ranks,
same seeded database. Five portraits change, nothing else does.

| Rank | Artist | Baseline | Candidate |
|---|---|---|---|
| 13 | Oceano | photograph of a namesake band | initial `O` |
| 16 | Aetheriality | grey silhouette | initial `A` |
| 17 | In Your Grave | grey silhouette | initials `IG` |
| 18 | Our Vices | grey silhouette | initials `OV` |
| 19 | Wake Me | grey silhouette | initials `WM` |

Ranks 6-12, 14, 15 and 20, and all five tiles above the list, carry the same
photograph in both arms. The Devil Wears Prada (tile #3) is the control: the
baseline already resolves it through the empty-string MD5, and its cached bytes
are identical in both arms (`34aefe17…`).

## The measurement that matters

Three of the four silhouettes arrive as **the same bytes as the shipped
reference**: `bd8dae144dc585a7eb090e2071fe386bbe0df6ebcc47ee0d96d5ec5c23274530`
for Aetheriality, In Your Grave and Our Vices, which is
`d41d8cd98f00b204e9800998ecf8427e.jpg` from the corpus. Wake Me's copy is
`0497f872…` — the same drawing, re-encoded by Deezer on the way out, which is
exactly why the fingerprint compares 32×32 thumbnails instead of hashes.

The baseline cached all four anyway. It knows that byte sequence's *identifier*
and rejects it there — that is how The Devil Wears Prada resolves — so a cached
copy of those very bytes proves the four arrived under a different name. The
library sweep in `library-sweep.csv` recorded which ones: `895abde0…` for
Aetheriality, `790f8499…` for In Your Grave, `5dbfc32c…` for Our Vices and
`e02c0c8d…` for Wake Me. This run did not re-read the identifiers; it did not
need to. Same picture, a name the list does not carry, list defeated.

`named-cache-proof.txt` records the mechanical half — four cached images in the
baseline arm, four negative markers and zero cached images in the candidate arm.

## The seeded ranking

Only a rendered rank fetches a portrait, and the four sit at ranks 40, 122, 131
and — with zero plays — in no ranking at all. The run copy therefore received
synthetic listen events; `seeded-ranking-proof.txt` names every injected
millisecond, and both arms received the identical copy, so the before/after
difference is untouched. Ranks 1-15 keep their real listening history: the four
were placed between the fifteenth and sixteenth real artist, which pushed Asking
Alexandria from rank 16 to 20 and left everything above it alone.

The reasoning behind seeding rather than raising the rendered-rank cap is E7 in
`docs/plans/portrait-placeholder-fingerprint.md`. In short: the baseline arm is
built from `git archive origin/dev` without patches, so a product switch on this
branch would not exist there; the harness does not scroll; and no cap can rank an
artist with zero plays.

## Known limit

The seeding anchors on a SQL ranking grouped by the effective album artist,
while the view folds spelling variants and MBIDs together first. The two agreed
on this run — the four landed at exactly 16-19 in the picture — but they are not
the same computation. If they ever drift, the four are still rendered (the check
refuses to place them above rank 16) and only the rank numbers in the review
notes stop matching. Confirm the ranks in the screenshot, not in the proof file.
