---
slug: the-real-window-passes-the-ux-audit-a
worktree: /home/marvin/Projects/reprise-the-real-window-passes-the-ux-audit-a
branch: feature/the-real-window-passes-the-ux-audit-a
phase: planned
codex_session:
created: 2026-09-15
---
# The real window passes the UX audit — strand A: the real window

Strand file. Shared context, the binding decisions D1–D8, the rulebook
discipline, the merge order and the post-merge cross-checks are in the
mother plan `the-real-window-passes-the-ux-audit.md` — read it first; it
is frozen and this strand never edits it. Touch only the files this strand
owns (listed under "Files" below); every other path belongs to the other
strand or to nobody in this round.

## Files, rulebook sections and tasks

Files: `crates/reprise-gnome/src/ui/window/**`, `ui/sidebar/sidebar_root.rs`,
`sidebar_issues_section.rs`, `sidebar_activity_slot.rs`,
`sidebar_navigation_scroller.rs`, `sidebar_layout_tests.rs`, `sidebar.rs`
(handles only), `ui/player_bar/**`, `ui/track_list/track_list_columns*.rs`,
`track_list_column_widths*.rs`, `table_columns/**`,
`track_list_row_interaction.rs`, `ui/style/**`,
`crates/reprise-core/src/library/session.rs`, `session_normalization.rs`.
Rulebook sections G (FB-8 amend, FB-15 append), U (STYLE-5 amend, STYLE-6
test list), C (PLAY-17 append). Not `track_list_reload.rs`,
`track_list_sort*.rs` (strand B).

**A0 — The instrument.** `window/window_layout_test_hook.rs` (`cfg(test)`,
thread-local `publish`/`take` like the online-module hook) publishing
`window`, `split_view`, `sidebar_page`, the sidebar's `Rc` (activity slot,
issues listbox, navigation scroller), the player-bar shell and the bar
widget, `content_nav` and the track list's `ColumnView` plus its scrolled
window. `publish` is called at the end of `surface::build` beside the
existing one. A helper in a new `window/real_window_tests.rs`:
`build_real_window(width, height, SidebarSeed)` — `gtk4::init`, unique
`application_id`, `register`, `test_db::open()`, session state saved with
`reprise_core::library::session::save` (`window_width/height`, not
maximised; the normaliser clamps at 600/`MIN_HEIGHT`), the same production
stylesheet `surface::build` installs (verify by grep; add
`style::install()` only if `build` does not), `surface::build`, `take`,
then pump the main loop until mapped and `window.width()/height()` equal
the request. `SidebarSeed` uses exactly the calls of
`sidebar_geometry_fixture()` (`sidebar_layout_tests.rs:493-546`): issue
rows via `build_issue_nav_row` on the shared `issues_listbox`, running
cards via `append_doctor_card`/`append_relink_card` + `set_reveal_child(true)`,
a device via `present_device_section_for_test` + `set_device_section`. Track
rows for A4 are inserted the way reprise-core's own query tests insert
tracks (find the helper; three tracks with a 2025 year and `12:34` length
are enough). `chain_report(&window) -> String` walks the two vertical
paths (content → sidebar scroller/pinned block; content → player bar) and
the horizontal one (content → `ColumnView`), printing css name, type,
bounds, `measure` min/nat at the allocated width, vexpand/valign; every
assertion below attaches it — this is what localises the band and the 635.

**A1 — No band under the pinned block (FB-8).** Test
`fb_8_the_real_sidebar_leaves_no_band_under_the_pinned_block` at 1280×720
with one issue row, one running card, one device: `scrolled.bottom ==
pinned.top`, `pinned.bottom == sidebar_page.bottom == split_view.bottom ==
bar.top` (±1 px), and `scrolled.height == sidebar_page.height −
pinned.height`. Fix wherever the report shows the slack — the candidates
in "Source" — and name the culprit in the commit message. FB-8's text gains
"…measured on the composed window, not a fixture".

