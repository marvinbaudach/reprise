# Simplified Android sync (MTP) — maintained implementation plan

Status: **stage implemented; manual device check outstanding**
Branch: `feature/android-sync-transfer-profiles`
Feature base: `ea1b3dc7c1`
Integrated `dev`: `6cdb1af33e`
As of: 2026-07-27

## Addendum of 2026-07-28 — three named sync targets replace the single
## managed folder

Turn 7 of the design document (`docs/plans/podcasts-youtube-radio-turn6.md`,
block E) replaces the single managed `Music/Reprise` area described below
with **three named, per-device configurable
sync targets** — playlists, YouTube audio tracks, podcast episodes
(`device_sync/targets.rs`, rule `MTP-38`) — and wires the actual
transfer accordingly (`MTP-23`). Where this plan below speaks of "the
managed folder" or "outside `Music/Reprise`" as the single
boundary, that now applies only to the **playlists target**; the other
two targets (default `/Music/Reprise-YouTube`, `/Podcasts/Reprise`)
write, delete and clean up in just the same way, only additively instead of
fully authoritatively (`MTP-17`). Music still follows the transfer profile;
podcast and YouTube audio is always copied 1:1, never re-encoded (`MTP-24`).
Both new targets carry a size cap with "oldest first" eviction
(`MTP-39`/`MTP-25`), the playlists target stays uncapped. No device
browser for freely choosable target folders exists yet (7d/E6) — the three
targets start with their proposed paths and `enabled = true`.

This plan replaces the earlier draft for a device file browser,
a Preferences sync tab, "Entire Library", pins, ratings back-sync, freely
configurable encoders and a global sync. The
binding execution state is in `.superpowers/sdd/progress.md`; the
commits remain ground truth.

## Product goal

Reprise mirrors an explicit selection of manual and smart playlists onto
connected Android MTP devices. The operation deliberately stays small:

- A device card in the sidebar shows state, delta and progress.
- A click opens the device-specific full page in the main window.
- With one device, the main menu entry opens this page directly, and with
  several devices it first opens a compact device selection.
- On the page, playlists and one of three transfer profiles are chosen:
  Opus 160 kbit/s as the default, MP3 256 kbit/s as the compatibility fallback
  or unchanged original files. Delta and storage projection are
  reviewed, and sync, cancel and eject are triggered.
- Every playlist shows its name, its last verified sync and its
  target size projected for the active transfer profile. A running sync shows
  progress and a smoothed MTP transfer rate.
- The full page follows a device dashboard hierarchy: identity, actions
  and storage come before the playlist work area; transfer profile and
  sync summary stay a compact secondary overview instead of a
  Preferences page.
- The playlist and overview cards keep the same edges regardless of dynamic
  status text. Track and labeled MTP transfer speed are
  on separate lines; the free device storage stays visible at the start of the
  sidebar detail line even during a running sync.
- Local playlist options are projected before the asynchronous MTP storage
  check and stay selectable during "Checking device". Only the
  actual sync start waits for a dependable check result.
- Changes are persisted per device immediately; the chosen
  transfer profile is restored for the same device after a reconnect and an
  app restart. There is no apply step and no second
  sync surface.
- `Music/Reprise` — the playlists sync target (`MTP-38`) — is a mirror root
  fully managed by Reprise. After the new desired state has been published
  completely, all files no longer needed are removed
  there. Outside this subfolder, Reprise writes, moves and deletes
  only within the two other named targets
  (`/Music/Reprise-YouTube`, `/Podcasts/Reprise`, see the addendum above) —
  nowhere else on the device.

Not part of this stage are:

- the entire library as a sync source;
- listing or browsing individual files or songs on the
  device;
- a sync page in Preferences;
- keep-on-device pins and ratings/playcount back-sync;
- freely configurable bitrates or parallel encoders;
- access to arbitrary phone content outside the area managed by
  Reprise;
- a companion app or bidirectional synchronization.

## Architecture

### `reprise-core`

The pure core layer owns the platform-neutral contracts:

