---
slug: refactor-wave-2026-10-w4a
worktree: /home/marvin/Projects/reprise-refactor-wave-2026-10-w4a
branch: feature/refactor-wave-2026-10-w4a
phase: coded
codex_session:
created: 2026-10-05
---
# Refactor wave 2026-10, wave 4 — strand A: the `ui/mod.rs` alias layer, part 1

Mother plan: `docs/plans/refactor-wave-2026-10-w4.md`; its "Shared context" binds this strand.
Origin: consolidation package 3.4 ("retire the `ui/mod.rs` alias layer feature by feature"); wave 2
deleted the unused aliases, the used ones remain.

This plan is your only channel. When the code disagrees with a table below, the code wins: keep
what the code does and say so in your final message.

## Purpose

`crates/reprise-gnome/src/ui/mod.rs` ends with a "Compatibility surface" block of `use` statements
that re-export feature-directory modules at the `ui::` level (`use device_sync::{…}`,
`pub(crate) use cover::{cover_download_worker, cover_loader}`, …). Call sites write
`crate::ui::device_sync_runtime` where the module really is `crate::ui::device_sync::device_sync_runtime`.
After this strand, 12 of the 20 families have no alias left: every call site names the real path,
and the comment block says which 8 families still carry aliases and why.

**Behaviour-preserving means:** paths only. No function, type, string, test body, signature or
module visibility changes beyond what the compiler demands to reach a module at its real path.

## Evidence (origin/dev @ 9465e997e8, 2026-10-05)

- `crates/reprise-gnome/src/ui/mod.rs` is 222 lines. The alias block starts at the comment
  `// Compatibility surface for the existing frontend.` (line ~157) and ends before
  `#[cfg(test)] mod reactive_light_tests` (line ~218). It holds 98 aliases across 20 families.
- Alias references were counted as `crate::ui::<alias>`, `ui::<alias>` (from `main.rs`),
  `super::<alias>` from files directly under `ui/`, and the grouped forms
  `use crate::ui::{…, <alias>, …}` / `use super::{…, <alias>, …}`. References to the real path
  (`crate::ui::<family>::<module>`, or `super::<module>` from inside the family directory) are not
  alias references and must not be touched.
- **The 12 families this strand retires** (alias → real module; measured references / files):

  | Family | Aliases (`use <family>::{…}` in `ui/mod.rs`) | refs / files |
  | --- | --- | --- |
  | `artist_news` | `artist_news_worker` (4 / 4) | 4 / 4 |
  | `device_sync` | `device_sync_feedback` (0), `device_sync_launcher` (0), `device_sync_page` (0), `device_sync_runtime` (34 / 11), `device_sync_smoke` (10 / 2), `device_sync_strings` (4 / 4) | 48 / 14 |
  | `library_views` | `artist_avatar` (1 / 1) | 1 / 1 |
  | `now_playing` | `artist_portrait_worker` (10 / 10), `now_playing_column` (3 / 1) | 13 / 11 |
  | `player_bar` | `library_player_bar` (6 / 5), `player_bar_layout` (2 / 2), `player_bar_state` (3 / 2), `waveform_seek` (2 / 2) | 13 / 10 |
  | `playlists` | `playlist_io` — `pub(crate)` (7 / 5) | 7 / 5 |
  | `preferences` | `preference_background_bar` (1), `preference_dependencies` (10 / 2), `preference_lastfm` (0), `preference_layout` (0), `preference_listenbrainz` (0), `preference_playback` (0), `preference_plugins` (9 / 5), `preference_rhythmbox` (0), `preferences_window` (4 / 4) | 24 / 10 |
  | `sidebar` | `sidebar_dnd` — `#[cfg(test)] pub(crate)` (5 / 3), `sidebar_session` — `pub(crate)` (6 / 4), `sidebar_device_card` (0), `sidebar_issue_strings` (1), `sidebar_presentation` (21 / 5), `sidebar_rebuild` (1) | 34 / 11 |
  | `spectrogram` | `spectrogram_batch` (0), `spectrogram_batch_progress` (1) | 1 / 1 |
  | `stats` | `stats_css` — `pub(crate)` (0), `stats_view` — `pub(crate)` (6 / 4) | 6 / 4 |
  | `tag_edit` | `autocomplete_entry` (7 / 4), `tag_editor_dirty` (6 / 3), `tag_editor_failures` (1), `tag_editor_form` (2), `tag_editor_save` (2 / 1), `tag_editor_state` (5 / 5), `tag_editor_style` (0), `tag_editor_widgets` (3 / 3), `tag_edit_flow` — `pub(crate)` (6 / 5), `tag_editor` — `pub(crate)` (3 / 3) | 35 / 11 |
  | `window` | `library_chrome` (1), `window_decoration_strings` (0), `window_decorations` (1), `window_navigation` (0) | 2 / 2 |

  Total: 48 aliases, ~188 references, ~80 distinct files. A "0" alias is still declared; deleting
  its `use` may reveal a reference the count missed (for instance a nested `super::super::` path).
  The compiler decides.
