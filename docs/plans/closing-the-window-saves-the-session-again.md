---
slug: closing-the-window-saves-the-session-again
worktree: /home/marvin/Projects/reprise-closing-the-window-saves-the-session-again
branch: feature/closing-the-window-saves-the-session-again
phase: refactored
codex_session:
created: 2026-09-12
---
# Closing the window saves the session again

Reprise segfaults on **every** window close under Wayland. The "slow close" a
user notices is `systemd-coredump` writing a 27–190 MB core; the window itself
is gone in ~4 ms. Because the crash happens in the *first* `close-request`
handler, the session-saving handler never runs: window geometry, queue, scroll
anchor and the `clean_exit` marker are lost on every exit — and the next start,
seeing no `clean_exit`, treats the previous run as a crash and scans the
library.

Introduced 2026-09-10 by "The mini-player becomes its own window (#917)".
Every `~/.local/bin/reprise` core dump since then carries the same stack.

Full evidence — gdb stack, measurement series, and the C reproducer with its
nine-variant truth table — is in
`docs/plans/closing-the-window-saves-the-session-again.EVIDENCE.md`.

## Root cause

`crates/reprise-gnome/src/ui/compact/minimal_view.rs:178-226` —
`MinimalView::new` eagerly builds a second `adw::ApplicationWindow` (the mini
player), `transient_for` the library window, and **never presents it**; only
`enter_compact()` calls `present()`. It then wires onto the *library* window:

```rust
let compact_window_weak = compact_window.downgrade();
window.connect_close_request(move |_| {
    if !closing.replace(true) {
        if let Some(compact_window) = compact_window_weak.upgrade() {
            compact_window.destroy();     // never realized -> no GdkSurface
        }
    }
    gtk4::glib::Propagation::Proceed
});
```

On close:

1. `gtk_window_close(library)` emits `close-request`.
2. This handler — connected at `ui/window/window.rs:407` via `build_mode`, i.e.
   **before** the session save wired at `ui/window/window.rs:480` — destroys the never-realized
   compact window.
3. Inside that destroy GTK emits `window-removed` on `AdwApplication`; its
   handler calls `gdk_surface_get_display(gtk_native_get_surface(compact_window))`.
   The window was never realized, so the surface is `NULL`, the display is
   `NULL`, and the next dereference segfaults.
4. The session save never runs.

Only a **never-realized** `GtkApplicationWindow` triggers this. Hiding a window
keeps it realized, which is why closing *from* the mini player after toggling
from Library mode (library window hidden, `realized=1 visible=0`) is unaffected
— measured. Direct startup in persisted Compact mode is the symmetric broken
case: the Library window has never been presented either.

## The fix

Give either window a surface before the close path tears down a window that has
never been presented. The Compact-startup path is the mirror of the Library-
startup path, so both guards deliberately have the same shape.

```rust
if let Some(compact_window) = compact_window_weak.upgrade() {
    // GTK 4.22 reads this window's GdkSurface while it emits
    // ::window-removed. A window that was never presented has none, so
    // realize it first — destroying it unrealized segfaults under Wayland
    // (X11 survives it). See the EVIDENCE file next to this plan.
    if !compact_window.is_realized() {
        gtk4::prelude::WidgetExt::realize(&compact_window);
    }
    compact_window.destroy();
}
```

The fully qualified call is required because both `NativeExt::realize` and
`WidgetExt::realize` apply to `adw::ApplicationWindow` and are imported by the
GTK prelude; `compact_window.realize()` is therefore ambiguous and does not
compile.

The Compact-window close handler applies the identical guard to
`library_window` before calling `library_window.close()`. That is what makes a
persisted Compact-mode startup safe.

Ctrl+Q must also enter this close chain. `GApplication::quit()` returns from the
main loop through `shutdown` without requesting `GtkWindow::close`, so the
`app.quit` action closes the Library window captured when its lifecycle actions
are wired. This deliberately avoids resolving `app.active_window()`: in Compact
mode that would select the mini-player and route quit back through the
never-presented Library window.

`is_realized()` is already used in this codebase (`ui/link_activation.rs:193`);
gtk4-rs is 0.11.4 and exposes `WidgetExt::realize()`.

Rejected during the grill, with reasons:

