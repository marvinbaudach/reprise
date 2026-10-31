---
slug: community-launch-c
worktree: /home/marvin/Projects/reprise-community-launch-c
branch: feature/community-launch-c
phase: planned
codex_session:
created: 2026-10-31
---
# Community launch — strand C: README, Showroom, issue templates

Mother plan: `docs/plans/community-launch.md` (decisions, target values, post-merge checks).
Message rules: spec §1. Every comparative claim must hold against Rhythmbox 3.5.1, and Reprise is
"an alternative for Rhythmbox users", never a "successor" or a "replacement".

**Owns:** `README.md`, `showroom/**`, `.github/ISSUE_TEMPLATE/**`, this file. Touch nothing else.

This strand lands last: its README points at a `.flatpakref` that exists only after the first
publish.

## Tasks

**C1 — README.**
- Put the core sentence from spec §1 directly under the title.
- Add a "Coming from Rhythmbox?" section:
  - what is imported: ratings, play counts and last played, date added, playlists;
  - where the import is offered: on first run and in Preferences;
  - the Flatpak note: the XDG music directory is accessible, and music elsewhere needs `flatpak
    override --user --filesystem=<dir> io.github.marvinbaudach.Reprise`.
- Downloads: list the `.flatpakref` first
  (`https://marvinbaudach.github.io/reprise-flatpak/reprise.flatpakref`, target value from strand
  B), then the bundle and the APK as today.
- Add "Known limitations":
  - the Flatpak is x86_64 only;
  - music outside the XDG music directory needs an override;
  - there is no stem separation in the Flatpak;
  - this is an early test release.
- Extend "How this project is built" with a short FAQ: what the AI does, what the maintainer does,
  and what the gates check. Its first sentence is the AI sentence from spec §1.
- Check "Why Reprise" against Rhythmbox 3.5.1. Keep what is true, and claim nothing unverified.

**C2 — Showroom.**
- Add a `ComingFromRhythmbox` section component, rendered after `ShowreelFilm` in `App.tsx`. For
  now it is text only; the clip arrives in P1.
- Its install link is the same `.flatpakref` URL.
- `npm run lint`, `npm run typecheck` and `npm test` stay green.

**C3 — issue templates.**
- New `.github/ISSUE_TEMPLATE/rhythmbox_import.yml`, labelled `needs-triage`, with these fields:
  - Reprise version;
  - install type: Flatpak repository, bundle, AUR, or source;
  - Rhythmbox source: distribution or Flathub;
  - library size in tracks;
  - checkboxes for what came over: ratings, play counts and last played, date added, playlists;
  - what did not come over;
  - expectations about automatic playlists, podcasts and radio;
  - the report from **"Copy Diagnostics"** in the primary menu (target value from strand A).
- `bug_report.yml` gains an optional field for the same report.

## Gate

The Showroom scripts above and the documentation gates of `scripts/check-merge-readiness.sh`.

## Execution record

- **C1 — README: complete.** The core sentence, Rhythmbox import details, repository install link,
  four known limitations, and AI FAQ are present. The existing Why Reprise section now describes
  Reprise's features without claiming Rhythmbox lacks them.
- **C2 — Showroom: complete.** The text-only Rhythmbox section follows the product film and links
  to the target Flatpak reference.
- **C3 — issue templates: complete.** The import report form and optional bug-report diagnostics
  field use the agreed `Copy Diagnostics` label.

Comparative wording was narrowed because Rhythmbox 3.5.1 documents overlapping library fields,
lyrics, podcasts, radio, and Android/device integrations. No remaining bullet under Why Reprise
asserts a comparative advantage. Post-merge checks X2, X3, and X4 must still verify the final
published URL, menu label, and shipped limitations across strands A and B.