- `device_sync/profile.rs`: exactly Opus 160, MP3 256 and Original, with Opus
  as the default, a conservative copy-vs-transcode decision, target size and a
  stable profile fingerprint. Only unambiguously lossless sources are
  transcoded; known lossy as well as unknown or ambiguous
  formats are copied unchanged.
- `device_sync/snapshot.rs`: manual and smart playlist snapshots,
  repetitions in M3U order as well as explicit availability.
- `device_sync/mirror.rs`: a deterministic mirror plan with add, replace,
  remove, playlist write and playlist remove. Safe physical entries under
  the authoritative mirror root that belong neither to the new desired track
  state nor to the desired playlist state are removed as orphans; unsafe paths
  stay untouched and are reported.
- `device_sync/storage.rs`: the current and projected composition of
  Reprise music, other music, other occupied space and free space,
  each with complete, unknown or inconsistent knowledge.
- `device_sync/page.rs`: a toolkit-neutral projection for the device page and
  the sidebar.
- `device_sync/settings.rs`: the per-device selection as well as track and
  playlist inventories.
- `device_sync/targets.rs`: the three named sync targets (`MTP-38`) —
  `StorageID`, path string, activation, optional cap — as a pure
  data layer, see the addendum above.
- `device_sync/cap.rs`: pure "oldest first" cap eviction (`MTP-39`),
  independent of transport and target kind.
- `device_sync/podcasts.rs`: the sync plan for podcast episodes and
  YouTube audio tracks — both via the same `build_plan`, distinguished only
  by `PodcastSyncSource` and target cap.
- `device_sync/category_diff.rs`: the per-category readable diff projection
  for the device dashboard (`MTP-45`/`MTP-22`), a pure display translation
  without transfer logic of its own.

`reprise-core` stays free of GTK, libadwaita, GStreamer and zbus.

### `reprise-platform-linux`

- `device_sync.rs` detects MTP targets exclusively and resolves per call the
  path of one of the three named sync targets (`target_path: &str`, see
  the addendum above) instead of a hard-wired area; the playlists
  inspection still aggregates foreign music only as a total, the two
  other targets are delivered separately as their own file inventories
  (`device_sync_inspection.rs`).
- `device_transfer.rs` transcodes exactly one unambiguously lossless file
  at a time, either via `opusenc → oggmux` with 160 kbit/s VBR or via
  `lamemp3enc → id3v2mux` with 256 kbit/s CBR. Tags and embedded covers
  are preserved in both results. Original and lossy passthrough
  need no encoder.
- Before a run, only the GStreamer pipelines actually needed are
  checked. If a factory is missing, the plan is blocked before the first
  destructive step.
- The MTP transfer first writes `<target>.part`, checks the expected size and
  only then publishes atomically to the final managed path.
- Partials are cleaned up independently under each of the three named targets
  (`cleanup_partials_in(target_path)`), no longer only under `Music/Reprise`.

### `reprise-gnome`

- `device_sync_runtime.rs` holds per device the state, generation, cancel
  token, storage snapshot, inventory, mirror plan and projection.
- `device_sync_planned.rs` executes serially per device; different devices
  may run independently in parallel.
- `device_sync_page.rs` is the only editable sync surface and lives
  as a normal full page in the content stack of the main window.
- `device_sync_launcher.rs` is the main menu entry point and the
  multi-device selection.
- `sidebar_device_card.rs` projects the same state in place, without rebuilding
  widgets on progress events.
- `device_sync_feedback.rs` owns connected, disconnected, cancel and
  completion feedback as well as the header spinner when the sidebar is not
  visible.

## Persistence and migration

The merged schema state is `user_version = 39`:

- v34: podcasts/radio;
- v35: Recently Added;
- v36: Android sync inventory;
- v37: modern transfer profile;
- v38: last verified sync per device playlist;
- v39: official track count for discography gaps.

v36:

- adds `device_settings.mp3_quality` with 128/192/256/320 and a default of 256;
- normalizes the old `"entire_library"` value to an empty
  playlist selection;
- makes `device_files` explicit with source path, source size, mtime, device
  path, device size and profile fingerprint;
