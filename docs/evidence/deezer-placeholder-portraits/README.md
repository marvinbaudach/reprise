# Deezer placeholder portrait implementation evidence

## Survey boundary

- Branch at survey time: `feature/deezer-placeholder-portraits` at
  `66decf0efd5dc2dac0da115af2a48170e395735c`, directly on the locally pinned
  `origin/dev` at `b99b71a932ed207ffdf1728cf46b14abb12bd5eb`.
- The worktree was clean and its only branch-local commit was the supplied
  implementation plan.
- Repository searches found portrait consumption only in the GNOME My Stats
  implementation and its window wiring. No GNOME change is needed for E1/E2.
- STATS-23 specifies the portrait to album to initials fallback. NET-1a, NET-5,
  NET-6, and SRC-11 specify permission, refresh, and cache behavior. None makes
  a claim about ordering same-named Deezer candidates, so `docs/ux-rules.md`
  was deliberately left unchanged.
- The real cache and real database were not read. This run explicitly forbids
  touching `~/.cache/reprise/` and user library data, so the older plan's live
  cache survey was not repeated.
- Before completion the branch was rebased cleanly onto `origin/dev` at
  `07f02b8fcc`. The intervening dev commits changed Radio GNOME files, plans,
  scripts, and the append-only ledger; the `reprise-core` tree was unchanged.

## Red-before-green method

Each raw red file was produced before its corresponding production change to
`artist_portrait/deezer.rs`. The command form was:

```text
cargo test -p reprise-core --lib <fully-qualified-test-name> -- --exact
```

Every file records `running 1 test` and `test result: FAILED`. The exact map is:

| Evidence | Test |
| --- | --- |
| `red-search-limit.txt` | `artist_portrait::deezer::tests::search_url_encodes_query` |
| `red-placeholder-parser.txt` | `artist_portrait::deezer::tests::parse_treats_deezer_placeholder_as_no_picture` |
| `red-placeholder-identifier.txt` | `artist_portrait::deezer::tests::placeholder_detection_reads_the_artist_identifier_segment` |
| `red-all-placeholders.txt` | `artist_portrait::tests::all_exact_placeholder_matches_write_marker_without_download` |
| `red-devil-prada.txt` | `artist_portrait::tests::devil_wears_prada_downloads_real_match_after_placeholder` |
| `red-oni-popularity.txt` | `artist_portrait::tests::oni_downloads_most_popular_exact_match_even_when_it_is_last` |
| `red-non-exact.txt` | `artist_portrait::tests::non_exact_name_never_wins_even_with_many_more_fans` |
| `red-image-priority.txt` | `artist_portrait::tests::real_image_outranks_a_more_popular_placeholder` |
| `red-missing-fans.txt` | `artist_portrait::tests::missing_and_null_fan_counts_choose_stably_without_panicking` |
| `red-defensive-field-fallback.txt` | `artist_portrait::deezer::tests::defensive_fallback_skips_placeholder_xl_for_real_big` |

`green-artist-portrait.txt` is the focused final run. It records 27
portrait tests passed and zero failed.

The original rewritten `picture_xl` to `picture_big` test was not a new red
behavior: it preserved the existing fallback while correcting the fixture from
an impossible mixed-identifier candidate to the measured missing-XL shape. The
review regression is named and commented as a defensive case because Deezer has
not been observed to emit different identifiers in the two fields.

## First live acceptance run and F5 repair

- The harness was executed for the first time in retained run
  `20260814T015553Z`. Both `origin/dev` and candidate builds completed before
  the first private run attempted to start cua-driver.
- The run stopped before launching Reprise or capturing a screenshot.
  `before/cua-driver.log` records that binding
  `before/private-cua-driver.sock` failed because the path must be shorter than
  `SUN_LEN`. The absolute socket path was 151 bytes, above Linux's 107-byte
  pathname budget for `sockaddr_un.sun_path`.
- F5 moves both the socket and the private `XDG_RUNTIME_DIR` out of the retained
  evidence tree and below a collision-safe `mktemp` root under the usable host
  runtime directory, falling back to `/tmp`. Logs, screenshots, copied profiles,
  and all other evidence remain below the caller-owned output directory.
- The private-run cleanup stops cua-driver while the short socket path is still
  available, removes the socket, and the outer cleanup removes the whole short
  runtime root. A 107-byte preflight guard now fails before daemon startup with
  the measured path length, and `--self-test-private-paths` covers rejection,
  the exact 107-byte boundary, allocated before/after paths, and cleanup without
  starting the acceptance.
- Per the repair request, the full acceptance harness was not rerun after this
  change. Its visible result remains pending.

## Offline gates

- `gate-reprise-core.txt`: 2,434 passed, 2 ignored, 0 failed. The command used
  private temporary XDG data, cache, and config roots.
- `gate-clippy.txt`: strict `cargo clippy --all-targets --workspace -- -D
  warnings` completed successfully.
- `gate-frontend-thinness.txt`: passed at the unchanged budgets: rusqlite 109,
  filesystem 13, threads 15, workers 7, and no banned handle, GStreamer, or
  zbus use. The optional cargo-machete subcheck reports its established skip
  because cargo-machete is not installed.
- `gate-format.txt`: `cargo fmt --all --check` passed without output.
- `gate-core-purity.txt`: no gtk4, libadwaita, gstreamer, or zbus dependency in
  `reprise-core`.
- `gate-acceptance-harness.txt`: Bash syntax and the private-path self-test
  passed; the earlier extracted cleanup and positive-image probes remain green;
  and the retained source lines show the path guard, short runtime allocation,
  cleanup, 60-second cap, and expand-and-confirm sequence.

The acceptance script retains executable mode. The first live run exposed F5
before the application started, so this directory still makes no visible My
Stats claim. The identical unbounded wait in the shared
`scripts/cua-common/session.sh` remains a pre-existing out-of-scope issue.

## Final live acceptance

The later F6-F12 repair sequence, F7 screenshot-race correction and completed
live before/after result are recorded in `final-acceptance.md`. The committed
screenshots, complete 20-entry cache listings, all-rank comparison, settle
timestamps, named byte comparison, placeholder hashes, AT-SPI ownership proof,
run manifest and network-fetch proof all come from the corrected successful run
`20260814T045813Z`. The regression oracle explicitly compares ranks 1 through
20 and identifies ranks 3 and 10 as the only intended changes. The focused
portrait suite contains 28 passing tests.
