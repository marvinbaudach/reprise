# Codex Handoff — CONTRAST-2 Regression + GRID-5 Display Test

Two defects on `feat/sidebar-visual-improvements` (base `main@b0965905`, your
own Batch A commits are already in). Fix them in order. **Never push.**

## Defect 1 — the list status bar no longer renders (regression from `171ed8ae`)

### Evidence, already measured — do not re-derive

A headless run of the release build (Music/Library view, 1920×1200) was sampled
per pixel:

| Position | Measured | Meaning |
|---|---|---|
| table rows, y=950..1000 | `srgb(27,30,34)` | `view_bg` — correct |
| left sidebar | `srgb(34,38,43)` | `sidebar_bg` — correct |
| now-playing stage | `srgb(34,38,43)` | `sidebar_bg` — correct |
| **status bar region, y=1030..1095** | `srgb(22,24,27)` | **`window_bg` — neither the bar's surface nor any text** |

Before `171ed8ae` this area showed `1,638 tracks · 4 days, 2 hours and 28
minutes`. It is now blank. STYLE-2 itself is fine; only the status bar is gone.

### Three visibility paths interact — this is where the bug lives

1. `status_bar.rs` — `StatusBar::new` starts the label `set_visible(false)`.
   `refresh()` sets text + `set_visible(true)` when `track_count > 0`, and
   `hide()` sets `false`.
2. `track_content.rs` (your change) — the surrounding `Box` mirrors the label
   via `connect_visible_notify`, plus one sync at build time
   (`status_bar.set_visible(status.is_visible())`).
3. `preferences.rs:207` — `apply_persisted_layout` calls
   `self.status_bar.set_enabled(status_visible)`. Note `set_enabled` in
   `status_bar.rs:74` **only hides on `false` and does nothing on `true`** — it
   never re-shows. The persisted value was `status_visible=true` in the failing
   run (confirmed in the app log).

Also relevant: `window.rs:277-282` calls `status_bar.refresh(...)` only for
`ViewSource::Library` and `status_bar.hide()` for every other source.

Find which path wins and fix the actual ordering/ownership problem. Do **not**
paper over it by force-showing the bar unconditionally — an empty library and a
non-Library source must still hide it.

### Required test

`contrast_2_status_bar_renders_with_content` [gtk] must assert the **rendered
result**, not the declaration: after a Library reload with a non-empty stats
result, the status surface is mapped, has non-zero allocated height, and its
label carries non-empty text. The existing tests pass while the bar is invisible
because they only check the CSS class and the stylesheet string — that is the
exact failure mode STYLE-1 (section S) exists to prevent. Keep them, but they do
not count as coverage here.

Commit: `fix(status): restore the rendered list status bar (CONTRAST-2)`

## Defect 2 — `grid_5_reveal_scrolls_to_playing_album` fails

Fails identically on clean `main@b0965905`, so it is **not** caused by Batch A.
It blocks the display-test gate on this branch regardless.

```
panicked at crates/reprise-gnome/src/ui/library_views/album_view.rs:480:14:
GtkGridView focused the revealed item
```

Reproduce with:
`xvfb-run -a dbus-run-session -- cargo test --locked -p reprise-gnome grid_5_reveal_scrolls_to_playing_album -- --ignored`

Decide whether the **rule** (GRID-5 is `[aktiv]`) or the **test** is wrong, and
say which in the commit body. GRID-5 requires: switch to the album view, clear a
visible search field and album filter, scroll via adjustment (explicitly not
`scrollIntoView`), focus the tile, highlight it ~1 s, NAV-9a as fallback. If the
feature is genuinely broken, fix the feature — GRID-5 being `[aktiv]` while its
display test fails is not an acceptable end state.

Commit: `fix(album-grid): make the GRID-5 reveal test reflect reality`
(adjust the message to whichever side you fixed)

## Gates before every commit

- `cargo fmt --check`
- `cargo clippy --locked --all-targets --workspace -- -D warnings`
- `env XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) cargo test --locked --workspace`
- `scripts/check-ux-traceability.sh`, `scripts/check-architecture.sh`
- Display tests one process each: `xvfb-run -a scripts/check-display-tests.sh`.
  If D-Bus in the sandbox blocks it, try
  `xvfb-run -a dbus-run-session -- scripts/check-display-tests.sh`; if that also
  fails, report them as pending rather than faking a green.
- Translate new UI strings in the same commit; `po/de.po` free of untranslated
  and fuzzy entries. Never mark glyphs with `N_!`.

## Policy

- If a premise here turns out wrong, STOP, write `.codex-blocked.md` with the
  exact error, and end the run. Do not improvise a different design.
- UI copy English; `docs/ux-rules.md` and the ledger German; commits English.
- No attribution footers. Never push.
