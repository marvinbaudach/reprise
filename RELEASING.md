# Releasing Reprise

This checklist prepares a Reprise snapshot for distribution without modifying
user data. Publishing is automated only by the guarded GitHub release channel
described below; store submissions and other distribution channels remain
separate maintainer actions.

## Automated GitHub release channel

A push to `main` starts `.github/workflows/release.yml`. For the normal weekly
promotion, the gate waits for the `Quality gate` check on the exact commit SHA.
Queued and in-progress checks are polled with a bounded timeout; only `success`
continues. A cancelled check is reported as a superseded promotion, while a
skipped or failed check is not green. Those decisions end the release run
successfully without building, so the original CI result remains the single
failure signal.

The desktop workspace version determines the tag `v<desktop-version>`. If that
tag already exists, the run is an idempotent no-op. Otherwise the Flatpak and
Android jobs build in parallel, and publication happens only after both jobs
succeed. The draft release is checked for four non-empty assets before it is
published and marked latest:

- `Reprise-<desktop-version>.flatpak`
- `Reprise-<desktop-version>.flatpak.sha256`
- `Reprise-Android-<android-version>.apk`
- `Reprise-Android-<android-version>.apk.sha256`

The full English release body comes from the matching dated section in
`CHANGELOG.md`. Software centres use the matching bilingual `<release>` entry
in `data/io.github.marvinbaudach.Reprise.metainfo.xml`. Both curated entries
must exist before promotion; `scripts/check-release-metadata.sh` enforces them
in the release gate. The generated GitHub header adds both app versions,
installation requirements, the Flatpak update limitation, the missing stems
backend in that bundle, Android's minimum version, and the shared licence.

Pull requests that touch release inputs run both build jobs and upload their
artifacts, but never publish. Without upload-key secrets, that APK deliberately
uses the existing debug-signing fallback and identifies it in the run summary.
Release-mode builds require the upload key and verify both its certificate
fingerprint and the APK version metadata. The upload-key creation, backup,
repository-secret, local signing, and inspection procedures are documented in
[`docs/releasing-android.md`](docs/releasing-android.md).

A manual dispatch defaults to `dry_run: true`. Use it on `main` after landing a
release-channel change to exercise the real check-run and tag decisions without
publishing. Do not set `dry_run: false` unless the curated text, upload-key
fingerprint, CI evidence, and both release artifacts are ready.

The named follow-up is a self-hosted Flatpak repository, which will provide a
real update path. It is deferred because the existing GitHub Pages deployment
serves the Showroom from the same publication surface; repository metadata and
the Showroom deployment need a collision-free Pages layout first. Until then,
the downloadable `.flatpak` bundle does not self-update.

## Automated release check

Install the host build dependencies listed in `README.md`, `cargo-audit`,
`desktop-file-validate`, `appstreamcli`, `xmllint`, gettext tools, and PyYAML.
Then run:

```sh
scripts/check-release.sh
```

The script verifies Rust formatting/lints/documentation/tests/audit, standalone
core purity, translation coverage, desktop metadata and icons, a complete
optimized Meson DESTDIR installation, source-file size limits, Flatpak manifest
structure, and the generated Cargo checksums. It runs `flatpak-builder-lint`
when that tool (or the `org.flatpak.Builder` Flatpak) is installed and reports
an explicit skip otherwise. It never launches Reprise or opens a user database.

Only `RUSTSEC-2024-0436` (`paste`, transitively through `lofty`) is an accepted
audit warning. Any additional advisory is a release blocker. The AppStream check
allows exactly two documented informational conditions: the pedantic
`cid-contains-uppercase-letter` note for `io.github.marvinbaudach.Reprise`,
which is intentionally accepted because an uppercase final component is
conventional for GNOME applications (for example, `org.gnome.TextEditor`) and
is not a failure without `--pedantic`, and the absent homepage while no public
project URL exists.

GTK regression tests that require a display are ignored by the normal test suite.
Run all currently discovered display tests in their own isolated processes because
GTK can only be initialized from one thread per process:

```sh
scripts/check-display-tests.sh
```

