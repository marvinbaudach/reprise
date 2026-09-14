---
slug: every-view-passes-the-ux-audit-a
worktree: /home/marvin/Projects/reprise-every-view-passes-the-ux-audit-a
branch: feature/every-view-passes-the-ux-audit-a
phase: shipped
codex_session:
created: 2026-09-13
---
# Every view passes the UX audit — strand A: geometry

Mother plan: `every-view-passes-the-ux-audit.md` (read it first; its
decisions and rulebook discipline bind this strand).

## File ownership

Writes only to:
- `crates/reprise-gnome/src/ui/sidebar/{sidebar_root,sidebar_navigation_scroller,sidebar_activity_slot,sidebar_issues_section,sidebar_device_section,sidebar,sidebar_layout_tests,sidebar_tests}.rs`
  (a new module line in `sidebar/mod.rs` is allowed only if a new file is
  unavoidable; prefer none — `mod.rs` is in flight on another branch)
- `crates/reprise-gnome/src/ui/window/**`
- `crates/reprise-gnome/src/ui/player_bar/**`
- `crates/reprise-gnome/src/ui/track_list/track_list_columns*.rs`,
  `crates/reprise-gnome/src/ui/track_list/track_list_column_widths*.rs` (new),
  one module line in `track_list/mod.rs`
- `crates/reprise-gnome/src/ui/preferences/preferences_window.rs`,
  `crates/reprise-gnome/src/ui/preferences/preferences_chrome_placement_tests.rs`
- `crates/reprise-gnome/src/ui/stats/**`
- the device row files under `crates/reprise-gnome/src/ui/device_sync/` that
  build the sidebar device card's status line
- `crates/reprise-gnome/src/ui/style/**`
- `docs/ux-rules.md` sections B (append NAV-20), S (append STYLE-14), F
  (append SET-19), V (append STATS-24), E (append MTP-65), and one sentence
  appended to FB-8 in G.

## Tasks

**A1 — Navigation list starved, pinned block holds phantom height.**
Evidence: at 1280×720 the list is cut at the PLAYLISTS header with ~90 px of
empty sidebar under the activity card; at 1024×600 the cut runs through the
YouTube row; at 1920×1080 expanding DEVICES removes "My Stats" and the wheel
does not reveal it. Cause: `sidebar_root.rs` stacks the scroller (only
`vexpand` child, no floor), `activity_slot.root` (DEVICES) and the pinned
`region` (ISSUES + cards, `valign End`); non-expand siblings take natural
height first. The empty band means the pinned block is allocated more than
it paints, which FB-8 forbids.
1. Failing test first: `fb_8_pinned_block_holds_only_what_it_paints` in
   `sidebar_layout_tests.rs`. Sidebar in a `gtk4::Window` with 660 px content
   height, Library rows + one playlist + six smart rows, one remembered
   device, one active and one finished progress card. Assert: (a) the pinned
   region's allocation equals the sum of its visible children's natural
   heights; (b) no nav row is cut (last visible row bottom ≤ pinned region
   top); (c) `vadjustment.upper > page_size`, scrollbar visible and
   targetable; (d) after `set_value(upper − page_size)` "My Stats" intersects
   the viewport; (e) a scroll event dispatched over a row moves the
   adjustment.
2. Phantom height: finished cards become `visible=false` when their fade
   ends (not opacity 0); no `Revealer` with `reveal_child=false` reserves its
   child; no `Stack` in the slot is `vhomogeneous`.
3. DEVICES moves inside the scroller below SMART (decision 1). The scroller
   gets `min_content_height` equal to the LIBRARY block (heading + five
   rows). ISSUES and running cards stay pinned. New **NAV-20** [active]
   [gtk]: "The navigation list never yields below its LIBRARY block. DEVICES
   scrolls with the places. ISSUES and running cards stay pinned and hold
   exactly the height they paint." FB-8 gains: "Resting device status
   scrolls with the places; a running sync card stays pinned."
   Test `nav_20_devices_scrolls_and_library_keeps_its_floor`.

