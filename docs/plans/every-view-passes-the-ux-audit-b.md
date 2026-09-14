---
slug: every-view-passes-the-ux-audit-b
worktree: /home/marvin/Projects/reprise-every-view-passes-the-ux-audit-b
branch: feature/every-view-passes-the-ux-audit-b
phase: shipped
codex_session:
created: 2026-09-13
---
# Every view passes the UX audit — strand B: feedback and counting

Mother plan: `every-view-passes-the-ux-audit.md` (read it first; its
decisions and rulebook discipline bind this strand).

## File ownership

Writes only to:
- `crates/reprise-gnome/src/ui/status_bar*.rs` (deleted) and the lines in
  `crates/reprise-gnome/src/ui/window/**` that mount it — **exception to
  strand A's ownership limited to removing the status-bar mount; record the
  exact lines in the commit message**
- the files that build the filter row's count label:
  `ui/browse/browse_filter_count.rs`, `ui/browse/browse_bar.rs`,
  `ui/browse/filter_bar_tests.rs`, and `ui/filter_bar_layout.rs`
- the files the status bar reaches into, located 2026-09-13 before the code
  phase (a grep for `status_bar`/`StatusBar` under `crates/reprise-gnome/src/ui`):
  `ui/mod.rs` (the `pub mod status_bar;` line only), `ui/window/window.rs`
  (construction, refresh/hide wiring, the `track_content::build` argument and
  the preferences argument — nothing else in that file), `ui/track_list/track_content.rs`
  (the overlay and its CSS), `ui/track_list/track_list.rs` (a doc comment),
  `ui/preferences/preferences.rs`, `ui/preferences/preference_layout.rs`,
  `ui/preferences/preference_layout_preview.rs`,
  `ui/preferences/preference_visual_strings.rs` (the Status Bar region of the
  Layout page goes with the bar; the persisted visibility setting may simply
  stay unread — no migration). Consequence in the rulebook: SET-16 in section
  F names the Status Bar region; mark it `[replaced by SET-16a]` and append
  SET-16a at the end of section F (strand A appends SET-19 there too — an
  add/add seam resolved at rebase, expected). The FIL-2 text in section K
  that describes the overlay is reworded in place under FIL-10.
- `crates/reprise-gnome/src/ui/now_playing/up_next_panel*.rs`
- `crates/reprise-gnome/src/ui/preferences/preference_background_bar*.rs`
- `crates/reprise-gnome/src/ui/podcasts/**`, `crates/reprise-gnome/src/ui/queue/**`,
  `crates/reprise-gnome/src/ui/updates/**`
- `crates/reprise-gnome/src/ui/strings.rs` — the status-bar constants only
- `docs/ux-rules.md` sections G (append FB-14; FB-9 unchanged), J (append
  QUE-15), K (append FIL-10), U (append CONTRAST-2b, mark CONTRAST-2a
  `[replaced by CONTRAST-2b]`)

Not owned: `now_playing/now_playing.rs` (cover-cloud branch). The footer
text comes from `format_up_next_footer` in `up_next_panel.rs`.

## Tasks

**B1 — One counter per view (decision 4).** Library shows "1,881 tracks" in
the filter row and "1,881 tracks · 4 days …" in a pill that covers the last
rows; the panel footer says "1,878 tracks · 4 days …".
1. Delete `status_bar.rs` and its tests; remove its mount.
2. The filter row's idle caption in the Library becomes "{n} tracks · {d} d
   {h} h" (dim caption); with a restriction it stays "{shown} of {n} tracks"
   (FIL-2a unchanged). New **FIL-10** [active] [gtk] for the idle caption;
   **CONTRAST-2b** replaces CONTRAST-2a: "No status overlay; the filter row
   is the only counting in every view." Tests
   `fil_10_idle_caption_carries_count_and_duration`,
   `contrast_2b_no_overlay_covers_a_row`; CONTRAST-2a's tests are re-hung.
3. The Up Next footer reads "Up next · {n} tracks · {d} d {h} h". New
   **QUE-15** [active] [gtk]; test `que_15_footer_names_its_scope`.
   QUE-4's formatting stays.

**B2 — Background activity once inside Preferences (decision 5).** The
floating chip names a different task than the bar while the bar counts "1"
with two running.
1. Remove the chip. The bar lists one row per running task; its count badge
   equals the row count; the empty notice shows at rest (FB-9 second
   choice). Test `fb_9_the_dialog_reports_every_running_task_once` in
   `preference_background_bar_tests.rs`.

**B3 — Loading states (decision 6).** Podcasts, Queue and the ✦ popover
paint empty, black, or the previous view for one to three seconds.
The Queue is excluded: its model is built synchronously from in-memory state
and never shows stale rows.
1. Podcasts and the popover switch to a centred spinner row the moment the view
   is requested and shows rows only after the first model delivery; the
   popover reserves its resting height so it never paints black. 150 ms is
   design intent, not asserted.
2. New **FB-14** [active] [gtk]: "A view waiting on data shows a loading row
   and never the previous view's rows; a popover reserves its resting
   height." Tests `fb_14_podcasts_show_a_loading_row_until_the_model_arrives`,
   `fb_14_updates_popover_…`.

**B4 — Up Next remove control (decision 10).** The permanent "—" per row
collides with "—" meaning unrated in the table. It becomes a hover- and
focus-revealed "×" with the accessible name "Remove from queue". Test under
the QUE rule that names the control today; if none does, extend QUE-15's
text with one sentence and cover it in its test.

## Verification inside this strand

`cargo test -p reprise-gnome` for the named tests plus the filter row,
now_playing, preferences, podcasts, queue and updates suites;
`scripts/check-ux-traceability.sh`. Whether the caption fits beside strand
A's collapsed columns is a post-merge check, not made here.
