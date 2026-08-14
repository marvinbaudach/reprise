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

## Red-before-green method

Each raw red file was produced before any production change to
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

`green-artist-portrait.txt` is the focused post-repair run. It records 26
portrait tests passed and zero failed.

The rewritten `picture_xl` to `picture_big` test is not listed as a new red
behavior: it preserves the existing fallback while correcting the fixture from
an impossible mixed-identifier candidate to the measured missing-XL shape.

## Offline gates

- `gate-reprise-core.txt`: 2,433 passed, 2 ignored, 0 failed. The command used
  private worktree-local XDG data, cache, and config roots.
- `gate-clippy.txt`: strict `cargo clippy --all-targets --workspace -- -D
  warnings` completed successfully.
- `gate-frontend-thinness.txt`: passed at the unchanged budgets: rusqlite 109,
  filesystem 13, threads 15, workers 7, and no banned handle, GStreamer, or
  zbus use. The optional cargo-machete subcheck reports its established skip
  because cargo-machete is not installed.
- `gate-format.txt`: `cargo fmt --all --check` passed without output.
- `gate-core-purity.txt`: no gtk4, libadwaita, gstreamer, or zbus dependency in
  `reprise-core`.

The acceptance script passed `bash -n` and has executable mode. It was not run,
so this directory makes no visible My Stats claim.