- **Not retired here** (foreign call sites; see the mother plan): `playback`, `cover`, `track_list`,
  `scan`, `lyrics`, `scrobbling`, `compact`, `browse`. Their `use` lines in `ui/mod.rs` stay
  exactly as they are.
- No source-scan test asserts on an alias path: `rg -n 'contains\("(use )?(crate::)?ui::'
  crates/reprise-gnome/src` prints nothing. The 127 `include_str!` tests under `src/ui` read file
  bodies for other tokens (`swell`, `kick`, CSS classes); an import-line change cannot affect them.
- `main.rs` references `ui::` modules only by real paths (`ui::window::build`,
  `ui::track_list::diagnostic_trail`, `ui::startup_report`, …). So the `pub(crate)` aliases of this
  strand's families (`playlist_io`, `sidebar_dnd`, `sidebar_session`, `stats_css`, `stats_view`,
  `tag_edit_flow`, `tag_editor`) are reached from inside `ui` only, and the family-level
  `pub(in crate::ui) mod …` declarations already suffice. If a compile error says otherwise, bump
  that one `mod` line in the family's `mod.rs` to `pub(crate)` and list it.
- Family directories declare their modules as `mod x;` (private) or `pub(in crate::ui) mod x;`;
  e.g. `ui/device_sync/mod.rs` and `ui/preferences/mod.rs`. Every aliased module is already
  `pub(in crate::ui)` (an alias could not re-export a private module).
- `rustfmt` sorts and regroups `use` lists; `cargo fmt` after every family keeps the diff honest.

## Owns

- `crates/reprise-gnome/src/ui/mod.rs` — the alias block and its comment only. Do not touch the
  `mod …;` list above it or `reactive_light_tests` below it.
- Every file under `crates/reprise-gnome/src` that references one of the 48 aliases (the ~80 files
  the compiler names). Edits are limited to `use` lines and inline paths.
- `crates/reprise-gnome/src/ui/<family>/mod.rs` for the 12 families — only a visibility bump on a
  `mod` line that the compiler demands.

Not owned: the 8 remaining families' aliases and their call sites; any file in the mother plan's
foreign list (none of the ~80 files is in it — measured; if the compiler sends you into one, stop
and report, do not edit it); `Cargo.toml`; any test body.

## Recipe (per family; one commit per family)

Order: `library_views`, `spectrogram`, `window`, `artist_news`, `stats`, `playlists`, `now_playing`,
`player_bar`, `preferences`, `sidebar`, `tag_edit`, `device_sync` (smallest first; the first three
teach the shape in minutes).

1. In `ui/mod.rs` delete every `use <family>::…;` and `pub(crate) use <family>::…;` statement of
   the family (and the `#[cfg(test)]` line that belongs to `sidebar_dnd`).
2. Pre-rewrite with sed, then let the compiler catch the rest. For each alias `X` of family `F`:
   ```
   rg -l --pcre2 '(?<![\w:])(crate::ui::|ui::)X\b' crates/reprise-gnome/src --glob '*.rs' \
     | xargs -r sed -i -E 's/(crate::ui::|\bui::)X\b/\1F::X/g'
   rg -l --pcre2 '(?<![\w:])super::X\b' crates/reprise-gnome/src/ui --max-depth 1 --glob '*.rs' \
     | xargs -r sed -i -E 's/\bsuper::X\b/super::F::X/g'
   ```
   The first pattern is safe everywhere (the real path is `crate::ui::F::X`, which contains
   `F::` and therefore never matches `crate::ui::X`). The second is restricted to files directly
   under `ui/` — inside `ui/F/` a `super::X` already is the real path and must not change.
   Grouped imports (`use crate::ui::{a, X, b};`) are not reached by sed: rewrite them by hand to
   `use crate::ui::{a, F::X, b};` (valid) or to a separate `use crate::ui::F::X;` line — whichever
   `rustfmt` leaves readable.
