# The index rebuild — what it cost and what it bought

The four before/after figures the showroom prints, with the provenance they were
missing while they were typed into a TypeScript module.

**Nothing here was measured for this record.** These are exactly the values the
page already showed; what they gained is a commit, a date and a method beside
each one. Re-measuring is a separate job, and doing it silently while writing
down provenance would have produced a record that looks sourced and is not.

`showroom/vite.config.ts` reads the table below through `readLedger()` and serves
it as `virtual:measurements`. Changing a value here changes the page; deleting
the table fails the build rather than shrinking it.

## Results

| What | Before | After | Delta | Commit | Date | Method |
|---|---|---|---|---|---|---|
| Title window over 100'000 tracks | 53'605 µs | 1'333 µs | −97.51 % | a41c53f460 | 2026-07-18 | same-host A/B over generated 100'000-track profiles, baseline `ddaa3f3` against `b3644cc` via `scripts/performance-query-compare.sh` |
| Playback ID projection | 8'125 µs | 298 µs | −96.33 % | a41c53f460 | 2026-07-18 | the same run and the same pair of commits |
| Main-thread CPU while idle | 110 ms/s | 64 ms/s | −41.8 % | 5752049757 | 2026-08-04 | median of three alternating runs, isolated instance on `:99` against a copy of a real 1847-track library |
| Tag reads on a warm start | 419 | 0 | −100 % | 5752049757 | 2026-08-04 | two launches back to back on one machine, cover index cleared against warm |

## The price

The price sits next to it, not in the small print: the title index costs 2'379'776 extra database bytes, up 9.85 %. The track list stays pinned by test to eight cached SQL windows and 1'600 retained rows — unchanged between 10'000 and 100'000 tracks.

## What is behind each number

**Rows one, two and the byte price** come from one benchmark session on
`feat/performance-optimizations`, closed in `a41c53f460` (18 July 2026). The
baseline is `ddaa3f3`, which added the query-plan instrumentation to
`crates/reprise-core/examples/scalability_baseline.rs`; the candidate is
`b3644cc`, which added the comparison script. The index itself landed in
`bf8394d7`. The database size is `std::fs::metadata(&config.db_path)?.len()`
taken on both sides of that pair. `docs/showcase.md` names the same three
commits.

**Rows three and four** come from the measurement journal in
`docs/plans/idle-frame-clock.md` (`5752049757`, 4 August 2026), which records
both runs in full: the idle figure as a median of three alternating runs against
the shipped Cairo bloom, the tag reads as two launches compared back to back so
the machine load is the same for both.

**One caveat, stated rather than smoothed over.** The raw output of the July
benchmark pair was never committed as an archived result file — the only
surviving trace is `docs/assets/reprise-performance.svg`, derived from it. The
method is reproducible from the two commits; the run artefact is not. That is
the weakest provenance on this page and it is named here rather than in
nobody's notes.

**And one figure in the price is not a measurement at all.** "Eight cached SQL
windows and 1'600 retained rows" are budgets a test asserts —
`MAX_CACHED_WINDOW_BUDGET` and `MAX_CACHED_TRACK_BUDGET` in
`crates/reprise-gnome/src/ui/track_list/track_list_model_scalability_tests.rs`.
They hold because a red test would stop a merge, which is a stronger claim than
an observation, not a weaker one — but it is a different kind of claim, and the
sentence says "pinned by test" for that reason.
