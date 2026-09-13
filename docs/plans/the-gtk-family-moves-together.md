---
slug: the-gtk-family-moves-together
worktree: /home/marvin/Projects/reprise-the-gtk-family-moves-together
branch: feature/the-gtk-family-moves-together
phase: shipped
codex_session:
created: 2026-09-13
---
# The GTK family moves together

Closes #884.

## Why

Dependabot PR #860 bumped `gio` alone from 0.22.8 to 0.22.9 and left `gio-sys`
at 0.22.8. gio 0.22.9 references `ffi::GDBusActionGroupClass`, which only
exists in gio-sys 0.22.9, so every job died with
`error[E0425]: cannot find type GDBusActionGroupClass in crate ffi`. The gtk-rs
crates are released as a family whose `-sys` and binding crates must move in
step; a single-crate bump is never coherent. The PR was closed unmerged and
`dev` still carries the 0.22.8 line.

This branch performs the family update from current `dev`: every gtk-rs 0.22
/ gtk4 0.11 / libadwaita 0.9 crate in the lock moves to the newest patch
release of its line, together, and the Flatpak sources follow.

## Facts to build on

- Current lock (origin/dev): glib 0.22.8, glib-sys 0.22.8, gobject-sys 0.22.6,
  gio 0.22.8, gio-sys 0.22.8, gdk4 / gdk4-sys / gsk4 / gsk4-sys / gtk4 /
  gtk4-sys 0.11.4, libadwaita 0.9.2, pango 0.22.8, pango-sys 0.22.0,
  cairo-rs 0.22.0, graphene-rs 0.22.8, graphene-sys 0.22.8, gdk-pixbuf 0.22.0,
  gdk-pixbuf-sys 0.22.0, gdk4-x11-sys 0.11.0, plus glib-macros, gtk4-macros,
  libadwaita-sys, gdk4-x11 and gdk4-wayland where present.
- There is no `[workspace.dependencies]` table. The root `Cargo.toml` comment
  says "gtk-rs 0.22 and gstreamer-rs 0.25 set the current dependency floor";
  the per-crate manifests use caret requirements (`"0.22"`, `"0.11"`, `"0.9"`),
  so this is a `Cargo.lock`-only change unless a crate turns out to be pinned
  exactly — then the pin moves too.
- The gstreamer family (gstreamer 0.25.x and its siblings) is **not** part of
  this update. Leave it untouched.
- The generator is on PATH as `flatpak-cargo-generator.py` (a `uv` script that
  needs network access). From the repo root:
  `flatpak-cargo-generator.py Cargo.lock -o flatpak/cargo-sources.json`
- The contract check: `scripts/check-flatpak-cargo-sources.sh`.

## Tasks

### T1 — move the family in one `cargo update`

Run a single `cargo update` naming every crate of the family that is in the
lock: glib, glib-sys, glib-macros, gobject-sys, gio, gio-sys, gdk4, gdk4-sys,
gsk4, gsk4-sys, gtk4, gtk4-sys, gtk4-macros, libadwaita, libadwaita-sys,
pango, pango-sys, cairo-rs, cairo-sys-rs, graphene-rs, graphene-sys,
gdk-pixbuf, gdk-pixbuf-sys, gdk4-x11, gdk4-x11-sys, gdk4-wayland,
gdk4-wayland-sys — drop any name that `cargo update` reports as not in the
lock. Do not pass `--precise`; the newest release within each crate's
existing semver line is the target.

Coherence check, and record its output in the commit body:

```
cargo tree -d -e normal | grep -E 'glib|gio|gdk|gsk|gtk4|adwaita|pango|cairo|graphene|pixbuf'
```

must print nothing (no two versions of any family crate), and for every
binding crate the `-sys` crate must carry the same version
(`grep -A1 -E '^name = "(gio|gio-sys|glib|glib-sys|gtk4|gtk4-sys|libadwaita|libadwaita-sys)"$' Cargo.lock`).

### T2 — Flatpak sources

Regenerate `flatpak/cargo-sources.json`, run the contract check.

### T3 — gates

The full Rust gate (see *Verification scope*), including
`cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'` which
must stay empty, and `scripts/check-flatpak-manifest.sh`. A new
deprecation warning from a newer binding crate is a real clippy error here
(`-D warnings`); fix it at the call site in the most local way and name it in
the summary. If a family crate's newest patch release breaks compilation in a
way that needs more than a local fix, stop, leave the tree at the last
coherent state that compiles, and report which crate and which error.

One commit for T1+T2 (`The GTK family moves together`), a separate commit for
any source change T3 required.

## Out of scope

- gstreamer-rs, and any crate outside the gtk-rs family.
- #883 (trash / rmcp / zvariant) and #885 (quick-xml) are separate branches.

## Parallelität

Not cut. One `Cargo.lock` change and its regenerated sources; there is nothing
to run in parallel.
