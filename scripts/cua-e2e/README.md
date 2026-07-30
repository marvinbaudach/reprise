# CUA GUI acceptance tests

This harness drives the real Reprise GTK4 process through CUA's AT-SPI
accessibility actions. It complements the coordinate-based `scripts/ptr-e2e`
suite with stable semantic targets and keeps every run away from the user's
desktop, database, session bus, audio devices, and music.

Run it after building the app:

```sh
cargo build
scripts/cua-e2e/run.sh
```

The runner creates a private Xvfb display and Openbox window manager. Each
scenario group gets a fresh D-Bus session, AT-SPI bus, CUA daemon, XDG profile,
fake audio sink, and copied FLAC fixtures. It exercises nine public workflows:

1. a fresh profile exposes the first-run wizard; activating `Skip for Now`
   reveals the `No music yet` empty-library state;
2. a populated profile scans two copied fixtures, proves Music exposes one
   canonical track table without Tracks/Albums/Artists mode tabs, types into
   the accessible `Search all fields` control to reveal `No results`, verifies
   a menu rescan stays in place with a perceivable progress bar, and runs the
   keyboard-only surface inventory;
3. a tag write preserves selection and scroll position;
4. the multi-track Tag Editor exposes its complete accessible structure;
5. Library Doctor opts in from Plugins, proves re-activating the already
   selected Music row escapes the Doctor page, scans copied fixtures, verifies
   wide and narrow review layouts, applies the reviewed plan, disables the
   module, and reverts the cleanup from the still-available action;
6. Song Visuals exposes exactly the active Grid, Bars, Flow, and Pulse modes;
7. a playing track remains responsive and visibly marked while 24 Title-header
   sort toggles repeatedly recycle every visible track cell;
8. `MTP-46`: a source module switched off contributes no row to a connected
   phone's Content section, and switching one off leaves its peer alone. This
   one starts the app three times against a single profile and writes the
   persisted module setting in between, because the Preferences dialog keeps
   every page in the accessibility tree at once — a label found there says
   nothing about the page being visible, so driving the switch itself would
   assert against invisible widgets;
9. the Podcasts source end to end and offline: with the module off its page is
   unreachable, with it on the empty state offers subscribing, and a fixture
   feed goes through the Add Podcast dialog — including the primary button
   turning from `Search` into `Preview` once a URL is entered — to a listed
   show whose counts (`1 show · 1 episode · 0 new`) prove the feed was parsed
   rather than its name taken from the URL. It closes the dialog before the
   phase ends: while one is open the app never exits, and `finish_scenario`
   then waits for an exit that never comes.

Every CUA action is bracketed by a fresh `get_window_state` snapshot. The run
fails on a degraded accessibility tree, a suspected no-op/escalation request,
missing semantic labels, GTK/GLib criticals, Rust panics, or RefCell borrow
failures. Each scenario has its own app log. The runner requires explicit
startup, database-ready, workflow-decision, scan, and smoke-shutdown markers,
so a log sent back by a user has the same searchable vocabulary as acceptance
evidence. `run-manifest.txt` records only the commit, build profile, CUA
version, platform, and display backend; it never records library paths or
profile contents. JSON snapshots, screenshots, logs, and that manifest remain
in a unique timestamped run directory below `/tmp/reprise-cua-e2e` by default;
temporary profiles and fixture copies are always deleted. Existing evidence is
never recursively cleared, including when a custom output root is selected.

Environment overrides:

- `CUA_E2E_PROFILE=release` selects `target/release/reprise`;
- `CUA_E2E_OUT_DIR=/path` changes the retained evidence directory;
- `CUA_E2E_SCREEN_RES=1600x900x24` changes the private display size;
- `CUA_E2E_QUIT_DELAY_SECS=15` changes the app's clean smoke-quit timeout;
- `CUA_E2E_KEYBOARD_QUIT_DELAY_SECS=150` changes the longer timeout reserved
  for the complete keyboard surface sweep and Library Doctor workflow.

The helper contract has a deterministic fake-driver test:

```sh
scripts/tests/cua-e2e.sh
```

Keyboard-only acceptance uses a separate inventory and dispatcher:

```sh
scripts/cua-e2e/keyboard.sh --check-manifest
scripts/cua-e2e/keyboard.sh --run PID WINDOW_ID
```

`keyboard-surfaces.tsv` is the fail-closed inventory of every released GUI
surface. A missing/duplicate surface, unknown group or scenario, missing focused
state, degraded snapshot, suspected no-op, or escalation recommendation makes
the contract fail. Every listed surface has a keyboard-only flow. `run.sh`
executes the complete manifest in two fully isolated populated-profile sessions
because cua-driver 0.8 loses its persistent AT-SPI listener during longer
sweeps; both groups retain before/after snapshots for every action and must
pass. The other workflows also use fresh D-Bus/AT-SPI sessions so one stale
accessibility bus cannot contaminate a later acceptance result.

For an iterative keyboard-only retry, run
`CUA_E2E_ONLY=populated-library scripts/cua-e2e/run.sh`; use
`CUA_E2E_ONLY=library-doctor scripts/cua-e2e/run.sh` for the Doctor workflow,
or `CUA_E2E_ONLY=track-sort-playing-marker scripts/cua-e2e/run.sh` for the
repeated-sort regression, `CUA_E2E_ONLY=source-modules` for the module switches, or
`CUA_E2E_ONLY=source-podcasts` for the offline feed workflow.
The default remains the complete matrix.

This headless X11 run proves accessibility exposure, input delivery, widget
state transitions, screenshots, and clean logs. Native Wayland rendering,
pointer feel, portals, media keys, audible playback, and compositor-specific
behavior remain release-manual checks.