The runner creates a private D-Bus session, Xvfb display, XDG data/cache roots,
and fake audio sink for every test. It must never be shortened to a live desktop
run. The synchronization display tests prove device empty/list/detail/progress
composition and the phone-playlist COPY drop controller; real hardware remains
manual.

The display runner must run the entire discovered list and report one final
balance sheet. It must not stop after the first red test: a fail-fast result says
nothing about the tests it skipped and therefore cannot establish the remaining
green count. Keep one exact Rust test per process; a collective invocation is
unreliable because GTK's global main context can move between harness threads.

For a focused rerun, first resolve the exact path from the live test list, then
run that path in an isolated process:

```sh
test_path='ui::module::tests::exact_test_name'
cargo test -p reprise-gnome -- --ignored --list \
  | sed -n 's/: test$//p' \
  | grep -Fx "$test_path"

test_runtime=$(mktemp -d)
test_data=$(mktemp -d)
test_cache=$(mktemp -d)
test_config=$(mktemp -d)
XDG_RUNTIME_DIR="$test_runtime" XDG_CONFIG_HOME="$test_config" \
XDG_DATA_HOME="$test_data" XDG_CACHE_HOME="$test_cache" \
GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
xvfb-run -a dbus-run-session -- cargo test -p reprise-gnome \
  --quiet -- --ignored --exact "$test_path" --nocapture
```

`XDG_CONFIG_HOME` is load-bearing, not decoration. GTK reads the window-button
layout from the host's `org.gnome.desktop.wm.preferences button-layout`, so a
maintainer who keeps close/minimize on the left makes the window-decoration
tests fail against their own desktop rather than against the code. Every
display rerun must start from an empty config.

Count the rerun only when its output says `1 passed`. Cargo reports
`test result: ok` for a non-existent `--exact` path with zero tests, which is a
missing check rather than a green check.

Before the final manual GNOME pass, run the mapped-window pointer regression
against the release binary:

```sh
cargo build --release
PTR_E2E_PROFILE=release scripts/ptr-e2e/run.sh
PTR_E2E_PROFILE=release PTR_E2E_NEWS_ONLY=1 scripts/ptr-e2e/run.sh
```

This separate harness uses copied fixtures, a temporary XDG profile, private
D-Bus/Xvfb, and a fake audio sink. It injects a real rating click, opens the
track context menu and tag editor by keyboard, rejects an invalid Year on
Enter, performs a held Queue drag with insertion-target capture and reorder,
exercises Space play/pause and Ctrl+M, opens and captures all five Preferences
pages, toggles layout/effect controls, verifies the plugin boundary, performs a
real library rescan, verifies exact isolated SQLite values, and rejects GTK/GLib criticals, Rust
panics, and `RefCell` borrow failures. It does not replace the native-Wayland,
audible-audio, media-key, or portal checks below.

The dedicated Artist News mode copies and tags its own FLAC fixtures, serves local
MusicBrainz-shaped responses, and exercises the real selection/runtime/persistence
paths in a mapped window. It proves explicit opt-in, Upcoming/New cards, delayed stale
selection rejection, close/reopen reuse, disable behavior, request-field privacy and a
shared interval of at least one second without network access.

## Optional exploratory UX check before `main`

Before promoting a tested `dev` snapshot to `main`, run the opt-in exploratory
CUA mission deck on that exact clean commit. This check is deliberately absent
from ordinary CI: it combines large generated data, heuristic UX oracles, and
optionally a reasoning agent, so its evidence requires human review.

```sh
scripts/cua-explore/run.sh --list-missions
scripts/cua-explore/run.sh \
  scripts/cua-explore/missions/first-time-exploration.json \
  target/cua-explore-evidence/first-time-seed-11 \
  --profile release --seed 11
```

Run all five supplied missions, including `offline-recovery` and
`large-library-stress`; attach a reasoning agent for the structured batch-edit,
sort, and filter workloads. Repeat newly observed anomalies with a fresh output
directory and seed. Review each retained `report.md` together with its
before/action/after states. A reproduced error blocks promotion unless the
maintainer explicitly accepts it with evidence; warnings and one-off heuristic
findings stay advisory. The complete operator guide, external-agent protocol,
privacy boundary, and limitations are in `scripts/cua-explore/README.md`.