- introduces `device_playlists` with a stable source and a unique device path;
- marks old Opus inventory with `legacy-opus-v1`, so that it is safely replaced
  on the next selected sync.

v37 adds `device_settings.transfer_profile` with the stable values
`opus_160`, `mp3_256` and `original`. New devices begin with `opus_160`;
devices already configured under v36 are migrated conservatively to `mp3_256`,
so that an upgrade does not silently change their previous output format.
The old columns `mp3_quality` and `opus_bitrate` are kept exclusively
as inert DB compatibility fields without a user interface.
v38 adds `device_playlists.last_synced_at` as an optional Unix timestamp.
Existing inventories migrate honestly with an unknown point in time. Reprise
sets the value for each selected playlist only after a successful
device readback; the previous time survives failed or only
partially published runs.
Inventory rows deliberately do not cascade with library tracks: a track
temporarily missing locally must not destroy information about the existing
device file.

## Deterministic mirror plan

1. Load the persisted manual and smart playlist selection.
2. Materialize every playlist in a stable order.
3. Deduplicate physical tracks across all playlists; M3U repetitions
   are preserved.
4. Derive target paths FAT-safely and collision-stably under
   `Music/Reprise/<Album Artist>/<Album>/<NN Title>.<ext>`. The
   extension follows the actual output: `.opus`, `.mp3` or the
   lowercased original extension.
5. Compare source fingerprint, profile and inventory:
   - unchanged and matching: keep;
   - new: copy/transcode;
   - changed or an old profile: replace;
   - no longer selected: remove;
   - not available locally, but inventoried on the device: keep;
   - a safe unknown physical entry under `Music/Reprise`: remove as an
     orphan;
   - an unsafe path: warn and leave untouched.
6. Plan root-level M3U snapshots and explicitly remove old managed playlist
   files.
7. Project delta, transfer bytes and storage state. Blockers contain
   no local or device paths.

For transcodes, the projection reserves the nominal Opus or
MP3 audio stream, the full source size for source-derived
tags/covers and additional container/frame overhead. Passthrough uses
the real source size. An unknown duration therefore has no artificially
small, bounded transcode estimate.

## Execution order and safety invariants

Before destructive work, device identity, connection, mirror plan,
the encoder pipeline actually needed and the current free storage are
checked again.

The order of a per-device run is:

1. clean up orphaned `.part` files;
2. transcode/copy tracks one after another;
3. inventory the verified new file, but for now keep old differing target
   paths;
4. publish all new playlist snapshots;
5. only after the playlists have been published completely, remove obsolete
   playlist files, tracks that are no longer selected and replaced old
   target paths;
6. re-inspect content and available storage;
7. publish the idle/synced state.

Further invariants:

- Never delete or overwrite files outside `Music/Reprise`.
- Within `Music/Reprise`, the fully published desired track and
  playlist state counts as authoritative; only after that are all remaining
  safe files removed.
- Never use the real Reprise database or music library in tests.
- At most one file operation per device; cross-device parallelism
  is allowed.
- Cancel affects only the named device and also stops further
  playlist publications and deletions.
- Generations discard late scan, progress and completion events.
- During a run, playlist and profile changes are locked before
  persistence.
- A failed inventory transaction must not delete an old differing
  target path.
- A failed or cancelled playlist publication must not remove any
  paths that the previous snapshot still references.
- Unknown capacity is presented as unknown, not as fitting.

## UI contract

The device page shows:

- a simple hero head made of device name, MTP connection, last
  device sync, storage bar and actions;
- transfer profile Opus 160 kbit/s, MP3 256 kbit/s or Original;
- the visible guarantee that lossy and unknown sources are never
  transcoded into another lossy format;
- every manual and smart playlist with entry, unique, missing,
  profile-dependent size projection and the last verified
  sync time;
- deduplicated total tracks and physical target size;
- comprehensible change, blocker and warning summaries without paths;
- a storage summary and segment bar for Music, after-sync delta,
  Other and Free;
