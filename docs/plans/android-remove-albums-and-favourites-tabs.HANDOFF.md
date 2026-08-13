# Handover — removing the Android Albums and Favourites tabs

Written 2026-08-13. Everything below is verified against artefacts unless it
says otherwise. Written for someone picking this up with no memory of the
session that produced it.

## Where this stands

| | |
| --- | --- |
| Plan | `docs/plans/android-remove-albums-and-favourites-tabs.md` (12 steps, 0–11) |
| Spec | `docs/superpowers/specs/2026-08-12-android-remove-albums-and-favourites-tabs-design.md` |
| Worktree | `/home/marvin/Projects/reprise-android-remove-albums-and-favourites-tabs` |
| Branch | `feature/android-remove-albums-and-favourites-tabs` |
| Phase | `refactored` — plan → code → check → refactor are all done |
| Size | 36 files, +1765/−839 against `origin/dev` |
| **Not done** | rebase, PR, merge |

The branch is **3 commits behind `origin/dev`** and must be refreshed and
re-gated before it is merged. The recorded baseline (below) belongs to the old
branch point and does not carry over.

## What changed

The Android app went from five browse tabs (Titles, Artists, Albums,
Favourites, Queue) to three: **Titles, Artists, Queue**.

Albums are now reached through an artist: artist list → artist page → album
page. The artist page shows the artist's albums (newest year first) and, below
them, an "Other titles" section for tracks with no album tag; that section is
hidden when empty. The album page is the one that already existed — it moved
under the artist rather than being rebuilt, and kept its play button and track
list. The Artists tab search returns album hits and artist hits as two sections,
so an album is still findable by name without an Albums tab.

The favourite **marking** on tracks was deliberately kept: the heart in track
rows, the dock player, the Now Playing scene and sheet, plus `RatingWriter` and
`set_track_rating` are untouched. Only the *list* of favourites is gone.

New on the Rust side: `query_artist_albums` and a query for an artist's untagged
tracks, both windowed, exposed through `list_artist_albums` and
`list_artist_untagged_tracks`. Removed: `list_albums`, `list_favourites`,
`search_favourites`.

## Decisions already taken — do not reopen without a reason

These were settled with the user during brainstorming and a plan grilling. They
are recorded so a later session does not spend its budget re-deciding them.

1. **Removal is outright**, not a hidden flag.
2. **Album search survives**, folded into the Artists tab search. Losing it was
   explicitly rejected.
3. **The favourite feature stays**; only the tab goes.
4. **Tracks with no album** get their own section rather than being hidden or
   folded into a fake "Unknown album".
5. **Artist → albums uses a new exact-match query**, not the existing fuzzy
   album filter (`LIKE` would let "Bad" pull in "Bad Religion").
6. **The two artist→albums queries stay separate**, but share their selection
   rule through one SQL constant so they cannot drift. Unifying them outright is
   a **follow-up task**, deliberately not done here — see Open threads.
7. **Album order on the artist page: newest year first**, matching the desktop.
8. **The artist-level play button is a regression**, ruled so by the user, and
   was restored during the refactor.

## What is proven, and how

Do not take these on trust from a summary — they were checked against files:

- **Baseline exists** at `.tmp/baseline/baseline.md` in the worktree, taken at
  the real merge-base before any edit. It records 24 already-red Rust tests by
  name (all `cover` / `cover_download` / `remote_image`) plus Android 316
  tests / 63 suites.
- **Rust gate after the refactor**: 2358 passed, 24 failed — the failing names
  compared against the baseline with `comm`, zero added or removed. **Those 24
  are not this branch's doing and are not to be "fixed" here.**
- **Android suite**: 66 `TEST-*.xml` files, **333 tests, 0 failures, 0 errors**.
  Gradle reports `BUILD SUCCESSFUL` for a task that ran nothing, so the XML count
  is the evidence, not the build status.
- **FFI tests** were run with `TMPDIR=/tmp`, verifiable in the logged commands.
  Any other TMPDIR manufactures four false failures (track ids come from scan
  order, which follows readdir order).
