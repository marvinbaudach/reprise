# Reaktionsfähigkeit — offene Befunde

Gemessen am 2026-08-07 gegen eine Kopie der echten Bibliothek (1.903 Titel),
Release-Build, isoliertes Profil unter Xvfb, `dbus-run-session`, `fakesink`.
Messstand und Verfahren liegen in
`docs/superpowers/specs/2026-08-07-search-latency-harness.py` und in der Spec
daneben.

**Erledigt und gemessen** (der Vollständigkeit halber, damit die offenen Punkte
im Verhältnis stehen):

| Was | vorher | nachher |
| --- | --- | --- |
| Tippen → Suchergebnis | 344 ms | 141 ms |
| Suche leeren → Ergebnis | 388 ms | 194 ms |
| Settings-Fenster erscheint | 314 ms | 22 ms |
| Tabellenaufbauten pro Start | 5 | 4 |

Was hier steht, ist **nicht** erledigt.

---

## 1. Ein Modelltausch kostet bis zu 367 ms

**Der größte verbleibende Hebel, und die gemeinsame Wurzel der drei gemeldeten
Trägheiten.**

`run_query` samt Modelltausch dauert gemessen 31–367 ms, während die SQL-Abfrage
darunter warm im Sub-Millisekundenbereich liegt. Die Zeit steckt vollständig im
`items_changed(0, old, new)` und dem Neuaufbau des `ColumnView`.

Jede Verbesserung hier wirkt dreifach: beim Start, beim Filtern und bei jedem
Sprung in eine andere Quelle. Umgekehrt ist jede Optimierung, die nur die Zahl
der Aufbauten senkt, eine Umgehung des eigentlichen Problems.

Die Spec zur Suche hat den Delta-Umbau als Option C bewusst verworfen — mit der
Begründung, 1.900 Titel rechtfertigten ihn nicht. Diese Messung ist das
Gegenargument: Nicht die Zeilenzahl kostet, sondern der Tausch selbst.

**Nächster Schritt:** aufschlüsseln, was die 367 ms füllt (Modell-Notify,
Zellen-Neubindung, Sections, Spaltenbreiten-Neuberechnung) — vor jeder Änderung.

## 2. Die Sitzungswiederherstellung baut die Liste zweimal

**~110–150 ms beim Start, belegt durch Aufruferketten mit Symbolen.**

```
  0 ms  reload                                    (Bau der Liste)
213 ms  set_source  <- Seitenleisten-Rückruf      (Aufbau 2)
267 ms  finish_track_source <- restore_browser_place  (Aufbau 3)
```

`route_to_place` wählt zuerst die Seitenleistenzeile, deren Rückruf `set_source`
auslöst und die Liste mit dem *Standardzustand* aufbaut. Unmittelbar danach baut
`restore_browser_place` dieselbe Ansicht erneut, jetzt mit den gespeicherten
Verfeinerungen. Der erste Aufbau ist vollständig verworfene Arbeit.

**Warum nicht sofort gefixt:** Der Startpfad ist durch START-1 und START-3
gebunden und hatte bereits einen bösen Fehler (leere Trackliste durch
reentranten Zugriff in `gtk_widget_allocate`, nur am Start und nur bei tiefem
Anker). Das verlangt einen eigenen, geplanten Durchgang statt eines schnellen
Eingriffs.

**Ansatz:** Die Auswahl der Seitenleistenzeile beim Startrouting so setzen, dass
sie keinen Quellenwechsel auslöst — die richtige Ansicht folgt eine Zeile
später ohnehin. Ein Guard analog zu `restoring_view`.

## 3. Ein vierter Aufbau ~2,9 s nach dem Start

Die Aufruferkette endet in `reload <- reload <- {closure#0}`, also einem der
verdrahteten Rückrufe (`window_runtime_wiring.rs:527`/`775`,
`window_action_wiring.rs:76`). Nach dem Watcher-Fix ist dies der letzte
verbliebene Aufbau ohne erkennbaren Anlass.

**Nächster Schritt:** dieselbe Backtrace-Sonde in `run_query`, aber mit
vollständiger Kette statt der ersten zehn Glieder — die Closure ist im
gekürzten Ausschnitt nicht auflösbar.

## 4. Der Titel-Link aus einer fremden Playlist

Vom Nutzer gemeldet, **nie eigenständig gemessen** — es liegt nur ein
Codebefund vor:

`route_to_place` → `restore_browser_place` fährt in einem Zug
`finish_track_source` (kompletter Listenaufbau, der intern bereits eine
sortierte Gesamtabfrage für die Wiederherstellung braucht) und unmittelbar
danach `current_view_ids()` (`view_session.rs:141`) — **dieselbe sortierte
Gesamtabfrage ein zweites Mal**, für die Positionsauflösung des Reveals.

Dazu kommt der Listenaufbau selbst, für den am 2026-08-04 für Music/Queue/
Playlist 136–182 ms Blockade gemessen wurden.

**Nächster Schritt:** messen wie die Suche — Klick auf den Player-Titel aus
einer fremden Playlist, Zeit bis die Zielansicht steht. Erst danach entscheiden,
ob die doppelte Abfrage der Hauptanteil ist oder nur ein Nebenposten neben
Befund 1.

## 5. Ungeklärt seit dem 2026-08-04: die Wechsel-Regression

Eine Zeitmessung im Wechselpfad (`perf/switch-cost`, `65d3d224e3`) meldete
**2,5–3,3 s im Modellschritt**, während dieselbe Sonde am selben Tag auf
älterem `dev` nur 130–180 ms zeigte. Entweder kam eine Regression herein, oder
die Wanduhr überspannt etwas, das die Hauptschleife weiterlaufen lässt.

Das ist bis heute nicht geklärt und berührt Befund 1 und 4 direkt. Die Klärung
kostet wenig: vorhandene Sonde (`switch_bench.py`) gegen den aktuellen Build,
kein Rebuild, ein Kern für zwei Minuten. **Vorher aus diesen Zahlen nichts
ableiten.**

---

## Reihenfolge, empfohlen

1. **Befund 5 klären** — billig, und er entscheidet, ob Befund 1 ein
   180-Millisekunden- oder ein Drei-Sekunden-Problem ist.
2. **Befund 4 messen** — die einzige vom Nutzer gemeldete Trägheit ohne Zahl.
3. **Befund 1 aufschlüsseln** — der Hebel, der auf alles andere wirkt.
4. **Befund 2 und 3** — je ~150 ms und ~360 ms, aber am Startpfad, also mit
   Bedacht.

Was dabei nicht passieren darf: an den Zahlen drehen, bis sie gefallen. Jede
dieser Änderungen braucht eine Vorher-Messung auf dem eigenen Abzweigpunkt und
eine Gegenprobe mit zurückgedrehter Änderung — sonst misst der Messstand etwas
anderes als die Änderung. Das hat sich am 2026-08-07 zweimal ausgezahlt:
einmal, weil ein Display-Test grün war, ohne etwas zu beweisen, und einmal,
weil eine Vermutung über die Ursache zweimal falsch war.
