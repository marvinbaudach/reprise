---
slug: the-repo-is-ready-for-gnome
worktree: /home/marvin/Projects/reprise-gnome-ready
branch: feature/the-repo-is-ready-for-gnome
phase: planned
codex_session:
created: 2026-09-01
---
# The repo is ready for GNOME

One branch, one gate run. It closes what a first-time GNOME visitor actually
sees, drops the plan residue this directory's own README says should already be
gone, removes eleven tests that are literal copies of other tests, and lifts
three byte-identical helpers out of the filter bars.

## What this is not

Four separate audits ran before this plan. Three of their headline findings are
**already fixed on `origin/dev`** and are only missing from `main`, which lags by
31 commits — the promotion carries them over, and re-implementing them would be
pure churn. Verified at `origin/dev` on 2026-09-01:

| Reported as open | Actually |
|---|---|
| README has no screenshot | `README.md` references `data/screenshots/` |
| `meson setup build` vs `_build` mismatch | both files say `_build` |
| `TESTING.md` quotes a stale test count | the number is gone |
| `.superpowers/sdd/progress.md` tracked (1.16 MB) | untracked on `dev` |
| REUSE non-compliance | `REUSE.toml` + four `LICENSES/` files present |
| a `.rs` file over the 800-line cap | cap is green |

**Do not touch anything owned by `feature/the-ci-stops-repeating-itself-a2`.**
That strand is finished, reviewed and rebased, and it owns
`.github/workflows/ci.yml`, `.github/workflows/release.yml`,
`.github/scripts/{check-gnome-ci,ci-paths,require-ci-results}.sh`,
`.github/tests/ci-path-routing.sh`, `scripts/check-display-tests.sh`,
`scripts/check-shell.sh`, `scripts/ci-quality.sh`,
`scripts/lib/extract-workflow-run-blocks.awk`, `scripts/tests/qa-linters.sh`.
This plan changes none of them.

**App performance is measured and closed, not deferred.**
`scripts/performance-baseline.sh --quick` at `1b68764703` (report kept out of the
repo, in the session scratchpad): startup 806 µs, `library_count` 111 µs, the
title/artist/album 200-row windows 169/303/265 µs — every one of them served by a
covering index (`idx_tracks_present_{title_nocase,artist_order,album_order}`)
with `uses_temp_sort: false` — `library_stats` 340 µs, `playback_ids` 303 µs,
and the track-list scroll probe 5,881 µs with the cache bounded at 4,000 of
10,000 tracks. The slowest probe, `filtered_count`, is 773 µs for a
search-term count: 0.77 ms on a keystroke. There is no measured bottleneck to
fix, so this plan changes no query and no widget for performance reasons.

## Tasks

### 1. `docs/plans/` follows its own retention policy

`docs/plans/README.md` already states the rule; the directory does not obey it.
140 tracked `.md` files, of which 37 carry `phase: shipped`, one `phase:
reverted`, and nine are `*HANDOFF*`/`*handover*` relay notes that the policy
drops outright once the work lands.

Delete every tracked file under `docs/plans/` that satisfies **either**

- its frontmatter `phase:` is `shipped`, `complete`, `dropped` or `reverted`, or
- its name matches `*HANDOFF*.md`, `*HANDOVER*.md` or `*-handover.md`,

**unless** its path is cited from `crates/`, `scripts/`, `.github/` or
`docs/ux-rules.md`. Compute that citation set, do not trust this list:

```
grep -rhoE 'docs/plans/[A-Za-z0-9._-]+\.md' crates scripts .github docs/ux-rules.md | sort -u
```

`scripts/check-architecture.sh` fails the build when a `docs/…` path named from
`crates/` or `scripts/` does not resolve, so that grep is the fence. Ten paths
match it today. Also keep `docs/plans/README.md` itself and the two maintained
implementation records the policy names, `android-sync.md` and
`ux-rules-acceptance-tests.md`, whatever their phase says.

Leave the 48 files that carry no `phase:` line alone — absence of a phase is not
evidence of completion, and the policy does not cover them.

Markdown links from surviving plans into deleted ones are expected and
deliberately not gated; `docs/plans/README.md` says so. Do not repair them.

