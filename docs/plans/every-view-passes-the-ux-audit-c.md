---
slug: every-view-passes-the-ux-audit-c
worktree: /home/marvin/Projects/reprise-every-view-passes-the-ux-audit-c
branch: feature/every-view-passes-the-ux-audit-c
phase: planned
codex_session:
created: 2026-09-13
---
# Every view passes the UX audit — strand C: language

Mother plan: `every-view-passes-the-ux-audit.md` (read it first; its
decisions and rulebook discipline bind this strand).

## File ownership

Writes only to:
- `crates/reprise-gnome/src/ui/strings_sidebar.rs`,
  `crates/reprise-gnome/src/ui/strings_releases.rs`,
  `crates/reprise-gnome/src/ui/strings_online_sources.rs`
- `crates/reprise-gnome/src/ui/preferences/preference_plugins*.rs`,
  `crates/reprise-gnome/src/ui/preferences/preference_online_master*.rs`
- `crates/reprise-gnome/src/ui/releases/**` — the column header only
- the file that maps smart-list entries to their sidebar icons (locate it;
  write the path here in the first commit; it must not be one of strand A's
  sidebar layout files — if it is `sidebar_presentation.rs`, which is in
  flight elsewhere, the icon change waits and is noted as skipped)
- tests beside each of the above
- `docs/ux-rules.md` sections AI (append GP-21 after GP-20), J (append QUE-2a, mark QUE-2 `[replaced by QUE-2a]`), R (the
  badge text), T network opt-in (SET-11a's quoted description)

Not owned: `preference_equalizer.rs`, `reprise-core/src/podcasts/feed.rs`
(follow-ups in the mother plan), `ui/strings.rs` (strand B).

## Tasks

**C1 — Capitalisation (decision 8).** Header capitalisation for labels,
titles, menu items and status badges; sentence case for descriptions and
status lines. Changes: "Recently Played", "Top Rated", "Recently Added";
Releases badge "Upcoming" beside "Missing" and "Incomplete". The Plugins
count badge keeps its rendering. New **GP-21** [active] [gtk] naming the two
styles; test `gp_21_labels_use_header_case` asserting every sidebar row
label and every Releases badge string.

**C2 — QUE-2a (decision 7).** "Playing from <place> · N tracks" becomes
the rule; "Next in Queue" stays the manual section's title; two sections as
before. QUE-2 marked replaced; its tests re-hung onto QUE-2a in the same
commit. No string change.

**C3 — Master description (decision 9).** `strings_online_sources.rs`:
"Turn off to keep Reprise offline: none of these plugins run, nothing is
requested, and their sidebar entries are hidden." SET-11a's quoted text and
the string test change in the same commit. "Local" and "audio-reactive"
stay.

**C4 — Small items (decision 10).**
1. "Recently Added" gets an icon that is not the playlist-add "+"
   (`list-add-symbolic`); use a clock/plus variant that exists in the
   symbolic set the app ships, e.g. `document-new-symbolic` or the
   `folder-download`-style glyph already used elsewhere in the sidebar.
2. The Releases "Cover" column header renders no text; the column keeps its
   name in the column editor. Covered by the NR rule that names the column
   editor; add one sentence there.

## Verification inside this strand

`cargo test -p reprise-gnome` for the named tests plus the sidebar strings,
releases, preferences plugin suites; `scripts/check-ux-traceability.sh`.
Whether the new badge text fits the badge geometry after strand A's dialog
changes is a post-merge check.
