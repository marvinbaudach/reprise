---
slug: community-launch
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-31
strands: a,b,c
merge_order: a,b,c
---
# Community launch — implementation plan

Spec: `docs/superpowers/specs/2026-10-31-community-launch-design.md`. Read it first. This mother plan
holds the shared context, the cut and the checks that span strands; each strand's tasks live in
`community-launch-a.md` (app), `community-launch-b.md` (distribution) and `community-launch-c.md`
(showcase). This file is frozen once the plan phase ends.

Binding for every strand: `AGENTS.md` (gates, English everywhere, never-crash lints, RefCell
discipline, the 800-line cap, the headless isolation recipe), `docs/ux-rules.md` (a new rule is an
agent proposal: `[planned]` plus `<!-- REVIEW: rule proposal -->`, rule-named tests
`fn <prefix>_<nr>_…`), and `docs/agents/branching.md`.

## Decisions from the grill (2026-10-31)

1. **Crash detection is a running sentinel.** A marker is written at startup and removed on clean
   shutdown, so kills and native aborts that never reach the panic hook are detected too. The panic
   hook additionally writes message, location and backtrace to the log.
2. **AUR keeps stems optional.** The August Flathub decision E5 stands over the first spec draft:
   the AUR build includes the stem backend, `onnxruntime` stays an optional dependency, and without
   it the feature degrades at runtime. The spec is corrected accordingly.
3. **AUR ships `reprise-git` only.** `main` moves only by promotion, so it always equals the latest
   release; a tagged `reprise` package would need a version and checksum bump for every release. A
   stable package follows only on demand.
4. **Call and launch texts live outside the repository**, in a doc written once Stage 1 passes.
   They include German variants, which `AGENTS.md` forbids in the repository, and they should not be
   public before the launch. Only the issue template is repository-side.
5. **Merge order A → B → first publish → C** (see below).
6. **Three strands** as cut below.
7. **Stage 1 acceptance runs in a VM** (Fedora Workstation or GNOME OS in GNOME Boxes), not under a
   second account on the maintainer's machine.

## Facts the strands rely on (checked 2026-10-31 against `origin/dev` `c47ec70bad`)

- `release.yml` runs on every push to `main`, on `workflow_dispatch` (input `dry_run`, default
  true) and on path-filtered PRs. Job `flatpak` builds `repo` (`:387`) and the bundle (`:410`) and
  uploads the bundle as an artifact; job `publish` (`:637`, `contents: write`) creates the GitHub
  release from `dist/`. No `vars.*` are used yet. The manifest sets no `branch:`, so the app ref is
  `app/io.github.marvinbaudach.Reprise/x86_64/master`.
- `scripts/check-flatpak-device-permissions.sh:17-31` requires `--talk-name=org.gtk.vfs.*` and
  `--filesystem=xdg-run/gvfsd` and rejects every other `--filesystem=*`. GP-15
  (`docs/ux-rules.md:8083`, `[planned] [manual]`): static sandbox permissions stay at the absolute
  minimum.
- Rhythmbox paths: `crates/reprise-gnome/src/ui/preferences/preference_rhythmbox.rs:21-22` define
  `REPRISE_RHYTHMDB_PATH` and `REPRISE_RHYTHMBOX_PLAYLISTS_PATH`; `:72-77`
  `default_rhythmdb_path()` falls back to `glib::user_data_dir()/rhythmbox/rhythmdb.xml`, which
  inside the sandbox is the app's own data directory; `:95-100` `default_playlists_path()` uses the
  sibling `playlists.xml`. Callers: `:91-93` `rhythmbox_import_available()` (used by
  `first_run.rs:372`), `:368-369`, `:527-528`. No test pins the default path. The importer matches
  tracks by the file path in each `location` URI. Flatpak detection is inlined as
  `Path::new("/.flatpak-info").is_file()` in `reprise-platform-linux` (`location.rs`, `trash.rs`,
  `diagnostics.rs`); there is no shared helper.
- The manifest's `finish-args` grant no filesystem access beyond `xdg-run/gvfsd`.
- `packaging/aur/PKGBUILD` is `reprise-git` (`git+https://github.com/marvinbaudach/reprise.git`,
  `arch-meson` with the default `stem_backend=true`, `onnxruntime` optional). GitHub's default
  branch is `main`. Neither `reprise` nor `reprise-git` exists on the AUR.
- `marvinbaudach/reprise-flatpak` does not exist yet.

## Manual maintainer steps (never executed by an agent)

- **M1** Create the public repository `marvinbaudach/reprise-flatpak` with a `gh-pages` branch and
  Pages serving that branch's root.
