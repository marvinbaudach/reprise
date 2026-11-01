---
slug: community-launch-b
worktree: /home/marvin/Projects/reprise-community-launch-b
branch: feature/community-launch-b
phase: reviewed
codex_session:
created: 2026-10-31
---
# Community launch — strand B: permissions, Flatpak remote, AUR, contract text

Mother plan: `docs/plans/community-launch.md` (decisions, facts, manual steps M1–M7, target values,
post-merge checks).

**Owns:** `flatpak/**`, `.github/workflows/release.yml`, `packaging/**`,
`scripts/check-flatpak-device-permissions.sh`, `RELEASING.md`, `AGENTS.md`,
`docs/agents/branching.md`, this file. Touch nothing else.

**Ownership correction (code phase, 2026-10-31):** this strand also owns the root manifest
`io.github.marvinbaudach.Reprise.yml`. `release.yml`, `scripts/check-flatpak-manifest.sh` and
`scripts/check-release.sh` build, lint and check that file rather than `flatpak/`'s copy, and the
two carry identical `finish-args`, so B1 changes both. No other strand owns it.

Nothing in this strand may need the manual steps to have happened. The publish job stays inert
until `FLATPAK_REMOTE_ENABLED` is set (M4), and the bundle changes apply only when
`FLATPAK_REPO_GPG_KEY` exists (M2).

## Tasks

**B1 — sandbox permissions.**
- Add to `finish-args`, each with a one-line comment giving the reason:
  - `--filesystem=xdg-music` (read-write: tags, `.lrc` files and covers are written beside music
    files);
  - `--filesystem=~/.local/share/rhythmbox:ro`;
  - `--filesystem=~/.var/app/org.gnome.Rhythmbox3/data/rhythmbox:ro`.
- Extend `scripts/check-flatpak-device-permissions.sh` to allow exactly these three and keep
  rejecting every other `--filesystem`.
- If a local `flatpak-builder` build is possible, verify that the `~/.var/app/…` grant is honoured.
  If Flatpak refuses it, drop that line and report "Rhythmbox from Flathub is not supported" as a
  known limitation in the strand summary (post-merge check X4). If no local build is possible, say so;
  P2 then verifies it.

**B2 — the bundle points at the remote.** In the `flatpak` job, when `vars.FLATPAK_REPO_GPG_KEY`
is set, build the bundle with
`--repo-url=https://marvinbaudach.github.io/reprise-flatpak/repo`,
`--runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo` and `--gpg-keys` written from that
variable. Otherwise build exactly as today.

**B3 — the publish job.** A new job `flatpak-remote` with `needs: publish`, running only on `push`
to `main` and only when `vars.FLATPAK_REMOTE_ENABLED == 'true'`. Once enabled, it fails loudly if a
secret is missing. Steps:
1. Import the GPG key (`FLATPAK_GPG_PRIVATE_KEY`, optional `FLATPAK_GPG_PASSPHRASE`).
2. Clone `reprise-flatpak`'s `gh-pages` at depth 1 with `FLATPAK_REPO_DEPLOY_KEY`.
3. `ostree init --mode=archive-z2 --repo=repo` if the branch has no repository yet.
4. `flatpak build-import-bundle --gpg-sign=<id>` of the release bundle.
5. `flatpak build-update-repo --gpg-sign=<id> --generate-static-deltas --prune --prune-depth=3`.
6. Write `reprise.flatpakrepo` and `reprise.flatpakref` (`Name=io.github.marvinbaudach.Reprise`,
   `Branch=master`, `Url`, `RuntimeRepo`, `GPGKey`, `IsRuntime=false`), a minimal `index.html` that
   links back to the project, and `.nojekyll`.
7. Verify that the new commit is in the summary (`ostree summary --view`).
8. Commit everything as a single orphan commit and force-push `gh-pages`, so that repository's
   history never grows.

The published `.flatpakref` URL is the target value
`https://marvinbaudach.github.io/reprise-flatpak/reprise.flatpakref`.

