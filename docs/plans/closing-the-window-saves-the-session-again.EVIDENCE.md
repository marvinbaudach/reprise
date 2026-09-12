# Evidence: the Wayland close crash

Measured 2026-09-11/12 against `~/.local/bin/reprise` 0.1.185, built by
`reprise-nightly-build` from `origin/dev` (2eb70d5152). Environment: Manjaro,
GNOME on Wayland, gtk4 1:4.22.4-1, libadwaita 1:1.9.3-1, mesa 26.2.2.

## Measurement series

Same trigger every time (`org.gtk.Actions.Activate("close")` on
`/io/github/marvinbaudach/Reprise/window/1`); only the backend differs:

| Backend | close → process exit | session saved | core dump |
|---|---|---|---|
| Wayland | 0.82 / 0.90 / 0.94 / 1.11 / 1.15 / 2.07 s | no | yes, SIGSEGV, 27–190 MB |
| `GDK_BACKEND=x11` (D-Bus action *and* real WM close) | 0.11 / 0.13 s | yes | no |

The window disappears after ~4 ms in both cases; under Wayland the process then
lingers for exactly as long as `systemd-coredump` needs. A longer session makes
a bigger core and therefore a longer hang — which is why the delay felt worse
after heavy use.

`coredumpctl` shows the same SIGSEGV for ordinary user sessions, not only for
the measurements: 2026-09-10 23:07 and 2026-09-11 14:47 / 20:28 / 21:58 /
22:21 / 22:58. The first is the day #917 landed.

## Stack

Identical in every core, and identical to the `Gdk-CRITICAL` that immediately
precedes it (`gdk_surface_get_display: assertion 'GDK_IS_SURFACE (surface)'
failed`):

```
#0  libgtk-4.so.1  (+0x3cab92)   <- dereferences the NULL display, rax = 0
#1  libgtk-4.so.1  (+0xadff4)    <- the ::window-removed handler
#2  g_cclosure_marshal_VOID__OBJECTv
#5  g_signal_emit
#6  gtk_window_destroy
#7  gtk4::…::connect_close_request::close_request_trampoline::<adw::ApplicationWindow>
#14 gtk_window_close
```

Disassembly of the faulting function shows the pattern plainly:

```
call   gtk_native_get_surface
call   gdk_surface_get_display      <- receives NULL, returns NULL
…                                   <- dereferences it
```

gdb established identity and ordering:

- `gtk_window_destroy` and `gtk_native_get_surface` receive the **same**
  pointer — the window being destroyed is the one without a surface;
- the signal being emitted is `window-removed` on `AdwApplication`;
- `reprise_core::library::session::save` is never reached;
- the connect-time stack of the destroying handler is
  `MinimalView::new ← compact_mode_controls::build_mode ← window::surface::build`.

## Truth table (C reproducer, no Rust build needed)

Each variant builds an `AdwApplicationWindow` plus a second one, wires two
`close-request` handlers on the first — one mirroring `MinimalView`, one
standing in for the session save — and closes the first window.

| Variant | Result |
|---|---|
| second window never realized, `destroy()` in close-request | **SIGSEGV**, save handler never runs |
| second window presented once, then hidden again | clean, save runs |
| second window still visible | clean, save runs |
| `set_application(None)` instead of `destroy()` | **SIGSEGV** |
| `destroy()` deferred to an idle callback | **SIGSEGV** (save runs first) |
| second window left alone | app never quits |
| second window never created | clean, save runs |
| `realize()` at build time, never presented | clean, save runs, `mapped=0` (nothing shown) |
| **`realize()` inside the close handler, then `destroy()`** | clean, save runs — **the chosen fix** |

So only a **never-realized** `GtkApplicationWindow` kills GTK 4.22.4 on Wayland
when it is removed from the application.

Reverse direction, measured separately after toggling from Library mode:
entering Compact leaves the Library window `realized=1 visible=0`, so the mini
player's `library_window.close()` reaches the session save normally. That
toggle case was never broken. Direct startup in persisted Compact mode was not
part of that measurement; there the Library window has never been presented,
has no surface, and needs the symmetric guard before its close chain begins.

## The reproducer

Build with `gcc repro.c -o repro $(pkg-config --cflags --libs libadwaita-1)`
and run it under Wayland. Exit code 139 is the crash; the `session save`
message shows whether the second handler survived.

