# Track-Liste: der Selektionsanker fällt auf den laufenden Song zurück

Date: 2026-08-12
Status: design approved, not yet implemented
Baseline: `origin/dev` @ 0b65af7035 (der lokale Checkout hängt 220 Commits
zurück; jede Zeilenangabe unten ist aus `origin/dev` gelesen)

## Problem

Läuft ein Song und man will von ihm aus mit Shift+Klick den ganzen Interpreten
markieren, zieht die Auswahl von einer ganz anderen Stelle auf. Die Folge ist
eine falsche Markierung — im schlimmsten Fall gefolgt von einem Löschen, das
Zeilen trifft, die der Nutzer nie gesehen hat
(`track_actions.rs:137`, `remove_selected_from_playlist`).

Das ist kein Fehler, sondern eine Lücke zwischen zwei bewussten Entscheidungen.
`reveal_policy` (`current_track_selection.rs:38`) hält fest, dass ein
anlaufender Song nur den Marker setzt und Selektion, Fokus und Viewport
unangetastet lässt — NAV-10a, abgesichert durch
`nav_10a_row_activation_marker_does_not_move_selection_or_viewport`. Der
`MarkerOnly`-Zweig (`current_track_selection.rs:323`) hält das explizit fest.

Gleichzeitig besitzt die Track-Liste **keine eigene Selektionslogik**: Sie hängt
eine nackte `gtk4::MultiSelection` in die `ColumnView`
(`track_list_builder.rs:43`) und überlässt Shift+Klick komplett GTK. GTKs
`GtkListBase` führt einen internen Anker, den nur ein Klick oder eine
Fokusbewegung setzt. Da die Wiedergabe beides bewusst nicht tut, bleibt der
Anker dort, wo der Nutzer zuletzt geklickt hat — oder nach einem Ansichts-,
Sortier- oder Filterwechsel bei Zeile 0. Genau dann zieht Shift+Klick über die
halbe Bibliothek auf.

Die Podcast-Ansicht hat dieses Problem nicht: Sie führt einen expliziten
`anchor` und kennt bereits die Regel „kein Anker, oder ein Anker, der nicht
mehr auf dem Schirm ist → verhalte dich wie ein einfacher Klick"
(`podcasts_selection.rs:43` und die Notiz bei Zeile 67). Diese Spec zieht die
Track-Liste auf denselben Stand und ergänzt eine Regel, die es dort noch nicht
gibt: den laufenden Song als Rückfallanker.

## Beschlüsse (2026-08-12, alle final)

1. **Stiller Anker, keine Selektion.** Der laufende Song wird Ausgangspunkt für
   Shift-Auswahlen, bekommt aber weder Selektionsfarbe noch Tastaturfokus. Eine
   bestehende Selektion wird nicht angefasst; Kontextmenü und Löschen
   verhalten sich unverändert.
2. **Ohne Anker zieht nichts auf.** Gibt es weder einen eigenen Anker noch
   einen sichtbaren laufenden Song, verhält sich Shift+Klick wie ein normaler
   Klick: genau eine Zeile. Der gefährlichste Fall — Shift+Klick zieht ab
   Zeile 0 über hunderte Zeilen — verschwindet damit vollständig.
3. **Maus und Tastatur teilen einen Anker.** Shift+Pfeil und Shift+Space laufen
   über dieselbe Logik wie Shift+Klick, inklusive Song-Rückfall und
   Einzelklick-Regel.
4. **Ein eigener Klick gewinnt, bis die Ansicht wechselt.** Nicht bis zum
   nächsten Trackwechsel: Klickt der Nutzer Zeile 40 an und läuft der Player
   derweil zu einem anderen Track weiter, bleibt der Anker auf 40. Sonst
   verschöbe die Wiedergabe den Anker im Rücken des Nutzers.

## Entwurf

### Zustand: Anker und Cursor

Zwei Positionen in `Shared`, beide `Cell<Option<u32>>`:

- **`anchor`** — der feste Ausgangspunkt einer Spanne.
- **`cursor`** — das bewegliche Ende, deckungsgleich mit GTKs Fokuszeile.

Eine Spanne läuft immer von `anchor` bis `cursor`. Ein Klick ohne Modifier und
ein Ctrl+Klick setzen beide auf die getroffene Zeile; eine Shift-Auswahl bewegt
nur den `cursor` und lässt den `anchor` stehen — dieselbe Regel, die
`podcasts_selection` mit *„a range never moves the anchor"* testet. Eine Spanne
wird immer neu vom Anker genommen, nie an die bestehende Auswahl angebaut.

`cursor` ist nötig, weil Beschluss 3 die Tastatur einschließt: Shift+Pfeil muss
wissen, von welcher Zeile aus es einen Schritt geht, und GTK4 bietet keinen
öffentlichen Getter für die Fokusposition einer `ColumnView`.

