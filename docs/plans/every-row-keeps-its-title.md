---
slug: every-row-keeps-its-title
worktree: /home/marvin/Projects/reprise-every-row-keeps-its-title
branch: feature/every-row-keeps-its-title
phase: shipped
codex_session:
created: 2026-09-13
---
# Every row keeps its title

Closes #641. Follow-up to #639, which closed the same defect class for toasts.

## Why

libadwaita defaults `use-markup` to TRUE on `AdwPreferencesRow` (and therefore
on every row that extends it: `AdwActionRow`, `AdwExpanderRow`, `AdwSwitchRow`,
`AdwComboRow`, `AdwEntryRow`, `AdwPasswordEntryRow`, `AdwButtonRow`,
`AdwSpinRow`) and on `AdwBanner`. A title or subtitle handed to one of them as
plain text goes through a Pango markup parser, and a bare `&`, `<` or `>`
makes the label render **empty**. Library data — track, album, artist,
episode and channel names, station names, file names, error text, URLs — and
translations (`po/de.po` already carries three msgstr with `&`) all reach such
titles and subtitles. `AdwStatusPage` is explicitly exempt: its title is not
parsed as markup.

#639 fixed toasts at one shared construction point
(`crates/reprise-gnome/src/ui/toasts.rs::plain`), guarded direct construction
in `scripts/check-gnome-idioms.sh` (FB-11), and proved it with a three-arm
display test that renders the widget in a real window and reads the
`GtkLabel` text back. This plan does the same for rows and banners.

## What exists today (measured against origin/dev)

- 85 direct constructions of markup-defaulting widgets under
  `crates/reprise-gnome/src/ui`: ActionRow 39, ExpanderRow 14, SwitchRow 13,
  ComboRow 7, EntryRow 5, PasswordEntryRow 3, Banner 3. No `.ui`/`.blp`
  templates exist; everything is built in Rust.
- 13 sites already set `use_markup(false)` by hand (the two banners,
  `first_run.rs`, `first_run_sources.rs`, `preference_plugins.rs`, five
  SwitchRows in `preference_rhythmbox.rs`, `library_doctor/remote_toggle.rs`,
  `library_doctor/start_page.rs`). Every other site relies on the default.
- ~75 `set_title` / `set_subtitle` calls with non-literal arguments; the ones
  on rows cluster in `ui/preferences/preference_rhythmbox.rs`,
  `preference_lastfm.rs`, `preference_listenbrainz.rs`,
  `preference_youtube.rs`, `preference_library.rs`,
  `preferences_search_results.rs`.
- The six `<b>…</b>` count strings (`strings_concerts.rs`, `strings_filter.rs`,
  `strings_releases.rs`, `strings_podcasts.rs` ×2, `strings_radio.rs`) are all
  consumed by `gtk4::Label::set_markup` on plain labels — never by a row — so
  they are unaffected by a row-level `use_markup(false)` and stay as they are.
- Test helpers that walk a rendered widget tree for label text:
  `toasts.rs` tests (`rendered_label_texts`, `LabelSettle`), plus
  `descendant_labels` in `date_format_display_tests.rs`,
  `collect_label_texts` in `up_next_panel_tests.rs`, `descendants<T>` in
  `browse_bar_tests.rs`.
- The rulebook: `docs/ux-rules.md` section F, FB-11 at ~line 1495, FB-12 is
  the last FB rule. `scripts/check-ux-traceability.sh` requires every
  `[active]` rule to have at least one test carrying its ID in the name
  (`fn fb_13_…`); the `#[ignore = "requires a display; run via xvfb-run"]`
  marker is allowed on any rule status.

## Design

**One shared construction point per widget, in one module.** New file
`crates/reprise-gnome/src/ui/rows.rs` (sibling of `toasts.rs`, registered in
`ui/mod.rs`), documented like `toasts.rs`, exposing `pub(super)` (or
`pub(in crate::ui)`) functions that return the libadwaita builder with
`use_markup(false)` already applied, so every call site keeps its builder
chain and only swaps the first segment:

```rust
pub(super) fn action_row() -> adw::builders::ActionRowBuilder {
    adw::ActionRow::builder().use_markup(false)
}
pub(super) fn expander_row() -> adw::builders::ExpanderRowBuilder { … }
pub(super) fn switch_row() -> … ; combo_row(); entry_row(); password_entry_row();
pub(super) fn banner(title: &str) -> adw::Banner {
    let banner = adw::Banner::new(title);
    banner.set_use_markup(false);
    banner
}
```

