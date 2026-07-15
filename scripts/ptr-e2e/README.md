# Pointer-level headless E2E harness

`run.sh` drives the real Reprise GTK4 window with real synthetic pointer and
keyboard events (`xdotool`), inside a throwaway Xvfb X server, and checks the
result through the app's own stderr log plus screenshot pixel data.

## Why this exists

The existing test suite drives the app through signal seams — calling
`RatingWidget::click_star_for_test`'s `emit_clicked`, `PlayerController`
methods directly, etc. Those tests verify the app's *logic* once an event has
already reached a handler, but they cannot catch a bug where the event never
arrives at the widget in the first place. That exact class of bug shipped
before: a `GestureClick` on a non-interactive `Box` inside a `GtkColumnView`
cell lost the click to the row's own selection machinery, so star-rating
clicks silently did nothing on a real desktop while every signal-seam test
stayed green (see `src/ui/track_list/rating.rs`'s module doc comment). This harness
injects input the same way a human would, so it can catch that class of bug.

## Usage

```sh
cargo build                 # debug binary, if not already built
scripts/ptr-e2e/run.sh
```

Environment overrides (all optional):

| Variable | Default | Meaning |
|---|---|---|
| `PTR_E2E_PROFILE` | `debug` | `debug` or `release` — which `target/<profile>/reprise` binary to exercise. Must already be built. |
| `PTR_E2E_SCREEN_RES` | `1600x900x24` | Xvfb resolution. Changing this invalidates the hardcoded click coordinates — see "Known limits" below. |
| `PTR_E2E_N_TRACKS` | `5` | Number of copies of the core crate's `sine.flac` fixture scanned into the library. |
| `PTR_E2E_OUT_DIR` | `/tmp/reprise-ptr-e2e` | Where screenshots and the app log are left after the run. Cleared at the start of each run. |
| `PTR_E2E_PREFERENCES_ONLY` | `0` | Set to `1` to run only the Preferences pointer flow, independently of list geometry. |

Exit code is `0` when every check passes, non-zero otherwise. On any exit
(pass, fail, or interrupted) the `cleanup()` trap kills the app, openbox, and
Xvfb, and removes the scratch directory — nothing is left running or on
disk except `PTR_E2E_OUT_DIR`.

## What it does

1. Allocates a fresh Xvfb display via `-displayfd` (never guesses/reuses a
   display number — see "Lessons" in `run.sh`'s header comment for why an
   earlier attempt at this harness failed on that exact point), starts
   `openbox` as the window manager (GTK4 windows never map without one), and
   builds an isolated profile: `dbus-run-session` (own session bus),
   scratch `XDG_DATA_HOME`/`XDG_CACHE_HOME`/`XDG_CONFIG_HOME`, and a
   `gtk-4.0/settings.ini`
   forcing `gtk-icon-theme-name=Papirus-Dark` — the theme under which a
   previous "all stars look filled" bug was only visible. The harness writes
   the private session-bus address into that scratch profile so its MPRIS
   assertions can never address the operator's real session bus.
2. Copies `PTR_E2E_N_TRACKS` copies of `crates/reprise-core/tests/fixtures/sine.flac` into a
   scratch music directory and launches Reprise with `REPRISE_SCAN_DIR`
   pointed at it, `REPRISE_AUDIO_SINK=fakesink`, and `REPRISE_LOG=debug` —
   no smoke-quit hook, so the app stays alive for interaction.
3. Waits (~15s) for a window whose `WM_CLASS` matches `reprise`, matched by
   class rather than title/name (the title is the human-readable app name,
   not reliable for matching), then uses `wmctrl` and the live geometry to
   wait until it has reached the harness's fixed maximized size.
4. Runs six pointer/keyboard flows and asserts on the app's own log:
   - **Star-rating click**: opens row 0's compact Rating button and chooses
     two stars in its popover, then greps for `rating changed` — proof that real
     pointer events reached both controls and the list-cell write-back.
   - **Keyboard context menu**: selects a track row and presses Shift+F10,
     then navigates to Edit tags, proving the selected track's context menu
     and tag editor open without a pointer. It enters an invalid Year and
     verifies Enter rejects it instead of applying or closing the dialog.
   - **Manual Up Next and drag reorder**: adds two tracks through the keyboard
     menu, proves the visible count reaches one and then two, opens Queue,
     holds a real drag over the second row, captures the active insertion
     target, and verifies release applies the reorder. A Library activation
     then establishes context A; private-bus MPRIS Next calls consume manual
     X and Y in reordered order, drive the visible count from two to one to
     zero, and finally resume context B.
   - **Space toggles play/pause**: while that real fakesink playback is live,
     presses Space twice and asserts `state=Paused` then `state=Playing` —
     proof a physical keypress reached the window-level action, not just that
     `PlayerController::toggle_pause()` works when called directly.
   - **Native compact layouts and input policy**: enters Card through the
     Library header, selects Cover, Pill, and Card through the shared menu,
     and checks every bounded window geometry and screenshot. It opens the
     same menu through its visible button, a free-surface right click, and
     Shift+F10; invokes the menu-only Return to Library action; and proves a
     Ctrl+M round trip retains Card. One real downward wheel step on the Card
     metadata changes the private MPRIS volume by exactly five percent while
     leaving paused position unchanged; the same wheel input over seek changes
     neither volume nor position.
   - **Preferences**: opens the real primary-menu item, then drives the
     redesigned vertical settings sidebar (a `.navigation-sidebar` list on the
     left) to switch pages — exercising the Player Bar choice cards, visiting
     every top-level page via its sidebar row, and proving all four Library
     Window switches write the expected values to the scratch SQLite database.
5. Takes a final screenshot and checks it isn't blank/solid-color (pixel
   standard deviation above a threshold), then rejects any application log
   containing GTK/GLib criticals, a Rust panic, or a `RefCell` borrow failure.

## Known limits

- **X11/Xvfb, not native Wayland.** Reprise normally runs under Wayland;
  this harness forces `GDK_BACKEND=x11` (and unsets `WAYLAND_DISPLAY`)
  specifically so it *cannot* accidentally paint a window on a real Wayland
  session — see the "GDK_BACKEND" lesson in `run.sh`. It does not exercise
  any Wayland-specific code path.
- **No audio.** `REPRISE_AUDIO_SINK=fakesink` means playback state
  transitions and GStreamer plumbing are exercised, but nothing is audible
  and no real audio-device integration is covered.
- **Needs the binary already built.** The harness does not build the app;
  `cargo run` will build if the binary is stale, but a cold build will make
  the first run slow. `cargo build` first is recommended.
- **Some coordinates are hardcoded, not queried.** There is no
  accessibility bus wired into this headless session, so widget geometry
  can't be queried — only inferred from a screenshot taken once during
  development, at `PTR_E2E_SCREEN_RES=1600x900x24` with 5 fixture tracks.
  `GtkColumnView`'s non-expanding columns lay out at natural width, so these
  offsets and the Preferences rows are stable for that exact input but will
  drift if the column set, fonts, theme, or resolution change. Header and
  compact-window clicks are calculated relative to the live window geometry,
  but their offsets still assume the current widget arrangement. The shared
  measured values live in `geometry.sh`; re-measure them from the numbered
  screenshots when changing the UI or harness resolution.
- **Single scenario per run.** The harness scans a fixed fixture set each
  time (no persistence across runs); it is not a substitute for exploring
  arbitrary library states, only for proving that specific pointer/keyboard
  flows reach their widgets.
