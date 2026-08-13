---
slug: first-aid-icon-app-id
worktree: /home/marvin/Projects/reprise-first-aid-icon-app-id
branch: feature/first-aid-icon-app-id
phase: planned
created: 2026-08-13
---

# The first-aid icon kept the old app-ID prefix

The app-ID migration (#418, `08d983ab27`) renamed the main artwork to
`io.github.marvinbaudach.Reprise*` but left one symbolic icon behind on the old
`reprise-*` prefix. A full Flatpak sandbox build on 2026-08-13 surfaced it:

```
Not exporting share/icons/hicolor/symbolic/apps/reprise-first-aid-symbolic.svg,
non-allowed export filename
```

Flatpak exports only those files under `share/icons/hicolor/**` whose basename
begins with the app ID. Inside the sandbox the icon still renders, because the
app finds it on its own icon search path — so this is not a visible bug today.
It is an inconsistency left over from the rename, and `flatpak-builder-lint`
will flag it before any Flathub submission.

The file lists below are a **starting point, not a fence**. Adjacent files may
be changed minimally and named in the commit message. Stop only if the
*contract* below turns out to be wrong.

## Goal

The icon is named `io.github.marvinbaudach.Reprise-first-aid-symbolic.svg`,
every reference follows, and a Flatpak build exports it without complaint.

## Contract

The icon is referenced by its **theme name** (without extension) from Rust, by
**path** from Meson and the brand-asset generator. Both spellings must move
together. These are the nine references found on 2026-08-13 — treat them as the
starting point and grep again rather than trusting the list:

| File | Line | Kind |
|---|---|---|
| `data/icons/hicolor/symbolic/apps/reprise-first-aid-symbolic.svg` | — | the file itself, rename with `git mv` |
| `data/meson.build` | 34 | install path |
| `scripts/build-brand-assets.sh` | 61, 95 | generated output path |
| `crates/reprise-gnome/src/ui/icons.rs` | 50 | theme name |
| `crates/reprise-gnome/src/ui/library_doctor/mod.rs` | 63, 78 | doc comment + `DOCTOR_GLYPH` |
| `crates/reprise-gnome/src/ui/library_doctor/tests.rs` | 49 | assertion |
| `crates/reprise-gnome/src/ui/library_doctor/summary_cards.rs` | 238 | assertion |
| `crates/reprise-gnome/src/ui/sidebar/sidebar_presentation.rs` | 471 | theme name |

`DOCTOR_GLYPH` in `library_doctor/mod.rs:78` is the single constant the UI reads;
the two assertions and the two other spellings must end up agreeing with it.

**Do not touch the main icons.** `scripts/check-logo-artwork.sh:215,353,357` and
`scripts/check-release.sh:33-34,85-86` pin
`io.github.marvinbaudach.Reprise.svg` and
`io.github.marvinbaudach.Reprise-symbolic.svg` by exact path. Neither script
mentions the first-aid icon, so neither should need editing — verify that rather
than assuming it.

**The SVG content does not change.** This is a rename, not a redraw. Do not
touch the geometry: `scripts/check-logo-artwork.sh` enforces shape counts and
viewBox on the main symbolic icon, and the same drawing conventions apply here.

## Done when

- `grep -rn 'reprise-first-aid' --include='*.rs' --include='*.build' --include='*.sh' .`
  returns nothing outside `docs/plans/`.
- `cargo build --locked -p reprise-gnome` succeeds.
- `cargo test --locked -p reprise-gnome --bin reprise` is green for the
  library-doctor and sidebar tests (note: `--lib` finds no tests in this crate,
  only `--bin reprise` does).
- `scripts/check-logo-artwork.sh` still passes.

Run anything heavy through `heavy-run medium --` and redirect output to a log
file rather than printing it. Do not launch the application; this repository
verifies headlessly.