**Lebensdauer.** Beide müssen einen Reload sowie jeden Sortier- und
Filterwechsel verlieren, weil Positionen einen Modellumbau nicht überleben. Das
geschieht nicht durch Reset-Aufrufe an jeder Stelle, die das Modell umbaut —
davon gibt es mehrere, und eine vergessene fiele als stiller Fehlgriff auf den
Nutzer zurück. Stattdessen führt jede Position eine Track-ID mit sich, und der
Lesepfad verwirft sie, sobald an dieser Position ein anderer Track steht. Ein
einziges Vorkommen, das nichts vergessen kann.

Gerechnet wird trotzdem mit der Position, nicht mit der ID: In einer Playlist
darf derselbe Track mehrfach stehen, weshalb auch der Löschpfad bewusst
positionsbasiert arbeitet
(`remove_selected_from_playlist_uses_positions_not_ids_for_duplicates`). Die ID
dient allein der Gültigkeitsprüfung. Nach dem Verwerfen greift der Rückfall.

### Ankerauflösung

Der laufende Song wird **nicht** in den Zustand geschrieben, sondern erst in dem
Moment aufgelöst, in dem eine Shift-Eingabe eintrifft:

```
effective_anchor = anchor.or_else(playing_position_in_current_view)
effective_cursor = cursor.or(effective_anchor)
```

`playing_position_in_current_view` ist das bereits vorhandene
`visible_position_for_track_in_source` (`current_track_selection.rs:57`), das
die Queue-Ansicht schon korrekt behandelt. Ist auch das `None`, fällt die
Eingabe auf `SelectMode::Only` zurück — Beschluss 2.

Daraus folgt Beschluss 4 ohne eigenen Code: Solange der Nutzer-Anker existiert,
gewinnt er; verworfen wird er nur beim Ansichtswechsel. Kein Zustand muss beim
Trackwechsel nachgezogen werden, und `current_track_selection.rs` wird gar nicht
angefasst.

### Komponenten

**1. Reine Logik (neues Modul).** `SelectMode { Only, Toggle, Range,
RangeAdditive }` und eine Funktion, die aus aktueller Auswahl, `anchor`,
`cursor`, Rückfallposition, Zeilenzahl, getroffener Zeile und Modus die neue
Auswahl samt neuem `anchor`/`cursor` berechnet. Naher Verwandter von
`podcasts_selection::apply_select` (`podcasts_selection.rs:43`), aber
positions- statt id-basiert. Keine GTK-Widgets, damit ohne Display testbar.

**2. Zeiger-Seam.** Ein `GestureClick` pro Zelle, installiert aus
`connect_setup`, nach dem Muster von `wire_context_menu_gesture`
(`track_list_context_menu.rs:102`) — das arbeitet bereits mit
`ListItem::position()` als stabilem Zeilenhandle und braucht kein Nachverdrahten
pro Bind, und es erspart uns, die Zeile aus Pixelkoordinaten zu erraten.

Der Gesture muss in der **Capture-Phase** liegen. `rating.rs` hält im Modulkopf
fest, dass innerhalb einer `ColumnView`-Zelle „the list row's own
click/selection machinery won the event over a plain `GestureClick`" — GTKs
Selektions-Gesture sitzt am `GtkListItemWidget`, also an einem Vorfahren der
Zelle. Da die Capture-Phase vollständig vor der Bubble-Phase läuft, kommt ein
Capture-Controller auf der Zelle davor; das Event wird per
`EventSequenceState::Claimed` beansprucht. Diese Annahme trägt den ganzen
Zeigerpfad und wird im Display-Test explizit nachgewiesen, nicht vorausgesetzt.

Der Gesture sieht alle primären Presses, greift aber nur bei einem Teil ein:

- **Mit Shift** — Spanne selbst setzen, Event per `Claimed` beanspruchen.
- **Ohne Shift** (einschließlich Ctrl+Klick) — nur `anchor` und `cursor` auf die
  getroffene Zeile merken, Event **nicht** beanspruchen. GTK selektiert
  anschließend wie gewohnt. Ein beobachtender statt beanspruchender Zweig ist
  hier nötig: Ctrl+Klick hinterlässt eine mehrzeilige Auswahl, aus der sich die
  getroffene Zeile nachträglich nicht mehr ablesen ließe.
- Der erste Press eines Doppelklicks merkt sich die Zeile ebenfalls — dasselbe
  Verhalten, das `pointer_intent` bei den Podcasts begründet
  (`podcasts_row_interaction.rs:64`).

Rechtsklickmenü, Drag-and-Drop und die Rating-Zelle bleiben unberührt.

**3. Tasten-Seam.** Ein `EventControllerKey` in der Capture-Phase auf der
`ColumnView`, neben dem vorhandenen Reorder-Controller
(`track_list_keyboard_reorder.rs:170`, der in der Standardphase hängt).
Behandelt werden zwei Fälle, beide ausgehend von `effective_cursor`:

