# cua-driver — two upstream bugs (2026-07-19)

Project: https://github.com/trycua/cua (`libs/cua-driver`), version **0.8.3**
(released 2026-07-15, current). Found while running
`scripts/cua-e2e/run.sh`.

**Status: drafted, not yet submitted.**

Both findings are publicly unreported (searched across issues and PRs).
Related, but not identical:

- [#1936](https://github.com/trycua/cua/issues/1936) /
  [#1938](https://github.com/trycua/cua/pull/1938) — unbounded AT-SPI waits
  when the target no longer serves D-Bus. Fixed via a **whitelist** of
  deadline-guarded paths, so neighbouring paths can still hang. Issue B below
  is presumably exactly such a path.
- [#2010](https://github.com/trycua/cua/issues/2010) (open) — `serve` sets
  `ScreenReaderEnabled=true` and thereby starts Orca, i.e. a second AT client
  on the same tree. Still to be ruled out for our harness.

## Important for context

The original trigger — a 12-minute hang in our harness — was **not** a driver
bug. The driven app (Reprise) died with SIGSEGV; the driver then blocked on a
dead peer. The app crash is fixed on our side.

What remains are the two points below: the driver should report the death of
its peer as a **terminal state** instead of burning 120 s per call, and
`doctor` should not crash.

---

## Issue A — `cua-driver doctor` aborts: panic in the SIGCHLD handler

**Title:** `doctor` aborts: panic inside `wait_timeout` SIGCHLD handler

**Environment**
- cua-driver 0.8.3, `x86_64-unknown-linux-gnu` (release tarball)
- Manjaro stable, Linux 6.18.38-1-MANJARO

**Repro**
```sh
cua-driver doctor
```
Aborts sporadically — five identical core dumps on one day here
(12:17, 12:22, 12:45, 13:30, 14:31).

**Expected:** `doctor` runs through and prints a report.

**Actual:** SIGABRT. Stack:
```
doctor::run
  -> ChildExt::wait_timeout
  -> __poll
  -> SIGCHLD delivery
  -> wait_timeout::imp::sigchld_handler   <- panic here
  -> core::panicking::panic_cannot_unwind
  -> abort
```

A panic in a signal handler is problematic regardless of the trigger:
unwinding across a signal handler boundary is not allowed, so
`panic_cannot_unwind` kicks in and calls `abort`. The handler should be
panic-free and ideally async-signal-safe.

---

## Issue B — daemon blocks indefinitely when the target app dies mid-session

**Title:** Daemon blocks indefinitely after the target process dies; client
reads 0 bytes for the full timeout

**Environment**
- cua-driver 0.8.3
- Manjaro stable, X11 under Xvfb
- GTK 4.22.4, libadwaita 1.9.2, at-spi2-core 2.60.5

**Repro**
1. Drive a GTK4 app over AT-SPI.
2. Let the app crash mid-session (SIGSEGV in our case).
3. Issue further tool calls.

**Expected:** The daemon notices that the peer is gone and ends the call
promptly with a clear error message ("target process exited").

**Actual:** The persistent listener reports the disconnect, after which the
daemon **never** writes to the client socket. Every subsequent call burns the
full 120 s. The daemon does **not** crash while doing so (no core dump) — it
blocks.

```
WARN cua_driver: could not activate the persistent AT-SPI listener:
  AT-SPI connect failed: ZBus Error: MethodError(
    OwnedErrorName("org.freedesktop.DBus.Error.NoReply"),
    Some("Remote peer disconnected"),
    Msg { type: Error, serial: 4294967295,
          sender: UniqueName("org.freedesktop.DBus"),
          reply-serial: 14, body: Str, fds: [] })

[cua-driver] WARNING: daemon proxy to
  /tmp/reprise-cua-e2e.CTHmiY/cua-driver.sock failed
  (timed out after 120s waiting for daemon response (received 0 bytes so far));
  running 'get_window_state' in-process.
  State-dependent tools may misbehave.
```

Presumably the same class as #1936/#1938 — an AT-SPI await outside the
`OP_TIMEOUT` whitelist. The death of the peer would be better placed as an
explicit terminal state than behind a deadline fallback.

**Limit of evidence:** post-mortem from logs and core dumps. The per-run socket
(`/tmp/reprise-cua-e2e.CTHmiY`) was removed during cleanup, so a live re-check
of the blocked socket was no longer possible afterwards. Before submitting, a
minimal reproduction would make sense: drive a throwaway GTK4 app, send it
`SIGSEGV`, then issue a `get_window_state` call.

---

## Our side (not upstream)

- **Fixed:** `primary_menu.rs` called `popup()` on an unrooted `MenuButton` —
  compact mode detaches the library tree, but the weak ref can still be
  upgraded. The guard now checks `root()`, not just liveness.
- **Open:** `scripts/cua-e2e/lib.sh` has no `kill -0` liveness check (only
  `run.sh` has one). That is how the crash turned into twelve minutes: six
  calls at 120 s each. A check in the `cua_*` helpers would abort immediately
  after an app death instead of waiting.
- **Open:** eleven further unguarded `popup()` sites are theoretically exposed
  to the same unrooting. They hang off gestures and keys on widgets **inside**
  the detached tree, so as things stand today they are unreachable — no path
  there is demonstrated.
