---
slug: refactor-wave-2026-10-w4
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-05
strands: a,b,c,d,e
merge_order: c,e,d,b,a
---
# Refactor wave 2026-10 — wave 4: the open remainder

Waves 1–3 landed (#1068, #1070, #1071, #1082, #1084, #1094, #1105, #1114); #1115 deleted their
plans, including the program mother plan. This file therefore carries the standing rules itself.
The user said "start everything that is open" on 2026-10-05; this wave is the cut of that list that
is not already done and not owned by another live session.

Strand plans: `refactor-wave-2026-10-w4a.md` (alias layer, part 1), `…-w4b.md` (parameter
objects), `…-w4c.md` (gettext catalogs), `…-w4d.md` (lyrics: smoke root cause and fixture gating),
`…-w4e.md` (gate hermeticity: motion-token gate and stems readiness order). Each strand runs in its
own worktree, headless, with the plan file as its only channel. Implementation is done by Sonnet
`worker` agents (Codex is unavailable); the plans are written for a literal reader.

## Shared context

- Base: `origin/dev @ 9465e997e8` (2026-10-05, after #1116). Every number below was measured there.
  **The main checkout `/home/marvin/Projects/reprise` is behind (`0322ae01df`)**; a worker must
  branch from `origin/dev`, never from the local `dev`.
- Everything is behaviour-preserving unless a strand says otherwise in one sentence (w4c changes
  catalog files, w4d changes what the smoke harness seeds and what release builds compile, w4e
  changes the order of two readiness checks). No user-visible string changes, no schema change, no
  SQL text change, no change on the wire, no accessibility-role change.
- Toolchain: CI runs a newer clippy (1.99) than local (1.97). `allow_attributes_without_reason` and
  `significant_drop_in_scrutinee` are workspace lints. Every strand passes
  `cargo clippy --all-targets --workspace -- -D warnings`, the same with `--all-features`
  (cli `mpris`+`worker`, mcp `mpris`, gnome/core `test-fixtures`), and
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`.
- Budgets that must equal reality after every landing: `http_boundary_budget` (5) and
  `too_many_arguments_budget` (29 → 19 after w4b) in `scripts/check-architecture.sh`; in
  `scripts/check-frontend-thinness.sh` `view_floor=2515`, `[rusqlite]=115`, `[filesystem]=13`,
  `[threads]=14`, `[workers]=7` — no strand here moves any of the thinness numbers.
- Every *code* file touched ends below 800 lines; near-cap files are named in the strand plans.
- `docs/plans/README.md` deletes wave plans on landing. **No code, script or comment may cite a
  `docs/plans/refactor-wave-…` path**: the "Documentation references from code" gate in
  `scripts/check-architecture.sh` fails on the first merge after the plan is gone.
- English everywhere; focused commits with prose subjects in the repository's style; no agent
  attribution lines in commits or PR bodies; one squashed pull request into `dev` per strand per
  `docs/agents/branching.md`; no `dev` → `main` promotion in this program.
- Worktrees live under `/home/marvin/Projects/reprise-<slug>` or `.worktrees/<slug>`, never under
  `/tmp`. Headless runs use the full isolation recipe from `AGENTS.md`
  (`dbus-run-session -- xvfb-run -a env XDG_DATA_HOME=… XDG_CACHE_HOME=… GDK_BACKEND=x11
  WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink …`); `scripts/check-lyrics-smoke.sh` already
  contains it.
- A gate run measures the worktree, not the commit: no foreign session in the strand's worktree.

## Foreign sessions and the overlap decisions

Four other sessions are live. Their files were read from
`git -C <worktree> diff --name-only origin/dev...HEAD` plus uncommitted status on 2026-10-05 (147
distinct paths, saved as the planner's `foreign-files.txt`). No strand here edits any of them.

| Session | Branch | What it owns that this wave had to route around |
| --- | --- | --- |
| loudness and cue sheets, R128 | `feature/loudness-and-cue-sheets-r128` | `crates/reprise-platform-linux/src/player.rs`, `player/tests/cava_tests.rs` and siblings; `crates/reprise-android-ffi/src/{playback,trash_boundary_tests}.rs`, `playback_session/trash_boundary.rs`; `crates/reprise-gnome/src/ui/lyrics/player_lyrics{,_tests}.rs`, `ui/playback/{audio_effects,external_media,external_media_artwork,player_controller,seek_start_tests,session_player_tests,up_next_transport,external_media_classify_tests}.rs`, `ui/session_restore.rs`, `ui/mpris_play_context.rs`, `ui/track_list/start_restore_tests.rs`, two device-sync test files |
| the tests stop waiting, strand b | `feature/the-tests-stop-waiting-b` | `crates/reprise-cli/src/retry.rs`, `crates/reprise-cli/tests/{busy_retry.rs,common/**}`, `ui/podcasts/add_dialog_followers.rs`, `ui/stats/stats_{bands,songs}_card_tests.rs`, ytdlp and watcher files in core |
| the tests stop waiting, strand c | `feature/the-tests-stop-waiting-c` | `ui/now_playing/song_visualizer_tests.rs` (re-tags `bars_fullscreen_render_budget_diagnostic` as `measurement:`), `ui/sidebar/sidebar_device_card_mirror_tests.rs`, `ui/concerts/concerts_visual_tests.rs`, `ui/strings_podcasts.rs`, `ui/style/composed_css_tests.rs`, `ui/track_list/diagnostic_trail_tests.rs`, `tests/gnome_conformance.rs`, doctor `remote/{arbitration,diagnostics,mod}.rs`, `queries/autocomplete.rs` |
| visualizer cold start | `fix/visualizer-cold-start` | `crates/reprise-core/src/playback/cava.rs`, `cava/smoothing.rs`, a new probe example |

Decisions recorded:

1. **CAVA flakes `ac_26`/`ac_28`** (`crates/reprise-platform-linux/src/player/tests/cava_tests.rs`,
   wall-clock since #1090) — **dropped from this wave.** The test file is in the R128 diff and the
   core CAVA path is the visualizer-cold-start session's subject. Whoever lands last owns the flake.
2. **`trash_boundary_tests::writer_is_free_…`** — **dropped.** `e3905f96b0` (#1109, "The trash tests
   wait for the restored queue to persist before they probe the writer") added the missing wait
   (+3 lines in `trash_boundary_tests.rs`); the same file is also in the R128 diff. Re-check only if
   it flakes again after R128 lands.
3. **`bars_fullscreen_render_budget_diagnostic`** — **dropped.** `the-tests-stop-waiting-c.md` item 5
   re-tags it `measurement:` and owns `song_visualizer_tests.rs`.
4. **`worker_without_fake_backend_and_no_provisioned_model_is_unavailable`** — **kept (w4e).** The fix
   lives in `crates/reprise-stems/src/provision.rs`, which no session touches; the CLI test file
   `tests/worker_basic.rs` is not in strand b's list (`tests/common/**` is, and w4e does not edit it).
5. **Alias layer** — **split.** Of the 20 alias families in `ui/mod.rs`, 8 have call sites inside
   foreign-owned files (`playback`, `cover`, `track_list`, `lyrics`, `scrobbling`, `scan`, `browse`,
   `compact`; 15 foreign files in total). w4a retires the 12 families with zero foreign call sites;
   the 8 others are "what stays for later" with the same recipe.
6. **Lyrics smoke** — **kept (w4d)** because the fix is in `ui/lyrics/lyrics_smoke.rs` (not foreign)
   and core; `player_lyrics.rs` (R128) is only called, never edited.

## The cut

| Strand | Delivers | Owns (summary; the strand plan has the exact list) | Size |
| --- | --- | --- | --- |
| **A — alias layer, part 1** | the 48 re-export aliases of 12 feature families leave `ui/mod.rs`; ~190 call sites use the real `crate::ui::<family>::<module>` paths; the "Compatibility surface" comment shrinks to the 8 remaining families | `crates/reprise-gnome/src/ui/mod.rs` (alias block only) and every file that references one of the 48 aliases (measured: ~80 files under `crates/reprise-gnome/src`, none foreign-owned); family `mod.rs` files only for a visibility bump the compiler demands | GTK, mechanical, ~80 files |
| **B — parameter objects** | 10 `too_many_arguments` suppressions whose reason asks for a parameter object are resolved with four types (`reprise_core::queries::TrackViewQuery` reused, `TagMutationChange`, `GroupRenderInputs`, `ResultSurface` + `AddOptions`) and one dropped unused flag; `too_many_arguments_budget` 29 → 19 | `ui/browse/browse_filter_count.rs`, `ui/track_list/{track_list_model,track_list_reload,tag_mutation_refresh,tag_mutation_refresh_block_move_display_tests}.rs`, `ui/updates/{concerts_section,popover}.rs`, `ui/podcasts/{podcasts_groups,podcasts_groups_tests,podcasts_sync_row_display_tests,podcasts_view,add_dialog}.rs`; `scripts/check-architecture.sh` line `too_many_arguments_budget=29` only | GTK, 12 files |
| **C — gettext catalogs** | the seven catalogs and the template are regenerated from the sources (1383 → 1329 live msgids; 546 obsolete entries dropped; de/es stay 100 % translated); a `scripts/update-catalogs.sh` holds the one extraction/merge recipe; `scripts/tests/gettext-catalogs.sh` fails on obsolete entries and on a stale template | `po/*.po`, `po/reprise.pot`, new `scripts/update-catalogs.sh`, `scripts/tests/gettext-catalogs.sh`, registration lines in `scripts/tests/qa-linters.sh` if its conventions demand one, one paragraph in `TESTING.md` | no Rust |
| **D — lyrics** | the smoke harness opens the global online-sources gate before the module (root cause of the red smoke since `ce66eb24a5`, 2026-07-30); the lrclib/NetEase fixture seams compile only under `cfg(any(test, feature = "test-fixtures"))` like the other five providers; the architecture gate pins that; the `.delay-ms` fixture delay the smoke relies on is honoured again | `crates/reprise-gnome/src/ui/lyrics/lyrics_smoke.rs` (+ a sibling test file), `crates/reprise-core/src/lyrics/{lrclib,netease}.rs` and their `_tests.rs` siblings, `crates/reprise-core/src/online_sources.rs` (test module only, if a pin is missing), `scripts/check-architecture.sh` (one new block after the HTTP-boundaries block) | core + 1 GTK file |
| **E — gate hermeticity** | `scripts/check-motion-tokens.sh` exempts sibling test files declared under `#[cfg(test)]` from the CSS scan (as it already exempts inline test blocks), with self-tests; `reprise-stems` reports a missing model before a missing runtime, so the CLI worker test is hermetic on a machine without onnxruntime | `scripts/check-motion-tokens.sh`, `scripts/tests/motion-tokens.sh`, `crates/reprise-stems/src/provision.rs` | scripts + 1 core-side file |

### Decisions a reviewer may dispute (one line each, with the reason)

1. A: only 12 of 20 families — the other 8 have call sites in 15 files that four live branches
   are editing; an import-line conflict in `player_controller.rs` is not worth a wave.
2. A: aliases are retired by deleting the `use` line and letting the compiler list the call sites —
   the measured counts in the plan are a guide, not a fence; the compiler is the oracle.
3. B: 10 of the 20 "parameter object" reasons — the six window-wiring sites (`install` ×2,
   `publish`, `wire_source_routing`, `route_to_place_with_viewport`, `preferences::new` with 22
   parameters) are one-caller composition-root wiring where a struct of handles is a side-grade;
   `replace_track` mirrors a platform-linux request whose own suppression says it keeps every
   transport parameter explicit (a cross-crate decision); `load_folders_if_current`,
   `reconcile_outcome` and `show_row_menu` are judgement calls recorded under "later".
4. B: `concerts_section::render` is resolved by dropping its unused `_has_credentials` flag (7
   parameters including `self` is under clippy's threshold), exactly what its reason asks.
5. C: a full regeneration (xgettext → msgmerge → strip), not just `msgattrib --no-obsolete` —
   54 msgids in the committed template no longer exist in the sources; the gate's `msgcmp` only
   checks the other direction, so a strip alone would keep 54 dead entries live.
6. C: `msgmerge --no-fuzzy-matching` — the gate forbids fuzzy entries anyway, so a changed string
   must surface as untranslated and be translated, not guessed.
7. C: the strip uses `msgattrib --no-obsolete` (gettext 1.0), not a text filter — obsolete entries
   carry their `#, rust-format` flag lines unprefixed, and a `#~`-line filter would leave those
   flags dangling on the next entry (measured: 84 such lines in de and es).
8. D: the core default (`online-sources-enabled` off on a fresh database) stays; the harness seeds
   what a user would have consented to, exactly as `test_db::open()` does for every GTK test.
9. D: the `.delay-ms` delay is restored in the lrclib fixture seam (test-only code) because the
   smoke's "Slow stale" rejection is the stale-response race it exists to exercise; without the
   delay the greps pass while the race is not run.
10. D: fixture gating is enforced by gating the `const …FIXTURE…_ENV` declarations — the compiler
    then forces every read behind the same `cfg`, and a two-line gate can check the declarations.
11. E: the sibling-file exemption applies to the CSS scan only, mirroring the existing inline-block
    rule; the Rust literal scan keeps reading every file as it always has.
12. E: model-before-runtime order — "download the model" is the state the app can act on; the
    native-runtime check follows once a model exists. The doubly-broken machine sees the second
    error after the download instead of before it.

## Disjointness

- **GTK files.** A edits import lines and paths in files that reference the 12 families; B edits
  signatures, bodies and call sites in 12 named files. A file can appear in both (for example
  `track_list_reload.rs` may import a `sidebar` or `playlists` alias). The merge order resolves it:
  B lands first, A rebases and re-runs its per-family recipe (the compiler lists the leftovers).
  D's one GTK file, `ui/lyrics/lyrics_smoke.rs`, references only `lyrics`/`playback` aliases, which
  A does not retire; no B file is in D. E touches no GTK file.
- **`crates/reprise-gnome/src/ui/mod.rs`.** Only A edits it.
- **Core files.** D edits `lyrics/{lrclib,netease}.rs` (+ tests) and at most the test module of
  `online_sources.rs`. E edits `reprise-stems/src/provision.rs`. B's only non-GTK dependency is
  the existing `reprise_core::queries::TrackViewQuery`, read, not edited. A and C touch no core file.
- **`scripts/check-architecture.sh`.** B changes the single line `too_many_arguments_budget=29`
  (line 301). D inserts a new block directly after the `== Engine HTTP boundaries ==` block (after
  line 270, before the too-many-arguments comment at line 298). Different hunks; whichever lands
  second rebases trivially.
- **Scripts.** C owns `scripts/tests/gettext-catalogs.sh` and the new `scripts/update-catalogs.sh`;
  E owns `scripts/check-motion-tokens.sh` and `scripts/tests/motion-tokens.sh`. If both need a line
  in `scripts/tests/qa-linters.sh` (registration of a new executable), they are different lines in
  the same list; E lands after C and rebases.
- **Catalogs and strings.** Only C touches `po/`. No strand adds, removes or changes a user-visible
  string, so `scripts/tests/gettext-catalogs.sh` stays green for A, B, D, E on the regenerated
  catalogs. (B and D add no `N_!` literal; a worker who finds itself adding one has left the plan.)
- **Manifests.** No strand edits a `Cargo.toml`.
- **Tests.** A changes no test body (paths only). B adapts call sites in three test files it owns.
  C adds shell assertions. D adds one sibling test file beside `lyrics_smoke.rs` and possibly one
  test in `online_sources.rs`. E adds shell self-test cases and one unit test in `provision.rs`.
  No test file is touched by two strands.

## Merge order

`c`, then `e`, then `d`, then `b`, then `a`.

- C first: no Rust, independent of everything, and the regenerated catalogs are what every later
  landing's gettext gate measures.
- E second: scripts and one core-side file; independent; small.
- D third: core lyrics plus one GTK file and the architecture-gate block.
- B fourth: the signature changes; lands before A so that A's mechanical path rewrite is the last
  thing to touch the import lines.
- A last, rebased onto the `dev` that B produced; its per-family recipe is re-run after the rebase.
- Any strand may land earlier than its slot if the strands before it are late, except A, which
  always lands after B.

## Post-merge cross-checks (on merged `dev`, after the last landing)

1. `scripts/check-architecture.sh`: `too_many_arguments_budget` equals the measured count (expected
   19); `http_boundary_budget` still 5; D's new fixture-seam block is green; size caps hold
   (`track_list_model.rs` 763, `track_list_reload.rs` 771, `podcasts_groups.rs` 719,
   `podcasts_groups_tests.rs` 742, `add_dialog.rs` 711, `provision.rs` 766 — none may cross 800).
2. `scripts/check-frontend-thinness.sh`: all five numbers unchanged (2515 / 115 / 13 / 14 / 7).
3. `scripts/check-motion-tokens.sh` and `scripts/tests/motion-tokens.sh` green; the gate still
   reports a literal in a production file (E's negative self-test proves it).
4. `scripts/tests/gettext-catalogs.sh` green on the regenerated catalogs; `rg -c '^#~' po/*.po`
   prints 0 for every file.
5. `cargo clippy --all-targets --workspace -- -D warnings`, the same with `--all-features`,
   `cargo clippy -p reprise-core --all-targets -- -D warnings` (test-fixtures off — proves D's
   gating leaves no dead code), `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps`, then
   `scripts/check-merge-readiness.sh` on a clean integration worktree.
6. `scripts/check-lyrics-smoke.sh` passes (warm `cargo build -p reprise-gnome --features
   test-fixtures` first; the script's own `timeout 15s` does not cover a cold build) and the request
   log holds exactly three lines.
7. `cargo test -p reprise-cli --features worker worker_without_fake_backend` passes with
   `ORT_DYLIB_PATH` unset on a machine without `libonnxruntime.so`.
8. Greps that must print nothing: `rg -n 'docs/plans/refactor-wave' crates scripts`;
   `rg -n '^(pub\(crate\) )?use (artist_news|device_sync|library_views|now_playing|player_bar|playlists|preferences|sidebar|spectrogram|stats|tag_edit|window)::' crates/reprise-gnome/src/ui/mod.rs`;
   `rg -n 'REPRISE_LYRICS_FIXTURE|REPRISE_LRCLIB_FIXTURE' crates/reprise-core/src --glob '*.rs' -B1 | rg -v 'cfg\(any\(test'` must show only gated declarations (read the output).
9. Scan `AGENTS.md`: no ownership section of this wave is left behind; the strand plans are deleted
   on landing per `docs/plans/README.md`.

## What stays for later

- **Alias layer, part 2** — the 8 families `playback` (6 aliases, 54 refs in 44 files), `cover`
  (4 / 130 / 75), `track_list` (18 / 122 / 55), `scan` (5 / 33 / 16), `lyrics` (6 / 15 / 11),
  `scrobbling` (4 / 10 / 8), `compact` (5 / 7 / 6), `browse` (2 / 3 / 3), blocked by the foreign
  files listed above (`player_controller.rs`, `session_restore.rs`, `external_media_artwork.rs`,
  `external_media.rs`, `up_next_transport.rs`, `audio_effects.rs`, `seek_start_tests.rs`,
  `mpris_play_context.rs`, `player_lyrics{,_tests}.rs`, `start_restore_tests.rs`,
  `diagnostic_trail_tests.rs`, `stats_{bands,songs}_card_tests.rs`,
  `sidebar_device_card_mirror_tests.rs`). Unblock condition: those branches merged or deleted.
  Same recipe as w4a; `cover_download_worker` alone has 95 references.
- **Parameter objects, remainder** — 10 suppressions whose reason still asks for an object:
  `window_playing_source_wiring::install` (7), `window_layout_test_hook::publish` (8),
  `window_deferred_source_wiring::install` (9), `library_shell::wire_source_routing` (16, wants
  `ContentPages` plus a pages bundle), `library_shell::route_to_place_with_viewport` (8),
  `preferences::new` (22, a context struct), `device_sync_types::replace_track` (10, needs the
  platform-linux copy-request type first), `device_sync_target_browser::load_folders_if_current`
  (8), `scan_worker::reconcile_outcome` (8), `missing_menus::show_row_menu` (9). Budget after this
  wave: 19. The nine "keeps … explicit" suppressions are not candidates by their own reason.
- **Catalog hygiene** — the `strings_scrobbling.rs:72` embedded-URL warning xgettext prints; the
  five incomplete locales (fr 1008, zh_CN ~1157, hi ~1167, ar ~1121, bn ~1185 untranslated after
  regeneration).
- **Lyrics** — one fixture variable with a subdirectory per provider (w3 left this; needs the
  smoke, `scripts/ptr-e2e/run.sh`, `scripts/cua-e2e/*.sh` and the mcp tests in one change); the
  legacy `REPRISE_LRCLIB_FIXTURE_*` names once the smoke script is migrated; the lyrics breaker's
  behaviour under fixtures.
- **Flakes owned elsewhere** — CAVA `ac_26`/`ac_28` (R128 / visualizer-cold-start),
  `trash_boundary_tests` (fixed by #1109, R128 owns the file), the render-budget diagnostic
  (tests-stop-waiting-c).
- **From wave 3, unchanged** — the ~700 internal `rusqlite::Error` signatures; folding the domain
  enums into `CoreError`; the transitional `From<CoreError> for rusqlite::Error`;
  `reprise-gnome`'s own `rusqlite` dependency; `SourceTransportError`; connecting the breaker to
  the other providers; `stream_proxy.rs`; `http_body.rs` into `net`; `reprise-stems`' own
  `ureq::get`; the real "one add dialog" (podcast phase model, unified result container under an
  ACC rule, radio's missing dialog title, `preview_name_claim`'s untranslated msgid).
- **CI never builds `reprise-cli --features worker`** (no `--all-features` test job), so the
  w4e CLI test stays a local-only proof; adding a feature matrix is a CI-polish item for the
  tests-stop-waiting program's strand d.
