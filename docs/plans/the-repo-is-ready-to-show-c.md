---
slug: the-repo-is-ready-to-show-c
worktree: /home/marvin/Projects/reprise-the-repo-is-ready-to-show-c
branch: feature/the-repo-is-ready-to-show-c
phase: planned
codex_session:
created: 2026-08-31
---
# Strand c — The project surface

Part of `docs/plans/the-repo-is-ready-to-show.md`. Read the mother plan first:
it carries the decisions, the full cut, the merge order and the post-merge
cross-checks. **Merge position: third, after strands a and b.**

Everything a visitor reads. This strand touches no application code.

## File ownership

`README.md`, `CONTRIBUTING.md`, `AGENTS.md`, `TESTING.md`, `CODE_OF_CONDUCT.md`,
`docs/**` **except** `docs/adr/**` (strand b's), `po/**`, `LICENSES/**`,
`REUSE.toml`, `.gitignore`, `.superpowers/**`, `.github/**`, `.github/**` **except** `.github/scripts/**` (strand a's — it invokes the gate
a3 deletes), `artifacts/**`,
`scripts/check-project-quality.sh`,
`data/io.github.marvinbaudach.Reprise.metainfo.xml`,
`data/io.github.marvinbaudach.Reprise.desktop`.

Touch nothing else. In particular: no `crates/`, no `android/`, no
`meson.build`, no `data/meson.build`, no Flatpak manifest, and **not**
`.github/scripts/**` and **not** `scripts/check-merge-readiness.sh` — strand a owns the gate list and has already
wired `check-release-metadata.sh` for the content you write in c3.

Strands a and b have landed when this runs, so the workspace is already at nine
crates and the release-metadata gate is already active.

---

## c1 — The tree becomes REUSE-compliant

Measured: **0 of 2,646** project-authored tracked files carry an SPDX header
(2,648 tracked minus two stock Gradle wrappers), there is no `.reuse/dep5` or
`REUSE.toml`, and `LICENSES/` holds only `CAVA-MIT.txt` and `PHOSPHOR-MIT.txt`.
The project is GPL-3.0-or-later throughout (`LICENSING.md`).

- Add `LICENSES/GPL-3.0-or-later.txt` (the verbatim SPDX text).
- Add a root `REUSE.toml` assigning copyright and licence **by path glob** —
  one file, no per-file headers. Cover the vendored exceptions the existing
  `LICENSES/` entries imply (cava, phosphor) and the two stock Gradle wrappers
  explicitly.
- Add a `reuse lint` step to `scripts/check-project-quality.sh`, guarded on the
  tool being present and **reporting its absence** rather than passing silently.

`reuse` is not installed on this machine. Do not install it; if it cannot be
run, say so in the commit message instead of claiming a passing lint.

Commit: `docs: make the tree REUSE-compliant`

## c2 — Every string reaches a catalogue

Verified: `crates/reprise-gnome/src/ui/strings_location.rs` (23 `N_!`),
`crates/reprise-gnome/src/ui/strings_scan.rs` (8) and
`crates/reprise-view/src/device_sync.rs` (33) are missing from
`po/POTFILES.in` — 64 strings invisible to all seven catalogues, including the
`de` and `es` ones reported as 100 % complete.

- Add the three paths to `po/POTFILES.in` and regenerate the template and
  catalogues. Strand b has already marked the five hardcoded `.set_label(...)`
  strings, so they appear in the same sweep.
- The metainfo carries hand-written `xml:lang="de"` translations inline
  (summary, description paragraphs, keywords) and **none for `es`**, although
  `es.po` is complete and `es` is listed in `po/LINGUAS`. Add the `es` variants
  to `data/io.github.marvinbaudach.Reprise.metainfo.xml` and
  `data/io.github.marvinbaudach.Reprise.desktop`.

Verify with `scripts/tests/gettext-catalogs.sh` — and note that the gettext gate
fails on the first locale only, so a green first locale is not proof for the
rest.

Commit: `i18n: give the orphaned string files a catalogue`

## c3 — The release list catches up

`data/io.github.marvinbaudach.Reprise.metainfo.xml:114` has
`<release version="0.1.84" date="2026-08-27">` as its newest entry;
`Cargo.toml` is at 0.1.111. Write the missing entries from `CHANGELOG.md`, in
the same style as the existing ones, up to 0.1.111.

Strand a has already added `scripts/check-release-metadata.sh` in full mode to
the merge gate, so this content is checked from now on. Run that script here.

While in the file: fill the empty `<content_rating>` stub and correct the stale
`display_length` (768 declared against a measured 600 floor).

Commit: `docs: bring the release list up to the shipped version`

## c4 — The internal ledger stops being published

`.superpowers/sdd/progress.md` is a tracked **1,161,859-byte** blob on
`origin/main` — 2,034 lines of raw agent task log, one of only three tracked
files over 1 MB. It is the artefact that reads as an AI content farm on sight,
and it is not disclosure: the disclosure is the README paragraph and the named
`Co-authored-by` trailers, which work as intended.

The project already removed a comparable file once — `50802ebfbc`, "Stop
tracking the pipeline scratch summary (#712)". This one was missed.

- `git rm --cached` it and add it to `.gitignore`. The local file stays.
- `AGENTS.md` calls it "the authoritative ledger". In the same commit, rewrite
  that section to name `git log` as the record — which `AGENTS.md` already calls
  the ground truth two lines later.

The `Claude-Session:` trailer on 29 existing commits is **out of scope**: it
originates in the Claude Code harness, not in this repo (verified — it is in
neither `~/.claude/settings.json` nor `.githooks/`). No history rewrite.

Commit: `docs: keep the task ledger out of the published tree`

## c5 — The contributor path stops contradicting itself

Each of these is a documented claim that is false at HEAD:

- **`CONTRIBUTING.md` says `meson setup build`, `README.md` says
  `meson setup _build`.** A contributor reading both in sequence gets a broken
  `meson compile -C build`. Settle on `_build` and fix the other. This is the
  one that actually breaks a person — do it first.
- **`CONTRIBUTING.md:26-32` lists 9 gates**, `scripts/check-merge-readiness.sh`
  has 27. Replace the hand-maintained list with a pointer to the script, which
  already derives the showroom's count from its own `gate()` calls.
- **`README.md:45-50` lists 6 crates.** After strand b the workspace is nine —
  write those nine. `AGENTS.md:9` and `:35` already say "nine-crate", so those
  lines become correct on their own; fix only their broken cross-references
  (`AGENTS.md:459` cites the deleted `docs/plans/list-views-fixes.md`).
- **`TESTING.md:8-11`** claims "1,482 passing tests… at performance close-out
  commit `a41c53f`" — 2,989 commits and about six weeks behind HEAD. It
  understates the project. Replace it with a pointer to the gate that counts, or
  with a figure measured while writing. **Do not carry a remembered number.**
- **`docs/agents/branching.md`** states, dated "verified live on 2026-08-17",
  that conventional-commit form is "not a matter of judgement". Measured on
  `origin/dev`: 1 of the last 40 squash-merged commits complies, the last 20 are
  0 of 20. Decision taken: **the documentation follows the practice.** Rewrite
  the commit-format section in `branching.md` and `CONTRIBUTING.md` to describe
  the narrative-title convention actually in use; remove the absolute wording
  and the stale "verified live" date.
- **`AGENTS.md` is the first link README's Contributing section gives a human**,
  and its H1 is "Resuming work on Reprise" — an agent-session runbook. Write a
  normal onboarding document and point README at that first; keep `AGENTS.md`
  linked, second.
- Add `.github/ISSUE_TEMPLATE/` (bug report, feature request). A PR template
  already exists.

Commit: `docs: fix the contributor path`

## c6 — The README shows the app

The README has no screenshot, although `data/screenshots/now-playing.png` exists
and is already wired into the metainfo. Its only image is an architecture
diagram, which reads as an engineering pitch rather than an app page.

Lead with what the app is and a screenshot. Keep the AI-disclosure paragraph —
decision E1 — but place and word it so it reads as engineering discipline
(machine-checked rulebook, the gate battery, the test suite) rather than as an
apology.

Separate commit from c5 so the visual change is reviewable on its own.

Commit: `docs: show the app in the README`

## c7 — The plans directory is compacted

Delete **82 files**: the 67 executed-and-superseded plans, the 15 handoff and
findings notes whose work has landed, and `docs/evidence/bounded-daemon-stop/`
(4 files), whose plan no longer exists — already a violation of the directory's
own retention rule. `docs/plans/` drops from 197 to ~115 files, roughly 26k of
~83k markdown lines.

**The 82 files, listed here so this strand needs no external input.** Validated
during the grill: none is build-gated, none is protected, none is untracked, and
`plugins-online-content-master-hierarchy.HANDOFF.md` below is *not* the
build-gated `plugins-online-content-master-hierarchy.md` (which stays).

- `always-download-episodes-core.md`
- `always-download-episodes.md`
- `always-download-episodes-ui.md`
- `an-agent-started-song-inherits-the-library-queue.md`
- `android-artist-photo-backfill-progress.md`
- `android-artists-show-only-portraits.md`
- `android-now-playing-desync-throttles-the-scene-a.md`
- `android-now-playing-desync-throttles-the-scene-b.md`
- `android-now-playing-desync-throttles-the-scene-c.md`
- `android-now-playing-desync-throttles-the-scene.HANDOFF.md`
- `android-now-playing-desync-throttles-the-scene.md`
- `android-saf-absence-misread-as-provider-failure.HANDOFF.md`
- `android-saf-absence-misread-as-provider-failure.md`
- `android-visualizer-pcm-in-playback-time.md`
- `bandsintown-app-id-is-not-a-setting.md`
- `bugliste-welle-2.HANDOFF.md`
- `chapter-two-one-incident.HANDOFF.md`
- `chapter-two-one-incident.md`
- `chapter-two-two-figures.md`
- `concerts-artist-portrait-column.md`
- `cua-harness-and-issue-sweep-2026-08-22.HANDOFF.md`
- `episode-covers-appear-seconds-after-start.md`
- `equalizer-profiles-lead-the-surface-core.md`
- `equalizer-profiles-lead-the-surface.md`
- `equalizer-profiles-lead-the-surface-ui.md`
- `external-changes-reach-device-sync.md`
- `fixes-444-mutations.HANDOFF.md`
- `frontend-performance-sweep-b.md`
- `frontend-performance-sweep-c.md`
- `gallery-hover-holds-the-frame-still.md`
- `issue-backlog-wave-1-1.md`
- `issue-backlog-wave-1-2.md`
- `issue-backlog-wave-1-3.md`
- `lastfm-preferences-one-primary-path.md`
- `layout-preferences-interactive-preview.md`
- `library-doctor-out-of-date-rows-are-unreadable.md`
- `lyrics-batch-only-new-finds.md`
- `navback-scroll-jump-to-top.findings.md`
- `one-bad-artist-no-longer-holds-the-queue.md`
- `one-centering-path-for-jump-and-clear.handover.md`
- `one-centering-path-for-jump-and-clear.md`
- `play-button-back-and-readable-primary-text.md`
- `plugins-online-content-master-hierarchy.HANDOFF.md`
- `podcast-cover-gate-review-findings.md`
- `queue-centering-ignores-section-headers.md`
- `queue-landing-flash-follows-the-drop.md`
- `queue-rebinds-on-filter-clear.md`
- `radio-genre-chip-drops-the-country.md`
- `release-channel-flatpak-and-apk.md`
- `releases-missing-cover-shows-the-band.md`
- `releases-multiselect-context-menu.md`
- `remembered-device-keeps-its-playlists.md`
- `resume-belongs-to-long-podcasts.md`
- `settings-writes-leave-the-tap-thread.md`
- `showroom-figures-and-timeline.HANDOFF.md`
- `showroom-figures-derive-themselves.md`
- `showroom-wave.HANDOFF.md`
- `source-artwork-is-decoded-in-full-on-every-view.md`
- `stats-hide-more-top-artists-stutters.md`
- `the-agent-surface-gets-its-units.md`
- `the-artist-page-clears-the-search-and-turns-the-cover.HANDOFF.md`
- `the-artist-page-clears-the-search-and-turns-the-cover.md`
- `the-fill-yields-not-the-accent.md`
- `the-mark-follows-the-page.HANDOFF.md`
- `the-panels-yield-the-table-its-width.md`
- `the-playlist-card-says-why-it-is-locked.md`
- `the-podcast-list-asks-for-every-cover-at-once.md`
- `the-row-height-certifies-itself-a.md`
- `the-row-height-certifies-itself-b.md`
- `the-row-height-certifies-itself.md`
- `the-row-says-what-it-is-loading.md`
- `the-sidebar-keeps-its-column.md`
- `the-sort-leaves-the-browse-bar.HANDOFF.md`
- `the-sort-leaves-the-browse-bar.md`
- `the-sync-bar-counts-work-not-bytes.md`
- `the-table-follows-the-music-again-a.md`
- `the-table-follows-the-music-again-b.md`
- `the-table-follows-the-music-again.md`
- `the-waveform-works-before-the-first-play.md`
- `track-list-blank-on-fresh-start.findings.md`
- `visuals-bars-fall-in-from-the-top-on-open.md`
- `youtube-channel-size-and-sorting.md`

Plus `docs/evidence/bounded-daemon-stop/` (4 files).

If this list is ever regenerated instead of used: The classification is in
`/tmp/claude-1000/-home-marvin-Projects-reprise/eb53fc23-1be8-4065-8012-cd106beaff90/scratchpad/docs.md`
(session-local — if it is gone, re-derive per file by checking whether the
commit that last touched the plan also changed code under `crates/`, `scripts/`
or `android/`; the `phase:` headers are demonstrably stale and must not be
trusted).

**Do not delete:**

- the 8 build-gated plans — `scripts/check-architecture.sh` scans `crates/` and
  `scripts/` for `docs/…md` citations and deleting these breaks the build:
  `architecture-consolidation.md`, `consolidation-plan.md`,
  `location-is-not-a-concerts-setting.md`, `multi-frontend-core.md`,
  `plugins-online-content-master-hierarchy.md`,
  `podcasts-youtube-radio-turn6.md`, `ptr-e2e-harness-debt.md`,
  `search-reload-blocks-the-main-thread.md`;
- `android-sync.md` and `ux-rules-acceptance-tests.md`, named as living plans by
  `AGENTS.md`;
- `queue-anchor-grill-followups.md` — read at build time by
  `showroom/vite.config.ts:12` and named as `INCIDENT_RECORD` at
  `showroom/src/data/measurements.ts:186`, neither of which any gate scans;
- `about-dialog-report-polish.md`, whose superseded status has only weak
  evidence;
- the 46 open-phase files and the 43 unclear ones;
- **under no circumstance** the 15 untracked files — they may belong to a
  parallel session. A fresh worktree will not contain them; if any appear, leave
  them.

Also: prune `docs/plans/assets/` (1.2 MB for three PNGs) to what a surviving
plan still references, and delete `artifacts/` (17 ad-hoc verification PNGs,
~2 MB, referenced nowhere).

Commit: `docs: compact the plans directory`

## c8 — Each rule gets one home

- The UX-rules contract is stated in `AGENTS.md` (~65-79) **and**
  `CONTRIBUTING.md` (~32-40). `docs/ux-rules.md` is the source; both shrink to a
  pointer.
- The branching rules are stated in `AGENTS.md` (~104-140) **and** in
  `docs/agents/branching.md`, which `AGENTS.md` also links. The linked document
  is the source.
- `AGENTS.md:53-54` restates a narrower plan-retention rule than
  `docs/plans/README.md`. Decision taken: the specific document inside the
  directory it governs wins. `AGENTS.md` shrinks to a pointer.

Commit: `docs: give each rule one home`

## c9 — Six GNOME rules become active

In `docs/ux-rules.md`, section `## AI. GNOME platform conformance` (from line
6762 — note it is section `AI`, not `AH`; section letters shift as content is
added, the `GP-` prefix is the stable anchor), flip to `[active]`:

- **GP-12, GP-13, GP-16** — rule-named tests exist in
  `crates/reprise-gnome/tests/gnome_conformance.rs` and their gates
  (`check-appstream.sh`, `desktop-file-validate`, name/summary length) pass
  independently of the other strands.
- **GP-3, GP-19, GP-20** — strand b scoped their detectors to production code
  and fixed what remained real (2 strong captures, 12 banner blocks, 19
  unexplained `#[allow(dead_code)]`).

**Do not flip:**

- **GP-1** — no `gp_1_` test exists. Flipping it is the retroactive flip
  `AGENTS.md` forbids ("a rule flips in the same commit that implements the
  behavior and adds its test — never retroactively"). Either write the test in
  this commit or leave it planned.
- **GP-2** — its detector looks for `sleep`/`block_on` and does not find the
  real violation strand b fixed. Making that detector correct is separate work.
- **GP-4** — the honest production number is ~24 `unwrap()`, not zero, and some
  are in UI paths.
- **GP-14** — strand a changed the manifest it certifies; this flip is
  post-merge cross-check 1 in the mother plan.

`scripts/check-ux-traceability.sh` enforces coverage only for `[active]` rules,
so run it after the flip and confirm all six are covered.

**This flip cannot be verified inside this strand.** The traceability gate only
checks that a rule-named test *exists*, not that its gate is clean — it passes
either way. GP-3, GP-19 and GP-20 hold only because strand b scoped their
detectors and fixed the real findings, and that is established by post-merge
cross-check 5's full gate run, not here.

Commit: `docs(ux-rules): activate the conformance rules that hold`

---

## Done when

`reuse lint` passes (or its absence is reported, not hidden); all seven
catalogues build with the three formerly orphaned files included and the
metainfo/desktop carry `es`; the metainfo lists releases up to 0.1.111 and
`check-release-metadata.sh` is green; `.superpowers/sdd/progress.md` is
untracked and ignored; `README.md` and `CONTRIBUTING.md` agree on `_build` and
list nine crates; `docs/plans/` holds ~115 files with all eight build-gated
plans intact and the 15 untracked files untouched; six GP rules are `[active]`
and `check-ux-traceability.sh` is green.

No file under `crates/`, `android/`, `meson.build`, `data/meson.build` or the
Flatpak manifest is touched by this branch.