3. `cargo check -p reprise-gnome --all-targets 2>&1 | rg -A6 '^error'` → fix every remaining
   unresolved path the same way. `--all-targets` is mandatory: test modules reference aliases too.
4. `cargo fmt`. Then `cargo clippy -p reprise-gnome --all-targets -- -D warnings` must be green —
   `unused_imports` is how a now-redundant `use crate::ui::F::X` next to an existing
   `use crate::ui::F::{…}` shows up.
5. Commit: `The <family> modules are reached by their real paths` (prose subject, repository style).

After the last family, rewrite the comment above the remaining block so it is true: the ownership
sentence stays; add one sentence naming the 8 families that still carry aliases and that they go
when their call sites are free of in-flight branches. Do not cite any `docs/plans/…` path.

## Known traps

- **Do not rename, move or re-export anything.** A module that is `pub(in crate::ui)` stays so;
  the alias layer's `pub(crate)` re-exports are deleted, not pushed down, unless the compiler
  proves a `crate::`-level caller exists (none measured).
- **`super::X` inside the family directory is the real path.** The sed for `super::` is scoped to
  `--max-depth 1`; keep it that way.
- **`#[cfg(test)]`-only aliases** (`sidebar_dnd`) have test-only call sites; `--all-targets` finds
  them, a plain `cargo check` does not.
- **Doc comments** may mention `crate::ui::X` in backticks (rustdoc intra-doc links resolve paths):
  `RUSTDOCFLAGS="-D warnings" cargo doc -p reprise-gnome --no-deps` must stay green; fix a broken
  link by using the real path.
- **Thinness and idioms gates** grep for real paths (`crate::ui::toasts::plain`,
  `crate::ui::rows::…`); they are unaffected, but run them.
- **B lands before this strand.** Rebase onto `dev` after B's landing and re-run steps 2–4 for any
  family whose files B touched (`track_list_reload.rs`, `podcasts_view.rs`, `popover.rs`, …).
- **File sizes**: this strand can only shorten files (one `use` line may split into two —
  `track_list_reload.rs` is 771 lines, `track_list_model.rs` 763; a split import there is fine, a
  new block is not).
- English everywhere, focused commits, no agent attribution lines, no `docs/plans/…` citation in
  code or comments.

## Verification

Run from the worktree root, in this order; every command must pass:

```
cargo fmt --check
cargo clippy -p reprise-gnome --all-targets -- -D warnings
cargo clippy --all-targets --workspace -- -D warnings
cargo clippy --all-targets --workspace --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
cargo test -p reprise-gnome --no-run
rg -n '^(pub\(crate\) )?use (artist_news|device_sync|library_views|now_playing|player_bar|playlists|preferences|sidebar|spectrogram|stats|tag_edit|window)::' crates/reprise-gnome/src/ui/mod.rs   # must print nothing
rg -n --pcre2 '(?<![\w:])(crate::ui::|super::|\bui::)(artist_news_worker|device_sync_runtime|device_sync_smoke|device_sync_strings|artist_avatar|artist_portrait_worker|now_playing_column|library_player_bar|player_bar_layout|player_bar_state|waveform_seek|playlist_io|preference_[a-z_]+|preferences_window|sidebar_dnd|sidebar_session|sidebar_presentation|sidebar_rebuild|sidebar_issue_strings|sidebar_device_card|spectrogram_batch|spectrogram_batch_progress|stats_css|stats_view|autocomplete_entry|tag_editor_[a-z_]+|tag_edit_flow|tag_editor|library_chrome|window_decoration_strings|window_decorations|window_navigation)\b' crates/reprise-gnome/src --glob '*.rs' | rg -v 'crates/reprise-gnome/src/ui/(device_sync|now_playing|player_bar|playlists|preferences|sidebar|spectrogram|stats|tag_edit|window|library_views|artist_news)/'   # must print nothing (the exclusion keeps real in-family super:: paths out)
scripts/check-architecture.sh
scripts/check-frontend-thinness.sh
scripts/check-gnome-idioms.sh
scripts/check-motion-tokens.sh
scripts/check-accessibility-semantics.sh
```

Do not run the unfiltered `reprise-gnome` test suite; the change is paths only and `--no-run`
plus clippy is the proof. Run `cargo test -p reprise-gnome <module>` for one module per family as
a smoke (for example `device_sync_runtime`, `playlist_io`, `tag_editor_state`). The orchestrator
runs `scripts/check-merge-readiness.sh` after the code phase.

Report: the number of aliases deleted (expected 48), the number of files edited, every visibility
bump with file:line, and any alias whose measured count was wrong.
