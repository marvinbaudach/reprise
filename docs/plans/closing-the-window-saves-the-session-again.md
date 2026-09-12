---
slug: closing-the-window-saves-the-session-again
worktree: /home/marvin/Projects/reprise-closing-the-window-saves-the-session-again
branch: feature/closing-the-window-saves-the-session-again
phase: reviewed
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
eight-variant truth table — is in
`docs/plans/closing-the-window-saves-the-session-again.EVIDENCE.md`.

## Root cause

`crates/reprise-gnome/src/ui/compact/minimal_view.rs:178-212` —
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
2. This handler — connected at `ui/window/window.rs:404` via `build_mode`, i.e.
   **before** the session save wired at line 475 — destroys the never-realized
   compact window.
3. Inside that destroy GTK emits `window-removed` on `AdwApplication`; its
   handler calls `gdk_surface_get_display(gtk_native_get_surface(compact_window))`.
   The window was never realized, so the surface is `NULL`, the display is
   `NULL`, and the next dereference segfaults.
4. The session save never runs.

Only a **never-realized** `GtkApplicationWindow` triggers this. Hiding a window
keeps it realized, which is why closing *from* the mini player (library window
hidden, `realized=1 visible=0`) is unaffected — measured.

## The fix

Give the compact window a surface before it is destroyed, in the one place that
destroys it.

```rust
if let Some(compact_window) = compact_window_weak.upgrade() {
    // GTK 4.22 reads this window's GdkSurface while it emits
    // ::window-removed. A window that was never presented has none, so
    // realize it first — destroying it unrealized segfaults under Wayland
    // (X11 survives it). See the EVIDENCE file next to this plan.
    if !compact_window.is_realized() {
        compact_window.realize();
    }
    compact_window.destroy();
}
```

`is_realized()` is already used in this codebase (`ui/link_activation.rs:193`);
gtk4-rs is 0.11.4 and exposes `WidgetExt::realize()`.

Rejected during the grill, with reasons:

- **Realize at build time** — same effect, but creates a `GdkSurface` at
  startup for a window most users never open, in a codebase that fought
  startup down from 4.0 s to 0.5 s (#387).
- **Lazy creation on first `enter_compact()`** — architecturally cleanest, but
  `compact_mode_controls::install` (line 130) and `ui/window/window.rs:411`
  both need the window during startup, so it needs a creation hook across three
  files. A refactor, not a bug fix; worth a follow-up issue.
- **A second save path on `GApplication::shutdown`** — at that point the
  widgets are destroyed, so it would write empty geometry and a missing scroll
  anchor over a good session.

## Tasks

1. **Fix the crash** in `crates/reprise-gnome/src/ui/compact/minimal_view.rs`:
   realize-if-unrealized before `destroy()`, with the comment above. Keep the
   `closing` guard exactly as it is.
2. **Document the ordering hazard** — a short comment at the `build_mode` call
   in `ui/window/window.rs:404` and at the session-save wiring: the session
   save is the *last* `close-request` handler, so anything connected before it
   takes the session down with it when it dies. Comments only; no behaviour
   change.
3. **Regression test** in the existing test module of
   `crates/reprise-gnome/src/ui/compact/compact_mode_controls.rs`, next to the
   test that closes from compact mode. Stay in library mode (never toggle), so
   the compact window is unrealized, and assert the invariant the fix creates:

   ```rust
   let realized = Rc::new(Cell::new(false));
   mode.compact_window().unwrap().connect_realize({
       let realized = realized.clone();
       move |_| realized.set(true)
   });
   window.close();
   wait_for_window_state("session saved", || saved.get());
   assert!(realized.get(), "the compact window was destroyed without a surface");
   ```

   This fails without the fix **under xvfb too**, because it asserts the
   realize, not the crash. Follow the existing test's shape for setup and the
   `wait_for_window_state` helper.
4. **Write the evidence file**
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
2. close it via `win.close` over D-Bus (`close-timing.sh` from the diagnosis) —
   expect **~0.1 s**, not 0.9–2 s;
3. `coredumpctl list --since -5min` — expect **no new entry**;
4. `application session saved` present in the log;
5. `clean_exit` non-`None` in `ui.session.v1` afterwards.

Run the same five steps a second time after toggling into compact mode and
back, so the realized path stays covered.

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

**No cut.** Tasks 1–3 all change code in `crates/reprise-gnome/src/ui/compact/`
(two files, one of them shared by tasks 1 and 3), task 2 adds comments in
`ui/window/window.rs`, and task 4 writes a doc file. There is no disjoint file
group worth a worktree, the whole change is on the order of thirty lines, and
the acceptance is a single manual Wayland run that cannot be split either.
Cutting this into strands would cost more in setup than the change takes.

Merge order: n/a. Post-merge cross-checks: n/a.
