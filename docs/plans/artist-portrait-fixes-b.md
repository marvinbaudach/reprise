---
slug: artist-portrait-fixes-b
worktree: /home/marvin/Projects/reprise-artist-portrait-fixes-b
branch: feature/artist-portrait-fixes-b
phase: shipped
codex_session:
created: 2026-10-12
---
# Strand B: no release covers, Commons fallback, credit in the cache (NET-8, NET-9)

Mother plan: `docs/plans/artist-portrait-fixes.md`, decisions 1, 2, 4, 5, 6 and 8.

**Owns:**
- `crates/reprise-core/src/artist_portrait/**`, including new sibling modules
- `crates/reprise-core/src/net/rate.rs`, for a new Wikimedia key
- `crates/reprise-android-ffi/src/artist_portrait.rs`, additive changes only
- the NET-8 and NET-9 entries in `docs/ux-rules.md`

Do not touch Kotlin or `reprise-gnome`: strand C consumes what this strand exposes.

## Facts

- `load_or_fetch_with(name, now, dir, search, download)` (`mod.rs`) is name-only and has no DB
  handle.
- Callers:
  - GNOME `artist_portrait_worker.rs:39`, via `load_or_fetch`, with no DB.
  - Android `artist_portrait_fetch` (`reprise-android-ffi/src/artist_portrait.rs:115`), which has
    a DB reader.
  - `backfill.rs`, which opens its own `Db`.
- Artist MBIDs:
  - From tags: `tracks.artist_mbid`. `artist_news_candidates::artists_for_fetch` reads
    `MAX(artist_mbid)` per artist name.
  - From name search: `artist_news_parsing::{artist_search_url, parse_artist_mbid}`.
- `musicbrainz::get` is rate-limited to 1 request per second and sends the project User-Agent.
- `AgentPolicy` has no host allowlist. The Flatpak manifest grants the network.
- `NET-1a`'s gate (`online_sources::network_allowed*`) already sits in front of every portrait
  fetch. Every new request goes through the same gate path.

## Tasks

1. **Rules.** Append to `docs/ux-rules.md` after NET-3d:

   > - **NET-8** [active] [core] — An artist portrait that is one of the artist's own release
   >   covers counts as no portrait. The fetch compares the Deezer picture's identifier with the
   >   `md5_image` of the artist's Deezer releases; on a match it drops any stale portrait for
   >   that artist and moves on to the next source. When the comparison cannot be made
   >   (transport failure or a Deezer error object), nothing is cached and the fetch is retried
   >   later.
   >
   > - **NET-9** [active] [core] — Without a usable Deezer photo, the portrait comes from
   >   Wikimedia Commons: the artist's MusicBrainz id (from tags, else an exact-name MusicBrainz
   >   search) leads through its Wikidata relation to the P18 image. The portrait is stored
   >   with its source, author, licence, licence URL and file-page URL; a cached image without
   >   that record counts as stale. A miss is recorded only when every source has answered
   >   without a photo.

2. **Failing tests first.** No test touches the network: every fetcher stays injected. Test
   names start with `net_8_` or `net_9_`.
   - `net_8_`:
     - Will Ramos fixture: a trimmed search response plus an albums response containing "Rain"
       rejects the Deezer picture.
     - A real photo whose identifier matches no cover is stored with `source: deezer` and no
       credit.
     - A transport error on the albums request leaves the cache untouched and returns the stale
       portrait or the error.
     - A Deezer `{"error":{"code":4,…}}` on the albums request behaves like a transport error
       and is never read as "no covers".
     - A rejection deletes a stale portrait file.
   - `net_9_`:
     - An MBID from the hint goes to MusicBrainz url-rels, then Wikidata P18, then Commons
       imageinfo. The download stores the image plus a sidecar with the credit.
     - Without a hint, the MBID comes from the exact-name MusicBrainz search.
     - No Wikidata relation, or no P18, writes the negative marker only after Deezer also had no
       photo.
     - A transport error anywhere in the chain is not cached and falls back to the stale
       portrait.
     - A cached image without a sidecar counts as stale.
     - The credit's author field is stripped of HTML. Commons `extmetadata.Artist` is HTML.

3. **`release_cover.rs`** (new):
   - `DeezerArtist` gains `id`.
   - `albums_url(id)` with a named page bound of 1000. Name the constant and say why the bound
     exists.
   - `parse_release_cover_identifiers(json) -> Option<HashSet<String>>` returns `None` without a
     `data` array.
   - `is_release_cover(picture_url, &covers)` is built on `deezer::image_identifier`.
   - Respect `MAX_SEARCH_RESPONSE_BYTES`, or give the albums response its own named bound.

4. **`wikimedia.rs`** (new). Each step is a URL builder plus a pure parser:
   1. MusicBrainz `ws/2/artist/<mbid>?inc=url-rels&fmt=json`, through `musicbrainz::get`, yields
      the Wikidata QID.
   2. `www.wikidata.org/w/api.php?action=wbgetclaims&entity=<QID>&property=P18&format=json`
      yields the file name.
   3. `commons.wikimedia.org/w/api.php?action=query&titles=File:<name>&prop=imageinfo&iiprop=url|extmetadata&iiurlwidth=1000&format=json`
      yields the thumbnail URL plus the credit: `Artist` stripped of HTML, `LicenseShortName`,
      `LicenseUrl` and `descriptionurl`.

   Use a new `RateLimitKey::Wikimedia` in `net/rate.rs` with a polite spacing (for example
   500 ms) and the project User-Agent, which Wikimedia requires. Only `https`. Only accept
   downloads from `upload.wikimedia.org`.

5. **`sidecar.rs`** (new) plus `cache.rs`:
   - `PortraitRecord { source: Deezer | Commons, credit: Option<PortraitCredit { author, license, license_url, file_page_url }> }`
     is stored as `<key>.json` and written together with the image under `CACHE_WRITE_LOCK`.
   - A verdict for an image without a readable sidecar is `NeedsFetch`.
   - Add `cache::remove_portrait`.
   - Export `load_cached_credit(name) -> Option<PortraitCredit>` and its `_from(dir)` twin.

6. **`mod.rs` orchestration.**
   - New signature: `load_or_fetch_in(name, mbid_hint: Option<&str>, dir)`.
     - Android FFI and `backfill.rs` pass `MAX(tracks.artist_mbid)` for the name.
     - GNOME's `load_or_fetch(name)` passes `None` and so falls back to the name search. Its
       call site does not change here; strand C may wire a hint.
   - Order:
     1. Deezer search.
     2. Cover check (NET-8).
     3. Placeholder check, unchanged.
     4. Commons chain (NET-9).
     5. Negative marker.
   - Keep the existing placeholder behaviour, which preserves a stale portrait, untouched.
   - Every file stays under 800 lines.

7. **FFI** (additive). Add `artist_portrait_credit(name) -> Option<AndroidPortraitCredit>`, a
   UniFFI record. The existing exports keep their shape. The bindings regenerate through
   `scripts/check-android-suite.sh`.

8. **Backfill.** Every artist now costs up to five more requests, each at its own provider
   spacing. Keep the backfill tests green, and do not change any spacing.

9. **Gates:**
   - `cargo fmt --check`
   - `cargo clippy --all-targets --workspace -- -D warnings`
   - `cargo test --workspace`
   - `cargo audit`: only RUSTSEC-2024-0436 is allowed
   - `scripts/check-ux-traceability.sh`
   - `scripts/check-architecture.sh`
   - the core purity proof
   - `scripts/check-android-suite.sh`, because the FFI changed
