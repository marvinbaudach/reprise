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

The runner creates a private D-Bus session, AT-SPI bus, Xvfb display, Openbox
window manager, CUA daemon, XDG profile, fake audio sink, and copied FLAC
fixtures. It exercises two public workflows:

1. a fresh profile exposes the first-run wizard; activating `Skip for Now`
   reveals the `No music yet` empty-library state;
2. a populated profile scans two copied fixtures; typing into the accessible
   `Search all fields` control reveals the `No results` state.

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
- `CUA_E2E_QUIT_DELAY_SECS=15` changes the app's clean smoke-quit timeout.

The helper contract has a deterministic fake-driver test:

```sh
scripts/tests/cua-e2e.sh
```

This headless X11 run proves accessibility exposure, input delivery, widget
state transitions, screenshots, and clean logs. Native Wayland rendering,
pointer feel, portals, media keys, audible playback, and compositor-specific
behavior remain release-manual checks.