**A2 — The minimum window hides the player bar (STYLE-5).**
1. Test `style_5_player_bar_survives_the_minimum_window` in a new
   `window/window_layout_tests.rs`: window at `MIN_WIDTH × MIN_HEIGHT` from
   `window_bootstrap.rs`; assert the player bar's bounds lie inside the
   window at its natural height and the sidebar shows its LIBRARY block.
2. Fix: the shell's vertical box gives the player bar its natural height as
   minimum (`vexpand false`), the split view is the only shrinking child.
3. Raise `MIN_HEIGHT` to the smallest value where the test passes (decision
   2); record the number in STYLE-5's text. **That number comes from a test
   run, never from estimation:** if the display test cannot execute in this
   sandbox, write the test, leave `MIN_HEIGHT` and STYLE-5's text unchanged,
   and report A2.3 as skipped with the reason.
   Seam note (2026-09-13): the shell's vertical box lives in
   `ui/player_bar/library_player_bar.rs` and `MIN_*` in
   `ui/window/window_bootstrap.rs`; strand B removes the status-bar mount from
   `ui/window/window.rs`, so keep any edit to `window.rs` to lines that
   concern the player bar — preferably none.

**A3 — Columns fall off the right edge (STYLE-6).** At 1920 the fifth
rating star is clipped; at 1280 Rating is gone; at 1024 Year/Length/Rating
are gone and cells end mid-word.
1. Test `style_6_the_table_never_overflows_its_viewport` in
   `track_list_columns_tests.rs`: default six columns in 700 px and 1000 px
   viewports; the sum of visible column widths ≤ viewport width and
   `hadjustment.upper == page_size`; at 1600 px all six are visible and the
   rating cell shows five stars unclipped.
2. New `track_list_column_widths.rs`: Cover, Title, Artist never collapse;
   collapse order Rating, Year, Length, Album (decision 3); Rating fixed to
   five stars' width; Year and Length fixed from their widest sample
   ("2025", "12:34"); Album expands. Collapsing never touches stored column
   preferences; widening restores.
3. Year and Length right-aligned. New **STYLE-14** [active] [gtk]: "Numeric
   columns are right-aligned; the rating column is wide enough for five
   stars whenever it is shown; columns collapse in the order Rating, Year,
   Length, Album." Test `style_14_numeric_columns_align_right`.
4. The playing row highlights as one row, not per cell (CSS in `style/`,
   decision 10). Covered by a rule-named test only if an existing PLAY rule
   describes the highlight; otherwise a plain widget test.

**A4 — Preferences at 720p.** `PREFERENCES_CONTENT_HEIGHT = 680 + bar`.
1. Test `set_19_pages_scroll_inside_a_short_window` in
   `preferences_chrome_placement_tests.rs`: dialog in a 720 px window; its
   bounds inside the window; every page's scrolled window reaches its last
   row. New **SET-19** [active] [gtk] with that sentence.
2. Fix only what the test shows: `content_height` clamps to the window
   height minus margin. The Background Activity bar stays a reserved strip
   (FB-9 second choice, decision 5); strand B owns its content.

**A5 — Mid-word ellipses.**
1. My Stats top-artist card titles wrap to two lines before ellipsizing;
   below 900 px content width the card row scrolls horizontally instead of
   squeezing. New **STATS-24** [active] [gtk]; test
   `stats_24_card_titles_wrap_before_they_cut`.
2. The device row status line drops its "· syncing/synced" tail when the
   row is narrower than the text's natural width (the MTP rule near line
   1094 already prefers omission over hiding). New **MTP-65** [active] [gtk];
   test `mtp_65_status_tail_yields_before_the_activity`.

## Verification inside this strand

`cargo test -p reprise-gnome` for the named tests plus the sidebar, window,
track_list, preferences and stats suites; `scripts/check-ux-traceability.sh`.
Cross-strand comparisons are in the mother plan's post-merge list and are not
made here.
