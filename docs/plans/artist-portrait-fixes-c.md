---
slug: artist-portrait-fixes-c
worktree: /home/marvin/Projects/reprise-artist-portrait-fixes-c
branch: feature/artist-portrait-fixes-c
phase: shipped
codex_session:
created: 2026-10-12
---
# Strand C: credit for Commons portraits (ART-4, ART-5)

Mother plan: `docs/plans/artist-portrait-fixes.md`, decisions 3 and 8.

**Start only after strands B and A have landed** (merge order `b, a, c`). Cut the worktree from
the `dev` that contains both.

**Owns:**
- `ArtistDetailHead*.kt`, `ArtistCover.kt` and `BrowseTabs.kt`, in the state A left them
- `LibrarySession.kt` and `AndroidLibrarySessionPort.kt`, for the new port method
- their tests under `android/app/src/test/java/io/github/marvinbaudach/reprise/`
- GNOME, under `crates/reprise-gnome/src/ui/`: `artist_portrait_tiles.rs`,
  `now_playing/artist_portrait_worker.rs`, `releases/releases_view.rs`,
  `stats/stats_band_card.rs`, the matching strings module, and the `po/` catalogs
- the ART-4 and ART-5 entries in `docs/ux-rules.md`

## Tasks

1. **Rules.** Append to `docs/ux-rules.md` after ART-3:

   > - **ART-4** [active] [android] — A portrait that comes from Wikimedia Commons carries a
   >   muted credit line beneath it on the phone's artist page — "Photo: <author> · <licence> ·
   >   Wikimedia Commons" — that opens the file page; it scrolls with the head (ART-3). A Deezer
   >   portrait shows no line, and small avatars never carry one.
   >
   > - **ART-5** [active] [gtk] — On the desktop, a Commons portrait shown large (the Releases
   >   column and the Stats artist card) carries the same credit as its tooltip and accessible
   >   description, and its context menu offers "Open image source", which opens the file page.
   >   A Deezer portrait has neither.

2. **Port.** Add `artistPortraitCredit(name)` to the `LibrarySession` port. It forwards to the
   FFI's `artist_portrait_credit` and is resolved off the main thread.

3. **Render.** Put the line inside the head item below the portrait. It is a link that opens the
   `file_page_url`. Every Kotlin file stays under 800 lines.

4. **Tests.** Both start from the artist list with a fake port:
   - `ART_4_aCommonsPortraitShowsItsCreditBeneathIt`
   - `ART_4_aDeezerPortraitShowsNoCredit`

5. **GNOME.**
   - The tiles read the core's `load_cached_credit` beside the path.
   - Set the tooltip and the accessible description, and add the context-menu item that opens
     the file page through `gtk::UriLauncher`.
   - If `artist_portrait_worker.rs` can get an MBID hint cheaply, it passes one. Otherwise it
     keeps `None`.
   - All strings are translatable, with the `po/` catalogs updated.
   - Tests: `art_5_` gtk tests for the tooltip, the description and the menu item on a Commons
     portrait, and their absence on a Deezer portrait.

6. **Gates:** the full AGENTS.md battery (`cargo fmt --check`, clippy with `-D warnings`,
   `cargo test --workspace`, `cargo audit`), the gettext checks, and:
   - `scripts/check-android-suite.sh`
   - `npm --prefix android run lint`
   - `npm --prefix android run test:lint`
   - `scripts/check-ux-traceability.sh`
