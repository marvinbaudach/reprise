# iTunes-Era Feature Study for Reprise

**Date:** 2026-07-12
**Status:** Research deliverable — not part of the design spec. Nothing here is
committed; it is a menu of candidates for the user to accept, defer, or reject.
**Scope:** Classic local-library iTunes / Apple Music for Mac & Windows (the
"digital jukebox" era, roughly iTunes 7–12), cross-referenced with what
serious Linux/cross-platform library players offer today: Rhythmbox,
Clementine, Strawberry, Quod Libet, Elisa, and MusicBee (Windows, widely run
under Wine by Linux curators who haven't found a native replacement).
Explicitly **out of scope**: the iTunes Store, Apple ID/DRM, Apple Music
streaming-catalog features, and anything requiring an Apple cloud account.

**What "already planned" means below:** cross-referenced against
`docs/superpowers/specs/2026-07-11-reprise-design.md` (the approved design
doc) and the current source tree (`src/models.rs`, `src/queries.rs`,
`src/library/scanner.rs`, etc.) as of this writing (stage 3 of the MVP,
per `README.md`).

---

## 1. How to read the inventory

Each row is scored against Reprise's own stated audience and principles, not
against "what iTunes did." The audience is a **curator**: someone with an
existing, well-tagged local library who wants fast browsing, accurate
listening statistics, and playlists that reflect years of accumulated
metadata — not a streaming user, not someone who wants Reprise to touch their
files automatically. Two Reprise principles recur constantly in the fit
column and are worth stating once:

- **"Never touch files unprompted"** — Reprise moves/renames/writes only on
  explicit user action (tag editor, trash-delete). Any iTunes feature that
  silently reorganizes or duplicates files on disk fails this test regardless
  of how popular it was in iTunes.
- **No telemetry** — Reprise phones home for nothing by default; even the
  post-MVP network modules (scrobbling, Radar) are opt-in and disclosed.
  Features that depend on a vendor's central data-collection graph (e.g. the
  real mechanism behind Genius) don't transplant cleanly to a project with
  this stance, even if a *local-only* analog of the same idea is fine.

Effort is relative to Reprise's current ~8,600-line Rust codebase, not
absolute engineering months.

---

## 2. Feature inventory

### 2.1 Browsing & views

