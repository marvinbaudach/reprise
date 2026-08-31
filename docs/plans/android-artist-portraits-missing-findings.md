# Android artist portraits stay blank — findings

Investigated 2026-08-30 on a Pixel 10 Pro XL (adb, release build `0.1.71`),
against desktop Reprise `0.1.84`.

## Reported vs. observed

Reported as "album covers do not load in the Android app". The evidence says
something narrower:

| Surface | State |
|---|---|
| Track covers, Titles tab | **Load.** Placeholders fill in as the list settles. |
| Album rows, artist detail | No image slot in the layout at all — nothing to load. |
| Artist thumbnails + artist detail portrait | **68 of 68 fallback gradients.** |

So the empty surface is artist portraits, not album covers.

## Root cause

Settings → Online sources → *Download artist photos* was **off**.

On Android an artist portrait has exactly one source: a Deezer fetch from the
phone.

- `crates/reprise-core/src/artist_portrait/deezer.rs:106` — `search()` /
  `download_image()` over `ureq` against `api.deezer.com` / `*.dzcdn.net`
- `crates/reprise-android-ffi/src/artist_portrait.rs:106-130` — the FFI fetch is
  gated on `ARTWORK_MODULE`
- `crates/reprise-android-ffi/src/online_sources.rs:16-26` — that gate is the
  settings switch
- `crates/reprise-core/src/modules.rs:137-144` — `default_enabled: false`

Device sync never fills the gap: it carries audio, analysis sidecars and
playlists only.

- `crates/reprise-core/src/device_sync/mirror.rs:129-149` — the mirror plan
- `rg 'portrait|artwork|cover' crates/reprise-core/src/device_sync/` — no hits

Matching evidence on the two machines:

- `/sdcard/Music`: 2279 audio files, **0 image files**
- Desktop `~/.cache/reprise/artist-portraits`: **44 portraits**, which stay there

The online scan the user ran was the desktop one. It fills the desktop cache,
and that cache is never synced.

## Verification

Flipping the switch on the device produced `Downloading artist photos 0/68`
immediately, ran to 68/68, and after an app restart every artist row showed a
real photo. Hypothesis confirmed, and the device is now in a working state.

## Why the default is not a one-liner

`ARTWORK_MODULE.default_enabled = false` is the project's documented network
policy (NET-1a, "affirmative persisted opt-in",
`crates/reprise-core/src/online_sources.rs:37-39`). Desktop and Android read the
same function (`reprise-gnome/src/ui/cover/cover_download_worker.rs:45`,
`reprise-android-ffi/src/online_sources.rs:9`), and there is no Android-only
seeding point — the database comes wholly from `reprise-core`.

Counted, not estimated: **15 test functions across 4 crates** assert the
opt-in, found with

```
rg -n -g '*.rs' 'fn (network_modules_default_off_and_apply_live|online_gate_fresh_database_defaults_to_disabled|the_switch_is_off_on_a_fresh_database|net_1a_cover_download_respects_the_module|get_reads_the_real_defaults|first_enable_turns_every_online_source_off_except_radio|net_1a_network_allowed_is_an_and_of_global_and_module|all_modules_includes_opt_in_artwork|artwork_has_one_namespaced_live_opt_in|net_1a_recompute_enabled_reflects_the_global_gate|switching_on_survives_the_first_enable_seed)' crates/
```

- `reprise-core`: `online_gate_fresh_database_defaults_to_disabled`,
  `net_1a_network_allowed_is_an_and_of_global_and_module`,
  `first_enable_turns_every_online_source_off_except_radio` (online_sources.rs);
  `all_modules_includes_opt_in_artwork`, `artwork_has_one_namespaced_live_opt_in`,
  `network_modules_default_off_and_apply_live` (modules.rs)
- `reprise-android-ffi`: `the_switch_is_off_on_a_fresh_database`,
  `switching_on_survives_the_first_enable_seed`
- `reprise-gnome`: `net_1a_cover_download_respects_the_module`, plus
  `net_1a_recompute_enabled_reflects_the_global_gate` in five workers
  (cover, artist_portrait, artist_news, podcasts, concerts)
- `reprise-mcp`: `get_reads_the_real_defaults`

**Chosen direction:** ask the user instead of flipping the default, so the
opt-in stays honest and the switch stops being something one has to find.

The prompt fires **after the first scan that found artists**, not on first app
start. A genuine first start has an empty library — nothing to fetch, and a
question about a feature the user has not seen yet — and for every existing
0.1.71 install first start is long past, so a first-start prompt would never
reach the people who have the problem. Firing after a scan also matches the
existing trigger: the switch's own subtitle says portraits are fetched "after
automatic scans, manual scans, or restores".

## Loose ends found along the way, not part of this bug

- The app knows **812 titles**; the device holds **2279 files**.
- Leftovers from an interrupted sync, e.g.
  `…/Humanity's Last Breath/Ashen (instrumental)/07 Catastrophize.reprise-analysis.part`
- While portraits downloaded, an already-rendered artist list kept its
  fallbacks; only an app restart showed the new photos.
- Some tracks keep the fallback cover legitimately — `02 Lifted`,
  `3 Axle`, `6 Gallon Gasoline Stomach` still carry track numbers in the title,
  i.e. untagged files with no embedded picture.
- Device app is `0.1.71`, desktop is `0.1.84`.
- `data/io.github.marvinbaudach.Reprise.metainfo.xml:34-35` promises "Missing
  covers are retrieved automatically", and the same paragraph names
  ListenBrainz and Last.fm as disabled by default while saying nothing about
  artwork. That reads as "covers just work" — the exact expectation behind this
  bug report, and an argument for asking the user rather than leaving the switch
  buried.
