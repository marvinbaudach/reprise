---
slug: the-ci-stops-repeating-itself-b
worktree: /home/marvin/Projects/reprise-the-ci-stops-repeating-itself-b
branch: feature/the-ci-stops-repeating-itself-b
phase: coded
codex_session:
created: 2026-08-31
---
# Strand B — the Flatpak SDK stops being re-downloaded

Mother plan: [`the-ci-stops-repeating-itself.md`](the-ci-stops-repeating-itself.md).
Read its "Why" and "Out of scope" before starting.

**Wave 1**, concurrent with strand `a1`. **Lands first of all three.**

## Why this strand exists and lands first

`Install Flatpak tooling and GNOME 50 SDK` is **11.1 min of the Flatpak job's
21.8**, and it downloads three refs that have nothing to do with the code under
test. It is the largest single confirmed win in the whole plan, and the change is
one `actions/cache` block — so it must not wait behind the review of the risky
CI restructuring.

## File ownership

```
.github/workflows/release.yml
.github/tests/release-workflow.sh
```

Touch nothing else. `ci.yml`, `scripts/**` and `.github/scripts/**` belong to
strands `a1` and `a2`.

## B1 — cache the user Flatpak installation

The install is:

```yaml
flatpak --user install --noninteractive --or-update --no-static-deltas flathub \
  org.gnome.Platform//50 \
  org.gnome.Sdk//50 \
  org.freedesktop.Sdk.Extension.rust-stable//25.08
```

`--user`, so the state lives in `~/.local/share/flatpak`.

Add `actions/cache` around it, keyed on the three ref strings so the key changes
exactly when a runtime is bumped and never otherwise. Keep the install step: on
a cache hit `--or-update` becomes a fast no-op, and it re-fetches when a ref has
moved; it compares commits rather than repairing corrupt checked-out files.

### No warm-up job is needed — and one would not have worked

GitHub scopes caches by ref: a run restores caches from its own ref or from the
default branch, which is `main` here. A cache written on `dev` is **invisible**
to the Release run, which only ever runs on `main`.

It self-warms instead: `Release` runs on `main` every time, so the first main
push after landing pays the 11.1 min and writes the entry, and every later main
push restores it. Pull-request runs of `release.yml` can read it too, since it
lives on the default branch.

### The ordering trap

`Reclaim disk for the Flatpak build` currently runs **after** the install. A
restored cache adds disk pressure before that reclaim happens. Move the reclaim
step **before** the cache restore.

### The win is an estimate — measure it

Up to −11 min, unverified. Restoring a multi-GB OSTree tree through
`actions/cache` may not beat the download, and OSTree hardlinks and ownership
survive `actions/cache` poorly. A restore that silently yields a broken repo is
worse than the download.

If the measurement disappoints, the fallback is a prepared container image
carrying the SDK, where the cost is a registry pull. **Do not** implement both.

## Verification

1. `.github/tests/release-workflow.sh` green — it is the only test that asserts
   this file's content, and `base-contracts` runs it via `.github/tests/*.sh`.
2. `gh workflow run release.yml -f dry_run=true` twice on the branch: the first
   run populates, the second must show a cache hit and a `Install Flatpak
   tooling and GNOME 50 SDK` step well under 11.1 min. Compare against the 21.8
   min baseline for the whole job.
3. `flatpak --user list` after the restore shows all three refs. This asserts
   ref presence; a corrupt-but-registered tree is not caught here and surfaces
   later in `flatpak-builder`.
4. The bundle still builds: `Create the single-file bundle` succeeds and the
   uploaded artifact is non-empty.

Cache-store size is checked post-merge in the mother plan, not here — the
budget is shared with strand `a2`.

## Not in this strand

The APK `target/` cache (dropped: `rust-cache` is limited to `core-suite`).
Anything in `ci.yml`. Parallelising the release stage against dev CI — see the
mother plan's "Out of scope".