- **Favourite feature intact**: `FavouriteHeart.kt`, `RatingWriter.kt`,
  `NowPlayingScene.kt`, `NowPlayingSheet.kt`, `DockMode.kt` and
  `set_track_rating` have **zero diff lines** against `origin/dev`.
- **No orphaned references** to the removed tabs in active code. The one
  surviving mention is in `docs/research/android-spike-2026-08.md`, a historical
  record that was correctly left alone.

## What is NOT proven

- **No visual verification.** Nothing was run on a device or emulator; the GTK
  display harness stalled and was recorded as unverified. Nobody has *seen* the
  new artist page. Screenshots or a device run would be the honest next check.
- **No line-by-line review of every Kotlin file.** Four reviewers covered Rust,
  security, the Compose surface and test quality, but that is review, not proof.
- **Process death** does not restore the open artist/album. There is no
  `SavedStateHandle` anywhere in `MobileSurfaceViewModel` or `MainActivity`, so
  nothing survives it by construction. This is pre-existing, not caused here —
  but it is a product question nobody has answered.

## Open threads

**Carried deliberately, with the user's agreement:**

- Four coverage gaps were declined this round: untagged tracks at activity
  level, paging either artist-page list, the "artist with neither albums nor
  untagged tracks" branch, and tap-to-play from an artist album. All are real
  gaps; they were judged not worth this round's budget.
- Nit findings left alone: unused `LibraryWindowRemoval`, the unreachable
  `openAlbumOrigin` restore guard, and a `search()` / tab-load asymmetry at
  `BrowseScreen.kt:316-321` (currently invisible, but the two places must agree
  if either changes).

**Noticed, not yet decided:**

- `query_artist_detail_albums` (the desktop's version) has **no production
  caller** on `origin/dev`. It is covered by one unit test and nothing else. That
  makes the "keep the desktop bit-identical" constraint cheaper than it looked —
  and raises whether the two artist→albums queries should simply be unified, or
  the desktop one deleted. Needs a decision; both projections differ (`MAX` vs
  `MIN` year), so it is not a pure rename.
- The T1 test is named
  `libraryHeartWritesFiveThenZeroAndBothSurviveAFreshActivityRead`. It proves the
  5-then-0 write for track 1 in full, but the *survives-a-fresh-read* half is now
  demonstrated on track 2, because an optimistic ViewModel cache
  (`confirmedRatings`, surviving `recreate()`) makes it unprovable on track 1.
  Sound, but the name overpromises. Renaming it would be honest.
- `ArtistTrackList.playbackSelection` was removed even though it sat on the
  refactor's do-not-touch list — it read the field that finding U3 required
  deleting, so it could not compile otherwise. Unavoidable, but worth knowing it
  went.

## Next steps

1. `git -C <worktree> rebase origin/dev` — 3 commits behind.
2. **Re-take the baseline** after the rebase and re-run both gates. The old
   baseline belongs to the old branch point. Red tests that match the *new*
   baseline are not yours.
3. Consider a device or emulator run to actually look at the artist page — the
   biggest unverified thing here.
4. Open the PR.

## Traps this session paid for

Recorded so the next one does not pay again:

- **Do not read the main checkout for this work.** `dev-local` was 239 commits
  behind `origin/dev`; two search sweeps against it reported that neither tab
  existed at all. Use `git show origin/dev:<path>` and `git grep <pat> origin/dev`.
- **`sed '1,/^---$/d'` twice does not strip YAML front matter** — the second one
  cuts to the first horizontal rule *inside* the document. It silently ate the
  plan's entire preamble (verification commands, the TMPDIR rule, the JDK 21
  requirement) out of the file handed to Codex. Use an awk state machine and
  count the lines afterwards.
- **The scratchpad is not durable.** An hourly tmpfs reaper deleted the accepted
  findings file within the half hour. Artefacts a later step depends on belong in
  the worktree.
- **The load governor matches on command text**, so a command merely *containing*
  a heavy entry point's name is blocked. Put the run in a script file and start it
  through `heavy-run`.