The synchronized-lyrics smoke copies and tags three synthetic FLAC fixtures and sets
`REPRISE_SMOKE_LYRICS=1` with `REPRISE_LRCLIB_FIXTURE_DIR` and a private request log.
It must show active-line indices 0 then 1, reject a delayed stale track in favor of the
latest track, and log only title, artist, album, and rounded duration. Run it only with
private XDG data/cache, private D-Bus/Xvfb, forced X11, unset Wayland, `fakesink`, and a
fixture-only MusicBrainz directory so neither lyrics nor cover paths can reach the
network.

```sh
scripts/check-lyrics-smoke.sh
```

## Bundled Ticketmaster credential

To bundle a Ticketmaster Discovery API key in a local release, inject it into
the environment as `REPRISE_TICKETMASTER_APIKEY` before running the release
build. Meson passes its build environment to Cargo:

```sh
# Populate REPRISE_TICKETMASTER_APIKEY through a secret manager first.
export REPRISE_TICKETMASTER_APIKEY
meson setup target/meson-release . --prefix=/usr -Dprofile=release
meson compile -C target/meson-release
unset REPRISE_TICKETMASTER_APIKEY
```

For a GitHub Actions release build, create an Actions repository secret named
exactly `REPRISE_TICKETMASTER_APIKEY` and pass it only to the build step:

```yaml
- name: Build release
  env:
    REPRISE_TICKETMASTER_APIKEY: ${{ secrets.REPRISE_TICKETMASTER_APIKEY }}
  run: meson compile -C target/meson-release
```

Never print the variable or enable shell tracing around the build. Keeping a
build credential out of the repository does not make it secret after
distribution: a bundled key is fundamentally extractable from a published
binary.

Do not add an Impact wrapper or a separate Impact API key. Ticketmaster
Discovery event links receive affiliate tracking when the Ticketmaster
Developer Portal associates the key with an Impact Publisher ID. Before any
public release, review as a separate product/legal question where a visible
affiliate disclosure must appear.

## Build artifacts

Create a clean optimized install tree without writing to `/usr`:

```sh
meson setup /tmp/reprise-release-build . --prefix=/usr -Dprofile=release
meson compile -C /tmp/reprise-release-build
DESTDIR=/tmp/reprise-release-root meson install -C /tmp/reprise-release-build
```

The install root must contain:

- `/usr/bin/reprise`
- `/usr/share/applications/io.github.marvinbaudach.Reprise.desktop`
- `/usr/share/metainfo/io.github.marvinbaudach.Reprise.metainfo.xml`
- both scalable application icons under `/usr/share/icons/hicolor`
- `/usr/share/locale/de/LC_MESSAGES/reprise.mo`

For Flatpak, follow `flatpak/README.md`. A full builder run and sandbox start are
required on a machine with `flatpak-builder`, GNOME Platform/SDK 50, and the
matching stable Rust extension.

Android synchronization additionally requires the host and Flatpak environment
to expose GVfs MTP. The manifest must contain exactly
`--talk-name=org.gtk.vfs.*` and `--filesystem=xdg-run/gvfsd`; direct USB access,
host filesystem access, and broad session/system bus access are forbidden by the
release check.

## AUR metadata

Regenerate `packaging/aur/.SRCINFO` whenever `packaging/aur/PKGBUILD` or the
Meson install set changes:

```sh
cd packaging/aur
makepkg --printsrcinfo > .SRCINFO
```

Review and push the separate AUR mirror by hand as documented in
`packaging/aur/README.md`. Release tooling must not push it.

## Manual GNOME QA

Use disposable test music and a disposable XDG data directory where practical.
Do not point development hooks at the maintainer's real library.

