# Final live acceptance

## Run identity and isolation

The exact requested command completed with exit 0 and retained its full output
under `acceptance/deezer-placeholder-portraits/runs/20260814T045813Z`.
`final-run-manifest.txt` pins the unmodified baseline to `origin/dev` commit
`07f02b8fcc82faf017a73782cd9466a080d4ff95` and the candidate to
`33d0da342ee2e0eca2470d65ce3f1946b2d5e9a7`.

Both runs used private Xvfb, Openbox, D-Bus, AT-SPI and XDG roots. The source
database was copied out with an immutable read-only SQLite backup because no
WAL was present. The application used only the copied databases and missing
track paths under the retained profiles. The real database and the two supplied
placeholder files were read only; the real portrait cache was not modified.

The worktree retained an unrelated user change in `.pipeline-codex.md`. A
temporary alternate index inside this worktree marked only that file
assume-unchanged for the harness clean-tree guard; it was removed when the run
ended. The candidate manifest still pins the exact committed HEAD, and the
unrelated Markdown file cannot affect either build.

## F6 diagnosis and action fallback

The private accessibility bus was not absent. `before-atspi-bus.txt` and
`after-atspi-bus.txt` show a private address and `registry_owner=true`; the
driver and Reprise were clients of that bus. The actual F6 cause was an
unbounded accessibility traversal of the copied large-library session. It
exceeded cua-driver's deadline, which degraded to X11 window metadata and
reported the result as if AT-SPI were unavailable.

The harness bounds Library snapshots to depth 20, My Stats snapshots to depth
40 and every traversal to 500 elements. It passes the queried private AT-SPI
address to Reprise explicitly, starts from an isolated Library session, and
reports each degraded result immediately with its reason and JSON path. The
first two startup attempts in each final session were degraded while GTK
registered; the bounded retry then produced the complete tree used for every
label readiness and content assertion.

The AT-SPI tree exposes the two relevant GTK controls with zero coordinates, so
cua-driver cannot activate them by accessibility token on this host. The
harness therefore uses fixed private-X11 `xdotool` clicks at `(100,692)` for My
Stats and `(390,640)` for Show more top artists. It retains CUA screenshots
immediately before and after each click and proves the resulting page,
expansion, artist labels and readiness through the working AT-SPI tree. This is
the plan's pixel fallback for the two actions only; the content assertions were
not rewritten as coordinate guesses.

## F7 harness race and repair

Run `20260814T042511Z` captured after waiting only for The Devil Wears Prada and
Oceano plus one second. Its candidate screenshot was written at
`04:29:33.822177941Z`; Bury Tomorrow's `limit=10` search did not begin until
`04:29:33.872332Z`, 50 ms after capture. The reporter's network-level review
placed the As I Lay Dying and Asking Alexandria CDN completions only about 0.8
and 0.5 seconds before capture. The retained cache mtimes are even tighter:
`04:29:33.349175184Z` and `04:29:33.480175948Z`, only 0.473 and 0.342 seconds
before the screenshot. GTK had not repainted those rows.

This was a harness race, not an application defect. In that run the before and
after files are SHA-256 identical for Bury Tomorrow
(`763e5fca...6af1`), As I Lay Dying (`24e63f41...95a7`) and Asking Alexandria
(`4c2ca999...df98`), with no `.notfound` marker in either profile. A fresh live
API check returned exactly one exact-name match for each: artist IDs 390473,
3823 and 288567, selecting image identifiers `e7dc5e9d...7c0c`,
`cfdc9597...ca7c` and `8b4e3a39...9d5d`; none is a placeholder sentinel. The
cached images have 105,111, 80,909 and 49,165 unique colours and normalized
RMSE 0.751, 0.743 and 0.353 against the known silhouettes, so all three are
photographs.

The repaired harness now observes the clean isolated cache until all 20
currently rendered ranks have a terminal image or `.notfound` outcome. The
wait remains bounded at 60 seconds and its failure names both the observed and
expected counts. Only after 20/20 does it allow a two-second GTK repaint margin.
`portrait-settle-proof.txt` records 20/20 at `05:00:56.824Z` for the baseline
and `05:02:30.978Z` for the candidate, with capture readiness two seconds later
in each phase.

## Oracles

| Oracle | Result | Evidence |
| --- | --- | --- |
| Baseline reproduces the defects | Pass | `before-my-stats.png` visibly shows grey silhouettes at rank 3, The Devil Wears Prada, and rank 10, Oceano. The former matches supplied placeholder reference 2 byte-for-byte; the latter is the independently measured re-encoding under identifier `415714b6...afe4`. |
| Candidate ranks 1-20 contain no grey silhouette | Pass | `after-my-stats.png`, inspected at the original 1560x1160 resolution; all 20 ranks are visible in the one screenshot. |
| Candidate rank 3 is a photograph | Pass | Visible screenshot plus cached SHA-256 `34aefe...`; the candidate log records selected image `ce8738d5...c62a` and HTTP 200. |
| Candidate rank 10 is a photograph | Pass | Visible screenshot plus cached SHA-256 `ca747e...`; the candidate log records selected image `68526b59...dd67` and HTTP 200. |
| Every rendered portrait settled before capture | Pass | Both `portrait-settle.txt` files record `rendered_ranks=20`, `settled_outcomes=20`, and a two-second repaint margin. Both caches contain 20 images and zero `.notfound` markers. |
| Portrait fetches actually ran | Pass | `fetch-proof.txt` records 20 API searches and 20 CDN image requests in each phase, including named request, selected image and 200 response evidence for both acceptance artists. |
| Newly cached bytes differ from both supplied placeholders | Pass | `named-cache-proof.txt` and `placeholder-reference-sha256.txt`. Candidate Prada and Oceano match neither reference. |
| Other ranks remain unchanged | Pass | The regression comparison actually covers ranks 1 through 20. Ranks 3 and 10 are the two intended corrections. Every other rank was compared: ranks 1-2, 4-9 and 11-20 retain the same artist identity, selected CDN identifier and visible photograph. `all-ranks-proof.txt` lists all 20 ranks; unchanged-image normalized RMSE is 0 to 0.0226 despite live CDN re-encoding. Ranks 11 Bury Tomorrow, 13 As I Lay Dying and 14 Asking Alexandria show photographs in both corrected captures. |

Rank 3 is the proof of the original E1/E2 selection behavior. Rank 10 proves
the newly observed Oceano sentinel handling. No other rank changed selection,
and no deviation remains to justify or leave unevaluable.

## Consequence for plan decision E1

E1's structural-stability premise now holds only for
`d41d8cd98f00b204e9800998ecf8427e`, the MD5 of the empty string. Deezer also
serves the same grey silhouette under `415714b66a5de709809dd3d05f58afe4`, an
ordinary per-artist image identifier with no structural meaning. The measured
image has 213 unique colours and normalized RMSE 0.058376 and 0.058337 against
the two known placeholder variants: the same artwork re-encoded.

Therefore Deezer can publish placeholders under arbitrary per-artist
identifiers, and the sentinel list may grow per affected artist rather than
remaining a one-off structural set. The plan's assumption that a third
identifier would be rare was disproven within one day. No automatic detector
was added: E1 explicitly excludes content-based detection and rejects it as
YAGNI. This weakened premise is recorded for a human decision, not silently
changed in implementation.

No oracle failed. Deezer and its CDN were reachable, returned HTTP 200, and
showed no throttle or portrait-request failure during the corrected run.
