---
slug: delete-responsiveness
worktree: /home/marvin/Projects/reprise-delete-perf
branch: perf/delete-responsiveness
phase: refactored
codex_session:
created: 2026-08-08
---
# Löschen soll sich sofort anfühlen

## Das Problem, wie der Nutzer es erlebt

Nach der Bestätigung im Löschdialog bleibt das Modal spürbar stehen, und danach
zuckt die Bibliotheksansicht.

## Was gemessen wurde (2026-08-08)

Isolierte Instanz auf einer Kopie der echten Bibliothek (2340 Tracks, 241 MB DB),
Xvfb :97, eigener D-Bus. Zeitachse aus den `tracing`-Logs, Bildschirm per
x11grab bei 50 fps mit Hover-Zeitanker, Hauptthread per `eu-stack` gesampelt
(ptrace über `LD_PRELOAD`-Opt-in freigegeben).

| Messgröße | Wert |
| --- | --- |
| Klick → Modal sichtbar verschwunden | **1613 ms** |
| Klick → Worker fertig, `finish()` beginnt | 97–198 ms |
| `finish()` blockiert den UI-Thread | 309–998 ms (12 Läufe, Median ~360) |
| Dieselbe Blockade bei **50** statt 1 Track | 501 ms — **praktisch unverändert** |
| Zustand des Hauptthreads währenddessen | 92 % `R`, 78 % CPU — es rechnet, es wartet nicht |
| Nachruckler durch den Watcher | ~1,7 s, etwa 2 s nach dem Löschen |

Der Dialog verschwindet **in einem einzigen Frame**, ohne Ausblendanimation: die
Hauptschleife ist während der gesamten Animationsdauer besetzt, und wenn sie
wieder frei wird, ist die Animation bereits abgelaufen.

Die Kosten sind **Fixkosten pro Löschvorgang**, nicht Kosten pro Track. Das ist
der Kern: nach dem Löschen wird die gesamte sichtbare Zeilenmenge neu gebaut.

Die Datenbankabfrage selbst ist **nicht** das Problem — zwischen
`query matched` und `delete batch completed` liegen nur 5–50 ms.

## Wo die Zeit wirklich hingeht (11 Stacks aus der Blockade)

### B1 — Ein synchroner X11-Roundtrip pro Stern (5 von 11 Samples)

```
XSync / XIQueryPointer                     ← blockierender Roundtrip
gdk_x11_device_xi2_surface_at_position
gdk_device_get_surface_at_position
gtk_widget_trigger_tooltip_query
gtk_widget_set_tooltip_text
RatingWidget::build_star                   ← rating.rs:277
  ← RatingWidget constructed
  ← SignalListItemFactory setup
  ← gtk_column_view_cell_widget_new
  ← gtk_list_item_manager_ensure_items
```

`build_star` setzt pro Stern einen Tooltip. GTK beantwortet **jedes**
`set_tooltip_text` mit `gtk_widget_trigger_tooltip_query`, das die Zeigerposition
synchron beim Display erfragt. Fünf Sterne × ~40 sichtbare Zeilen ≈ **200
blockierende Roundtrips**, jedes Mal wenn die Zellen neu gebaut werden.

Und genau das passiert beim vollen Model-Swap: `items_changed(0, alt, neu)` lässt
GTK jede Zelle verwerfen und `setup` neu durchlaufen, statt sie zu recyceln.

### B2 — Der Seitenleisten-Rebuild hängt im Löschpfad

```
delete_tracks::finish
 → purge_queue_ids → notify_queue_changed
   → sidebar_rebuild::rebuild                    (19 synchrone Abfragen)
     → count_releases_view → query_complete_history_in
       → artist_news_query::local_library_index  ← Index über die ganze Bibliothek
```

### B3 — Eine zweite Abfrage zusätzlich zur Hauptabfrage

```
delete_tracks::reload_after_catalog_delete
 → reload_with_anchor_and_viewport
   → Shared::current_view_ids
     → queries::query_visible_track_ids_browsed  ← eigener SQLite-Durchlauf
```

### B4 — a11y-D-Bus-Verkehr pro Zeile