| Feature | What it is | Value | Already planned? | Fit | Effort | Recommended stage |
|---|---|---|---|---|---|---|
| Column Browser (Genre→Artist→Album cascade) | Up to 5 cascading filter columns above the track list; selecting a value narrows the next column and the list ([Apple Support](https://support.apple.com/guide/itunes/find-a-song-with-the-column-browser-itns72c6bc8b/windows), [Macworld](https://www.macworld.com/article/209476/itunes_column_browser.html)) | High | **Yes** — MVP "Browse-Leiste" (Genre/Artist/Album, design doc lines 109, 315-317) | Clean fit, read-only filter | — | Already MVP |
| Songs/Albums/Artists/Genres grid views | Alternate top-level views beyond the flat song table; Albums view is a cover-art grid | High | **Partially** — Album grid is in "Spätere Ausbaustufen" (design doc line 626); Artists/Genres grid views are not mentioned | Clean fit | M | Post-MVP module (album grid already slated; artist grid is new) |
| Get Info / Detail panel per artist | Discography, bio, tour dates | Med | **Yes** — Artist/Album-Info panel is a full later-stage spec (design doc lines 524-554) | Clean fit (uses opt-in MetadataProvider) | — | Already later-stage |

### 2.2 Playlists & queue

| Feature | What it is | Value | Already planned? | Fit | Effort | Recommended stage |
|---|---|---|---|---|---|---|
| Smart Playlists (rule engine) | Multi-condition rules (field/operator/value), match-all/match-any, live update ([Apple Support](https://support.apple.com/guide/itunes/create-delete-and-use-smart-playlists-itns3001/windows)) | High | **Yes, engine only** — `smart_playlists(rules_json, ...)` exists in schema (design doc lines 244-253), only 3 predefined playlists ship in MVP | Clean fit | — (engine done) | See §3 — the **UI** is the real gap |
| Smart Playlist **rule editor UI** | The dialog to build/edit arbitrary rules, not just the 3 canned ones, incl. nested groups | High | **No** — explicitly deferred twice (design doc line 141 "bewusst nicht im MVP", line 637 "spätere Ausbaustufe") | Clean fit, the engine already speaks this language | M | See §3 — recommend pulling forward |
| "Up Next" / play queue with reordering | Apple Music's persistent up-next list, distinguishable from a saved playlist | High | **Yes** — Reprise's queue (shuffle/repeat, DnD reorder) already covers this; MPRIS position/seek shipped per recent commit | Already implemented | — | Done |
| Party Shuffle / Genius Shuffle / Genius Mixes | Auto-generated shuffle mix from a seed track or "genius" clustering; classic Genius relied on Apple's central library-similarity graph ([Apple Support](https://support.apple.com/guide/itunes/use-itunes-genius-itns22073/windows)) | Med | No | **Partial conflict** — a literal Genius clone needs a vendor telemetry graph (anonymized library data sent to Apple) Reprise's no-telemetry stance rules out; a **local-only** version (seed track → similar tracks by tags/genre/rating/ReplayGain-adjacent loudness, all in-process) is compatible | M | Later, local-only variant; decline the telemetry-based design | 
| MusicBee-style Auto-DJ (seed + rules, favors high-rated tracks) | Continuous auto-queue biased toward ratings/recency, configurable ("MusicBee Auto DJ") | Med | No | Clean fit if purely local — this is really "Genius done locally," same recommendation as above | M | Same later slot as Genius-analog above (avoid building both) |

### 2.3 Statistics & metadata

| Feature | What it is | Value | Already planned? | Fit | Effort | Recommended stage |
|---|---|---|---|---|---|---|
| Play Count + Last Played + Date Added | Core listening stats | High | **Yes** — in schema (design doc lines 236-241), threshold >50% listened | Clean fit | — | Done |
| **Skip Count / Last Skipped** | Increment when a track is skipped before completion — iTunes exposed this as its own column ([Apple Community](https://discussions.apple.com/thread/1567710)) | High | **No** — schema has no `skip_count`/`last_skipped_at` field; grep confirms no mention anywhere in `src/` | Clean fit — same >50%-threshold plumbing already exists, this is its mirror image | S | See §3 — flagged gap |
| Rating (1–5 stars) | Explicit star rating | High | **Yes** — implemented (`rating 0–5`) | Clean fit | — | Done |
| Rating vs. binary Love/Heart | Apple Music later added a heart "Love" alongside stars; a lighter-weight one-tap signal | Low–Med | No | Fine fit but redundant with 5-star + smart playlists ("rating ≥ 4" already expresses "loved") | S | Decline — no clear value add over existing stars; revisit only if user feedback asks for a single-tap "like" |
| Compilations / Album Artist correctness (VA albums) | `album_artist = "Various Artists"` + compilation flag so a variety album doesn't scatter across every performer's artist bucket ([Apple Community](https://discussions.apple.com/thread/7579565), [bliss](https://www.blisshq.com/music-library-management-blog/2011/03/26/five-ways-organize-various-artist-compilations/)) | High | **Half-planned** — `album_artist` column exists in schema and scanner (`src/library/scanner.rs`, `src/models.rs`) but there is no compilation-aware grouping/browsing logic or UI treatment described anywhere in the spec | Clean fit, pure metadata/grouping logic, no file writes needed | S–M | See §3 — flagged gap, a real library-quality issue Reprise's schema is *this close* to already solving |
| Classical Grouping/Work/Movement/Composer | Extra tags (`Work`, `Movement`, `Movement Name`, `Grouping`, `Composer`) so cascading movements of one work stay together and shuffle doesn't fragment a symphony ([Kirkville](https://kirkville.com/apple-is-finally-making-itunes-better-for-classical-music/), [MusicBrainz Picard docs](https://picard-docs.musicbrainz.org/en/variables/variables_classical.html)) | Med (high for the classical-listening subset) | **No** — no `composer`/`work`/`movement`/`grouping` field anywhere in `tracks` schema or lofty-reading code | Clean fit — read-only metadata display + a "disable shuffle within work" grouping rule; no file writes required beyond what tag editor already supports | M | See §3 — flagged gap |
| Multiple libraries (switch on launch) | iTunes let you hold Option/Shift at launch to pick among separate `.itl` library files ([Apple Support](https://support.apple.com/guide/itunes/use-multiple-itunes-libraries-itns3259/windows)) | Low | No | Marginal fit — Reprise's one-process/one-SQLite-DB model assumes a single library; multi-library adds real complexity (settings scoping, MPRIS identity, watcher config) for a niche need (most curators run one library) | L | Decline — YAGNI; revisit only if concrete multi-household/multi-device demand appears |
| Get Info batch tag edit | Multi-select → edit shared fields (Artist, Album, Album Artist, Grouping, Genre, Year) in one dialog; per-track fields (title, track #) still edited individually ([Apple Community](https://discussions.apple.com/thread/2066255)) | High | **Yes, mentioned as multi-select behavior** — design doc line 328-330 "bei Mehrfachauswahl werden gemeinsame Felder … gesammelt bearbeitet" (tag editor) | Clean fit, matches the "explicit user action" write principle exactly | — (already spec'd) | Already MVP tag editor scope |
| Duplicates finder | Detect same recording present multiple times (same fingerprint/tags, different files) — Strawberry has `--check-duplicates`/`--fuzzy` CLI flags and SQL-query docs; Clementine added "remove duplicates from playlist" ([LWN on Strawberry](https://lwn.net/Articles/1069368/), [Clementine](https://x.com/clementine_app/status/221911871450648576)) | High | **No** — not mentioned in spec; Reprise already has the adjacent move-detection fingerprint logic (title+artist+album+duration±2s+size) that a duplicates finder would reuse almost verbatim | Clean fit — a *report*, no automatic file deletion; user picks what (if anything) to trash | S–M (reuses existing fingerprint code) | See §3 — flagged gap, cheap because the hard part (fingerprinting) is already built for move-detection |
| Missing-file locator | Detect and let user re-point/remove tracks whose file vanished | High | **Yes** — `missing` flag + sidebar "Fehlende Dateien" source (design doc lines 240, 306-311) | Done | — | Done |

### 2.4 Audio

| Feature | What it is | Value | Already planned? | Fit | Effort | Recommended stage |
|---|---|---|---|---|---|---|
| Sound Check (loudness normalization) | Per-track/album volume analysis and adjustment ([iLounge](https://www.ilounge.com/index.php/articles/comments/sound-check-and-crossfade)) | High | **Yes** — ReplayGain via `rgvolume`, MVP scope (design doc lines 83-84) | Clean fit, ReplayGain is the open, tag-based superset of Sound Check | — | Done |
| Gapless playback | No silence/click between consecutive tracks of a continuous work | High | **Yes** — `playbin3` gapless is called out explicitly (design doc line 181) | Clean fit | — | Done |
| Crossfade | Overlapping fade between unrelated tracks, 1–12s ([iLounge](https://www.ilounge.com/index.php/articles/comments/sound-check-and-crossfade)) | Med | **Yes** — explicitly deferred as a later module (design doc lines 144, 621) | Clean fit | M | Already later-stage, no change recommended |
| 10-band Equalizer with presets | Global EQ, savable presets | High | **Yes** — MVP scope, GStreamer `equalizer-10bands` (design doc line 81) | Clean fit | — | Done |
| **Per-song/per-album EQ preset override** | Assign a specific EQ preset to an individual track/album that overrides the global setting, stored with the track ([AddictiveTips](https://www.addictivetips.com/windows-tips/specify-equalizer-settings-per-song-in-itunes/)) | Low–Med | No | Clean fit technically (one more `settings`-style column + pipeline lookup) but low real-world value — most curators set EQ once per listening context (headphones/speakers), not per song; adds a UI surface (per-row menu) for a rarely-used knob | S–M | Decline for MVP-adjacent work; low priority backlog item if requested |
| Visualizer | Audio-reactive animated graphics during playback | Low | No | Weak fit — GNOME HIG discourages decorative full-window visual noise; no curator workflow value; competes for engineering time against the browsing/stats features that define the audience | M | Decline — doesn't serve the stated curator audience, HIG friction |

### 2.5 File & library management

| Feature | What it is | Value | Already planned? | Fit | Effort | Recommended stage |
|---|---|---|---|---|---|---|
| "Keep Media folder organized" (auto move/rename on tag change) | iTunes silently renames/moves files on disk to match `Artist/Album/Track` when tags change ([Apple Community](https://discussions.apple.com/thread/255201417)) | — | No | **Direct conflict** — violates "never touch files unprompted" by design; this is the single clearest iTunes-era feature Reprise's philosophy already rejects | — | **Decline outright** — philosophical non-fit, already implicit in the spec's file-safety principle |
| "Copy files to Media folder on import" | Auto-duplicates any file added from outside the library folder into a managed tree | — | No | Same conflict as above — moves/copies files without an explicit user action beyond "add to library" | — | **Decline outright**, same reasoning |
| CD import/rip, CD burn | Optical media ripping/burning | — | No | Explicitly out — declared non-goal (design doc line 143) | — | Confirmed decline, no change |
| Home Sharing (LAN library streaming to other iTunes instances / Apple TV) | Browse and stream from another iTunes library over the local network, no cloud involved ([Apple Support](https://support.apple.com/guide/itunes/set-up-the-itunes-remote-app-itnsa1c27e74/windows)) | Med | **Partially adjacent** — the planned companion-app "Remote-Steuerung" protocol (design doc lines 562-567) covers *controlling* Reprise remotely, but not *browsing/streaming another Reprise library's catalog* from a second desktop instance | Clean fit — same LAN-only, paired, no-cloud posture already chosen for the companion app; DAAP is Rhythmbox's classic implementation of this exact idea | M–L | See §3 — flagged gap, natural extension of the already-planned remote protocol |
| Remote app (iOS remote control) | Phone app for transport control, browsing, Genius, volume | High (for the audience that wants it) | **Yes** — full companion-app spec exists (design doc lines 558-594) | Clean fit, LAN-only opt-in matches no-telemetry stance | — | Already later-stage |
| Device sync (MTP/USB, WLAN) | Push playlists/ratings to a phone or DAP | High | **Yes** — full spec (design doc lines 569-584) | Clean fit | — | Already later-stage |

---

## 3. Cross-reference notes: what Linux/cross-platform players actually validate

The iTunes inventory above is one input; the second is what curator-grade
Linux/cross-platform players still bother to ship today, which is a better
signal of durable value than "iTunes had it once":

- **Rhythmbox** still ships DAAP sharing (`libdmapsharing`) as an active,
  maintained plugin as of its late-2025 releases
  ([flathub issue](https://github.com/flathub/org.gnome.Rhythmbox3/issues/1),
  [GNOME wiki](https://wiki.gnome.org/Apps(2f)Rhythmbox(2f)Plugins.html)) —
  direct precedent for LAN library sharing being worth maintaining even in
  2025-2026, reinforcing the Home-Sharing-analog gap above.
- **Strawberry** treats duplicate detection as a first-class feature with
  dedicated CLI flags (`--check-duplicates`, `--fuzzy`) and documents SQL
  queries for finding/removing library duplicates
  ([LWN](https://lwn.net/Articles/1069368/)) — strong validation that this is
  considered core "collection hygiene," not a nice-to-have. Clementine's
  users asked for the same thing ([GitHub issues](https://github.com/clementine-player/Clementine/issues/5615),
  [Clementine](https://x.com/clementine_app/status/221911871450648576)).
- **Quod Libet** exposes skip count as a queryable/displayable field
  (`~#skipcount`) alongside play count and rating
  ([Quod Libet docs](https://quodlibet.readthedocs.io/en/latest/guide/stats_rating.html)) —
  independent confirmation that skip count belongs in the same statistics
  family Reprise already tracks, not an Apple-specific quirk.
- **MusicBee** stores ratings as file tags (not a sidecar DB) and offers
  Auto-DJ as a rules-based, favor-high-rated auto-queue
  ([Slant comparison](https://www.slant.co/versus/1424/7243/~musicbee_vs_quod-libet)) —
  the rating-in-tags approach is explicitly what Reprise's spec *rejects*
  (design doc line 138-139: ratings stay DB-only, tag editor never writes
  them), a deliberate, already-made decision worth reaffirming rather than
  reconsidering. Auto-DJ again validates the "local Genius-analog" as a
  recurring, independently-invented idea across at least three players
  (iTunes Genius, MusicBee Auto-DJ, and Rhythmbox's own "Song Playback
  Order"-adjacent shuffle plugins).

---

## 4. Flagged conflicts (declined, with reasons)

| Feature | Why it's declined |
|---|---|
| "Keep organized" / auto-move-on-tag-edit | Direct violation of "never touch files unprompted." |
| "Copy files to library on import" | Same file-safety violation — silently duplicates files onto disk without a distinct user action. |
| Genius (as originally implemented) | Depends on Apple's central, cross-user library-similarity graph — architecturally a telemetry feature. A **local-only** re-implementation (seed track → tag/genre/rating-based similarity, computed entirely on-device) is fine and is folded into the recommendation below as "local Auto-DJ," not "Genius." |
| Visualizer | No workflow value for a curator audience; adds decorative surface that fights GNOME HIG's restraint and Reprise's "flat, functional" design direction. |
| Multiple libraries | Adds real architectural complexity (per-library settings/DB/MPRIS identity) for a niche need; YAGNI given the single-library assumption baked into the current schema and window model. |
| Rating-in-file-tags (MusicBee-style) | Already a considered, correct decision in the current spec (ratings DB-only) — flagging here only to confirm the study doesn't recommend reopening it. |

---

## 5. Recommended additions to the roadmap

Presented as candidates for approval, not commitments. Ordered roughly by
value-for-effort.

- **Skip Count tracking.** Cheapest item on this list — mirrors the existing
  >50%-played play-count threshold with an inverse condition, reuses the same
  event plumbing. Pairs naturally with the already-prioritized scrobbling
  module (a skip is a signal scrobblers care about too) and with future smart
  playlist rules ("rarely finished" as a rule field).
- **Smart Playlist rule editor UI.** The engine (`rules_json`, generic
  field/operator/value model) already exists and ships 3 canned playlists in
  the MVP; building the editor dialog is mostly UI work against an already-
  solved backend. Currently scheduled as a vague "later," but the low
  marginal backend cost argues for pulling it earlier — possibly into the
  MVP tail rather than post-MVP.
- **Duplicates finder.** Reuses the move-detection fingerprint logic
  (title+artist+album+duration±2s+size) nearly as-is; the new part is a
  report view plus a delete/trash action the tag-editor/context-menu
  infrastructure already supports. Cross-validated as core "collection
  hygiene" by both Strawberry and Clementine's user base.
- **Compilation / Album-Artist correctness for VA albums.** The `album_artist`
  column already exists end-to-end (scanner, schema, browse-bar queries) —
  the gap is compilation-aware grouping/display logic (e.g. a "Various
  Artists" bucket in the artist browse column instead of scattering the
  album across every guest performer). Real library-quality issue for
  compilation-heavy collections, and the schema is most of the way there
  already.
- **Classical Grouping/Work/Movement/Composer fields.** New `composer`,
  `work`, `movement`, `movement_name` columns plus lofty read support and an
  optional "don't shuffle within a work" rule. Medium effort because it's new
  schema + scanner + tag-editor surface, but well-precedented (iTunes 12.5+,
  MusicBrainz Picard) and closes a real gap for classical listeners in the
  target audience.
- **Local Auto-DJ / seed-based smart shuffle.** A local-only analog of
  Genius/MusicBee Auto-DJ: pick a seed track, generate a queue biased by
  shared genre/tags/rating, computed entirely in-process — no telemetry, no
  vendor graph. Independently reinvented by three different players, which
  is a stronger signal than iTunes alone would be. Medium-large effort;
  recommend sequencing after the smart-playlist rule editor since it can
  reuse the same rule-matching engine as a source of "candidate tracks."
- **Home-Sharing-style LAN library browsing.** Extends the already-planned
  companion-app remote-control protocol (design doc §"Begleit-App") to also
  let a second desktop Reprise instance browse/stream another instance's
  library over LAN — same paired, no-cloud, opt-in-network posture already
  chosen for that module. Rhythmbox's still-maintained DAAP plugin is direct
  precedent that this remains worth supporting. Larger effort (streaming
  protocol, not just remote-control commands), so sequence behind the
  companion app itself.

Explicitly **not** on this shortlist despite passing the value bar: per-song
EQ override and a separate Love/Heart control — both real but low enough
value against existing features (global EQ, 5-star rating) that they don't
merit roadmap ink right now; listed in §2 for completeness and revisit only
on concrete user demand.