- **Shift+Pfeil hoch/runter** — Ziel ist ein Schritt vom Cursor, auf die
  Listengrenzen geklemmt. Nach dem Setzen der Spanne ziehen Fokus und
  Sichtbarkeit mit `scroll_to(target, ListScrollFlags::FOCUS)` nach: Bei einer
  Tastatureingabe *soll* die Zeile in den Blick kommen, anders als beim
  Trackwechsel.
- **Shift+Space** — Ziel ist der Cursor selbst, die Spanne wird also ohne
  Bewegung neu vom Anker genommen. Kein `scroll_to`, weil sich der Fokus nicht
  verschiebt.

Pfeiltasten **ohne** Modifier werden nicht abgefangen: GTK bewegt Fokus und
Auswahl wie gewohnt, und ein Handler auf `selection_changed` zieht `anchor` und
`cursor` nach, wenn danach genau eine Zeile selektiert ist. So bleiben
Fokusrahmen und Anker synchron, ohne GTKs Fokusnavigation nachzubauen.

Ctrl+Shift geht als `RangeAdditive` durch dieselbe Auflösung — sonst kehrte für
diese eine Kombination das Zeile-0-Verhalten zurück.

### Was ausdrücklich nicht passiert

Kein `scroll_to`, kein `grab_focus`, kein `select_item` als Folge eines
Trackwechsels. `current_track_selection.rs` bleibt unverändert, NAV-10a gilt
weiter. Der laufende Song wirkt rein passiv und nur in dem Moment, in dem eine
Shift-Eingabe eintrifft.

### Randfälle

- Anker oder Cursor jenseits der Zeilenzahl nach einem Modellumbau → verworfen,
  Rückfall greift.
- Laufender Song nicht in der Ansicht → `Only`.
- Queue-Ansicht → `visible_position_for_track_in_source` liefert dort die
  Queue-Position; unverändert.
- Leere Liste, oder ein `ListItem` mit `INVALID_LIST_POSITION` → nichts tun und
  wie der Kontextmenü-Pfad protokollieren.
- Spanne rückwärts (Cursor oberhalb des Ankers) → normal, Grenzen werden
  sortiert.

## Tests

Der Schwerpunkt liegt auf der reinen Funktion, denn sie trägt die Regel:

- Nur-Klick setzt Anker und Cursor; eine Spanne bewegt den Anker nicht.
- Spanne ohne Anker und ohne laufenden Song → genau die geklickte Zeile.
- Spanne ohne Anker, aber mit sichtbarem laufenden Song → vom Song aus.
- Eigener Anker schlägt den laufenden Song.
- Spanne rückwärts; Spanne wird neu genommen, nicht angebaut.
- `RangeAdditive` erhält die bestehende Auswahl außerhalb der Spanne.
- Anker jenseits der Zeilenzahl → wie kein Anker.

Dazu Display-Tests (`#[ignore]`, via `xvfb-run`):

- Shift+Pfeil runter erweitert vom laufenden Song aus und bewegt den Fokus mit.
- Eine Einzelauswahl zieht Anker und Cursor nach — das ist der Nachzug für
  Pfeiltasten ohne Modifier —, während eine mehrzeilige Auswahl den Anker
  stehen lässt.
- Ein Anker, dessen Zeile inzwischen einen anderen Track trägt, wird gegen das
  echte Modell verworfen.
- Eine Spanne weit oberhalb des Viewports zieht ihn nicht dorthin — die
  Regression gegen NAV-10b. Nur der Tastaturpfad scrollt, und dort ist es
  gewollt.

**Dass der Klick überhaupt ankommt, kann keiner dieser Tests zeigen.** Sie rufen
Handler auf, statt Eingaben zu senden, und genau dazwischen liegt die
Capture-Phasen-Annahme. Dafür ist `scripts/ptr-e2e/` da: Es treibt das echte
Fenster mit `xdotool` in einem Wegwerf-Xvfb und liest das Ergebnis aus dem
stderr-Log der App. Sein README nennt diese Fehlerklasse namentlich — sie ist
schon einmal an der Rating-Zelle ausgeliefert worden, während alle
Signal-Seam-Tests grün blieben. Der Nachweis, dass ein echter Shift-Klick die
Spanne beim laufenden Song beginnen lässt und nicht bei Zeile 0, gehört deshalb
dorthin, nicht in die Rust-Suite.

## Bekannte Grenzen

GTKs interner Anker bleibt daneben bestehen und läuft nach einer
Shift-Auswahl von unserem ab. Solange alle Shift-Eingaben abgefangen werden,
liest ihn nichts mehr; jede künftige Selektionsgeste muss aber über dieselbe
Auflösung gehen, statt sich auf GTK zu verlassen.

Ein Bestätigungsdialog beim Löschen mehrerer Zeilen kam im Gespräch auf und
wurde am 12.08.2026 **verworfen** — kein Folge-Task. Er hätte die Folge
abgesichert, nicht die Ursache, und die Ursache behebt diese Spec.
