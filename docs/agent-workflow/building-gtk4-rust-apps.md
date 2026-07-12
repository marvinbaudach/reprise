# Building GTK4 Rust Apps — pitfalls (project-local skill)

Hard-won pitfalls, each one a real bug caught in Reprise. gtk4-rs 0.11 / GTK 4.22 era.
Applies to `crates/reprise-gnome` and `crates/reprise-platform-linux`. Both agents
(Codex, Claude) should read this before touching GTK/GStreamer/MPRIS code.

## Widget & input traps (ColumnView cells)

| Trap | Fix |
|---|---|
| `GestureClick` on a plain `Box`/`Image` inside a ColumnView cell may never fire (row machinery wins) | Use real `gtk::Button`s (`has_frame(false)`, css `flat`) per interactive element — reliable delivery + keyboard a11y for free |
| `starred-symbolic` vs `non-starred-symbolic` look nearly IDENTICAL on some icon themes (Papirus) — stateful UI reads wrong | Render state with text glyphs (★/☆) or app-shipped resources, never theme symbolic pairs whose *difference* carries the meaning |
| `GtkEventBox` does not exist in GTK4 (GTK3-only); coordinate-math click mapping on a Box is fragile | Buttons per element; if you must map coordinates, remember RTL mirroring |
| `GtkRange`'s own click gesture runs at BUBBLE phase and mutates the value synchronously on trough-press BEFORE your same-phase gesture | Put your guard gesture at `PropagationPhase::Capture`; never claim the sequence |
| A drag/press guard flag can stick (release/cancel not delivered on real stacks) | Self-heal: when consuming the flag, cross-check `gesture.is_active()`; also reset on state changes |
| Factory `bind` closures capturing row data: per-`setup` closures serve stale rows after recycling | Rebuild/replace the callback on every `bind`; for async per-row work use a per-cell **generation token** (see `cover_loader.rs`) so a late result for a recycled row is dropped |
| `adw::ToolbarView::remove` is NOT a safe no-op on an unattached widget — emits `Adwaita-CRITICAL: tried to remove non-child` | Guard reparents: `if widget.parent().is_some() { toolbar_view.remove(&widget); }` |

## RefCell discipline (the #1 recurring panic class)

A `Ref`/`RefMut` temporary lives to the END OF THE STATEMENT. Any GTK/callback call in that
same statement can synchronously re-enter (rebind, items_changed → factory bind → `borrow_mut`)
→ `BorrowMutError` panic at runtime, invisible to the compiler.

```rust
// PANICS under reentrancy: Ref alive across the call
(self.imp().on_changed.borrow())(value);

// SAFE: clone/copy out in its own statement, borrow drops, then call
let cb = self.imp().on_changed.borrow().clone();   // Option<Rc<dyn Fn>>
if let Some(cb) = cb { cb(value); }
```

Store callbacks as `RefCell<Option<Rc<dyn Fn(..)>>>` (Rc, not Box) so clone-out is cheap.
Review your own diffs for `borrow()` inside a call argument.

## Async off-thread work (covers, downloads)

- `gdk::Texture` is NOT `Send` — build it on the main thread AFTER an `.await`, never inside
  a `gio::spawn_blocking` closure. Return a `PathBuf`/bytes from the worker, wrap into a
  texture on the main loop.
- Decode/resize/network are slow — run them via `gio::spawn_blocking` (thumbnails) or a
  dedicated worker thread (downloads), never on the main loop.

## GStreamer

- A failed `set_state(Playing)` can permanently wedge that `playbin3` instance for ALL later
  URIs. Recover by rebuilding the playbin (reapply sink overrides; reuse the shared
  `Arc<Mutex<Element>>` so ticker threads pick it up — do NOT spawn a second ticker) and retry
  ONCE. Transition the old element to `Null` before dropping it.
- Headless/test audio: env-override the sink (`REPRISE_AUDIO_SINK=fakesink`), set `sync=true`
  on it or "playback" finishes in milliseconds with no position ticks.
- Keep ONE `glib` crate version across gtk4/gstreamer deps (check Cargo.lock).

## MPRIS / GtkApplication

- `org.mpris.MediaPlayer2` requires `HasTrackList`, `SupportedUriSchemes`, `SupportedMimeTypes`;
  Player requires `Rate/MinimumRate/MaximumRate/Volume`. `mpris:length` is MICROseconds.
  `Position` is exempt from PropertiesChanged — emit `Seeked` instead.
- Bus loss / name taken must never be fatal: warn + run without MPRIS.
- GNOME Shell (`js/ui/mpris.js`) HIDES a player the moment `CanPlay` flips false — keep
  `CanPlay`/`CanPause` intrinsically true whenever a track is loaded (do NOT tie them to
  playing/paused). Symptom of getting this wrong: controls vanish when playback starts.
- GtkApplication is single-instance via the session bus: guard `connect_activate` with
  `if let Some(w) = app.active_window() { w.present(); return; }`.

## SQLite position lists (playlists/queues)

- SQLite checks PKs IMMEDIATELY: renumbering `(list_id, position)` rows in place can transiently
  collide. Safe: delete-all-then-bulk-reinsert in one transaction, or prove your update order
  can't collide.
- LIKE escaping: escape `\` FIRST, then `%`/`_`, and declare `ESCAPE '\'`. Write the regression
  test so it FAILS on the unescaped version (assert the exact escaped param).
- `busy_timeout` + WAL for UI-thread + worker-connection topologies.

## Verification (see also AGENTS.md — this is safety-critical)

Fully isolate every agent-driven run — this project's real DB got clobbered twice by
under-isolated smokes:

```
dbus-run-session -- xvfb-run -a env \
  XDG_DATA_HOME=$(mktemp -d) XDG_CACHE_HOME=$(mktemp -d) \
  GDK_BACKEND=x11 WAYLAND_DISPLAY= REPRISE_AUDIO_SINK=fakesink \
  <REPRISE_SMOKE_* hooks> cargo run
```

- **own display** (xvfb) + **own data/cache** (`XDG_DATA_HOME`/`XDG_CACHE_HOME`) + **own bus**
  (dbus-run-session). On a Wayland host `DISPLAY=:N` is NOT enough — GTK4 prefers Wayland, so
  force `GDK_BACKEND=x11` and unset `WAYLAND_DISPLAY` or the app opens a window on the real
  desktop. A leaked bus name hijacks the user's real launches; a non-scratch data home writes
  the user's real DB.
- Build permanent `REPRISE_SMOKE_*` env hooks into the app (scan dir, auto-activate, open a
  view, toggle a setting, auto-quit-after-N-s) so flows are drivable headless.
- What headless CANNOT verify: pointer gestures, icon-theme rendering, lock-screen, actual
  cover rendering, a real network download — list those for a human tester, never claim them done.