**B4 — AUR `reprise-git` (decisions 2 and 3).** Keep `packaging/aur/PKGBUILD` as `reprise-git`:
- pin the source to `#branch=main`;
- keep the stem backend and `onnxruntime` as an optional dependency;
- bring `depends`/`makedepends` in line with the README's build requirements (Rust 1.92+, GTK
  4.22+, libadwaita 1.9+);
- generate and commit `.SRCINFO` with `makepkg --printsrcinfo`;
- run `namcap` on the PKGBUILD if it is available.

A real build is a manual step before M6.

**B5 — contract and release documents.**
- `AGENTS.md`: replace "Not released yet — no backwards compatibility" with the consolidation
  plan's task 0.7 text (item 4), reworded so it is true before and after the first publish, for
  example "From the first build published to the self-hosted Flatpak repository onward,
  installations exist …". Update the promotion rule repeated in the GitHub-flow section the same
  way.
- `docs/agents/branching.md`: name the self-hosted Flatpak repository as the public channel whose
  first publish ends the standing promotion permission.
- `RELEASING.md`:
  - Replace the "named follow-up" paragraph (`:82-86`) with how the remote is operated: the
    enabling variable, keys and their rotation, pruning, and what to check after a publish.
  - Update the manual Rhythmbox checklist item (around `:380`) to cover the installed Flatpak and
    both host locations, citing START-7 (target value; strand A writes the rule).
- `flatpak/README.md`: the remote and the `.flatpakref`.

## Gate

`actionlint` on `release.yml`, `scripts/check-flatpak-device-permissions.sh`,
`scripts/check-flatpak-manifest.sh`, `bash -n` on the PKGBUILD, and the documentation gates of
`scripts/check-merge-readiness.sh`.

## Implementation record

Completed on `feature/community-launch-b`:

- B1 added the three scoped filesystem grants to both manifests and extended the
  device-permissions gate to require exactly one of each. The Flathub Rhythmbox
  grant could not be verified locally because this run explicitly forbids
  Flatpak builds; P2 must verify it in the installed sandbox.
- B2 adds remote metadata and the public key to bundles only when
  `FLATPAK_REPO_GPG_KEY` is non-empty. With the variable absent, bundle creation
  keeps its previous command and behavior.
- B3 adds an inert `flatpak-remote` publisher gated on a `main` push and
  `FLATPAK_REMOTE_ENABLED == 'true'`. It requires the signing/deploy credentials,
  publishes a signed repository, prunes to three builds, and replaces the Pages
  branch with one orphan commit.
- B4 keeps `reprise-git`, pins its source to `main`, retains optional
  `onnxruntime`, updates Rust/GTK/libadwaita build requirements, and regenerates
  `.SRCINFO`. A real package build remains a manual step before M6.
- B5 documents the compatibility cut-off, promotion permission, remote
  operations, install reference, and START-7 Rhythmbox checks.

The run did not perform a Flatpak build, any Cargo command, or a real AUR build,
per task constraints. `actionlint` availability and the requested gates are
recorded in the strand summary.

Finishing review (second pass) corrected four defects in the first implementation:

- The device-permissions gate had a syntax error (a case pattern list split across lines).
- `flatpak-remote` deleted the deploy key right after cloning, so the final `git push` could not
  authenticate; the key is now removed after the push.
- A cloned `gh-pages` lacks the empty directories ostree needs, and `ostree init` does not recreate
  them; the job now creates them before importing.
- A passphrase-protected signing key could not sign non-interactively; the passphrase is now preset
  in the gpg agent. The summary check also looks for the new commit rather than the application id.
- `AGENTS.md` and `docs/agents/branching.md` keep the sentence `scripts/tests/github-flow.sh` requires.

`flatpak-builder-lint` reports `finish-args-flatpak-appdata-folder-org.gnome.Rhythmbox3-data-rhythmbox-ro-access`
for the Flathub-Rhythmbox grant. The rule is `[planned]` (GP-14), so it only warns; the grant stays.