- ACC-7 visible-focus acceptance: complete the whole application with only
  the keyboard in default and High Contrast themes, then repeat with Large
  Text. Every focus stop must remain visible and distinct from hover,
  selection, and now-playing state. Verify names, roles, states, values, and
  actions with Orca and a switched-off monitor; cover the on-screen keyboard,
  real GNOME/Wayland dialogs and portals, and reduced animation. Record the
  result before changing ACC-7 from `[planned]` to `[active]`.

- Confirm first-run copy/layout, Skip, Set Up Library, and the portal folder
  chooser. The copy must disclose automatic cover lookup without showing a
  disable switch. A detected `rhythmdb.xml` must show a clearly default-off,
  one-time Rhythmbox data-import choice; after the initial library scan it must
  offer ratings, play history, dates, and playlists, but no column-layout
  import. No false offer or later menu/Preferences entry may exist.
- Check English and German UI for clipping, untranslated text, natural plurals,
  keyboard mnemonics, narrow-window adaptation, touch/pointer interaction, and
  light/dark appearance.
- My Stats editorial pass (UX STATS-10 through STATS-15, STATS-18 through
  STATS-20): open My Stats on a populated library. The page must read hours →
  bands → songs → genres, each at full width. Hero time and play count must
  agree with the top-track list, and the "This week" KPI with what the current
  week actually holds. Click a song row: it starts playing and its rank turns
  into the equaliser, while its title and artist stay links into the library.
  Follow the leading band to the artist, then use Back. Hover a genre segment
  and check the tooltip names share, duration and leading artist. Check that
  eyebrows, legend and sublines stay readable against the view background in
  all three dark themes.
- My Stats grouping (UX STATS-9): on a library with a deliberately mis-tagged
  artist ("Lorna Shore" / "lorna shore" / "Lorna Shore "), Top Artists must show
  one entry with the summed plays and hours, labelled in the clean spelling, and
  the band card must show one merged artist. Two genuinely different
  artists must never merge. Follow the "unify spellings" hint into the tag editor
  and cancel it, then confirm with a tag dump that the files and DB rows are
  unchanged by merely opening My Stats.
- Exercise Minimal View through the menu and Ctrl+M, including playback controls,
  repeated Full/Minimal transitions, close/reopen, and restoration of the full
  window geometry. Exercise every Preferences page and restart to verify persisted
  theme, accent source, density, sidebar/status, player-bar position, columns,
  library root and module states. With the System accent selected, change the
  GNOME accent and confirm Reprise updates without a restart.
- NPP-1: Toggle the Now Playing panel repeatedly at wide and narrow sizes; its 300 px
  column must slide in and out like the 240 px left sidebar without covering content.
- NPP-3: In both light and dark appearance, confirm the effective-accent glow stays in
  the upper third, fades into the neutral-dark stage, and disappears in the idle state.
- NPP-5/NPP-6: Play copied music with synchronized lyrics and inspect the 100/45/32/28
  line hierarchy, centered accent underline, 150 ms line fades, and calm centered glide.
- NPP-13: Change tracks with animations enabled and disabled; the right panel, active
  tab, queue, and footer must remain visible while only the outgoing cover fades over
  the resolved new cover. The queue advances independently, the effective accent stays
  stable through the cover transition, and new synchronized lyrics start with line
  zero centered.
- For the reduced-motion variants above, toggle GNOME's system animation
  setting for the duration of the test and restore its previous value after
  closing Reprise. A per-profile GTK `settings.ini` can be overridden by the
  live desktop's XSettings value and is not sufficient evidence on GNOME.
- With disposable tagged audio and real speakers, adjust all equalizer bands and
  presets while playing, then compare ReplayGain Off, Per Track, and Per Album on
  files containing valid ReplayGain tags. Equalizer and ReplayGain belong only to
  Playback and must not be duplicated on Plugins.
- Scan the seven advertised extensions and play representative available codecs
  through real speakers. Verify seek, volume, previous/next, queue, shuffle, and
  repeat without surprise autoplay.
- Verify MPRIS quick settings, media keys, notifications, lock screen, metadata,
  cover art, shuffle/repeat writes, and clean shutdown on a real GNOME session.