Expect roughly 45 deletions. If the count lands far outside 35–55, stop and say
so rather than adjusting the rule to hit the number.

Delete the matching `docs/evidence/` subdirectories too, where the plan that
produced them is going away — the policy names them in the same breath.

### 2. `docs/` says which half is for visitors

Add `docs/README.md`, a short index, no more than 40 lines. Two lists:

- **For readers of the project** — `ux-rules.md` (the behaviour contract that
  outranks the code), `showcase.md`, `adr/`, `releasing-android.md`.
- **Internal working notes** — `plans/`, `superpowers/`, `agents/`,
  `automation/`, `research/`, `measurements/`, `evidence/`, `benchmarks/`.
  One sentence saying these are kept in the open on purpose but are session
  material, not documentation.

`_config.yml` already excludes `superpowers/`, `plans/` and `automation/` from
the rendered Pages site and explains why; the new file covers people who browse
the tree on GitHub instead. Do not change `_config.yml`.

### 3. The README sends humans to CONTRIBUTING first

In `README.md`, the Contributing section links `AGENTS.md` before
`CONTRIBUTING.md`. `AGENTS.md` is a 35 KB agent runbook whose first heading is
"Resuming work on Reprise" — a fine thing to disclose, a poor first click for a
person who wants to build the project.

Put `CONTRIBUTING.md` first and describe `AGENTS.md` as what it is: the runbook
that coding agents pick up from. Keep both links. Change nothing else in the
section, and do not touch the "How this project is built" disclosure.

### 4. Eleven tests that are copies of other tests

Each pair below was confirmed byte-identical apart from whitespace. Delete the
first path in each group; the second keeps the coverage.

1. `crates/reprise-core/src/db_recent_migration_tests.rs:32`
   `migrate_v5_to_v6_creates_lastfm_queue_and_preserves_listenbrainz_rows`
   — identical to `crates/reprise-core/src/db_tests.rs:482`. Both modules are
   included from `db.rs` and both run on every invocation.
2. `crates/reprise-gnome/src/ui/window/library_chrome_css.rs:34`
   `style_1_chrome_surfaces_declare_background_and_edge`
   — identical to `crates/reprise-gnome/src/ui/window/library_chrome.rs:217`.
3. `crates/reprise-gnome/src/ui/track_list/current_track_selection_glide_tests.rs`
   lines 132, 140 and 156 — `visible_position_finds_the_current_track_in_view_order`,
   `visible_position_uses_queue_occurrence_then_falls_back_to_first_match` and
   `queue_does_not_highlight_a_pending_duplicate_of_the_current_track` — carried
   over verbatim from `current_track_selection_tests.rs:379/387/403` when that
   file was forked. The two genuinely new tests in the glide file stay.
4. The six `net_2a_*` tests that
   `crates/reprise-core/src/db_recent_migration_tests.rs` shares by name with
   `crates/reprise-core/src/db_network_migration_tests.rs`. Both drive the same
   `migrate_with_cache_dirs` entry point over identically seeded schema state;
   the network file is the on-topic home. Delete the copies in
   `db_recent_migration_tests.rs`, keep every one in
   `db_network_migration_tests.rs`.

`style_1_` and `net_2a_` are UX-rule traceability anchors.
`scripts/check-ux-traceability.sh` requires **at least one** test per active
rule ID, and every deletion above leaves the other copy in place — so the gate
stays green. Before deleting any of them, confirm that:

```
grep -rhA5 --include='*.rs' '#\[test\]' crates | grep -oE 'fn (style_1|net_2a)_[0-9a-z_]+'
```

still reports a surviving name for each ID afterwards.

Nothing else in the suite is thinned. 6,694 Rust tests over ~440k lines is one
test per 65 lines; a duplicate scan of every test body with literals normalised
found five pairs in the whole workspace. The 847 tests carrying
`#[ignore = "requires a display; run via xvfb-run"]` are a deliberately gated
suite and are load-bearing for the traceability gate — leave every one of them.

### 5. The filter bars stop repeating three helpers

