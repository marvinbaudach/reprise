# Releasing Reprise

This checklist prepares Reprise 0.1.0 for distribution without publishing or
modifying user data. The repository currently has no public remote, and this
workflow must not push, tag, upload, or submit anything automatically.

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
allows exactly two documented informational conditions: the established
uppercase component ID `org.reprise.Reprise` and the absent homepage while no
public project URL exists.

Fourteen GTK regression tests require a display and are ignored by the normal test
suite. Run each in its own process because GTK can only be initialized from one
thread per process, while Rust's test harness gives separate tests separate
threads even with `--test-threads=1`:

```sh
for test in \
  closed_popover_stays_parented_until_pending_actions_finish \
  widgets_show_running_counts_and_terminal_result \
  progress_widgets_show_running_fraction_and_counts \
  widgets_reveal_progress_and_hide_after_finish \
  reentrant_set_on_changed_does_not_panic \
  enter_activates_the_apply_button_from_every_entry_row \
  interaction_surface_expands_to_the_whole_cell \
  movable_row_owns_drag_and_drop_controllers \
  token_entry_is_a_masked_password_row \
  header_and_restore_buttons_switch_one_application_window_in_one_activation \
  bar_layout_has_required_accessible_controls_and_fits \
  cover_layout_has_required_accessible_controls_and_fits \
  pill_layout_has_required_accessible_controls_and_fits \
  card_layout_has_required_accessible_controls_and_fits
do
  XDG_DATA_HOME="$(mktemp -d)" XDG_CACHE_HOME="$(mktemp -d)" \
    xvfb-run -a cargo test -p reprise-gnome "$test" -- --ignored
done
```

Before the final manual GNOME pass, run the mapped-window pointer regression
against the release binary:

```sh
cargo build --release
PTR_E2E_PROFILE=release scripts/ptr-e2e/run.sh
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

## Build artifacts

Create a clean optimized install tree without writing to `/usr`:

```sh
meson setup /tmp/reprise-release-build . --prefix=/usr -Dprofile=release
meson compile -C /tmp/reprise-release-build
DESTDIR=/tmp/reprise-release-root meson install -C /tmp/reprise-release-build
```

The install root must contain:

- `/usr/bin/reprise`
- `/usr/share/applications/org.reprise.Reprise.desktop`
- `/usr/share/metainfo/org.reprise.Reprise.metainfo.xml`
- both scalable application icons under `/usr/share/icons/hicolor`
- `/usr/share/locale/de/LC_MESSAGES/reprise.mo`

For Flatpak, follow `flatpak/README.md`. A full builder run and sandbox start are
required on a machine with `flatpak-builder`, GNOME Platform/SDK 50, and the
matching stable Rust extension.

## Manual GNOME QA

Use disposable test music and a disposable XDG data directory where practical.
Do not point development hooks at the maintainer's real library.
The detailed live ledger of confirmed and pending checks is
`docs/agent-workflow/MANUAL-QA.md`; keep it synchronized with every manual pass.

- Confirm first-run copy/layout, Skip, Set Up Library, and the portal folder
  chooser. Cover download must default off. A detected Rhythmbox installation
  must show a clearly default-off import offer; no false offer appears without it.
- Check English and German UI for clipping, untranslated text, natural plurals,
  keyboard mnemonics, narrow-window adaptation, touch/pointer interaction, and
  light/dark appearance.
- Exercise Minimal View through the menu and Ctrl+M, including playback controls,
  repeated Full/Minimal transitions, close/reopen, and restoration of the full
  window geometry. Exercise every Preferences page and restart to verify persisted
  theme, density, sidebar/status, player-bar position, columns, library root and
  module states.
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
  whole-row drag, insertion lines, reset and restart persistence), a real read-only
  Rhythmbox column import, playlists, M3U import/export, and drag/reorder gestures.
- Connect a disposable ListenBrainz account, verify the displayed account name,
  then test playing-now, the half-track/four-minute threshold, offline persistence
  across restart, retry delivery, disable-without-sending, and Disconnect clearing
  both the keyring item and local pending queue. Never use a production token in
  automated tests or logs.
- Batch-edit multiple copied tracks. Mixed fields must show multiple values and
  unchanged per-track fields must remain untouched.
- Confirm database-only removal leaves copied files intact. Confirm move to Trash
  places copied files in the host file manager's Trash, both on host and Flatpak.
- Close and reopen twice: geometry, source, search, browse, sort, queue order,
  current track, shuffle, and repeat must restore while playback remains stopped.
- Inspect installed launcher name/comment/keywords, both icons, AppStream data,
  and application ID consistency on the actual desktop.

Record the OS, GNOME version, runtime branches, architecture, codec packages, and
results for the release notes. Screenshots must be captured manually from a real,
populated test library; do not fabricate them from headless output.

## Public publication handoff

Three external prerequisites remain and cannot be inferred or manufactured:

1. Publish the source through a maintainer-controlled public remote and create an
   immutable 0.1.0 archive/tag with a verified SHA-256 checksum.
2. Establish a verifiable project identity appropriate for the existing
   `org.reprise.Reprise` application ID.
3. Make the maintainer-controlled project/contact URL embedded in the
   MusicBrainz `User-Agent` real and reachable before distributing builds with
   online cover download enabled; do not publish the current placeholder URL.

After those exist, replace the local `type: dir` Flatpak source with the immutable
archive, add the real homepage to AppStream, rerun every automated and manual check,
and submit through the maintainer's chosen public forge/Flathub account. Creating
the remote, tag, release, signatures, screenshots, or Flathub pull request is an
explicit maintainer action and is outside this local no-push handoff.
