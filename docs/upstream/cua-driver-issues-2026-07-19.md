# cua-driver — zwei Upstream-Bugs (2026-07-19)

Projekt: https://github.com/trycua/cua (`libs/cua-driver`), Version **0.8.3**
(Release 2026-07-15, aktuell). Gefunden beim Betrieb von
`scripts/cua-e2e/run.sh`.

**Status: formuliert, noch nicht eingereicht.**

Beide Fundstellen sind öffentlich unberichtet (Suche über Issues und PRs).
Verwandt, aber nicht deckungsgleich:

- [#1936](https://github.com/trycua/cua/issues/1936) /
  [#1938](https://github.com/trycua/cua/pull/1938) — unbegrenzte AT-SPI-Waits,
  wenn das Ziel D-Bus nicht mehr bedient. Behoben über eine **Whitelist**
  deadline-gesicherter Pfade, benachbarte Pfade können also weiterhin
  festhängen. Issue B unten ist vermutlich genau so ein Pfad.
- [#2010](https://github.com/trycua/cua/issues/2010) (offen) — `serve` setzt
  `ScreenReaderEnabled=true` und startet damit Orca, also einen zweiten
  AT-Client auf demselben Baum. Für unseren Harness noch auszuschließen.

## Wichtig für die Einordnung

Der ursprüngliche Anlass — ein 12-minütiger Hang in unserem Harness — war
**kein** Treiberfehler. Die getriebene App (Reprise) ist mit SIGSEGV
gestorben; der Treiber blockierte danach auf einem toten Gegenüber. Der
App-Absturz ist bei uns behoben.

Was bleibt, sind die zwei Punkte unten: der Treiber sollte den Tod seines
Gegenübers als **terminalen Zustand** melden, statt pro Aufruf 120 s zu
verbrennen, und `doctor` sollte nicht abstürzen.

---

## Issue A — `cua-driver doctor` bricht ab: Panic im SIGCHLD-Handler

**Titel:** `doctor` aborts: panic inside `wait_timeout` SIGCHLD handler

**Environment**
- cua-driver 0.8.3, `x86_64-unknown-linux-gnu` (Release-Tarball)
- Manjaro stable, Linux 6.18.38-1-MANJARO

**Repro**
```sh
cua-driver doctor
```
Bricht sporadisch ab — hier fünf identische Core-Dumps an einem Tag
(12:17, 12:22, 12:45, 13:30, 14:31).

**Expected:** `doctor` läuft durch und gibt einen Report aus.

**Actual:** SIGABRT. Stack:
```
doctor::run
  -> ChildExt::wait_timeout
  -> __poll
  -> SIGCHLD-Zustellung
  -> wait_timeout::imp::sigchld_handler   <- panic hier
  -> core::panicking::panic_cannot_unwind
  -> abort
```

Ein Panic in einem Signal-Handler ist unabhängig vom Auslöser problematisch:
Unwinding über eine Signal-Handler-Grenze ist nicht erlaubt, deshalb greift
`panic_cannot_unwind` und ruft `abort`. Der Handler sollte panikfrei und
idealerweise async-signal-safe sein.

---

## Issue B — Daemon blockiert unbegrenzt, wenn die Ziel-App mitten in der Sitzung stirbt

**Titel:** Daemon blocks indefinitely after the target process dies; client
reads 0 bytes for the full timeout

**Environment**
- cua-driver 0.8.3
- Manjaro stable, X11 unter Xvfb
- GTK 4.22.4, libadwaita 1.9.2, at-spi2-core 2.60.5

**Repro**
1. Eine GTK4-App über AT-SPI treiben.
2. Die App mitten in der Sitzung abstürzen lassen (bei uns: SIGSEGV).
3. Weitere Tool-Aufrufe absetzen.

**Expected:** Der Daemon bemerkt, dass das Gegenüber weg ist, und beendet den
Aufruf zügig mit einer klaren Fehlermeldung („target process exited").

**Actual:** Der persistente Listener meldet den Disconnect, danach schreibt
der Daemon **nie** auf den Client-Socket. Jeder Folgeaufruf verbrennt die
vollen 120 s. Der Daemon stürzt dabei **nicht** ab (kein Core-Dump) — er
blockiert.

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

Vermutlich dieselbe Klasse wie #1936/#1938 — ein AT-SPI-Await außerhalb der
`OP_TIMEOUT`-Whitelist. Der Tod des Gegenübers wäre als expliziter terminaler
Zustand besser aufgehoben als hinter einem Deadline-Fallback.

**Nachweisgrenze:** Post-mortem aus Logs und Core-Dumps. Der Per-Run-Socket
(`/tmp/reprise-cua-e2e.CTHmiY`) wurde beim Cleanup entfernt, eine
Live-Nachprobe des blockierten Sockets war danach nicht mehr möglich. Vor dem
Einreichen wäre eine minimale Repro sinnvoll: eine Wegwerf-GTK4-App treiben,
ihr `SIGSEGV` schicken, dann einen `get_window_state`-Aufruf absetzen.

---

## Unsere Seite (nicht upstream)

- **Behoben:** `primary_menu.rs` rief `popup()` auf einem entwurzelten
  `MenuButton` — Compact-Mode hängt den Library-Baum ab, der Weak-Ref lässt
  sich aber weiter upgraden. Guard prüft jetzt `root()`, nicht nur Lebendigkeit.
- **Offen:** `scripts/cua-e2e/lib.sh` hat keine `kill -0`-Lebendprüfung (nur
  `run.sh` hat eine). Deshalb wurden aus dem Absturz zwölf Minuten: sechs
  Aufrufe à 120 s. Eine Prüfung in den `cua_*`-Helfern würde nach einem
  App-Tod sofort abbrechen statt zu warten.
- **Offen:** Elf weitere ungesicherte `popup()`-Stellen sind theoretisch
  derselben Entwurzelung ausgesetzt. Sie hängen an Gesten und Tasten auf
  Widgets **innerhalb** des abgehängten Baums, sind also nach heutigem Stand
  nicht erreichbar — belegt ist kein Pfad dorthin.
