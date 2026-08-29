# Getting `dev` green again — session handover

Session of 2026-08-28. Written because `dev` is still red at handover time and
the reasoning behind three landings and one open PR should not die with the
session.

## What landed

| PR | Merge commit | What |
| --- | --- | --- |
| #725 | `a123bf464d` | The sort leaves the browse bar and joins the table customization surface |
| #731 | `d731511342` | The wide-toggle test pins its sidebar like the window does |
| #625 | `06ae442415` | Bundle the AcoustID client key into the Flatpak build — **landed by a different session**, not this one |

`#725` had been sitting open and conflicting since 08:12. Only `po/*` conflicted
(gettext line-number churn); every Rust file auto-merged. It was rebased, the
catalogues regenerated with the `xgettext` invocation from
`scripts/tests/gettext-catalogs.sh` itself and `msgmerge --no-fuzzy-matching`
per locale with dev's catalogues as the compendium, so the msgids from
#726/#727/#729 kept their translations. `de`/`es` ended at 0 untranslated,
0 fuzzy.

## `dev` is red, and it is not one defect but two

**Neither is caused by the three landings above.** Both come from #732 ("The row
says what it is loading").

### 1. The architecture gate — a file one line over the ceiling

```
crates/reprise-gnome/src/ui/podcasts/podcasts_view.rs has 801 lines;
Rust source files must stay below 800
```

**Fixed in PR #734** (`fix/podcasts-view-drops-below-the-line-ceiling`,
worktree `/home/marvin/Projects/reprise-podcasts-view-thinness`, commit
`26eafb522e`). The contiguous tail of `impl PodcastsView` — `toggle_download`
through `flush_download_toast` — moved into a sibling
`podcasts_view_downloads.rs`. 801 → 515 lines, new file 294. Verified to be a
move rather than a rewrite: diffing the removed block against the new file's
body yields 20 lines, all of them the same `fn x` → `pub(super) fn x` shape.
Five methods had to widen because sibling submodules call them — the same reason
`install_actions` already carries `pub(super)`.

Two things worth knowing before touching that area again: the name
`podcasts_view_actions.rs` was **already taken** (623 lines), and the
`podcasts_view_*` submodules are declared with `#[path]` **inside
`podcasts_view.rs`**, not in `mod.rs`. A plan that assumes otherwise will
destroy existing code.

### 2. The motion-tokens gate — still unfixed

This is the open work. `scripts/check-motion-tokens.sh` fails:

```
ERROR: literal CSS animation duration outside ui/motion.rs or ui/style/tokens.rs:
  crates/reprise-gnome/src/ui/podcasts/css.rs
    15:  transition: border-color 250ms ease, background-image 250ms ease;
    29:  animation: reprise-podcast-shimmer 1900ms linear infinite;
    33:  animation: reprise-podcast-spin 900ms linear infinite;
    36:  animation: reprise-podcast-breathe 2000ms ease-in-out infinite;
ERROR: literal animation duration outside ui/motion.rs or ui/style/tokens.rs:
  crates/reprise-gnome/src/ui/podcasts/podcasts_groups.rs
    363:            artwork.widget().set_transition_duration(200);
```

Five literals, two files, both from #732. They belong in `ui/motion.rs` or
`ui/style/tokens.rs`. **PR #734 does not touch them, so #734 alone will not turn
`dev` green.**

Note the gate-stage geography: motion tokens runs in a *different* CI job from
the architecture check, which is why a run can show "GNOME quality suite:
success" while `dev` is still red. Read the job list, never the run conclusion.

### 3. rustfmt, and the ceiling behind it

#736 ("The waveform works before the first play") landed with six files under
`ui/playback/` unformatted, so both quality suites stopped at `rustfmt --check`
before running a single test. **Landed as #737 (`f4496e6205`).**

The part worth carrying forward: `cargo fmt` alone did not fix it. #736's new
field declaration is 101 columns, rustfmt wraps it, and that pushed
`player_controller.rs` from 799 to exactly 800 lines — one over the architecture
ceiling. **The file had cleared that lint only because it was merged
unformatted.** The two lints live in different CI jobs and neither sees the
other's premise, so a formatting fix can hand you an architecture failure that
was latent all along. The documented `### Toast + track-list-reload seam` pair
(`show_toast`, `reload_track_list`) therefore moved into
`player_controller_toast.rs` in the same PR — 767 lines left. One widening was
needed and it is the minimum: the field `reload_track_list` became `pub(super)`,
because only its reader left the file. Its declaration site is
`playback/mod.rs`, not a `#[path]` — unlike the podcasts submodules.

## The pending promotion

`dev` is **10 commits ahead of `main`**. The promotion was requested and is
deliberately **not** done: the `main` ruleset wants a green gate, and no dev CI
run since `d731511342` has completed at all — two were cancelled by the next
merge (concurrency group), the others died in a gate stage before the test suite
ran. Promote only after one full dev run completes green.

## Traps this session paid for

- **A green CI run can mean a skipped job.** The sidebar test looked flaky —
  red on #727, green on #729, red on #730 and #725. #729's run **skipped** the
  GNOME quality suite by path routing. Every run that actually executed the
  suite since #727 had failed. Diagnosing "flakiness" starts with
  `gh run view <id> --json jobs`, not with the run's conclusion.
- **A fresh worktree cannot pass the Android gate stage.** `android/local.properties`
  and the generated UniFFI Kotlin bindings are gitignored. Generate them with
  `ANDROID_HOME=/home/marvin/.local/share/android-sdk scripts/android-build.sh`;
  do not copy them from the main checkout, and do not reach for
  `MERGE_READINESS_SKIP_ANDROID_QUALITY=1` locally — it buys a gap, not coverage.
- **`MERGE_READINESS_BASE_REF=origin/dev` is mandatory.** The script defaults to
  `origin/main`, against which every feature branch reads as stale.
- **A worktree can be rebased under you.** The #725 worktree was rebased by
  another session at 16:01:52 while this one was working in it. It was harmless —
  verified by diffing old tip against new tip (exactly #730) and confirming the
  branch's own payload was character-identical — but it has to be checked, not
  assumed. Before trusting any gate run, confirm no foreign process is in the
  worktree.

## Concrete next steps

1. Land #734 once its gate is green (`land.sh 734 --no-plan` — no plan file
   claims that branch). Its own merge-readiness run failed **only** at the
   motion-tokens stage, i.e. on the inherited red described above, and passed
   every stage before it.
2. Move the five literals out of `podcasts/css.rs` and
   `podcasts/podcasts_groups.rs` into `ui/motion.rs` / `ui/style/tokens.rs`,
   land that.
3. Watch the first dev CI run that completes and contains both. Expect
   cancellations; the evidence is the next *completed* run whose head has the
   commits as ancestors.
4. Only then promote `dev` → `main` (merge PR, no fast-forward).

Unrelated but noticed: the `Base and contract checks` job also emits a Biome
notice — `biome.jsonc` pins schema `2.5.8` while CI installs Biome `2.5.10` at
job time. It is an `info`, not the failure, but it is the same class of trap as
CI pulling Rust at job time and will bite once a rule changes.