```
clear_cover_album_link → AccessibleExtManual::update_property
 → gtk_at_spi_context_state_change → g_dbus_connection_emit_signal
```

Eine D-Bus-Nachricht je Zeile beim Ent-Binden.

### B5 — Warnungssturm als Dauerlast

`link_activation::present()` (link_activation.rs:77) setzt bei jedem Zeilen-Bind
`set_accessible_role(Link)`. ColumnView recycelt Zeilenwidgets, GTK lässt die
Rolle nur einmal setzen und warnt ab dem zweiten Mal. Aus dem Journal der echten
Instanz: **5408 Warnungen in zwei Minuten, 812 in einem einzigen Reload**. Jede
ist ein synchroner Journal-Write auf dem Hauptthread. Läuft auch beim Scrollen.

### B6 — Der Nachruckler: ein „nichts geändert"-Reconcile, der trotzdem alles neu baut

Move-to-Trash entfernt die Datei, der Watcher meldet sich ~2 s später. Aus dem
Journal der echten Instanz, 5 von 5 Löschvorgängen:

```
12:24:07.023  delete batch completed              (37 Tracks sichtbar, Filter "elect")
12:24:09.234  watcher: reconciling UI …  added=0 updated=0 moved=0 vanished=0
12:24:10.221  query matched 2330 tracks  filter=       ← Filter weg, ganze Bibliothek
12:24:10.938  query matched 37 tracks    filter=elect  ← und zurück
```

Alle Änderungszähler sind null. Der Guard `reconcile_changes_rows`
(scan_watcher.rs:26) überspringt `track_list.reload()` korrekt — aber
`sidebar.refresh("watcher reconcile")` daneben (scan_watcher.rs:93) läuft
**ungegated** durch und zieht den vollen Rebuild plus Reload nach sich.

## Aufgaben

Reihenfolge nach Wirkung pro Risiko. Nach **jeder** Aufgabe messen (siehe
Verifikation), damit der Beitrag jeder einzelnen belegt ist.

### T1 — Tooltips ohne Display-Roundtrip (klein, größter Einzelgewinn)

`RatingWidget::build_star` (rating.rs:277) darf beim Zellenaufbau keinen
Roundtrip auslösen. Der GTK-konforme Weg ist der bedarfsgesteuerte Tooltip:
`set_has_tooltip(true)` plus ein `query-tooltip`-Handler, der den Text erst
liefert, wenn der Tooltip wirklich gebraucht wird — statt `set_tooltip_text`
beim Aufbau.

**Alle** `set_tooltip_text`-Aufrufe im Zeilenaufbau gleich mitnehmen; sie stehen
in `rating.rs` (2), `track_list_columns.rs` (2) und
`track_list_title_column.rs` (1). Für jeden prüfen, ob er im setup/bind-Pfad
liegt — nur die dort liegenden umstellen.

Der sichtbare Tooltip-Text und sein Timing dürfen sich nicht ändern.

### T2 — Der Watcher rebaut die Seitenleiste nur bei echter Änderung (klein)

In `scan_watcher.rs` den `sidebar.refresh("watcher reconcile")` unter denselben
`changes_rows`-Guard stellen, der schon `track_list.reload()` schützt.

**Falle, die zu prüfen ist:** die Zähler dürfen nicht veralten. Nach dem Löschen
werden sie ohnehin aktualisiert — über den in B2 gefundenen Pfad
(`finish` → `purge_queue_ids` → `notify_queue_changed` → `sidebar_rebuild`).
Der Watcher-Rebuild danach ist also redundant. Vor dem Gaten belegen, dass jeder
Weg, der die Zähler ändert, seinen eigenen Refresh hat — sonst zeigt die
Seitenleiste veraltete Zahlen. Ein Test soll das festhalten.

### T3 — Zeilen entfernen statt die Liste neu bauen (der Kern)

`reload_after_catalog_delete` (delete_tracks.rs:258) ruft heute `reload()`, also
einen vollen Model-Swap. Stattdessen den bereits etablierten Pfad für minimale
Bereiche nutzen: `track_list_model_change::changed_range` mit
`set_query_browsed_ai_changed`, so wie `tag_mutation_refresh` es tut —
inklusive des dortigen Generations-Guards.