- **Realize at build time** — same effect, but creates a `GdkSurface` at
  startup for a window most users never open, in a codebase that fought
  startup down from 4.0 s to 0.5 s (#387).
- **Lazy creation on first `enter_compact()`** — architecturally cleanest, but
  `compact_mode_controls::install` (`ui/compact/compact_mode_controls.rs:123`)
  and `ui/window/window.rs:414`
  both need the window during startup, so it needs a creation hook across three
  files. A refactor, not a bug fix; worth a follow-up issue.
- **A second save path on `GApplication::shutdown`** — at that point the
  widgets are destroyed, so it would write empty geometry and a missing scroll
  anchor over a good session.

## Tasks

1. **Fix both startup modes** in
   `crates/reprise-gnome/src/ui/compact/minimal_view.rs`: realize-if-unrealized
   before tearing down either inactive window, with matching comments. Keep the
   `closing` guard exactly as it is.
2. **Document the ordering hazard** — a short comment at the `build_mode` call
   in `ui/window/window.rs:407` and at the session-save wiring
   (`ui/window/window.rs:480`): the session
   save is the *last* `close-request` handler, so anything connected before it
   takes the session down with it when it dies. Comments only; no behaviour
   change.
3. **Regression tests** in the existing test module of
   `crates/reprise-gnome/src/ui/compact/compact_mode_controls.rs`, next to the
   test that closes from compact mode. Cover both direct startup modes without
   toggling first, and persist a real session marker through
   `reprise_core::library::session::save`, then load it back from the test
   database. For Library startup, sample the compact window's surface from its
   `unrealize` signal:

   ```rust
   let surface_at_unrealize = Rc::new(Cell::new(None));
   mode.compact_window().unwrap().connect_unrealize({
       let surface_at_unrealize = surface_at_unrealize.clone();
       move |window| surface_at_unrealize.set(Some(window.surface().is_some()))
   });
   window.close();
   assert_eq!(surface_at_unrealize.get(), Some(true));
   assert_eq!(session::load(&conn).search, "library close survived");
   ```

   For Compact startup, assert that the Library window is initially unrealized,
   then sample `window.surface().is_some()` from its `close-request` chain after
   closing the compact window. Require `Some(true)` and reload the
   `"compact close survived"` marker. Each test fails without its matching
   guard **under xvfb too**, because it asserts the missing surface rather than
   relying on the Wayland crash.
4. **Route Ctrl+Q through window close** in `ui/shortcuts.rs`, with a focused
   display regression in `ui/shortcuts_lifecycle_tests.rs` proving that
   activating `app.quit` emits the Library window's `close-request` chain.
5. **Write the evidence file**
   `docs/plans/closing-the-window-saves-the-session-again.EVIDENCE.md` — it is
   authored in the plan phase and must be carried into the branch together with
   this plan, so `land.sh` commits both.

## Verification

**The display suite cannot see the crash.** Its tests are
`#[ignore = "requires a display; run via xvfb-run"]` and xvfb is X11 — exactly
the arm that does not crash. Task 3's test works around that by asserting the
realize rather than the survival.

*Automated, part of the branch:*
- the new test via the repo's usual display-test invocation (xvfb-run), plus
  the normal gate.

*Manual Wayland acceptance — the actual proof, against a built binary of the
branch, before landing:*

1. launch the binary under Wayland, wait for startup;
2. close it via `win.close` over D-Bus using EVIDENCE.md's own "How the
   measurements were taken" block — expect roughly the 0.1 s X11 baseline,
   rather than the measured 0.82–2.07 s Wayland crash range;
3. `coredumpctl list --since -5min` — expect **no new entry**;
4. `application session saved` present in the log;
5. `clean_exit` non-`None` in `ui.session.v1` afterwards.

Run the same five steps after direct persisted Compact-mode startup and again
after toggling into Compact mode, so both never-presented and already-realized
paths stay covered. Also quit once with Ctrl+Q and require the same session-save
evidence.

*Rollout:* land only after that acceptance run, then trigger
`reprise-nightly-build` by hand — the installed binary comes from the nightly
build of `origin/dev`, so waiting for cron means another night of lost
sessions.

## Risks

- `realize()` on a window that is about to be destroyed is unusual. If a future
  GTK forbids realize during `close-request`, this breaks loudly (crash or
  critical), not silently — which is why the comment must survive.
- The fix does not remove the unused window; startup still builds a mini-player
  widget tree the user may never open. Performance question, not correctness;
  it belongs to the lazy-creation follow-up.
- The underlying defect is in GTK 4.22.4, so other apps can hit it. An upstream
  report is worthwhile but deliberately not part of this branch.

## Parallelität

**No cut.** Tasks 1–4 are a single close-lifecycle correction spread across
the compact controller, window wiring, shortcuts, and their focused regression
tests; task 5 records the evidence. The tasks are small and causally ordered,
and the acceptance is a single manual Wayland run that cannot be split either.
Cutting this into strands would cost more in setup than the change takes.

Merge order: n/a. Post-merge cross-checks: n/a.