`chooser_row` is byte-identical in
`crates/reprise-gnome/src/ui/releases/releases_filter_bar.rs` and
`crates/reprise-gnome/src/ui/concerts/concerts_filter_bar.rs`:

```rust
fn chooser_row(label: &str) -> gtk4::ListBoxRow {
    let label = gtk4::Label::builder()
        .label(label)
        .xalign(0.0)
        .margin_top(7)
        .margin_bottom(7)
        .margin_start(10)
        .margin_end(10)
        .build();
    gtk4::ListBoxRow::builder().child(&label).build()
}
```

Move it into `crates/reprise-gnome/src/ui/filter_bar_layout.rs` as
`pub(crate) fn chooser_row`, and have both call sites use it. Do the same for
`padded` (`releases_filter_bar.rs`) and `page_box` (`concerts_filter_bar.rs`)
**only if** a second definition of that helper is byte-identical somewhere else
under `crates/reprise-gnome/src/ui/`; a helper with one call site stays where it
is. Check first, do not assume.

This is the whole filter-bar item. The five bars are **not** four copies of one
file: each already uses the shared `FilterBarLayout` skeleton (12–21 references
apiece) and the rest of each file is genuine per-domain logic — radio facets,
release windows, concert radius and horizon, podcast state. Do not unify them,
do not introduce a trait, and do not move domain logic into the shared module.

### 6. `cross-target.yml` stops rebuilding cargo-xwin from source

`.github/workflows/cross-target.yml` runs `cargo install --locked cargo-xwin`
on every run — measured at 83 s in run `33538980700` — and caches only
`~/.cargo/registry` and `~/.cargo/git`, never `~/.cargo/bin`.

Cache the built binary, keyed on the pinned version already in the workflow
(`0.23.0`) plus the runner OS, and skip the install when the cache hits. This is
the only CI file this plan touches; `ci.yml` and `release.yml` belong to a2.

### 7. Two dead items

`crates/reprise-gnome/src/ui/.../session_player.rs` — `restore_should_start_playback`,
reached only from `debug_assert!` and tests — and `issue_collapse.rs` —
`CollapsedList::new`, zero callers. Both were named by an audit that ran against
a stale checkout: **verify against this worktree first** with a call-site grep,
and remove only what is genuinely unreachable. If either is live, say so and
leave it.

## Out of scope

- Anything under a2's ownership list above.
- The untracked `docs/plans/*.md` files in the shared main checkout. They are
  other sessions' working copies, they are invisible to anyone browsing the
  repository, and untracked means no history to recover them from.
- `CHANGELOG.md` and `data/io.github.marvinbaudach.Reprise.metainfo.xml`. They
  are 39 versions stale and that is the single most reviewer-visible defect in
  the repository — but `bump-version.sh` selects the desktop app for any path
  under `data/*`, so the commit that curates the release text raises the version
  the text was curated for. They are curated after this branch lands, in the
  same window as the promotion.
- Any query or widget change made for performance. See the measurement above.

## Verification

The worktree must be clean, including untracked files, and the base ref must be
`dev` — against `origin/main` every feature branch reads as structurally stale.

```
cd /home/marvin/Projects/reprise-gnome-ready
MERGE_READINESS_BASE_REF=origin/dev heavy-run heavy -- \
  ./scripts/check-merge-readiness.sh --no-fetch ; echo "EXIT=$?"
```

Read the exit status directly, never through a pipe — `script | tail` reports
tail's status and is always 0.

The stages that decide this branch: **UX traceability** (task 4 must leave one
test per rule ID), **Architecture** (task 1 must not delete a cited plan),
**Rust lint** and **Workspace tests** (task 5), **Shell** (task 6).

## Done when

- `docs/plans/` holds no `phase: shipped`/`reverted` plan and no relay note
  except the ones a citation pins, and `scripts/check-architecture.sh` is green.
- `docs/README.md` tells a visitor which half of `docs/` is written for them.
- `README.md`'s Contributing section links `CONTRIBUTING.md` first.
- The eleven duplicated tests are gone and `check-ux-traceability.sh` is green.
- `chooser_row` has one definition.
- `cross-target.yml` restores cargo-xwin from a cache instead of building it.
- The full gate above prints `EXIT=0`.