- **M2** Generate a dedicated GPG key for the Flatpak repository. Store the armored private key as
  the `reprise` Actions secret `FLATPAK_GPG_PRIVATE_KEY`, its passphrase (if any) as
  `FLATPAK_GPG_PASSPHRASE`, and the base64 of the binary public key (`gpg --export <id> | base64
  -w0`) as the Actions variable `FLATPAK_REPO_GPG_KEY`.
- **M3** Create an SSH deploy key with write access on `reprise-flatpak`; store the private half as
  the `reprise` secret `FLATPAK_REPO_DEPLOY_KEY`.
- **M4** Set the Actions variable `FLATPAK_REMOTE_ENABLED=true` once strands A and B have landed.
  The next promotion to `main` publishes the first build, and public distribution begins.
- **M5** Dress rehearsal (spec §2.6), only with explicit approval at that moment.
- **M6** AUR account and SSH key; push `reprise-git` to the AUR before the broad launch.
- **M7** Post the test-round call and, later, the launch texts from the maintainer's own accounts.

## Parallelität

Three strands with disjoint file ownership.

| Strand | Purpose | Owns |
| --- | --- | --- |
| A — app | crash reports (Wave 0 tasks 0.1–0.4) and Rhythmbox locations | `crates/**`, `po/**`, `docs/ux-rules.md`, `docs/plans/community-launch-a.md` |
| B — distribution | permissions, signed Flatpak remote, AUR, contract and release text | `flatpak/**`, `.github/workflows/release.yml`, `packaging/**`, `scripts/check-flatpak-device-permissions.sh`, `RELEASING.md`, `AGENTS.md`, `docs/agents/branching.md`, `docs/plans/community-launch-b.md` |
| C — showcase | README, Showroom section, issue templates | `README.md`, `showroom/**`, `.github/ISSUE_TEMPLATE/**`, `docs/plans/community-launch-c.md` |

**Target values written into tasks instead of cross-strand reads:**

| Value | Written by | Used by |
| --- | --- | --- |
| `https://marvinbaudach.github.io/reprise-flatpak/reprise.flatpakref` | B3 | C1, C2 |
| Menu label "Copy Diagnostics" | A4 | C3 |
| Rule ID START-7 (Rhythmbox locations) | A5 | B5 |
| Probed directories: `~/.local/share/rhythmbox`, `~/.var/app/org.gnome.Rhythmbox3/data/rhythmbox`, the XDG music directory | A5 | B1 |
| Known limitations: x86_64-only Flatpak; music outside the XDG music directory needs an override; no stem separation in the Flatpak; early test release | B1, B3 | C1 |

**Merge order:** `a,b,c`.
1. A lands first, so the first public build already carries crash reports and the importer change.
2. B lands second. Its publish job stays inert until M4 sets `FLATPAK_REMOTE_ENABLED`.
3. M1–M4, then a promotion publishes the first build.
4. C lands last, because its README points at a `.flatpakref` that exists only after that publish.

**Post-merge cross-checks** (none of these can run inside a strand):

- **X1** Every directory the resolver probes (A5) is covered by a manifest grant (B1). The
  device-permissions gate and `crates/reprise-gnome/tests/gnome_conformance.rs` pass on the merged
  tree.
- **X2** The `.flatpakref` URL in README and Showroom (C) equals the one `flatpak-remote` publishes
  (B3), and it resolves after the first publish.
- **X3** The issue templates' "Copy Diagnostics" wording (C3) matches A4's label.
- **X4** README's known limitations (C1) match what A and B shipped, including B1's fallback if the
  Flathub-Rhythmbox grant is refused.
- **X5** `RELEASING.md`'s START-7 reference (B5) resolves in `docs/ux-rules.md` (A6), and
  `scripts/check-ux-traceability.sh` passes.
- **X6** Stage 1 acceptance (P2).

## After the strands

- **P1 — import clip.** Generated library plus generated `rhythmdb.xml`, recorded with the
  `scripts/showreel/` pipeline under the `AGENTS.md` isolation recipe, never with the maintainer's
  library or database. Embedded in the Showroom section and the README in a small follow-up PR.
- **P2 — Stage 1 acceptance** in a VM: install from the `.flatpakref`; receive a later promotion
  through `flatpak update`; the import is offered and matches tracks in the XDG music directory;
  `flatpak kill io.github.marvinbaudach.Reprise` mid-session, and the next start offers Copy
  Diagnostics with a report that contains no library path.
- **P3 — dress rehearsal** (M5).
- **P4 — call and launch texts** as a doc outside the repository, once P2 and P3 pass.
