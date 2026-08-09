# Zurück aus einer gefilterten Ansicht springt kurz an die Spitze

## Symptom

Interpretenlink in der Trackliste anklicken → gefilterte Interpretenansicht →
dort etwas abspielen → Zurück (Alt+Left / Maustaste 8). Die Bibliothek
erscheint für einen Moment **an ihrer Spitze** und rückt dann an die richtige
Stelle. Der Endzustand stimmt; sichtbar ist der Ruckler dorthin.

## Bewiesene Ursache

`apply_scroll_anchor_if_allocated`
(`crates/reprise-gnome/src/ui/track_list/track_list_reload.rs:396`) berechnet
die Zeilenhöhe so:

```rust
let (upper, page) = (adjustment.upper(), adjustment.page_size());
let height = upper / current_ids.len() as f64;
```

Die Funktion wird laut ihrem eigenen Kommentar bewusst **synchron direkt nach
dem Modelltausch** aufgerufen, „while the old allocation is still usable" —
`upper` gehört also noch zur **verlassenen** Ansicht, `current_ids.len()` ist
schon die **neue** Zeilenzahl. Alte Gesamthöhe geteilt durch neue Zeilenzahl
ergibt eine viel zu kleine Zeilenhöhe, und das Scroll-Ziel landet entsprechend
weit oben.

Gemessen (Display-Test, 2276 Zeilen Bibliothek, 22 Zeilen Interpretenansicht,
Anker auf Zeile 1100, echte Zeilenhöhe 34 px):

```
height = 748 / 2276      = 0,329 px   (statt 34)
target = 1100 × 0,329    = 361,5      (statt 37400)
erster gemessener Wert   = 361,5114
```

Der Fehler fällt nur bei **Längenwechsel** der Liste auf. Bei gleichbleibender
Zeilenzahl (Rating, Tag-Save, Reload derselben Ansicht) kürzt sich der Fehler
weg — deshalb ist er nie aufgefallen.

Verschärfend: die Funktion meldet `true`, sobald sie *irgendetwas* geschrieben
hat. Der Aufrufer (`schedule_scroll_restore`, Zeile ~385) bricht seine
Idle-Nachbesserung dann ab — die Korrektur auf den richtigen Wert kommt nur
noch von den anderen Schreibern.

Derselbe Rechenweg steht ein zweites Mal in
`view_state_memory::restore_scroll_when_ready`
(`crates/reprise-gnome/src/ui/track_list/view_state_memory.rs:186`, Zeile 201).

## Lösung

Die Zeilenhöhe darf nicht aus einer möglicherweise veralteten `upper`
abgeleitet werden. Sie ist eine Eigenschaft der Darstellungsdichte, nicht der
Liste, und ändert sich beim Ansichtswechsel nicht.

1. **Zeilenhöhe zwischenspeichern.** `Shared` bekommt ein
   `last_row_height: Cell<f64>` (Start 0.0). Eine kleine Hilfsfunktion misst
   `upper / n_items` und schreibt den Wert nur, wenn er plausibel ist
   (`n_items > 0`, `upper > 0`). Aufgerufen wird sie **vor** dem Modelltausch
   in `reload_with_anchor_and_viewport` / `run_query` und immer dann, wenn eine
   Restore-Runde eine konsistente Geometrie vorfindet.

2. **Beide Restore-Pfade benutzen den gespeicherten Wert** statt
   `upper / current_ids.len()`:
   - `track_list_reload::apply_scroll_anchor_if_allocated`
   - `view_state_memory::restore_scroll_when_ready`
   Ist noch kein Wert gespeichert (allererster Aufbau), bleibt die bisherige
   Ableitung als Rückfallebene.

3. **Bereitschaft prüfen statt blind schreiben.** Geschrieben wird erst, wenn
   das Adjustment zur neuen Liste passt:
   `(upper - current_ids.len() as f64 * height).abs() <= height`.
   Trifft das nicht zu, gibt die Funktion `false` zurück — die vorhandene
   Idle-Retry-Kette läuft dann weiter, statt einen falschen Wert festzuschreiben
   und abzubrechen. Das bereits vorhandene `column_view.scroll_to(position, …)`
   übernimmt in der Zwischenzeit die grobe Positionierung; GTK löst die
   Zeilenposition selbst auf und braucht keine Pixelrechnung.

Nicht anfassen: die Anker-Semantik (Track-ID + Offset), die History, die
`AdjustmentHold`-Logik und die Reihenfolge in `route_to_place`.

## Verifikation

Der Regressionstest liegt bereits im Branch:
`crates/reprise-gnome/src/ui/track_list/navback_anchor_display_tests.rs`
(eingehängt am Ende von `track_list_reload.rs`).

Vier Varianten desselben Weges (schlicht / abweichende Sortierung / Tabelle
hatte Fokus / alles zusammen). Jede fährt Bibliothek → Interpretenansicht →
Wiedergabe → Zurück und tastet den Scrollwert alle 8 ms ab. Geprüft wird
**nicht nur der Endwert** (der war immer korrekt), sondern dass **kein
Zwischenwert** über die Ankerzeile hinaus nach oben rutscht.

Pflicht:
- Alle vier Tests grün. Jeder Test in einem **eigenen Prozess** — GTK
  verweigert zwei Initialisierungen im selben Testbinary:
  `xvfb-run -a cargo test -p reprise-gnome --bins -- --ignored --exact <voller::testpfad> --nocapture`
- **Gegenprobe:** mit zurückgedrehtem Fix müssen alle vier rot sein (sie sind
  es aktuell, mit `min=361,5` statt `37400`).
- Die bestehenden Viewport-Tests dürfen nicht kippen, insbesondere
  `reveal_track_display_tests`, `start_restore_tests`,
  `search_viewport_display_tests`, `glide_reload_display_tests`,
  `context_menu_scroll_display_tests`.

## Separater Befund (nicht Teil dieses Fixes)

`build_track_ids_query_browsed` (`crates/reprise-core/src/queries/clauses.rs:420`)
hängt `LIMIT 10000` (`QUEUE_LIMIT`) an. Ab 10 000 Tracks liefert
`current_view_ids()` also eine gekürzte Liste, während das Modell alle Zeilen
führt — nachgemessen: 10 000 statt 20 000. Damit sind Zeilenhöhe, Ankerposition
und Selektionswiederherstellung jenseits dieser Grenze falsch bzw. finden den
Anker gar nicht. Für die aktuelle Bibliothek (2276 Tracks) ohne Wirkung, aber
ein echter latenter Fehler; eigener Vorgang.
