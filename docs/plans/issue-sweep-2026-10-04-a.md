---
slug: issue-sweep-2026-10-04-a
worktree: /home/marvin/Projects/reprise-issue-sweep-2026-10-04-a
branch: feature/issue-sweep-2026-10-04-a
phase: shipped
codex_session:
created: 2026-10-04
---
# Strand a — cover matching folds typographic variants (#1059)

A local tag with a hyphen ("Selected Ambient Works 85-92") never matched the MusicBrainz release with an en dash
("85–92"). The miss wrote a `.notfound2` marker that blocks the album for 7 days, so the NET-7b/7c retries never reach
the network for it. Desktop and Android share this core path.

## Diagnosis
    #1059 diagnosis (worker, origin/dev 78005329e3):
    Root cause: crates/reprise-core/src/cover_download.rs:275-309 parse_best_release: local norm() (:276) only collapses
    whitespace + lowercases; compare at :309 so "85-92" vs "85–92" = NoMatch; plain search misses (:428), the
    strip_release_decoration fallback misses (:439) -> write_negative_in (:594) writes <key>.notfound2
    (negative_marker_path_in :128), honoured 7 days by negative_marker_blocks (:194); lookup at :405 hits the marker first,
    so NET-7b/c retries never reach the network. Android uses the same core fetch (reprise-android-ffi album_cover.rs).
    All typographic variants are affected on album AND artist: dashes U+2010 U+2011 U+2013 U+2014 U+2212; apostrophes
    U+2019 U+2018 U+02BC ` U+00B4; quotes U+201C U+201D; ellipsis U+2026. Diacritics out of scope.
    strip_release_decoration (cover_download_title.rs:4) handles - – — but misses U+2010 U+2011 U+2212.
    No reusable helper: library::group_key::normalize_group_key (NFKD, used by Library Doctor + STATS-9) does not fold
    these; do NOT touch it (Library Doctor has the same gap -> follow-up).
    Fix: pub(super) fn match_key(s) in cover_download_title.rs: fold dash set -> '-', apostrophes -> '\'', quotes -> '"',
    … -> "...", then collapse whitespace, lowercase; use it in parse_best_release instead of norm; widen
    strip_release_decoration's dash set through a shared is_dash(char). Do NOT change album_key (:82-89, cache identity).
    Tests (cover_download_tests.rs or cover_download_retry_tests.rs): en-dash hit vs hyphen tag = Match and reverse; table
    over all variants for album + artist; negative controls (85-92 vs 85-93 NoMatch, other artist rejected); e2e via
    fetch_and_cache_with with fake mb_fetch/caa_fetch -> Downloaded, no negative marker (model:
    stripped_search_fallback_uses_the_stripped_title_for_query_and_comparison, tests :513); strip row "Album ‐ Single".
    Marker: NO bump. cover_download.rs:24-28 says NEGATIVE_MARKER_GENERATION was one-shot (#908), "Do not bump this again:
    future stale markers must be retired by the TTL" (also docs/plans/the-cover-search-finds-what-is-there.md:203).
    No UX rule governs matching. Mention in the PR that the marker blocked the NET-7b/c retries for this case.

## Tasks (test-first)
1. Add `match_key` (and a shared `is_dash(char)`) to `crates/reprise-core/src/cover_download_title.rs`: fold the dash
   set to `-`, apostrophes to `'`, double quotes to `"`, `…` to `...`, then collapse whitespace and lowercase.
2. `parse_best_release` in `cover_download.rs` compares through `match_key` instead of its local `norm`.
3. `strip_release_decoration` recognises the whole dash set as the separator.
4. Tests as listed in the diagnosis, including negative controls and an end-to-end fetch that downloads and writes no
   negative marker. Each new behavioural test must fail before the fix.

## Out of scope
- `album_key` (cache identity) stays unchanged.
- No marker generation bump and no migration (documented one-shot policy, `cover_download.rs:24-28`). Stale markers
  expire through the TTL.
- Library Doctor's `normalize_group_key` has the same gap; that is a follow-up issue, not this strand.