```c
/* Reproducer + fix candidates for the Wayland close crash.
 *
 * Mirrors MinimalView::new: a second AdwApplicationWindow (transient, never
 * presented) and a close-request handler on the main window that destroys it,
 * followed by a second handler standing in for wire_close.
 *
 * VAR:
 *   repro        - like origin/dev: destroy() inside close-request
 *   hidden       - second window presented, then hidden (control)
 *   visible      - second window remains visible (control)
 *   unset_app    - set_application(NULL) instead of destroy()
 *   idle_destroy - destroy() deferred to the next idle
 *   alone        - leave the second window alone (app remains open)
 *   lazy         - second window never created (control)
 *   realize_build - realize at build time without presenting (control)
 *   realize_close - realize in close-request, then destroy (chosen fix)
 */
#include <adwaita.h>

static GtkWidget *ghost;
static const char *var;

static gboolean close_cb(gpointer data) {
  g_message("closing main window");
  gtk_window_close(GTK_WINDOW(data));
  return G_SOURCE_REMOVE;
}

static gboolean idle_destroy_cb(gpointer data) {
  if (GTK_IS_WINDOW(data)) {
    g_message("idle: destroying ghost");
    gtk_window_destroy(GTK_WINDOW(data));
  }
  return G_SOURCE_REMOVE;
}

/* Handler 1: stands in for MinimalView::new */
static gboolean on_close_minimal(GtkWindow *w, gpointer u) {
  g_message("handler 1 (MinimalView) ran");
  if (!ghost) return FALSE;
  if (g_str_equal(var, "alone")) {
    return FALSE;
  } else if (g_str_equal(var, "unset_app")) {
    gtk_window_set_application(GTK_WINDOW(ghost), NULL);
  } else if (g_str_equal(var, "idle_destroy")) {
    g_idle_add(idle_destroy_cb, ghost);
  } else {
    if (g_str_equal(var, "realize_close")) gtk_widget_realize(ghost);
    gtk_window_destroy(GTK_WINDOW(ghost));
  }
  return FALSE;
}

/* Handler 2: stands in for wire_close (the session save) */
static gboolean on_close_save(GtkWindow *w, gpointer u) {
  g_message("handler 2 (session save) RAN - session would be persisted");
  return FALSE;
}

static void activate(GtkApplication *app, gpointer u) {
  GtkWidget *win = adw_application_window_new(app);
  gtk_window_set_default_size(GTK_WINDOW(win), 900, 600);
  adw_application_window_set_content(ADW_APPLICATION_WINDOW(win),
                                     gtk_label_new("library window"));

  if (!g_str_equal(var, "lazy")) {
    ghost = adw_application_window_new(app);
    gtk_window_set_transient_for(GTK_WINDOW(ghost), GTK_WINDOW(win));
    gtk_window_set_decorated(GTK_WINDOW(ghost), TRUE);
    adw_application_window_set_content(ADW_APPLICATION_WINDOW(ghost),
                                       gtk_label_new("compact window"));
    if (g_str_equal(var, "hidden") || g_str_equal(var, "visible")) {
      gtk_window_present(GTK_WINDOW(ghost));
    }
    if (g_str_equal(var, "hidden")) gtk_widget_set_visible(ghost, FALSE);
    if (g_str_equal(var, "realize_build")) gtk_widget_realize(ghost);
  }

  g_signal_connect(win, "close-request", G_CALLBACK(on_close_minimal), NULL);
  g_signal_connect(win, "close-request", G_CALLBACK(on_close_save), NULL);

  gtk_window_present(GTK_WINDOW(win));
  g_timeout_add_seconds(3, close_cb, win);
}

int main(int argc, char **argv) {
  var = g_getenv("VAR");
  if (!var) var = "repro";
  AdwApplication *app = adw_application_new("io.github.marvinbaudach.CloseRepro",
                                            G_APPLICATION_NON_UNIQUE);
  g_signal_connect(app, "activate", G_CALLBACK(activate), NULL);
  int status = g_application_run(G_APPLICATION(app), argc, argv);
  g_message("g_application_run returned %d", status);
  return status;
}
```

The chosen fix is the printed source's `VAR=realize_close` branch, equivalent
to:

```c
if (!gtk_widget_get_realized(ghost)) gtk_widget_realize(ghost);
gtk_window_destroy(GTK_WINDOW(ghost));
```

which runs clean (`realized=1 mapped=0` — nothing becomes visible) and lets
handler 2 run.

## How the measurements were taken

```bash
# launch detached under Wayland, wait for startup, then close over D-Bus
binary=$(readlink -f ~/.local/bin/reprise)
REPRISE_LOG=info setsid nohup "$binary" > run.log 2>&1 &
for _ in {1..150}; do
  PID=$(pgrep -n -f -x -- "$binary" || true)
  [[ -n $PID ]] && break
  sleep 0.1
done
[[ -n ${PID:-} ]] || { echo "Reprise process not found" >&2; exit 1; }
sleep 15
T0=$(date +%s.%N)
gdbus call --session --dest io.github.marvinbaudach.Reprise \
  --object-path /io/github/marvinbaudach/Reprise/window/1 \
  --method org.gtk.Actions.Activate "close" "[]" "{}"
while kill -0 "$PID" 2>/dev/null; do sleep 0.02; done
echo "close -> exit: $(echo "$(date +%s.%N) - $T0" | bc)"

coredumpctl list --since -5min | grep reprise     # expect nothing after the fix
grep -c "application session saved" run.log       # expect 1
```

`clean_exit` afterwards:

```bash
sqlite3 ~/.local/share/reprise/reprise.db \
  "select value from settings where key='ui.session.v1';" \
  | python3 -c "import sys,json; print(json.load(sys.stdin).get('clean_exit'))"
```
