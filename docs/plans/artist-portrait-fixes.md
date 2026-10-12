---
slug: artist-portrait-fixes
worktree:
branch:
phase: planned
codex_session:
created: 2026-10-12
strands: a,b,c
merge_order: b,a,c
---
# Artist portraits: scroll whole, never a release cover, real photos with credit

Two defects found from Android screenshots on 2026-10-12, plus the feature the grill added on
top of them.

## Problems

1. **Android artist page.** In `BrowseTabs.kt`, the title row and the Play button are pinned
   above `ArtistDetailSections`. The portrait is the first item of the `LazyColumn`, so on
   scroll it is clipped at the list's top edge, directly under the Play button. Nothing marks
   that edge: the fixed `ArtistPortraitShimmer` runs continuously across it. The shimmer also
   stays put after the portrait has left.
2. **Release cover as portrait.** Deezer is the only source (`artist_portrait/{mod,deezer}.rs`).
   For artists without a photo, Deezer serves one of the artist's own release covers as the
   artist picture. Measured 2026-10-12:
   - For Will Ramos (Deezer 12773623), the identifier in `picture_xl`
     (`6c36703da072ad07d31351b20208acf8`) equals the `md5_image` of his album "Rain" in
     `/artist/12773623/albums`.
   - The control arm has zero matches: Pharrell Williams (103) and Hayley Williams (281737).
   - Deezer rotates which cover it uses. The phone's cached portrait shows a different cover.

   GNOME shares the core path.

## Decisions (settled in the grill, 2026-10-12)

| # | Decision |
|---|---|
| 1 | Reject release covers **and** add a second source for real photos in this plan. |
| 2 | Source order: a Deezer photo that passes the cover check wins. Otherwise use MBID → MusicBrainz url-rels → Wikidata P18 → Commons. |
| 3 | Commons credit is a muted line under the portrait on the Android artist page, linking to the Commons file page. On GNOME, which has no artist page, the credit is a tooltip plus accessible description on large portraits (the Releases column and the Stats artist card), plus an "Open image source" context-menu item. Small avatars carry no credit. Both frontends use the Commons chain (user, 2026-10-12: "mach das gnome zeug auch"). |
| 4 | Detection uses Deezer's documented `/artist/{id}/albums`, comparing `md5_image` with the identifier of the picture URL. The undocumented CDN probe is rejected. |
| 5 | If the cover check cannot be made (transport error, or Deezer's `{"error":…}` object, which arrives with HTTP 200), nothing is cached. The stale portrait is kept if one exists, and the fetch is retried later. |
| 6 | A rejected release cover deletes the artist's stale portrait and falls through to the Commons chain. The cache gains a sidecar `<key>.json` holding the source and the credit. An image without a sidecar counts as stale. No migration: AGENTS.md, "Not released yet". |
| 7 | Android layout: an overlaid pinned title row, transparent at rest. It takes `surfaceContainer` once content passes under it. The Play button, the portrait and (strand C) the credit form one head item that scrolls away. The shimmer follows the head and fades. |
| 8 | New rules, `[active]` in their implementing commit, each with a rule-named test: ART-3 `[android]`, NET-8 `[core]`, NET-9 `[core]`, ART-4 `[android]`, ART-5 `[gtk]`. One level tag per rule. |
| 9 | Optional post-merge step, only with the user's consent at that moment: clear `artist-portraits/` on the desktop and on the phone. |

## Parallelität

There are three strands. A and B run in parallel. C needs both and starts as a second code run
after A and B have landed.

| Strand | File | Owns | Rules |
|---|---|---|---|
| A: Android head | `artist-portrait-fixes-a.md` | `android/app/src/main/java/io/github/marvinbaudach/reprise/{BrowseTabs,ArtistCover,ArtistPortraitShimmer}.kt`, new `ArtistDetailHead*.kt` siblings, artist-page tests under `android/app/src/test/java/io/github/marvinbaudach/reprise/` | ART-3 |
| B: Core sources | `artist-portrait-fixes-b.md` | `crates/reprise-core/src/artist_portrait/**`, `crates/reprise-core/src/net/rate.rs`, `crates/reprise-android-ffi/src/artist_portrait.rs` (additive) | NET-8, NET-9 |
| C: Credit UI | `artist-portrait-fixes-c.md` | the Android head files (after A), `LibrarySession.kt`, `AndroidLibrarySessionPort.kt`, their tests; `crates/reprise-gnome/src/ui/{artist_portrait_tiles.rs,now_playing/artist_portrait_worker.rs,releases/releases_view.rs,stats/stats_band_card.rs}`, the GNOME strings module and `po/` | ART-4, ART-5 |

**Shared file.** `docs/ux-rules.md` is touched by every strand, in sections that sit far apart:

- A appends ART-3 after ART-2, around line 855.
- B appends NET-8 and NET-9 after NET-3d, around line 3860.
- C appends ART-4 and ART-5 after ART-3, only once A has landed.

The disjointness check in the code phase will list `docs/ux-rules.md` for A and B. That single
path is the declared, expected overlap. Any other overlap stops the run.

**Merge order:** `b, a, c`.

- B goes first because it changes the data both frontends read.
- A rebases trivially over B.
- C is cut from the `dev` that contains both.

**Post-merge cross-checks.** These read across strands, so no strand runs them on its own:

1. On merged `dev`, `scripts/check-ux-traceability.sh` shows ART-3, ART-4, ART-5, NET-8 and
   NET-9 all active and covered.
2. On the phone, after decision 9 or TTL expiry:
   - Will Ramos shows either a Commons photo with a credit line or the generated avatar, never a
     release cover.
   - Scrolling the artist page cuts nothing at an undrawn edge, and the title row takes its
     colour.
3. An artist with a real Deezer photo still shows it, with no credit line.
4. On GNOME, a Commons portrait in Releases or Stats shows the credit as its tooltip, the menu
   item opens the file page, and a release cover no longer appears.