Add a function only for widget types the tree actually constructs (the seven
above; add `button_row` / `spin_row` / `preferences_row` only if a grep finds
them). Sites that used `Widget::new()` become `rows::x().build()`; sites that
used `Widget::builder()` become `rows::x()`. The 13 hand-written
`use_markup(false)` calls become redundant and are removed, so the helper is
the single place that knows about the default.

**Not a blanket flag.** The helper only touches rows and banners. `GtkLabel`
`set_markup` sites (the six count strings) are untouched; `AdwStatusPage`
stays untouched.

**The guard.** `scripts/check-gnome-idioms.sh` gets a second block modelled on
the FB-11 one: a pattern matching direct construction of any of those widget
types (`(ActionRow|ExpanderRow|SwitchRow|ComboRow|EntryRow|PasswordEntryRow|ButtonRow|SpinRow|PreferencesRow|Banner)::(new|builder)`,
with the same `use … as …` and bare-path forms FB-11's pattern covers),
excluding `ui/rows.rs` itself, reporting under the new rule ID with the hint
"use crate::ui::rows::…".

**The rule.** `docs/ux-rules.md`, section F, directly after FB-12:

> - **FB-13** [active] [gtk] — A row keeps its title. Titles and subtitles of
>   preference rows (`AdwPreferencesRow` and everything that extends it:
>   action, expander, switch, combo, entry and password rows) and banner titles
>   are plain text, never markup, and never pass through a markup parser.
>   libadwaita defaults `use-markup` to TRUE on all of them, so library data
>   and translations containing `&`, `<` or `>` would otherwise render an
>   empty label — the same defect class FB-11 closed for toasts. Rows and
>   banners are therefore built only through `ui/rows.rs`, which turns the
>   default off at the one construction point; `check-gnome-idioms.sh` rejects
>   direct construction. `AdwStatusPage` is exempt because its title is not
>   parsed as markup, and the count labels that deliberately use Pango markup
>   are plain `GtkLabel`s and stay outside this rule.

Use the next free FB number if FB-13 is already taken on the branch.

**The proof.** In `rows.rs`, a `#[cfg(test)] mod tests` with one display test
`fb_13_row_plain_text_survives_markup_characters`, ignored with the display
marker, same three-arm shape as `fb_11_toast_plain_text_survives_markup_characters`:

1. an `action_row()` with title `"Tom & Jerry <Live>"` and subtitle
   `"AT&T > Radio"`, placed in a real window (through a
   `PreferencesGroup`/`ListBox` if the row needs a list parent to render),
   walked for rendered `GtkLabel` texts — both strings must be present;
2. a `banner("Library & Radio <offline>")` — its text must be present;
3. control arm: the same row with `set_use_markup(true)` flipped back on —
   the two strings must be **absent** from the rendered labels, observed for
   at least as long as the positive arm took (copy the `absence_wait` logic
   from the toast test). If `rendered_label_texts` is private to `toasts.rs`,
   move it (and `LabelSettle`) into a small shared test-support module under
   `ui/` and point the toast test at it — do not duplicate it.

## Tasks

### T1 — `rows.rs` and the sweep

Create the module; migrate all 85 construction sites (grep the widget list
above across `crates/reprise-gnome/src/ui`, including `library_doctor/`,
`preferences/`, `first_run*`, `scan/`, `devices/`, `podcasts/`, `radio/` —
wherever the grep leads); remove the 13 redundant explicit
`use_markup(false)` calls. Behaviour of every other property is unchanged.

### T2 — the guard

Extend `scripts/check-gnome-idioms.sh`; run it: it must be clean after T1 and
must flag a deliberately added `adw::ActionRow::builder()` in some UI file
(try it, see the violation, revert it — mention both outcomes in the summary).

### T3 — the rule and the proof

FB-13 in `docs/ux-rules.md`; the display test in `rows.rs`.
`scripts/check-ux-traceability.sh` must pass.

Commits: T1 `Every row is built at one plain-text construction point`, T2
`The idiom gate rejects direct row construction`, T3
`Every row keeps its title`.

## Out of scope

- `AdwStatusPage`, `GtkLabel::set_markup` sites, the six count strings.
- Android, showroom, the toast path (already covered by FB-11).

## Parallelität

Not cut. T2 and T3 both depend on T1's module name and the migrated tree; the
sweep itself is mechanical and splitting it across worktrees would only buy
merge conflicts in `ui/mod.rs`.