- Exercise browse facets, search, the column-layout editor (switches, buttons,
  whole-row drag, insertion lines, reset and restart persistence), a real
  read-only first-run Rhythmbox data import plus second-start suppression,
  playlists, M3U import/export, and drag/reorder gestures.
- With a disposable Android device unlocked in File transfer/MTP mode, verify
  detection, device music browsing, phone-playlist creation, drag-to-copy,
  per-file/overall progress, same-device FIFO ordering, cancellation, and safe
  disconnect/reconnect. Confirm all writes stay under `Music/Reprise` and the
  relative `.m3u8` order matches the dragged tracks.
- Connect a disposable ListenBrainz account, verify the displayed account name,
  then test playing-now, the half-track/four-minute threshold, offline persistence
  across restart, retry delivery, disable-without-sending, and Disconnect clearing
  both the keyring item and local pending queue. Never use a production token in
  automated tests or logs.
- With a disposable Last.fm account and disposable desktop API application, verify
  masked BYO credentials, browser authorization, account status, playing-now,
  scrobbling thresholds, restart/offline retry, disable-without-sending, and
  Disconnect clearing only Last.fm credentials and its independent queue. Never put
  the API key, shared secret, session key, or real account metadata in the repository,
  terminal history, screenshots, test fixtures, or logs.
- Batch-edit multiple copied tracks. Mixed fields must show multiple values and
  unchanged per-track fields must remain untouched.
- Confirm database-only removal leaves copied files intact. Confirm move to Trash
  places copied files in the host file manager's Trash, both on host and Flatpak.
- Close and reopen twice: geometry, source, search, browse, sort, queue order,
  current track, shuffle, and repeat must restore while playback remains stopped.
- Inspect installed launcher name/comment/keywords, both icons, AppStream data,
  and application ID consistency on the actual desktop.
- Tooltip discipline (UX TIP-1b, TIP-2b, TIP-3, TIP-4, TIP-5): hover every
  icon-only button in both window modes — wording is verb + object, with the
  shortcut in parentheses where one exists (TIP-1b). Every disabled control
  names its reason: visibly for labeled controls (Connect without a token,
  Open Browser without credentials, a failed Rhythmbox prescan), in the
  tooltip for icon-only ones (Eject while syncing) — TIP-2b. Information shown
  in a tooltip must also be reachable without hovering — for the sync card
  check count, size, and ETA in the device view (TIP-3). No menu or context
  item shows a tooltip (TIP-4). Tooltips use stock GTK behavior: no custom
  delays, no rich content; dynamic values (percent, time, elided full text)
  are fine (TIP-5).
- Motion discipline (UX MOT-8): with a copied library of at least 10,000
  tracks, reload, scroll, filter, and drag rows in Library, Playlist, and Queue
  views. Individual rows must never stagger, fade in, or move during reloads;
  only the whole surface may transition when switching views. Podcasts ↔ Music
  must retain the standard-duration motion without a hard cut or a frame where
  both dense tables are readable. Queue drop and single-remove motion is an
  allowed exception, not a release requirement.
- Manual rescan entry points (UX NAV-15b): the header ⋮ menu offers no rescan,
  so start a scan from Preferences → Library and again from the track list's
  retry button in its empty or unavailable state. Each must start a real scan
  and raise the scan card; on an already-scanned library the card must still
  be perceivable rather than flashing past.
- STYLE-1 "floating" check: reveal every collapsible bar (search bar, banners,
  the scan card) once. If it lays flat over the content without its own surface
  and edge, the background is missing — `ToolbarStyle::Flat` swallowed it.
  Repeat in all three dark themes: the window colour sits differently against
  each, so a wrong surface is obvious in one and subtle in another.

Record the OS, GNOME version, runtime branches, architecture, codec packages, and
results for the release notes. Screenshots must be captured manually from a real,
populated test library; do not fabricate them from headless output.

## Other publication channels

The GitHub release is not a Flathub, AUR, COPR, Play Store, or F-Droid
publication. Those channels keep their own review and maintainer handoffs. The
Flatpak manifest continues to use the repository checkout as its CI source; a
future distribution manifest must use the immutable published source archive
and its verified checksum instead.