**A2 — The pinned block yields, the LIBRARY floor never (FB-15).**
`sidebar_issues_section.rs`: the region (ISSUES box + activity slot) goes
into a `ScrolledWindow` — `vscrollbar_policy Automatic`, `hscrollbar_policy
Never`, `propagate_natural_height true`, no `min_content_height`, `vexpand
false`, `valign End` — so its natural height is exactly its content and its
minimum is what a scroller needs. The existing fixture tests (`fb_8_*`,
`doc_5c`, `doc_5e`, `nav_20_*`, `npp_1_*`) stay green. New **FB-15**
[active] [gtk]: "The pinned block never claims more than it paints and
never raises the window's minimum: below the room the LIBRARY floor
leaves, the block scrolls inside itself. At the minimum window the ISSUES
heading, one row and one running card are visible." Test
`fb_15_three_running_cards_never_raise_the_window_minimum` on the real
assembly at `MIN_WIDTH × MIN_HEIGHT` with three issue rows, three running
cards and one device: `window.measure(Vertical, MIN_WIDTH).0 <= MIN_HEIGHT`,
LIBRARY row 5 bottom ≤ pinned top, bar whole, pinned `vadjustment.upper >
page_size`.

**A3 — An honest minimum (STYLE-5).** With A2 in place, measure the real
assembly's vertical minimum with one issue row and one running card; round
up to the next 10; that is `MIN_HEIGHT` (D2, ceiling 560). One constant
in reprise-core (`library::session::MIN_WINDOW_HEIGHT`, beside a
`MIN_WINDOW_WIDTH = 600`) used by the normaliser's clamp and re-exported as
`window_bootstrap::MIN_HEIGHT`; `session.rs`'s clamp test (line ~426)
follows. STYLE-5's last sentence becomes "At the enforced 600 × {N}
minimum, the structural player bar's bounds lie inside the window at its
natural height, the LIBRARY block is whole, and the pinned block shows its
ISSUES heading, first row and first running card." Test
`style_5_the_real_window_holds_its_player_bar_at_the_minimum` (the fixture
test is re-hung onto this one and deleted): window allocation ==
`MIN_WIDTH × MIN_HEIGHT`; `measure(Vertical).0 <= MIN_HEIGHT`;
`measure(Horizontal, MIN_HEIGHT).0 <= MIN_WIDTH`; bar bounds inside the
window at natural height; the sidebar column's and the content column's
minima both ≤ the room they get (the report names which column defines
the minimum). If the horizontal measure fails, report it — do not widen
MIN_WIDTH in this strand.

**A4 — The table never overflows the real viewport at 1280 (STYLE-6).**
Test `style_6_the_real_table_never_overflows_at_1280` at 1280×720 with the
sidebar shown and three track rows: the `ColumnView`'s
`hadjustment.upper == page_size`; every visible header and the last row's
cells lie inside the viewport; the Length and Year cell labels' bounds end
≤ viewport right. Candidates: the fit runs before `set_show_sidebar(true)`
narrows the pane and never re-runs; the fit ignores the vertical
scrollbar's width or the cell padding STYLE-14's right alignment now
exposes. Fix in the fit, re-fitting on the viewport's width (the scrolled
window's `hadjustment` `page-size` notify), never on reload
(`track_list_reload.rs` is strand B's). STYLE-6 lists the new test.

**A5 — The playing row is one highlight (PLAY-17).** When cells receive
`NOW_PLAYING_CLASS` (`track_list_columns.rs:33`), the enclosing `row` node
(walk parents until `css_name() == "row"`) gets `now-playing-row`; CSS in
`track_list_row_interaction.rs:22` moves the tint to `row.now-playing-row`
and keeps `.now-playing-leading`'s accent; the per-cell background goes.
New **PLAY-17** [active] [gtk]: "The playing row is highlighted as one row:
one tint across the full row width with no seams between cells, plus the
leading accent." Test `play_17_the_playing_row_is_one_highlight`: after
marking, the row widget carries the class, no cell carries a background
rule (CSS parsed with `css_parse_errors` empty), and unmarking clears
both.

Verification inside A: the five tests through the isolated runner, the
existing sidebar/window/track-list display suites, `cargo test -p
reprise-core library::session`, traceability. The tour comparison against
the control arm is post-merge.