Damit sieht GTK ein `items_changed(pos, n, 0)` über die gelöschten Zeilen und
**recycelt** die übrigen Zellen, statt sie neu aufzubauen. Das lässt B1 und B4
für die unveränderten Zeilen entfallen.

**Fallen, die dieser Bereich dokumentiert hat** — alle drei müssen im Ergebnis
geprüft sein:

- `items_changed` setzt die Fokuszeile auf 0; ein sich schließender Dialog macht
  das sichtbar (die Ursache hinter #44, #48, #51, PR #209). Die vorhandene
  `deletion_focus_position`-Logik in `delete_tracks.rs` ist der Ort, an dem der
  Fokus bewusst gesetzt wird — sie muss weiter greifen.
- Teil-Deltas re-sectionieren nur die abgedeckten Zeilen. Wenn die Ansicht
  Sections trägt, muss der Bereich sie mit abdecken.
- Die Scrollposition darf sich nicht verschieben. `capture_reload_anchor` /
  `AdjustmentHold` bleiben zuständig; prüfen, dass der Hold mit einem
  Teil-Delta nicht gegen den Anker schreibt.

### T4 — Die a11y-Rolle nur setzen, wenn sie sich ändert (klein)

`link_activation::present` / `unpresent` (link_activation.rs:77 und :90) sollen
die Rolle nur zuweisen, wenn sie von der aktuellen abweicht. Das beendet den
Warnungssturm aus B5 und die Journal-Writes auf dem Hauptthread.

Ein Regressionstest soll festhalten, dass ein zweimaliges `present` auf demselben
Widget keine Rollenzuweisung mehr auslöst.

### T5 — Den Seitenleisten-Zähler entschärfen (mittel)

`count_releases_view` baut über `local_library_index` einen Index über die ganze
Bibliothek, nur um eine Zahl zu bilden (B2). Das gehört nicht synchron in jeden
Rebuild. Entweder billiger abfragen oder das Ergebnis über die Lebensdauer eines
Rebuilds hinaus halten und nur bei Änderung neu bilden.

Diese Aufgabe erst angehen, wenn T1–T4 gemessen sind: möglicherweise ist der
Rebuild danach selten genug, dass sie sich erübrigt.

### T6 — Nur falls nach T1–T5 noch nötig: das Modal entkoppeln

Wenn `finish()` immer noch lange genug blockiert, um die Schließanimation zu
verschlucken, die Arbeit hinter die Animation legen. Reihenfolge von Toast,
Fokus-Wiederherstellung und Reload dabei bewusst neu sortieren.

Erst messen, dann entscheiden — nicht vorsorglich bauen.

## Verifikation

Grundregel aus der Historie dieses Bereichs: **nur per Timer messen.**
Frame-Sampling liefert hier null Samples und sieht dann fälschlich grün aus. Und
zu jeder Messung die **Gegenprobe mit deaktiviertem Fix** — sonst ist nicht
belegt, dass die Änderung die Verbesserung verursacht hat.

Zielwerte gegen die Ausgangslage oben:

- Klick → Modal weg: deutlich unter 1613 ms
- `finish()`-Blockade: deutlich unter dem Median von ~360 ms
- Kein zweiter Reload ~2 s nach einem Move-to-Trash
- Der aktive Such-/Browse-Filter überlebt einen Löschvorgang sichtbar
- Keine `RepriseTrackCover`-Warnungen mehr im Journal
- Fokus und Scrollposition nach dem Löschen unverändert (die drei Fallen aus T3)

Der Messaufbau liegt beschrieben in dieser Datei; die Zeitachse lässt sich ohne
jede Instrumentierung aus `journalctl --user` rekonstruieren, weil
`delete batch completed`, `query matched` und `watcher: reconciling` bereits
geloggt werden.

## Nicht Teil dieser Arbeit

- Die 230 MB `track_spectrograms` in der DB (geprüft: der Fremdschlüssel ist
  über `INTEGER PRIMARY KEY` indiziert, das Löschen macht dort keinen Full Scan).
- Der Trash-Vorgang selbst (~20 ms je Datei, läuft off-thread).
- Das Öffnen der Datenbank im Worker (<1 ms).
