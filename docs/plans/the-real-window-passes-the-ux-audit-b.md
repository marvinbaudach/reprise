---
slug: the-real-window-passes-the-ux-audit-b
worktree: /home/marvin/Projects/reprise-the-real-window-passes-the-ux-audit-b
branch: feature/the-real-window-passes-the-ux-audit-b
phase: planned
codex_session:
created: 2026-09-15
---
# The real window passes the UX audit — strand B: the leftovers

Strand file. Shared context, the binding decisions D1–D8, the rulebook
discipline, the merge order and the post-merge cross-checks are in the
mother plan `the-real-window-passes-the-ux-audit.md` — read it first; it
is frozen and this strand never edits it. Touch only the files this strand
owns (listed under "Files" below); every other path belongs to the other
strand or to nobody in this round.

## Files, rulebook sections and tasks

Files: `crates/reprise-gnome/src/ui/preferences/preference_equalizer*.rs`,
`preferences_tests.rs`, `crates/reprise-core/src/podcasts/**`,
`crates/reprise-core/src/db.rs` + one new migration file,
`crates/reprise-core/src/queries/smart.rs`,
`ui/track_list/track_list_sort*.rs`, `track_list_reload.rs`,
`ui/strings_sidebar.rs`, `ui/sidebar/sidebar_presentation.rs`,
`sidebar_rebuild.rs`, `ui/releases/releases_presentation.rs` (the `gp_21`
test only), `ui/now_playing/up_next_panel*.rs`, `po/**`. Rulebook sections
F (SET-17 amend), AF (POD-27), Z (BROWSE-15), AI (GP-21 amend), J
(QUE-2a), B (NAV-16 amend).

**B1 — The profile row follows the switch (SET-17).**
`preference_equalizer.rs:94-99` sets only the bands expander's sensitivity;
the profile row (`preset_row`, line 53) follows the same `active` notify
and the initial state. SET-17 gains "The profile row and the bands are
insensitive while the equalizer is off." Test
`set_17_the_profile_row_follows_the_switch` in `preferences_tests.rs`.

**B2 — Titles never keep an entity (POD-27).** New
`podcasts/feed_text.rs`: `decode_html_entities(&str) -> Cow<str>` — named
(the set `resolve_reference` already knows plus the HTML4 Latin-1 names)
and numeric decimal/hex, one pass, idempotent on decoded text. `feed.rs`
applies it to title, author and description after extraction whether the
text came from `Event::Text`, `GeneralRef` or `CData` (CDATA is where the
literal `&amp;` survives today). The stored title is rewritten on refresh
through `store.rs::update_subscription_details` (verify it writes the title
unconditionally; if it writes only on change, decoding makes the change).
New **POD-27** [active] [core]: "Feed text never keeps an HTML entity:
titles, authors and descriptions are decoded once at parse time, CDATA
included, and a refresh repairs a stored title." Test
`pod_27_titles_never_keep_an_entity` with a CDATA fixture ("Gülsha &amp;
Maja Podcast" → "Gülsha & Maja Podcast", `&#8217;` → `’`), plus a refresh
test over a stored entity title.

**B3 — A smart place opens in its own order (BROWSE-15).**
`default_sort_for_source(ViewSource::Smart(id))` returns the smart list's
`(sort_field, sort_dir)` (looked up once per route; `smart.rs` already
reads the row), so `track_list_reload.rs` queries with view sort == member
sort and the outer `ORDER BY` reproduces the inner one; the header shows a
sort indicator only when a column exists for the field (`last_played_at`
has none); a header click sorts within the view until the place changes.
New **BROWSE-15** [active] [core] [gtk]: "A smart list opens in the order
its definition names — Recently Played newest play first, Recently Added
newest first, Top Rated best first; a column sort applies until the place
changes." Tests `browse_15_a_smart_place_opens_in_its_own_order`
(`track_list_sort` tests) and a core test in `queries/smart.rs` that the
member order survives when the view sort equals it.

**B4 — Sidebar labels in header case (GP-21).** Migration **v85**
(`SUPPORTED_SCHEMA_VERSION` 84 → 85, its own file like
`db_recently_added.rs`): rename `'Recently played'` → `'Recently Played'`
where `rules_json = '[{"field":"last_played_at","op":"not-null"}]'`, `'Top
rated'` → `'Top Rated'` where `rules_json` is the seeded rating rule,
`'Recently added'` → `'Recently Added'` where `role = 'recently_added'`; a
user's own list with one of those names and different rules keeps it.
`strings_sidebar.rs`: `SIDEBAR_IMPORT_ERRORS = "Import Errors"`,
`SIDEBAR_MISSING_FILES = "Missing Files"`, `SIDEBAR_RECENTLY_ADDED =
"Recently Added"`. GP-21's "sidebar row labels follow in a later change"
becomes "Sidebar row labels and the seeded smart lists are header case."
Tests: `gp_21_sidebar_labels_use_header_case` asserting every
`SIDEBAR_*` row label and the three seeded names on a fresh DB; a core
migration test (v84 fixture → v85 renames the seeded three, keeps a user
list named `Top rated` with other rules). Same commit: `po/reprise.pot`
regenerated with the gate's own xgettext line, `msgmerge
--no-fuzzy-matching` over all seven catalogues, de/es translated
(memory `gettext-gate-fails-on-the-first-locale-only`); every test fixture
that spells the old names updated.

**B5 — Recently Added wears no "add" glyph.** `sidebar_presentation.rs:52`
`NavIcon::RecentlyAdded` → `document-new-symbolic` (exists in the Adwaita
symbolic set; `list-add-symbolic` reads as an action). Test
`recently_added_wears_no_add_glyph`: the name differs from
`list-add-symbolic` and `IconTheme::for_display().has_icon` is true.

**B6 — QUE-2a (decision 7 of the mother plan).** Section J: append
**QUE-2a** [active] [gtk] — the context section's title is `Playing from
<place> · N tracks` (`queue_context_tail`), the manual section stays "Next
in Queue", two sections as before; mark QUE-2 `[replaced by QUE-2a]`.
`que_2_two_sections_headers_conditional` is re-hung as
`que_2a_two_sections_and_the_context_title_names_its_source` asserting the
title text. No string change.

**B7 — NAV-16 clean-up (rulebook only).** NAV-16 lists Releases and
Concerts as optional places, then says smart lists never offer the menu
while the sidebar builds both under the SMART heading
(`sidebar_rebuild.rs:294-327`). Add one sentence: "Releases and Concerts
sit in the SMART group but are module places and offer the menu; the rows
built from `smart_playlists` do not." Tests unchanged.

Verification inside B: `cargo test -p reprise-core podcasts db queries`,
`cargo test -p reprise-gnome` for the named tests plus preferences,
sidebar strings, up-next suites; `scripts/tests/gettext-catalogs.sh` rc
read directly; traceability.

