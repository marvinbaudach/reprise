---
slug: the-tests-stop-waiting-c
worktree: /home/marvin/Projects/reprise-the-tests-stop-waiting-c
branch: feature/the-tests-stop-waiting-c
phase: refactored
codex_session:
created: 2026-10-05
---
# The tests stop waiting — strand c: test cleanup

Mother plan: `docs/plans/the-tests-stop-waiting.md`. Read its "Why", "The cut"
and "Decisions from the grill" before starting. Touch only the files this
strand owns.

## Strand c — test cleanup

**Owns:** the test files named below, `remote/mod.rs` and `arbitration.rs` in
`library_doctor/remote/`.

Deleted (prove nothing; none is rule-named):

1. `library_doctor/remote/diagnostics.rs` — whole file (one ignored test that
   opens the **owner's real `reprise.db`** by hard-coded path and calls
   MusicBrainz) and its `#[cfg(test)] mod diagnostics;` (`remote/mod.rs:11-12`).
   Narrow `arbitration.rs:200 ranked_candidates`, `:290 has_clear_lead`,
   `:294 candidates_contradict` from `pub(super)` to private (their only outside
   caller was the diagnostic; `is_complete` stays `pub(super)`).
2. `playback/bass_pressure_tests.rs:169 probe_real_tracks` — no assertions,
   German strings, nothing calls it.
3. `ui/style/composed_css_tests.rs:27 probe_composed_css_errors` — prints only;
   `the_composed_stylesheet_parses_without_errors` (:14) is the real guard.
4. Constant-equals-literal: `queries/autocomplete.rs:360-363`
   (`tag_6_dropdown_needs_two_chars` covers the behaviour),
   `reprise-platform-linux/src/location.rs:315-319`,
   `ui/strings_podcasts.rs:745-748`. **Kept:** `main.rs:431-433` (only link from
   `APP_ID` to the Flathub identity).

Re-tagged `measurement:` (CI runs them today; they assert nothing or measure
wall clock on shared runners; none is rule-named — a rule-named test must never
be re-tagged, `check-ux-traceability.sh:100-117` would fail):

5. `song_visualizer_tests.rs:248 render_bass_pressure_moments_ppm`,
   `:464 render_bars_gallery_ppm`, `:490 bars_fullscreen_render_budget_diagnostic`,
   `sidebar_device_card_mirror_tests.rs:285 device_card_contrast_ladder_visual_fixture`,
   `concerts_visual_tests.rs:40 concerts_visual_acceptance_fixture`.
   Reason text: `measurement: <what it does and which env it needs>`.

Hardened / restructured:

6. `diagnostic_trail_tests.rs:327 measure_generated_library_reload_latency`
   opens `reprise_core::db::default_path()`: panic unless `XDG_DATA_HOME` is set
   and does not resolve under `$HOME/.local/share` — run by hand without the
   isolation it would migrate the real database.
7. `reprise-platform-linux/src/waveform.rs:495` — re-tag
   `measurement: needs REPRISE_SPECTROGRAM_LOUD_TRACK and REPRISE_SPECTROGRAM_QUIET_TRACK`.
8. `reprise-gnome/tests/gnome_conformance.rs`: each gate script runs once per
   binary via `OnceLock` (appstream ×3, gnome-idioms ×3, ai-hygiene ×2 today);
   all ten test names stay.

Kept as documented manual harnesses (no change): `placeholder_measurement.rs:31`,
`reprise-stems/tests/e2e_separation.rs:25,99`,
`preferences_chrome_placement_tests.rs:561`, `cover_cloud_gallery_tests.rs:6,88`.

Verification: workspace clippy + tests for the touched crates;
`scripts/check-ux-traceability.sh`; `scripts/check-display-tests.sh --list`
no longer lists the five re-tagged tests and lists everything else unchanged.
The count comparison against strand a's runner happens post-merge.
