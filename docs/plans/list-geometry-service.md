---
slug: list-geometry-service
worktree: /home/marvin/Projects/reprise/.worktrees/list-geometry-service
branch: feat/list-geometry-service
phase: grilled
codex_session:
created: 2026-08-10
---
# Ein Geometrie-Dienst für die Listenansichten

Gegrillt am 10.08.2026, sieben Beschlüsse. Ersetzt den dritten Abschnitt der
Übergabe `~/Projects/reprise-navback/HANDOFF-navback.md` („Die allgemeine
Lösung"), dessen Prämisse hier widerlegt wird.

## Die Übergabe lag falsch, und zwar messbar

Sie hielt `adjustment.set_upper(...)` in `apply_scroll_anchor_if_allocated` für
einen Hack, der weg muss (Review-Finding 2+3). Drei Versuche, ihn zu entfernen,
waren auf echtem Display rot; in allen dreien überlebte der Zwischenwert **507**,
was die Übergabe einem nicht isolierten Schreiber zuschrieb.

**Messung 1.** Alle sechs Schreiber auf das Adjustment instrumentiert
(`ui/scroll_probe.rs`, env-gegated), der `set_upper`-Block schaltbar gemacht,
derselbe Display-Test zweimal gefahren:

| Lauf | Pre-Seed | Ergebnis |
|---|---|---|
| Kontrolle | aktiv | grün, `first=min=max=37400` |
| Experiment | unterdrückt | rot, `samples(n=65 first=507 min=507 max=37400)` |

Die vollständige Schreiberfolge des **roten** Laufs:

```
anchor             want=76667  from=37400  upper=77384  page=239
hold               want=37400  from=76667  upper=77384  page=239
anchor             want=37400  from=37400  upper=77384  page=239
view_state_restore want=37400  from=37400  upper=77384  page=239
```

**Niemand schreibt 507.** Die Kontrolle zeigt die Herkunft — erste Zeile ihres
Protokolls, vor jedem Schreibzugriff:

```
anchor.set_upper   want=77384  from=748.0  value=507.0  page=241
```

`748 − 241 = 507`, also `upper − page` der **verlassenen** gefilterten Ansicht.
Der Wert steht schon da, wenn die Wiederherstellung beginnt.

### Die Ursache ist ein Fenster, kein Schreiber

Zeitachse eines Modelltauschs (22 → 2276 Zeilen):

1. `items_changed` tauscht das Modell. Das Adjustment beschreibt weiter die alte
   Liste: `upper=748`, `page=241`, `value=507`.
2. Unser Code läuft synchron und will den Anker auf 37400 setzen. Die Range
   erlaubt maximal 507. **Schreiben** heißt: GTK klemmt intern auf 507.
   **Nicht schreiben** heißt: 507 bleibt stehen. Beide Wege zeigen 507.
3. Später alloziert GTK, `upper` wird 77384, erst jetzt ist 37400 schreibbar.
4. Zwischen 2 und 3 liegt mindestens ein Frame — der sichtbare Sprung.

Es gibt genau einen Ausweg: **die Range in Schritt 2 richtig machen.** Genau das
tut der Pre-Seed. Er ist die Lösung, nicht der Fehler. Damit ist auch erklärt,
warum die drei Versuche scheiterten — sie arbeiteten an den Schreibern, während
das Problem war, dass in diesem Fenster *korrekterweise* niemand schreibt.

### Messung 2: eine Widget-Messung rettet den Moment nicht

Der Pre-Seed braucht `n_rows × row_height`. Zweiter Lauf, Sonde am Eingang von
`apply_scroll_anchor_if_allocated`: gibt es dort messbare Zeilen-Widgets?

```
SCROLLROWS row_widgets=207 first=("GtkColumnViewRowWidget", 25, 25) distinct_heights=[0, 25]
… ×4 …
SCROLLROWS row_widgets=207 first=("GtkColumnViewRowWidget", 25, 25) distinct_heights=[0, 25, 34]
```

Die echte Zeilenhöhe ist **34**. 207 Zeilen-Widgets sind da, aber die erste
meldet durchgehend **25**, und der Bestand bleibt gemischt `{0, 25, 34}` — 0 für
unrealisierte, 25 für recycelte Zeilen. „Nimm die erste realisierte Zeile" ergäbe
`upper = 2276 × 25 = 56900` statt 77384 und ein Ziel von 27500 statt 37400.

**Im kritischen Moment ist keine Live-Quelle brauchbar — weder `upper` noch ein
Widget.** Verlässlich ist nur ein Wert, der in einem nachweislich ruhigen Zustand
gemerkt wurde.

### Warum es an fünf Stellen zugleich auftaucht

Die Zeilenhöhe wird unabhängig fünfmal aus `upper / n_rows` zurückgerechnet:

| Stelle | Kontext |
|---|---|
| `track_list_geometry.rs:31` | `row_height()` |
| `track_list_geometry.rs:107` | `row_height_for_restore()`, Fallback ohne Cache |
| `scroll_center.rs:54` | `centered_scroll_value()` |
| `view_state_memory.rs:163` | lokale `row_height()`, **beim Erfassen** des Ankers |
| `track_list_reload.rs:317` | `schedule_centered_scroll_refinement()`, 16-ms-Timer |

Die vierte war in der Übergabe nicht erfasst und ist die unangenehmste: sie sitzt
im Capture, speichert also schon einen falschen Offset — durch keine
Wiederherstellung zu retten. Die fünfte läuft auf einem Timer und überlebt daher
jeden idle-basierten Fix.

Dazu der zirkuläre Bereitschaftstest: `restore_geometry_is_ready` prüft
`upper ≈ n × cached`, wobei `cached` selbst aus `upper` stammt. Er kann nicht
fehlschlagen. Und `should_replace_cached_height` samt `MAX_STALE_DRIFT_IN_ROWS`
existiert nur, um „Dichtewechsel" von „veraltetes Adjustment" zu trennen — eine
Unterscheidung, die sich mit einer vertrauenswürdigen Quelle erübrigt.

## Die Beschlüsse

| # | Frage | Beschluss |
|---|---|---|
| 1 | Höhenquelle | **Zwei Quellen müssen sich einig sein** |
| 2 | Fenster schließen | **Range vorziehen, atomar per `configure()`** |
| 3 | Kaltstart | **Höhe je Dichte persistieren** |
| 4 | Reichweite | **Ansichtsneutral bauen, Trackliste zuerst** |
| 5 | Sektionsköpfe | **Erst messen, dann entscheiden** |
| 6 | Sonde | **Bleibt, env-gegated** |
| 7 | Schnitt | **Zwei Spuren: Mechanik und Messung** |

### 1 · Vertraut ist, worauf sich zwei Quellen einigen

```
ruhig := |upper/n_rows − widget_h| < ε  und  alle gebundenen Zeilen gleich hoch

ruhig       → trusted_h = widget_h        (merken + persistieren)
nicht ruhig → trusted_h bleibt stehen     (benutzen)
kalt        → aus den Einstellungen, sonst Dichte-Token als Untergrenze
```

Das bricht die Zirkularität: die Bereitschaft wird an einer Größe geprüft, die
nicht aus `upper` stammt. Am kritischen Punkt gilt `748/2276 = 0,33 ≠ 25` —
„nicht ruhig", also bleibt der vertraute Wert stehen. Genau richtig.

Die Widget-Messung nimmt die **häufigste Höhe ungleich null** unter den
realisierten Zeilen; der Einigungstest fängt ab, wenn dieser Modalwert (wie im
kritischen Moment die 25) danebenliegt.

Der Dichte-Token ist **keine** Zeilenhöhe: `ROW_MIN_HEIGHT_STANDARD = 28` ist eine
`min-height` auf den Zellinhalt, GTK addiert sein Chrome (gemessen 34). Der
Kommentar bei `ROW_MIN_HEIGHT_COMPACT` warnt zudem, dass der Token gar nicht
binden muss. Er taugt als Untergrenze, nie als Wahrheit.

### 2 · Atomar vorziehen

```rust
// statt set_upper(…) gefolgt von set_value(…) — zwei Emissionen, ein
// Zwischenzustand mit neuer Range und altem Wert
adjustment.configure(target, lower, n_rows as f64 * trusted_h,
                     step_increment, page_increment, page_size);
```

Der Pre-Seed verliert seine Notbremsen `hold.is_some()` und
`has_no_section_headers`; Bedingung ist allein `content_height() == Known(_)`.
Der Kommentar muss sagen, dass dies das Allokationsfenster schließt — nicht, dass
es ein Notbehelf ist. **Review-Finding 2+3 gilt als widerlegt und wird im Plan
mit der Messung zu den Akten gelegt.**

### 3 · Kaltstart aus den Einstellungen

Je Dichte ein Schlüssel, Vorbild `LIST_DENSITY_KEY = "ui.list_density"` in
`crates/reprise-core/src/library/settings.rs:233` samt `get_/set_…_in(conn)`:

```
ui.row_height.comfortable
ui.row_height.standard
ui.row_height.compact
```

Beim Start geladen, beim ersten ruhigen Zustand geprüft und gegebenenfalls
korrigiert, bei Dichtewechsel der betroffene Eintrag verworfen. Ein zwischen zwei
Sitzungen veralteter Wert ist nie schlechter als der heutige Zustand.

### 4 · Ansichtsneutral

`ListGeometry::for_view(&column_view)` — eine Instanz je Ansicht, keine Bindung
an `track_list::Shared`. Umgestellt wird die Trackliste; Radio kommt über das
geteilte `scroll_center` mit. Podcasts, Up-Next und Seitenleiste docken später an.

### 5 · Sektionen ehrlich offen

```
content_height(n_rows, n_sections)
  n_sections == 0            → Known(n × h)
  Kopfhöhe gemessen          → Known(n × h + s × header_h)
  sonst                      → Unknown → kein Pre-Seed
```

`Unknown` ist das heutige Verhalten — nur benannt statt versteckt. Ob der Sprung
in der Queue-Ansicht überhaupt auftritt, ist **nicht gemessen**; das erledigt
Aufgabe M4, und erst danach wird Aufwand gebucht.

### 6 · Die Sonde bleibt

`ui/scroll_probe.rs` bleibt env-gegated im Repo: sie hat in zwei Läufen zwei
Annahmen umgedreht, und ihr Wert liegt in den Display-Tests, wo es keinen
`tracing`-Subscriber gibt. Ohne gesetzte Variable kostet sie ein `var_os` pro
Schreibzugriff.

`REPRISE_NO_SET_UPPER` wird zu **`REPRISE_NO_PRESEED`** und bleibt ebenfalls — als
Gegenprobe. Sie beweist, dass der grüne Test aus dem richtigen Grund grün ist.

## Aufgaben

Zwei Spuren, keine gemeinsame Datei. Die Ownership gehört vor Arbeitsbeginn in
`AGENTS.md`, nicht nur in diesen Plan.

### Spur 1 · Mechanik

Besitzt: `ui/list_geometry.rs` (neu), `ui/scroll_probe.rs`,
`track_list/track_list_geometry.rs` (entfällt), `track_list/track_list_reload.rs`,
`track_list/view_state_memory.rs`, `track_list/reload_restore.rs`,
`ui/scroll_center.rs`, `reprise-core/src/library/settings.rs`.
Sequenziell in dieser Reihenfolge.

- **G1** `list_geometry.rs`: `RowHeight`, `ContentHeight`, Einigungsregel aus
  Beschluss 1, Modalwert-Messung, `content_height`, `is_settled`. Die Arithmetik
  GTK-frei und einzeln testbar halten, Vorbild `reload_restore.rs`.
- **G2** Persistenz je Dichte nach Beschluss 3, inklusive Verwerfen bei
  Dichtewechsel über den bestehenden Eintrittspunkt `apply_list_density`.
- **G3** Alle fünf Divisionsstellen auf den Dienst umstellen — **`view_state_memory.rs:163`
  im Capture zuerst.**
- **G4** Pre-Seed auf `configure()` umstellen, `hold.is_some()` und
  `has_no_section_headers` entfernen, `restore_geometry_is_ready` durch
  `is_settled` ersetzen. `REPRISE_NO_PRESEED` als Gegenprobe verdrahten.
- **G5** `should_replace_cached_height`, `MAX_STALE_DRIFT_IN_ROWS` und
  `row_height_for_restore` ersatzlos entfernen.
- **G6** Warteschleifen ersetzen: `SCROLL_RESTORE_MAX_ATTEMPTS`, der 16-ms-Timer
  in `schedule_centered_scroll_refinement` und die dreifache synchrone Anwendung
  in `schedule_scroll_restore` weichen einem einmaligen `changed`-Abonnement.

### Spur 2 · Messung — startet sofort, unabhängig von Spur 1

Besitzt ausschließlich Testdateien.

- **M1** `delete_follow_display_tests.rs` (nav_10b) zeichnet nur den Endzustand
  auf. Sampler nachrüsten, Muster aus `navback_anchor_display_tests.rs`: 8 ms,
  `MIN_SAMPLES = 20`, Ausgabe `samples(n= first= min= max=)`.
- **M2** Neuer Löschtest für den vom Nutzer gemeldeten Fall, den heute nichts
  abdeckt: **große Liste, weit unten stehend, großer Block gelöscht.** Der
  bestehende Test löscht *eine* Zeile aus 200 (`6800 → 6766`) — der Anker bleibt
  in beiden Ranges gültig, das Fenster ist 34 px breit und unsichtbar.
  Zu prüfende Vorhersage: beim Schrumpfen klemmt der vorgezogene kleinere `upper`
  den Wert **einen Frame früher** nach oben.
- **M3** Gegenprobe-Lauf als Testrezept dokumentieren: derselbe Test mit
  `REPRISE_NO_PRESEED=1` muss **rot** sein.
- **M4** Queue-Ansicht mit Sektionsköpfen, große Liste, Sampler: Sprung nachweisen
  oder ausschließen. Ergebnis entscheidet über Beschluss 5.

## Abnahme

- Die 22 Viewport-Display-Tests grün, **einzeln** gefahren (im Rudel sind sie
  flaky, je Lauf andere Tests rot).
- M1 und M2 grün, mit Zwischenwerten — nicht nur Endzustand.
- M3 rot: ohne Pre-Seed muss der Sprung wieder auftreten.
- `rg -n 'upper\s*/' crates/ -g '!*_tests.rs'` findet heute fünf Codestellen plus
  zwei Doc-Kommentare; danach dürfen nur noch Kommentare übrig sein.
- Volle Nicht-Display-Suite grün, Suitenzahl gegengeprüft (eine fehlende Suite
  hat hier schon einmal einen grünen Lauf vorgetäuscht).

**M3 counterprobe recipe.** Once the geometry-service track provides
`REPRISE_NO_PRESEED`, run the exact M2 test in its own display process:

```text
scratchpad/probe-any.sh large-delete-no-preseed ui::delete_tracks::large_block_display_tests::browse_11_large_block_delete_keeps_the_deep_viewport_off_the_top REPRISE_NO_PRESEED=1
```

The run is accepted only when exactly one test executes and it fails on the
sampled viewport journey. A passing run means the positive test is insensitive
to the pre-seed; a zero-test run means the filter path is wrong.

## Wie hier gemessen wird

Ein Prozess pro Display-Test, frische XDG-Wurzeln, `GDK_BACKEND=x11`, eigene
D-Bus-Session — sonst greift die Testinstanz über den Session-Bus die laufende
App an. Skript: `scratchpad/probe-any.sh <tag> <voller::testpfad> [ENV=…]`.

Der Testpfad enthält das **Elternmodul**: die Tests aus
`navback_anchor_display_tests.rs` heißen
`ui::track_list::track_list_reload::navback_anchor_display_tests::…`. Ein
falscher Pfad meldet `ok. 0 passed; 2335 filtered out` — also immer die Zahl vor
`passed` lesen, nicht nur das `ok`.

Codex kann keine Display-Tests fahren und sagt das ehrlich — aber seine Diffs
sahen hier dreimal plausibel aus und waren dreimal rot. Nachmessen ist Pflicht.
Nach jedem Xvfb-Lauf `xvfb-orphan-gc --apply`.
