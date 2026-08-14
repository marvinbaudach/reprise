# Final live acceptance

## Run identity and isolation

The exact requested command completed with exit 0 and retained its full output
under `acceptance/deezer-placeholder-portraits/runs/20260814T042511Z`.
`final-run-manifest.txt` pins the unmodified baseline to `origin/dev` commit
`07f02b8fcc82faf017a73782cd9466a080d4ff95` and the candidate to
`5d671e97cf7530fcb5f22f60599d64f5793ea016`.

Both runs used private Xvfb, Openbox, D-Bus, AT-SPI and XDG roots. The source
database was copied out with an immutable read-only SQLite backup because no
WAL was present. The application used only the copied databases and missing
track paths under the retained profiles. The real database and the two supplied
placeholder files were read only; the real portrait cache was not modified.

## F6 diagnosis and action fallback

The private accessibility bus was not absent. `before-atspi-bus.txt` and
`after-atspi-bus.txt` show a private address and `registry_owner=true`; the
driver and Reprise were clients of that bus. The actual F6 cause was an
unbounded accessibility traversal of the copied large-library session. It
exceeded cua-driver's deadline, which degraded to X11 window metadata and
reported the result as if AT-SPI were unavailable.

The harness now bounds Library snapshots to depth 20, My Stats snapshots to
depth 40 and every traversal to 500 elements. It passes the queried private
AT-SPI address to Reprise explicitly, starts from an isolated Library session,
and reports each degraded result immediately with its reason and JSON path.
The first two startup attempts in each final session were degraded while GTK
registered; the bounded retry then produced the complete tree used for every
label readiness and content assertion.

The AT-SPI tree exposes the two relevant GTK controls with zero coordinates,
so cua-driver cannot activate them by accessibility token on this host. The
harness therefore uses fixed private-X11 `xdotool` clicks at `(100,692)` for My
Stats and `(390,640)` for Show more top artists. It retains CUA screenshots
immediately before and after each click and proves the resulting page,
expansion, artist labels and readiness through the working AT-SPI tree. This is
the plan's pixel fallback for the two actions only; the content assertions were
not rewritten as coordinate guesses.

## Oracles

| Oracle | Result | Evidence |
| --- | --- | --- |
| Baseline reproduces the defect | Pass | `before-my-stats.png` visibly shows the grey person silhouette at rank 3, The Devil Wears Prada. Its cached SHA-256 is `0d659e...`, exactly placeholder reference 1. |
| Candidate ranks 1-10 contain no grey silhouette | Pass | `after-my-stats.png`; inspected at original 1560x1160 resolution. |
| Candidate rank 3 is a photograph | Pass | Visible screenshot plus cached SHA-256 `34aefe...`; the candidate log records the selected `ce8738d5...c62a` CDN image and HTTP 200. |
| Candidate rank 10 is a photograph | Pass | Visible screenshot plus cached SHA-256 `ca747e...`; the candidate log records the second exact Oceano match `68526b59...dd67` and HTTP 200. |
| Portrait fetches actually ran | Pass | `fetch-proof.txt` records 20 API searches and 20 CDN image requests in each phase, including named request, selected image and 200 response evidence for both acceptance artists. |
| Newly cached bytes differ from both supplied placeholders | Pass | `named-cache-proof.txt` and `placeholder-reference-sha256.txt`. Candidate Prada and Oceano match neither reference. |
| Other ranks remain unchanged | Pass | Visual comparison keeps the same identities and artwork at ranks 1-2 and 4-9: Lorna Shore, Falling in Reverse, Annisokay, Chelsea Grin, Woe Is Me, The Browning, Bring Me The Horizon and From Ashes to New. No deviation remains to name. |

Rank 3 is the only proof of the E1/E2 selection behavior. Rank 10 proves cache
recovery and the newly observed Oceano sentinel only; it is not presented as
independent evidence for the original selection rule.

No oracle failed and none was unevaluable. Deezer and its CDN were reachable,
returned HTTP 200, and showed no throttle or request failure during the final
run.
