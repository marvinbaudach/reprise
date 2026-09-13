---
slug: every-view-passes-the-ux-audit
worktree:
branch:
phase: planned
codex_session:
created: 2026-09-13
strands: a,b,c
merge_order: a,b,c
---
# Every view passes the UX audit

Mother plan. Frozen at the end of the plan phase (2026-09-13). Strand files:
`every-view-passes-the-ux-audit-a.md` (geometry), `-b.md` (feedback and
counting), `-c.md` (language). Each strand carries this file unchanged.

## Source and goal

UX audit of 2026-09-13: two live screenshots, a compliance pass against
`docs/ux-rules.md`, and a headless Xvfb tour of every view at 1920×1080,
1280×720, 1024×600 and the enforced minimum 600×400 (48 PNGs; the tour ran the
schema-84 binary of `feature/the-flatpak-sources-follow-the-lock`, so the
cover-cloud work in flight on `song-visuals-ask-the-stored-category` is not in
the evidence and nothing here depends on it).

Goal: the app stays operable at every window size, says each fact once, shows
that it is loading, and uses one vocabulary (GNOME HIG plus the rulebook).
Presentation only. The rulebook's "every feature reaches every frontend"
clause is answered here: nothing below reads or changes data a caller could
name, so no MCP surface changes.

## Decisions taken in the grill (binding)

1. DEVICES scrolls with the places inside the navigation list; only ISSUES
   and running cards stay pinned (FB-8 gains that sentence).
2. The window minimum height is measured and raised to the smallest value at
   which the player bar and the LIBRARY block fit; the layout fix that keeps
   the bar from shrinking lands regardless.
3. Narrow tables collapse columns in the order Rating, Year, Length, Album;
   Cover, Title, Artist never collapse; Year, Length, Rating get fixed widths.
4. The library status pill goes. The filter row's idle caption carries count
   and duration; the Up Next footer gains its scope word "Up next ·".
5. Inside Preferences the floating activity chip goes; the Background
   Activity bar lists every running task and its count matches.
6. Views waiting on data show a centred spinner row (design intent 150 ms,
   not asserted) and never the previous view's rows.
7. QUE-2a adopts the code's "Playing from <place> · N tracks".
8. Labels, titles, menu items and status badges use HIG header
   capitalisation; descriptions and status lines use sentence case. The
   Plugins count badge keeps its uppercase rendering.
9. Only the Online-content master description is reworded.
10. All four small consistency items ship, split by file owner.
11. Equalizer preset sensitivity and podcast-title entity decoding are
    follow-ups (see below), because their files are in flight elsewhere.
12. Three strands, merge order A, B, C.

## Rulebook discipline (binds every strand)

- A behaviour change flips or adds a rule in the same commit as its
  rule-named test. IDs are append-only. Next free IDs are named per task; a
  strand never uses an ID another strand's file names.
- Geometry is proven by result (STYLE-1): `compute_bounds` allocations, never
  "property X is set".
- `scripts/check-ux-traceability.sh` is green per strand.
- `docs/ux-rules.md` is shared. Each strand edits only the sections its
  strand file lists and appends new rules at the end of a section. No strand
  renumbers, reflows or touches another strand's section.

## Follow-ups (not in this plan)

- **Equalizer preset row follows the switch.** `preference_equalizer.rs:87`
  binds only the bands expander; the preset row stays operable with the
  equalizer off. After `feature/every-row-keeps-its-title` lands: bind the
  preset row too, test `set_17_preset_follows_the_equalizer_switch`.
- **Podcast titles keep HTML entities.** "Gülsha &amp; Maja Podcast" is
  stored with the literal entity (CDATA title, entities not decoded). After
  `feature/the-feeds-read-quick-xml-0-42` lands: a `decode_html_entities`
  pass in a new `podcasts/feed_text.rs` over title/author/description
  (named and numeric, idempotent), stored title rewritten on refresh, test
  `pod_27_titles_never_keep_an_entity` with a CDATA fixture.
- "Recently Played" sorts by artist, not recency (BROWSE decision).
- NAV-16 names Releases and Concerts as optional places while the sidebar
  groups them under SMART, where NAV-16 forbids the turn-off menu. Rulebook
  clean-up.
- Keyboard and AT-SPI audit, compact mode, cover-cloud branch at low
  resolution.

## Parallelität

Three strands, disjoint by file, each one Codex run in its own worktree from
`origin/dev`. Ownership globs live in the strand files; the summary:

- **A — geometry:** `ui/sidebar/` layout files (not `mod.rs`, `sidebar_rebuild.rs`,
  `sidebar_presentation.rs`, `sidebar_module_menu.rs`, which
  `feature/the-sidebar-stops-counting-what-is-turned-off` owns), `ui/window/**`,
  `ui/player_bar/**`, `ui/track_list/track_list_columns*.rs` and the new
  `track_list_column_widths*.rs`, `ui/preferences/preferences_window.rs` and
  `preferences_chrome_placement_tests.rs`, `ui/stats/**`, the device row
  files, `ui/style/**`. Rulebook sections B, S, F (SET-19 only), V, E.
- **B — feedback and counting:** `ui/status_bar*.rs` (deleted), the filter
  row count label's files, `ui/now_playing/up_next_panel*.rs`,
  `ui/preferences/preference_background_bar*.rs`, `ui/podcasts/**`,
  `ui/queue/**`, `ui/updates/**`, `ui/strings.rs` (status constants only).
  Rulebook sections G, J, K, U. Not `now_playing/now_playing.rs`.
- **C — language:** `ui/strings_sidebar.rs`, `strings_releases.rs`,
  `strings_online_sources.rs`, `ui/preferences/preference_plugins*.rs`,
  `preference_online_master*.rs`, `ui/releases/**` (column header only), the
  sidebar icon table for smart lists. Rulebook sections AI (GP-21), J (QUE-2a), R, T (network).

**Merge order: A, B, C.** A changes the allocations B's filter-row caption is
measured against; B's footer format is what C's QUE-2a wording lands on.

**Post-merge cross-checks** (read files no single strand owns):
1. Headless tour at 1280×720 and 1024×600 (recipe:
   `memory/reprise-screenshot-harness.md`): every sidebar place visible or
   reachable by scrollbar; no empty band under the cards; player bar whole at
   the minimum window; no clipped column; exactly one counter in the
   library view; no activity chip in Preferences.
2. `scripts/check-ux-traceability.sh` green on dev after the last strand.
3. The filter-row caption (B) fits at 1024 px beside A's collapsed columns.
4. `ui/strings.rs` after B's constant removal compiles beside C's string
   changes.
5. The Releases header row (C removes the "Cover" text) keeps the column
   pinned per NR rules with A's width policy untouched (Releases is not a
   track table).