- the running step, track, file progress and smoothed MTP copy rate;
- the primary actions `_Sync now` and `_Cancel` respectively, with mnemonics;
- eject only for a connected, inactive device;
- a readable disconnected status on cable loss.

Playlist rows are rebuilt only when their sources change.
In doing so, the current focus is restored on the same source or the nearest
remaining row. No `RefCell` borrow persists across
GTK setters or signal paths.

The local agent/D-Bus/MCP interface uses the same three stable
profile values `opus_160`, `mp3_256` and `original`. A configuration changes
the explicit transfer profile state; the old `opus_bitrate`
compatibility field stays null in the process.

## Closing state

Tasks 1 to 13, both dev integrations and the adversarial
safety/storage follow-ups are complete. The agent, D-Bus and
MCP contract matches the device page:

- `music_get_device_sync_state` delivers manual and smart
  playlist identities together with the verified last sync, the last device
  sync, transfer profile, deduplicated totals, changes, storage composition
  including write access, blockers, warnings, controls and progress without
  serial numbers or paths.
- `music_device_sync` accepts `configure`, `start`, `cancel` and `eject`.
  `configure` receives sources as stable pairs of `kind`
  (`playlist` or `smart`) and `id` as well as `profile`; without a value,
  `opus_160` applies. `eject` is available only for a connected, inactive
  device. All mutations require `device:sync`.
- Overlapping playlist sources use the same deduplicated mirror plan:
  every physical track is transferred only once, while every written
  playlist keeps its order and deliberate repetitions.
- The compatible old bitrate fields stay inert and are not reactivated as a
  product feature by the new configuration.

The commits and gate results are evidenced individually in the progress
ledger. What remains outstanding are exclusively the checks listed below
with an explicitly approved test device. The UX rules MTP-7 to
MTP-15 are active with their rule-named tests.

## Verification

Before every stage commit, the following must pass:

```bash
cargo fmt --check
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace
cargo audit
cargo tree -p reprise-core | grep -E 'gtk4|libadwaita|gstreamer|zbus'
scripts/check-architecture.sh
scripts/check-accessibility-semantics.sh
scripts/check-input-parity.sh
scripts/check-motion-tokens.sh
scripts/check-ux-traceability.sh
scripts/check-device-sync-gstreamer.sh
```

The core purity output must be empty. All substantially changed code files
stay under 800 lines.

## Automated MTP E2E simulation

Stable E2E tests need no attached phone. The
`SimulatedMtpDeviceBackend` replaces the real MTP/GIO backend at its
application boundary with a connected phone that has an exclusively
temporary storage root. The tests still go through the real
transcoders, mirror planning, GIO file operations, inventory transactions,
playlist publication, progress and cancel states as well as the
final device readback.

The simulation checks Opus 160, MP3 256 and byte-exact original passthrough,
independent parallel devices as well as the complete cleanup of foreign,
non-inventoried audio, playlist and other files exclusively
under `Music/Reprise`. It
also owns the isolated background CUA run `android-sync-page`: it
opens the device card of the simulated phone, verifies the non-modal
full page with hero head, transfer profile, playlist selection,
profile-dependent size, last sync, delta and storage, selects a playlist via
the real GUI action and then proves an actually published Opus file
in the temporary device root. Markup parser errors in dynamic names
turn this run red.

The simulation
deliberately emulates neither USB nor `libmtp` nor the GVfs device detection:
these layers depend on host and hardware and stay additional
manual acceptance checks, not prerequisites of the reproducible suite.

## Manual stage review checks

These checks need explicit approval and a test device; they cannot be
replaced by headless tests:

1. connect a real Android device and check the connected toast/card;
2. check the Opus 160 and MP3 256 results including tags/covers as well as
   byte-exact FLAC passthrough in the Original profile;
3. observe copy progress and rate on the real GVfs MTP backend;
4. check cable loss during copy as well as reconnect/partial cleanup;
5. check eject in the idle and disabled state during a sync;
6. start two real devices independently and cancel one of them;
7. visually check focus, mnemonics, storage segments, High Contrast and
   reduced animations.

Without this approval, the implementation accesses neither a phone nor
real music files or the real database.
